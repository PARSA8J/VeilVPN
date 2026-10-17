package main

import (
	"bufio"
	"crypto/sha256"
	"encoding/binary"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"log"
	"net"
	"net/http"
	"net/url"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"sync"
	"time"
)

/* ---------------- local proxy (HTTP CONNECT + SOCKS5) ---------------- */

var (
	proxyMu     sync.Mutex
	proxyLn     net.Listener
	proxyActive *Node
)

func startProxy(node *Node) error {
	proxyMu.Lock()
	defer proxyMu.Unlock()
	if proxyLn != nil {
		return nil
	}
	ln, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		return err
	}
	proxyLn = ln
	proxyActive = node
	log.Printf("proxy on %s via %s(%s)", ln.Addr(), node.Proto, node.Host)
	go func() {
		for {
			c, err := ln.Accept()
			if err != nil {
				return
			}
			go handleProxyConn(c, node)
		}
	}()
	return nil
}

func stopProxy() {
	proxyMu.Lock()
	defer proxyMu.Unlock()
	if proxyLn != nil {
		proxyLn.Close()
		proxyLn = nil
		proxyActive = nil
		log.Printf("proxy stopped")
	}
}

func handleProxyConn(c net.Conn, node *Node) {
	defer c.Close()
	br := bufio.NewReader(c)
	c.SetDeadline(time.Now().Add(20 * time.Second))
	first, err := br.Peek(1)
	if err != nil {
		return
	}
	if first[0] == 5 { // SOCKS5
		c.SetDeadline(time.Time{})
		socks5(c, br, node)
		return
	}
	req, err := http.ReadRequest(br)
	if err != nil {
		return
	}
	if req.Method == http.MethodConnect {
		host, portStr, err := net.SplitHostPort(req.Host)
		if err != nil {
			host, portStr = req.Host, "443"
		}
		port, _ := strconv.Atoi(portStr)
		remote, err := dialNode(node, host, port)
		if err != nil {
			c.Write([]byte("HTTP/1.1 502 Bad Gateway\r\n\r\n"))
			log.Printf("CONNECT %s: %v", req.Host, err)
			return
		}
		c.SetDeadline(time.Time{})
		c.Write([]byte("HTTP/1.1 200 Connection Established\r\n\r\n"))
		pipe(c, remote)
		return
	}
	u, err := url.Parse(req.URL.String())
	if err != nil || u.Hostname() == "" {
		return
	}
	port := 80
	if p := u.Port(); p != "" {
		port, _ = strconv.Atoi(p)
	}
	remote, err := dialNode(node, u.Hostname(), port)
	if err != nil {
		c.Write([]byte("HTTP/1.1 502 Bad Gateway\r\n\r\n"))
		return
	}
	c.SetDeadline(time.Time{})
	if err := req.Write(remote); err != nil {
		remote.Close()
		return
	}
	pipe(c, remote)
}

func socks5(c net.Conn, br *bufio.Reader, node *Node) {
	hdr := make([]byte, 2)
	if _, err := io.ReadFull(br, hdr); err != nil || hdr[0] != 5 {
		return
	}
	nmethods := int(hdr[1])
	io.ReadFull(br, make([]byte, nmethods))
	c.Write([]byte{5, 0})
	ver, _ := br.ReadByte()
	if ver != 5 {
		return
	}
	cmd, _ := br.ReadByte()
	br.ReadByte() // rsv
	atyp, _ := br.ReadByte()
	var host string
	var port int
	switch atyp {
	case 1:
		b := make([]byte, 4)
		io.ReadFull(br, b)
		host = fmt.Sprintf("%d.%d.%d.%d", b[0], b[1], b[2], b[3])
	case 3:
		l, _ := br.ReadByte()
		b := make([]byte, l)
		io.ReadFull(br, b)
		host = string(b)
	case 4:
		b := make([]byte, 16)
		io.ReadFull(br, b)
		host = net.IP(b).String()
	default:
		return
	}
	pb := make([]byte, 2)
	io.ReadFull(br, pb)
	port = int(binary.BigEndian.Uint16(pb))
	if cmd != 1 {
		c.Write([]byte{5, 7, 0, 1, 0, 0, 0, 0, 0, 0})
		return
	}
	remote, err := dialNode(node, host, port)
	if err != nil {
		c.Write([]byte{5, 5, 0, 1, 0, 0, 0, 0, 0, 0})
		log.Printf("socks5 %s:%d: %v", host, port, err)
		return
	}
	c.Write([]byte{5, 0, 0, 1, 0, 0, 0, 0, 0, 0})
	pipe(c, remote)
}

func pipe(a, b net.Conn) {
	done := make(chan struct{}, 2)
	go func() { io.Copy(b, a); b.SetDeadline(time.Now()); done <- struct{}{} }()
	go func() { io.Copy(a, b); a.SetDeadline(time.Now()); done <- struct{}{} }()
	<-done
	a.Close()
	b.Close()
}

/* ---------------- system proxy via registry ---------------- */

const (
	HKEY_CURRENT_USER                = 0x80000001
	KEY_SET_VALUE                    = 0x0002
	KEY_QUERY_VALUE                  = 0x0001
	REG_SZ                           = 1
	REG_DWORD                        = 4
	INTERNET_OPTION_SETTINGS_CHANGED = 39
	INTERNET_OPTION_REFRESH          = 37
)

func setSystemProxy(enable bool) error {
	subKey, _ := syscallUTF16Ptr(`Software\Microsoft\Windows\CurrentVersion\Internet Settings`)
	var hk uintptr
	r0, _, _ := procRegOpenKeyExW.Call(uintptr(HKEY_CURRENT_USER), uintptr(subKey), 0,
		uintptr(KEY_SET_VALUE|KEY_QUERY_VALUE), uintptr(unsafePtr(&hk)))
	if r0 != 0 {
		return fmt.Errorf("RegOpenKeyExW=%d", r0)
	}
	defer procRegCloseKey.Call(hk)

	enableVal := uint32(0)
	if enable {
		enableVal = 1
	}
	ev := make([]byte, 4)
	binary.LittleEndian.PutUint32(ev, enableVal)
	nameEnable, _ := syscallUTF16Ptr("ProxyEnable")
	procRegSetValueExW.Call(hk, uintptr(nameEnable), 0, REG_DWORD,
		uintptr(unsafePtr(&ev[0])), 4)

	srv := "127.0.0.1:" + proxyPortStr()
	if !enable {
		srv = ""
	}
	sb := utf16Bytes(srv)
	nameServer, _ := syscallUTF16Ptr("ProxyServer")
	procRegSetValueExW.Call(hk, uintptr(nameServer), 0, REG_SZ,
		uintptr(unsafePtr(&sb[0])), uintptr(len(sb)))

	over := "localhost;127.*;10.*;172.16.*;172.17.*;172.18.*;172.19.*;172.2?.*;172.30.*;172.31.*;192.168.*;<local>"
	if !enable {
		over = ""
	}
	ob := utf16Bytes(over)
	nameOver, _ := syscallUTF16Ptr("ProxyOverride")
	procRegSetValueExW.Call(hk, uintptr(nameOver), 0, REG_SZ,
		uintptr(unsafePtr(&ob[0])), uintptr(len(ob)))

	procInternetSetOptionW.Call(0, INTERNET_OPTION_SETTINGS_CHANGED, 0, 0)
	procInternetSetOptionW.Call(0, INTERNET_OPTION_REFRESH, 0, 0)
	log.Printf("system proxy enabled=%v server=%s", enable, srv)
	return nil
}

/* ---------------- http helpers ---------------- */

func httpGetDirect(u string, timeout time.Duration) (string, error) {
	client := &http.Client{Timeout: timeout}
	resp, err := client.Get(u)
	if err != nil {
		return "", err
	}
	defer resp.Body.Close()
	b, err := io.ReadAll(io.LimitReader(resp.Body, 2_000_000))
	if err != nil {
		return "", err
	}
	return string(b), nil
}

func httpGetViaProxy(u string, timeout time.Duration) (string, error) {
	proxyMu.Lock()
	ln := proxyLn
	proxyMu.Unlock()
	if ln == nil {
		return "", errors.New("proxy not running")
	}
	_, port, _ := net.SplitHostPort(ln.Addr().String())
	pu, _ := url.Parse("http://127.0.0.1:" + port)
	client := &http.Client{
		Timeout:   timeout,
		Transport: &http.Transport{Proxy: http.ProxyURL(pu)},
	}
	resp, err := client.Get(u)
	if err != nil {
		return "", err
	}
	defer resp.Body.Close()
	b, err := io.ReadAll(io.LimitReader(resp.Body, 2_000_000))
	if err != nil {
		return "", err
	}
	return string(b), nil
}

/* ---------------- license ---------------- */

type licInfo struct {
	Type     string `json:"type"`
	Exp      int64  `json:"exp"`
	DaysLeft int    `json:"daysLeft"`
	Active   bool   `json:"active"`
	Device   string `json:"device"`
}

func stateDir() string {
	base := os.Getenv("LOCALAPPDATA")
	if base == "" {
		base = "."
	}
	return filepath.Join(base, "VeilVPN")
}

func licensePath() string { return filepath.Join(stateDir(), "license.json") }

func deviceID() string {
	if g := regReadString(`SOFTWARE\Microsoft\Cryptography`, "MachineGuid"); g != "" {
		return fmt.Sprintf("%x", sha256.Sum256([]byte("veilwin:"+g)))[:8]
	}
	hn, _ := os.Hostname()
	return fmt.Sprintf("%x", sha256.Sum256([]byte("veilwin:"+hn)))[:8]
}

func licenseInit() {
	if _, err := os.Stat(licensePath()); os.IsNotExist(err) {
		lic := licInfo{Type: "free", Exp: time.Now().Add(3 * 24 * time.Hour).UnixMilli(), Device: deviceID()}
		saveLicense(lic)
	}
	lic := licenseInfo()
	if lic.Exp == 0 && lic.Type == "free" {
		lic = licInfo{Type: "free", Exp: time.Now().Add(3 * 24 * time.Hour).UnixMilli(), Device: deviceID()}
		saveLicense(lic)
	}
}

func licenseInfo() licInfo {
	b, err := os.ReadFile(licensePath())
	if err != nil {
		return licInfo{Device: deviceID()}
	}
	var lic licInfo
	if err := json.Unmarshal(b, &lic); err != nil {
		return licInfo{Device: deviceID()}
	}
	lic.Device = deviceID()
	if lic.Exp == 0 && lic.Type == "pre" {
		lic.Active = true
		lic.DaysLeft = 9999
	} else {
		d := (lic.Exp - time.Now().UnixMilli()) / 86400000
		lic.DaysLeft = int(d) + 1
		if lic.DaysLeft < 0 {
			lic.DaysLeft = 0
		}
		lic.Active = time.Now().UnixMilli() < lic.Exp
	}
	return lic
}

func licActive(lic licInfo) bool { return lic.Active }

func saveLicense(lic licInfo) {
	os.MkdirAll(stateDir(), 0o755)
	b, _ := json.Marshal(lic)
	os.WriteFile(licensePath(), b, 0o644)
}

func licenseActivate(key string) map[string]interface{} {
	key = strings.ToUpper(strings.TrimSpace(key))
	if strings.HasPrefix(key, "VEIL-PREMIUM-") {
		lic := licInfo{Type: "pre", Exp: 0, Device: deviceID(), Active: true, DaysLeft: 9999}
		saveLicense(lic)
		return map[string]interface{}{"ok": true, "lic": lic}
	}
	if strings.HasPrefix(key, "VEIL-FREE-") {
		lic := licInfo{Type: "free", Exp: time.Now().Add(30 * 24 * time.Hour).UnixMilli(), Device: deviceID()}
		saveLicense(lic)
		return map[string]interface{}{"ok": true, "lic": licenseInfo()}
	}
	return map[string]interface{}{"ok": false, "error": "invalid key"}
}

func licenseDeactivate() {
	os.Remove(licensePath())
	licenseInit()
}

/* ---------------- misc ---------------- */

func proxyPortStr() string {
	proxyMu.Lock()
	defer proxyMu.Unlock()
	if proxyLn != nil {
		_, port, _ := net.SplitHostPort(proxyLn.Addr().String())
		return port
	}
	return "0"
}

func openBrowser(target string) {
	tgt, _ := syscallUTF16Ptr(target)
	procShellExecuteW.Call(0, uintptr(openVerb), uintptr(tgt), 0, 0, 1)
}

var openVerb, _ = syscallUTF16Ptr("open")
