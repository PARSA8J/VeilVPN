package main

import (
	"encoding/binary"
	"encoding/json"
	"fmt"
	"net"
	"sort"
	"strings"
	"sync"
	"time"
	"unsafe"
)

var (
	advapi32               = NewLazy("advapi32.dll")
	procRegOpenKeyExW      = advapi32.NewProc("RegOpenKeyExW")
	procRegSetValueExW     = advapi32.NewProc("RegSetValueExW")
	procRegQueryValueExW   = advapi32.NewProc("RegQueryValueExW")
	procRegCloseKey        = advapi32.NewProc("RegCloseKey")
	wininet                = NewLazy("wininet.dll")
	procInternetSetOptionW = wininet.NewProc("InternetSetOptionW")
	shell32                = NewLazy("shell32.dll")
	procShellExecuteW      = shell32.NewProc("ShellExecuteW")
)

func syscallUTF16Ptr(s string) (uintptr, error) {
	u, err := syscallUTF16FromString(s)
	if err != nil {
		return 0, err
	}
	return uintptr(unsafe.Pointer(&u[0])), nil
}

func utf16Bytes(s string) []byte {
	u, _ := syscallUTF16FromString(s)
	b := make([]byte, len(u)*2)
	for i, x := range u {
		binary.LittleEndian.PutUint16(b[i*2:], x)
	}
	return b
}

func unsafePtr(p interface{}) unsafe.Pointer {
	switch v := p.(type) {
	case *uintptr:
		return unsafe.Pointer(v)
	case *byte:
		return unsafe.Pointer(v)
	case *uint32:
		return unsafe.Pointer(v)
	}
	return nil
}

func regReadString(sub, name string) string {
	sk, _ := syscallUTF16Ptr(sub)
	nm, _ := syscallUTF16Ptr(name)
	var hk uintptr
	r0, _, _ := procRegOpenKeyExW.Call(uintptr(0x80000002), sk, 0, 0x0001|0x0020, uintptr(unsafePtr(&hk)))
	if r0 != 0 {
		return ""
	}
	defer procRegCloseKey.Call(hk)
	var buf [512]byte
	var cb uint32 = 512
	var typ uint32
	r1, _, _ := procRegQueryValueExW.Call(hk, nm, 0, uintptr(unsafePtr(&typ)), uintptr(unsafePtr(&buf[0])), uintptr(unsafePtr(&cb)))
	if r1 != 0 || cb < 2 {
		return ""
	}
	u := make([]uint16, cb/2)
	for i := range u {
		u[i] = uint16(buf[i*2]) | uint16(buf[i*2+1])<<8
	}
	return strings.TrimRight(syscallUTF16ToString(u), "\x00")
}

/* ---------------- servers fetch + ping ---------------- */

var (
	nodesMu    sync.Mutex
	nodesList  []Node
	customSrc  string
	seedLoaded bool
)

func setCustomSource(u string) {
	nodesMu.Lock()
	customSrc = u
	nodesMu.Unlock()
	go loadSeedAndRefresh()
}

var sources = []string{
	"https://raw.githubusercontent.com/Epodonios/v2ray-configs/main/All_Configs_Sub.txt",
	"https://cdn.jsdelivr.net/gh/Epodonios/v2ray-configs@main/All_Configs_Sub.txt",
	"https://raw.githubusercontent.com/mahdibland/V2RayAggregator/master/sub/sub_merge.txt",
	"https://cdn.jsdelivr.net/gh/mahdibland/V2RayAggregator@master/sub/sub_merge.txt",
	"https://raw.githubusercontent.com/barry-far/V2ray-Configs/main/All_Configs_Sub.txt",
	"https://cdn.jsdelivr.net/gh/barry-far/V2ray-Configs@main/All_Configs_Sub.txt",
}

func loadSeedAndRefresh() {
	nodesMu.Lock()
	seedLoadedOnce := seedLoaded
	csrc := customSrc
	nodesMu.Unlock()

	if !seedLoadedOnce {
		seed := []Node{}
		for _, line := range strings.Split(string(seedTXT), "\n") {
			if n := parseNodeURI(strings.TrimSpace(line)); n != nil {
				seed = append(seed, *n)
			}
		}
		nodesMu.Lock()
		nodesList = seed
		nodesMu.Unlock()
		go pingAll(120)
	}

	urls := []string{}
	if csrc != "" {
		urls = append(urls, csrc)
	}
	urls = append(urls, sources...)

	found := map[string]string{}
	var mu sync.Mutex
	var wg sync.WaitGroup
	for _, u := range urls {
		wg.Add(1)
		go func(u string) {
			defer wg.Done()
			txt, err := httpGetDirect(u, 20*time.Second)
			if err != nil {
				return
			}
			for _, line := range decodeSub(txt) {
				if n := parseNodeURI(line); n != nil {
					mu.Lock()
					found[n.Host+fmt.Sprintf(":%d", n.Port)+n.Proto] = line
					mu.Unlock()
				}
			}
		}(u)
	}
	wg.Wait()

	nodesMu.Lock()
	next := nodesList[:0]
	seen := map[string]bool{}
	for _, line := range found {
		if n := parseNodeURI(line); n != nil {
			k := n.ID
			if !seen[k] {
				seen[k] = true
				for _, old := range nodesList {
					if old.ID == n.ID && old.Ping > 0 {
						n.Ping = old.Ping
						break
					}
				}
				if len(next) < 600 {
					next = append(next, *n)
				}
			}
		}
	}
	nodesList = next
	seedLoaded = true
	nodesMu.Unlock()
	pushServersEvent()
	go pingAll(80)
}

func decodeSub(txt string) []string {
	txt = strings.TrimSpace(txt)
	var lines []string
	if strings.Contains(txt, "://") {
		lines = strings.Split(txt, "\n")
	} else {
		dec := string(b64d(txt))
		if strings.Contains(dec, "://") {
			lines = strings.Split(dec, "\n")
		} else {
			lines = strings.Split(txt, "\n")
		}
	}
	out := []string{}
	for _, l := range lines {
		l = strings.TrimSpace(l)
		if strings.HasPrefix(l, "vmess://") {
			continue
		}
		if len(l) > 8 && (strings.HasPrefix(l, "vless://") || strings.HasPrefix(l, "trojan://") || strings.HasPrefix(l, "ss://")) {
			out = append(out, l)
		}
	}
	return out
}

func pingBatch(ids []interface{}) map[string]float64 {
	out := map[string]float64{}
	for _, idv := range ids {
		id, _ := idv.(string)
		n := findNode(id)
		if n == nil {
			continue
		}
		nodesMu.Lock()
		idx := findIdx(id)
		if idx >= 0 && nodesList[idx].Ping > 0 {
			out[id] = nodesList[idx].Ping
			nodesMu.Unlock()
			continue
		}
		nodesMu.Unlock()
		if ms, err := tcpPing(n.Host, n.Port); err == nil {
			nodesMu.Lock()
			idx = findIdx(id)
			if idx >= 0 {
				nodesList[idx].Ping = ms
			}
			nodesMu.Unlock()
			out[id] = ms
		}
	}
	return out
}

func findIdx(id string) int {
	for i := range nodesList {
		if nodesList[i].ID == id {
			return i
		}
	}
	return -1
}

func tcpPing(host string, port int) (float64, error) {
	t0 := time.Now()
	c, err := net.DialTimeout("tcp", net.JoinHostPort(host, fmt.Sprintf("%d", port)), 3500*time.Millisecond)
	if err != nil {
		return 0, err
	}
	c.Close()
	return float64(time.Since(t0).Milliseconds()), nil
}

func pingAll(limit int) {
	type target struct {
		idx  int
		host string
		port int
	}
	nodesMu.Lock()
	targets := []target{}
	n := 0
	for i := range nodesList {
		if n >= limit {
			break
		}
		if nodesList[i].Ping > 0 {
			continue
		}
		targets = append(targets, target{idx: i, host: nodesList[i].Host, port: nodesList[i].Port})
		n++
	}
	nodesMu.Unlock()
	var wg sync.WaitGroup
	sem := make(chan struct{}, 24)
	for _, t := range targets {
		t := t
		wg.Add(1)
		go func() {
			defer wg.Done()
			sem <- struct{}{}
			if ms, err := tcpPing(t.host, t.port); err == nil {
				nodesMu.Lock()
				if t.idx < len(nodesList) {
					nodesList[t.idx].Ping = ms
				}
				nodesMu.Unlock()
			}
			<-sem
		}()
	}
	wg.Wait()
	pushServersEvent()
}

func nodesSortedByPing() []*Node {
	nodesMu.Lock()
	defer nodesMu.Unlock()
	sorted := make([]*Node, 0, len(nodesList))
	for i := range nodesList {
		sorted = append(sorted, &nodesList[i])
	}
	sort.Slice(sorted, func(i, j int) bool { return sorted[i].Ping < sorted[j].Ping })
	return sorted
}

func serversSnapshot() []map[string]interface{} {
	nodesMu.Lock()
	defer nodesMu.Unlock()
	out := []map[string]interface{}{}
	for i := range nodesList {
		n := &nodesList[i]
		cc := n.CC
		if cc == "" {
			cc = "XX"
		}
		m := map[string]interface{}{
			"id":   n.ID,
			"p":    n.Proto,
			"cc":   cc,
			"name": n.displayName(),
		}
		if n.Ping > 0 {
			m["ping"] = n.Ping
		}
		out = append(out, m)
	}
	return out
}

func pushServersEvent() {
	b, _ := json.Marshal(map[string]interface{}{"kind": "servers", "servers": serversSnapshot()})
	pushEvent(b)
}
