package main

import (
	"encoding/json"
	"log"
	"net/http"
	"strings"
	"sync"
	"time"
)

var (
	evMu      sync.Mutex
	evChan    chan []byte
	stateMu   sync.Mutex
	coreState = "disconnected"
	coreErr   string
	coreNode  *Node
	features  = map[string]bool{"kill": true, "auto": true, "adblock": false}
)

func pushEvent(b []byte) {
	if evChan == nil {
		evChan = make(chan []byte, 32)
	}
	select {
	case evChan <- b:
	default:
	}
}

func pushStatus() {
	stateMu.Lock()
	st := struct {
		State  string `json:"state"`
		Error  string `json:"error,omitempty"`
		Server *Node  `json:"server,omitempty"`
	}{coreState, coreErr, coreNode}
	stateMu.Unlock()
	b, _ := json.Marshal(map[string]interface{}{"kind": "status", "state": st})
	pushEvent(b)
}

func setState(s, errStr string) {
	stateMu.Lock()
	coreState = s
	coreErr = errStr
	if s == "disconnected" {
		coreNode = nil
	}
	stateMu.Unlock()
	pushStatus()
}

func apiEvents(w http.ResponseWriter, r *http.Request) {
	if evChan == nil {
		evChan = make(chan []byte, 32)
	}
	select {
	case b := <-evChan:
		w.Header().Set("Content-Type", "application/json")
		w.Write(b)
	case <-time.After(25 * time.Second):
		w.Header().Set("Content-Type", "application/json")
		w.Write([]byte("{}"))
	case <-r.Context().Done():
	}
}

func apiDispatch(w http.ResponseWriter, r *http.Request) {
	action := strings.TrimPrefix(r.URL.Path, "/api/")
	var payload map[string]interface{}
	_ = json.NewDecoder(r.Body).Decode(&payload)
	if payload == nil {
		payload = map[string]interface{}{}
	}
	log.Printf("api: %s %v", action, payload)

	var result map[string]interface{}
	switch action {
	case "fetchServers":
		result = map[string]interface{}{"servers": serversSnapshot()}
	case "pingBatch":
		ids, _ := payload["ids"].([]interface{})
		result = map[string]interface{}{"pings": pingBatch(ids)}
	case "connect":
		result = handleConnect(payload)
	case "disconnect":
		result = handleDisconnect()
	case "getState":
		stateMu.Lock()
		st := coreState
		nd := coreNode
		stateMu.Unlock()
		lic := licenseInfo()
		var licMap map[string]interface{}
		if b, err := json.Marshal(lic); err == nil {
			json.Unmarshal(b, &licMap)
		}
		feats := map[string]interface{}{}
		for k, v := range features {
			feats[k] = v
		}
		result = map[string]interface{}{
			"status":   st,
			"platform": "windows",
			"version":  version,
			"features": feats,
			"lic":      licMap,
		}
		if st == "connected" && nd != nil {
			result["server"] = map[string]interface{}{"id": nd.ID, "cc": nd.CC, "name": nd.displayName()}
		}
	case "licenseInfo":
		var m map[string]interface{}
		if b, err := json.Marshal(licenseInfo()); err == nil {
			json.Unmarshal(b, &m)
		}
		result = m
	case "activateKey":
		key, _ := payload["key"].(string)
		result = licenseActivate(key)
	case "deactivateKey":
		licenseDeactivate()
		result = map[string]interface{}{"ok": true}
	case "setFeature":
		k, _ := payload["key"].(string)
		on, _ := payload["on"].(bool)
		features[k] = on
		result = map[string]interface{}{"ok": true}
	case "setLang":
		result = map[string]interface{}{"ok": true}
	case "setSource":
		u, _ := payload["url"].(string)
		setCustomSource(u)
		result = map[string]interface{}{"ok": true}
	case "getIpInfo":
		result = handleIpInfo()
	case "openUrl":
		u, _ := payload["url"].(string)
		if u == "" {
			u = botURL
		}
		openBrowser(u)
		result = map[string]interface{}{"ok": true}
	default:
		http.Error(w, `{"error":"unknown action"}`, 404)
		return
	}

	w.Header().Set("Content-Type", "application/json")
	if result == nil {
		result = map[string]interface{}{"ok": false, "error": "internal"}
	}
	json.NewEncoder(w).Encode(result)
}

func handleConnect(payload map[string]interface{}) map[string]interface{} {
	lic := licenseInfo()
	if !licActive(lic) {
		return map[string]interface{}{"ok": false, "error": "activate"}
	}
	id, _ := payload["id"].(string)
	node := findNode(id)
	if node == nil {
		node = bestNode()
		if node == nil {
			return map[string]interface{}{"ok": false, "error": "no servers"}
		}
	}
	stateMu.Lock()
	coreState = "connecting"
	coreNode = node
	coreErr = ""
	stateMu.Unlock()
	pushStatus()
	go connectCore(node)
	return map[string]interface{}{
		"ok": true, "async": true,
		"server": map[string]interface{}{"id": node.ID, "cc": node.CC, "name": node.displayName()},
	}
}

func connectCore(node *Node) {
	setState("connecting", "")
	ok, errStr := testNode(node)
	if !ok {
		for i, alt := range failoverCandidates(node) {
			if i >= 3 {
				break
			}
			stateMu.Lock()
			coreNode = alt
			stateMu.Unlock()
			pushStatus()
			ok, errStr = testNode(alt)
			if ok {
				node = alt
				break
			}
		}
	}
	if !ok {
		setState("disconnected", errStr)
		return
	}
	if err := startProxy(node); err != nil {
		setState("disconnected", "proxy: "+err.Error())
		return
	}
	if err := setSystemProxy(true); err != nil {
		stopProxy()
		setState("disconnected", "sysproxy: "+err.Error())
		return
	}
	setState("connected", "")
	go watchCore(node)
}

func watchCore(node *Node) {
	for {
		time.Sleep(12 * time.Second)
		stateMu.Lock()
		st := coreState
		cur := coreNode
		stateMu.Unlock()
		if st != "connected" || cur == nil {
			return
		}
		ok, _ := testNode(cur)
		if !ok {
			log.Printf("watchdog: node dead, reconnecting")
			stopProxy()
			setSystemProxy(false)
			go connectCore(cur)
			return
		}
	}
}

func handleDisconnect() map[string]interface{} {
	stopProxy()
	setSystemProxy(false)
	setState("disconnected", "")
	return map[string]interface{}{"ok": true}
}

func handleIpInfo() map[string]interface{} {
	out := map[string]interface{}{}
	stateMu.Lock()
	connected := coreState == "connected"
	stateMu.Unlock()
	urls := []string{"https://api.ip.sb/geoip", "http://ip-api.com/json/?fields=query,countryCode,city"}
	var body string
	if connected {
		for _, u := range urls {
			if b, err := httpGetViaProxy(u, 12*time.Second); err == nil {
				body = b
				break
			}
		}
	}
	if body == "" {
		for _, u := range urls {
			if b, err := httpGetDirect(u, 12*time.Second); err == nil {
				body = b
				break
			}
		}
	}
	if body != "" {
		var j map[string]interface{}
		if err := json.Unmarshal([]byte(body), &j); err == nil {
			ip, _ := j["query"].(string)
			if ip == "" {
				ip, _ = j["ip"].(string)
			}
			cc, _ := j["countryCode"].(string)
			city, _ := j["city"].(string)
			if ip != "" {
				out["ip"] = ip
				out["cc"] = cc
				out["city"] = city
			}
		}
	}
	return out
}

func failoverCandidates(from *Node) []*Node {
	out := []*Node{}
	list := nodesSortedByPing()
	for _, n := range list {
		if n.ID != from.ID {
			out = append(out, n)
		}
		if len(out) >= 6 {
			break
		}
	}
	return out
}

func bestNode() *Node {
	list := nodesSortedByPing()
	if len(list) == 0 {
		return nil
	}
	return list[0]
}

func findNode(id string) *Node {
	nodesMu.Lock()
	defer nodesMu.Unlock()
	for i := range nodesList {
		if nodesList[i].ID == id {
			return &nodesList[i]
		}
	}
	return nil
}
