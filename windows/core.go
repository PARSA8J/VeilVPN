package main

import (
	"bufio"
	crand "crypto/rand"
	"crypto/aes"
	"crypto/cipher"
	"crypto/hmac"
	"crypto/md5"
	"crypto/sha1"
	"crypto/sha256"
	"crypto/tls"
	"encoding/base64"
	"encoding/binary"
	"encoding/hex"
	"errors"
	"fmt"
	"hash/fnv"
	"io"
	"net"
	"strconv"
	"strings"
	"time"
)

/* ---------------- Node model + URI parsing ---------------- */

type Node struct {
	ID, Proto, Host, CC, Remark string
	Port                        int
	UUID, Password, Method      string
	Net                         string // tcp | ws
	TLS                         bool
	Sni, Path, HostHeader       string
	Ping                        float64
}

func (n *Node) displayName() string {
	r := strings.Map(func(r rune) rune {
		if r >= 0x1F000 || r == 0xFE0F || r == 0x200D {
			return -1
		}
		return r
	}, n.Remark)
	r = strings.TrimSpace(r)
	if len(r) > 1 {
		return r
	}
	return n.Host
}

func stdb64(s string) []byte {
	d, err := base64.StdEncoding.DecodeString(s)
	if err != nil {
		return []byte{}
	}
	return d
}

func b64d(s string) []byte {
	s = strings.TrimSpace(s)
	s = strings.NewReplacer("-", "+", "_", "/").Replace(s)
	for len(s)%4 != 0 {
		s += "="
	}
	return stdb64(s)
}

func crandRead(b []byte) (int, error) { return crand.Read(b) }

func flagToCC(f string) string {
	rs := []rune(f)
	if len(rs) != 2 {
		return ""
	}
	cc := make([]byte, 2)
	for i, r := range rs {
		if r < 0x1F1E6 || r > 0x1F1FF {
			return ""
		}
		cc[i] = byte(r - 0x1F1E6 + 'A')
	}
	return string(cc)
}

func ccOfRemark(remark string) string {
	idx := strings.IndexFunc(remark, func(x rune) bool { return x >= 0x1F1E6 && x <= 0x1F1FF })
	if idx >= 0 {
		rs := []rune(remark[idx:])
		if len(rs) >= 2 {
			if cc := flagToCC(string(rs[0]) + string(rs[1])); cc != "" {
				return cc
			}
		}
	}
	for _, tok := range strings.FieldsFunc(remark, func(r rune) bool { return !isLetterNum(r) }) {
		if len(tok) == 2 && tok == strings.ToUpper(tok) && isAZ(tok) {
			return tok
		}
	}
	return ""
}

func isLetterNum(r rune) bool { return r > 32 && r < 127 }
func isAZ(s string) bool {
	for _, c := range s {
		if c < 'A' || c > 'Z' {
			return false
		}
	}
	return true
}

func parseNodeURI(uri string) *Node {
	uri = strings.TrimSpace(uri)
	switch {
	case strings.HasPrefix(uri, "vless://") || strings.HasPrefix(uri, "trojan://"):
		return parseVlessTrojan(uri)
	case strings.HasPrefix(uri, "ss://"):
		return parseSS(uri)
	}
	return nil
}

func parseVlessTrojan(uri string) *Node {
	proto := "vless"
	if strings.HasPrefix(uri, "trojan") {
		proto = "trojan"
	}
	body := uri[len(proto)+3:]
	var remark string
	if i := strings.Index(body, "#"); i >= 0 {
		remark = urlDecode(body[i+1:])
		body = body[:i]
	}
	qs := map[string]string{}
	if i := strings.Index(body, "?"); i >= 0 {
		for _, kv := range strings.Split(body[i+1:], "&") {
			if j := strings.Index(kv, "="); j > 0 {
				qs[kv[:j]] = urlDecode(kv[j+1:])
			}
		}
		body = body[:i]
	}
	i := strings.LastIndex(body, "@")
	if i < 0 {
		return nil
	}
	userinfo := body[:i]
	hostport := body[i+1:]
	host, port, err := splitHostPort(hostport)
	if err != nil {
		return nil
	}
	typ := qs["type"]
	if typ == "" {
		typ = "tcp"
	}
	if typ == "grpc" || typ == "kcp" || typ == "quic" || typ == "httpupgrade" {
		return nil
	}
	if qs["security"] == "reality" {
		return nil
	}
	if qs["flow"] == "xtls-rprx-vision" {
		return nil
	}
	n := &Node{Proto: proto, Host: host, Port: port, Remark: remark,
		UUID: userinfo, Net: "tcp"}
	if proto == "trojan" {
		n.Password = userinfo
		n.TLS = true
	} else {
		n.TLS = qs["security"] == "tls"
	}
	if typ == "ws" {
		n.Net = "ws"
	}
	n.Path = qs["path"]
	n.HostHeader = qs["host"]
	n.Sni = firstNonEmpty(qs["sni"], qs["host"], host)
	n.CC = ccOfRemark(remark)
	n.ID = nodeID(n)
	return n
}

func parseSS(uri string) *Node {
	body := uri[5:]
	var remark string
	if i := strings.Index(body, "#"); i >= 0 {
		remark = urlDecode(body[i+1:])
		body = body[:i]
	}
	if i := strings.Index(body, "/?plugin="); i >= 0 {
		return nil
	}
	var userinfo, hostport string
	if i := strings.LastIndex(body, "@"); i >= 0 {
		userinfo, hostport = body[:i], body[i+1:]
		if d := b64d(userinfo); len(d) > 0 && strings.Contains(string(d), ":") {
			userinfo = string(d)
		}
	} else {
		d := b64d(body)
		if len(d) == 0 {
			return nil
		}
		s := string(d)
		i := strings.LastIndex(s, "@")
		if i < 0 {
			return nil
		}
		userinfo, hostport = s[:i], s[i+1:]
	}
	host, port, err := splitHostPort(hostport)
	if err != nil {
		return nil
	}
	j := strings.Index(userinfo, ":")
	if j < 0 {
		return nil
	}
	method, pass := userinfo[:j], userinfo[j+1:]
	if !strings.Contains(method, "gcm") && !strings.Contains(method, "chacha") {
		return nil
	}
	if strings.Contains(method, "chacha") {
		return nil // windows build: AES-GCM only (no external chacha dep)
	}
	n := &Node{Proto: "ss", Host: host, Port: port, Remark: remark,
		Method: method, Password: pass, Net: "tcp"}
	n.CC = ccOfRemark(remark)
	n.ID = nodeID(n)
	return n
}

func nodeID(n *Node) string {
	h := fnv.New32a()
	h.Write([]byte(fmt.Sprintf("%s:%d:%s%s", n.Host, n.Port, n.UUID, n.Password)))
	return fmt.Sprintf("%08x%s", h.Sum32(), n.Proto[0:1])
}

func splitHostPort(hp string) (string, int, error) {
	if strings.HasPrefix(hp, "[") {
		e := strings.Index(hp, "]")
		if e < 0 || e+2 > len(hp) || hp[e+1] != ':' {
			return "", 0, errors.New("bad ipv6 hostport")
		}
		p, err := strconv.Atoi(hp[e+2:])
		return hp[1:e], p, err
	}
	i := strings.LastIndex(hp, ":")
	if i < 0 {
		return "", 0, errors.New("bad hostport")
	}
	p, err := strconv.Atoi(hp[i+1:])
	if err != nil {
		return "", 0, err
	}
	return hp[:i], p, nil
}

func firstNonEmpty(ss ...string) string {
	for _, s := range ss {
		if s != "" {
			return s
		}
	}
	return ""
}

func urlDecode(s string) string {
	s = strings.ReplaceAll(s, "+", " ")
	out := make([]byte, 0, len(s))
	for i := 0; i < len(s); i++ {
		if s[i] == '%' && i+2 < len(s) {
			if v, err := strconv.ParseUint(s[i+1:i+3], 16, 8); err == nil {
				out = append(out, byte(v))
				i += 2
				continue
			}
		}
		out = append(out, s[i])
	}
	return string(out)
}

/* ---------------- dial through nodes ---------------- */

func tlsDial(host string, port int, sni string, timeout time.Duration) (net.Conn, error) {
	plain, err := net.DialTimeout("tcp", net.JoinHostPort(host, strconv.Itoa(port)), timeout)
	if err != nil {
		return nil, err
	}
	cfg := &tls.Config{ServerName: firstNonEmpty(sni, host), MinVersion: tls.VersionTLS12}
	c := tls.Client(plain, cfg)
	c.SetDeadline(time.Now().Add(timeout))
	if err := c.Handshake(); err != nil {
		plain.Close()
		// some CDN nodes have mismatched certs -> allow insecure retry
		plain2, e2 := net.DialTimeout("tcp", net.JoinHostPort(host, strconv.Itoa(port)), timeout)
		if e2 != nil {
			return nil, err
		}
		cfg2 := &tls.Config{ServerName: firstNonEmpty(sni, host), InsecureSkipVerify: true, MinVersion: tls.VersionTLS12}
		c2 := tls.Client(plain2, cfg2)
		c2.SetDeadline(time.Now().Add(timeout))
		if err := c2.Handshake(); err != nil {
			plain2.Close()
			return nil, err
		}
		c2.SetDeadline(time.Time{})
		return c2, nil
	}
	c.SetDeadline(time.Time{})
	return c, nil
}

func tcpDial(host string, port int, timeout time.Duration) (net.Conn, error) {
	c, err := net.DialTimeout("tcp", net.JoinHostPort(host, strconv.Itoa(port)), timeout)
	if err != nil {
		return nil, err
	}
	c.SetDeadline(time.Time{})
	return c, nil
}

// wsConn: minimal WebSocket framing over an established (TLS) connection.
type wsConn struct {
	base net.Conn
	r    *bufio.Reader
}

func wsDial(base net.Conn, host string, port int, path, hostHeader string) (*wsConn, error) {
	if path == "" {
		path = "/"
	}
	hh := host
	if hostHeader != "" {
		hh = hostHeader
	} else if port != 80 {
		hh = fmt.Sprintf("%s:%d", host, port)
	}
	key := "dGhlIHNhbXBsZSBub25jZQ=="
	req := fmt.Sprintf("GET %s HTTP/1.1\r\nHost: %s\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: %s\r\nSec-WebSocket-Version: 13\r\nUser-Agent: Mozilla/5.0\r\n\r\n", path, hh, key)
	base.SetDeadline(time.Now().Add(15 * time.Second))
	if _, err := base.Write([]byte(req)); err != nil {
		base.SetDeadline(time.Time{})
		return nil, err
	}
	br := bufio.NewReader(base)
	status, err := br.ReadString('\n')
	if err != nil {
		base.SetDeadline(time.Time{})
		return nil, err
	}
	if !strings.Contains(status, "101") {
		base.SetDeadline(time.Time{})
		return nil, fmt.Errorf("ws handshake: %s", strings.TrimSpace(status))
	}
	for {
		line, err := br.ReadString('\n')
		if err != nil {
			base.SetDeadline(time.Time{})
			return nil, err
		}
		if line == "\r\n" {
			break
		}
	}
	base.SetDeadline(time.Time{})
	return &wsConn{base: base, r: br}, nil
}

func (w *wsConn) writeFrame(payload []byte, fin bool) error {
	out := make([]byte, 0, len(payload)+14)
	op := byte(0x82) // FIN+binary
	if !fin {
		op = 0x02
	}
	out = append(out, op)
	maskBit := byte(0x80)
	l := len(payload)
	if l < 126 {
		out = append(out, maskBit|byte(l))
	} else if l < 65536 {
		out = append(out, maskBit|126, byte(l>>8), byte(l))
	} else {
		out = append(out, maskBit|127)
		for i := 7; i >= 0; i-- {
			out = append(out, byte(l>>(i*8)))
		}
	}
	var mask [4]byte
	_, _ = crandRead(mask[:])
	out = append(out, mask[:]...)
	for i := 0; i < l; i++ {
		out = append(out, payload[i]^mask[i&3])
	}
	_, err := w.base.Write(out)
	return err
}

func (w *wsConn) readFrame() ([]byte, error) {
	for {
		b0, err := w.r.ReadByte()
		if err != nil {
			return nil, err
		}
		fin := b0&0x80 != 0
		op := b0 & 0x0F
		b1, err := w.r.ReadByte()
		if err != nil {
			return nil, err
		}
		masked := b1&0x80 != 0
		l := int(b1 & 0x7F)
		if l == 126 {
			var b2 [2]byte
			if _, err := io.ReadFull(w.r, b2[:]); err != nil {
				return nil, err
			}
			l = int(binary.BigEndian.Uint16(b2[:]))
		} else if l == 127 {
			var b2 [8]byte
			if _, err := io.ReadFull(w.r, b2[:]); err != nil {
				return nil, err
			}
			l = int(binary.BigEndian.Uint64(b2[:]))
		}
		var mask [4]byte
		if masked {
			if _, err := io.ReadFull(w.r, mask[:]); err != nil {
				return nil, err
			}
		}
		payload := make([]byte, l)
		if _, err := io.ReadFull(w.r, payload); err != nil {
			return nil, err
		}
		if masked {
			for i := range payload {
				payload[i] ^= mask[i&3]
			}
		}
		switch op {
		case 0x8:
			return nil, io.EOF
		case 0x9:
			w.writePong(payload)
			continue
		case 0xA:
			continue
		default:
			_ = fin
			return payload, nil
		}
	}
}

func (w *wsConn) writePong(p []byte) {
	out := []byte{0x8A}
	if len(p) < 126 {
		out = append(out, byte(len(p)))
	} else {
		out = append(out, 126, byte(len(p)>>8), byte(len(p)))
	}
	out = append(out, p...)
	w.base.Write(out)
}

/* --------- net.Conn adapter over ws frames --------- */

type wsNetConn struct {
	ws   *wsConn
	base net.Conn
	rbuf []byte
}

func (c *wsNetConn) Read(p []byte) (int, error) {
	if len(c.rbuf) == 0 {
		f, err := c.ws.readFrame()
		if err != nil {
			return 0, err
		}
		c.rbuf = f
	}
	n := copy(p, c.rbuf)
	c.rbuf = c.rbuf[n:]
	return n, nil
}

func (c *wsNetConn) Write(p []byte) (int, error) {
	if err := c.ws.writeFrame(p, true); err != nil {
		return 0, err
	}
	return len(p), nil
}

func (c *wsNetConn) Close() error                       { return c.base.Close() }
func (c *wsNetConn) LocalAddr() net.Addr                { return c.base.LocalAddr() }
func (c *wsNetConn) RemoteAddr() net.Addr               { return c.base.RemoteAddr() }
func (c *wsNetConn) SetDeadline(t time.Time) error      { return c.base.SetDeadline(t) }
func (c *wsNetConn) SetReadDeadline(t time.Time) error  { return c.base.SetReadDeadline(t) }
func (c *wsNetConn) SetWriteDeadline(t time.Time) error { return c.base.SetWriteDeadline(t) }

/* --------- protocol clients: return net.Conn to dst --------- */

func dialNode(n *Node, dstHost string, dstPort int) (net.Conn, error) {
	timeout := 12 * time.Second
	var base net.Conn
	var err error
	switch {
	case n.Net == "ws" && n.TLS:
		base, err = tlsDial(n.Host, n.Port, n.Sni, timeout)
	case n.Net == "ws":
		base, err = tcpDial(n.Host, n.Port, timeout)
	case n.TLS:
		base, err = tlsDial(n.Host, n.Port, n.Sni, timeout)
	default:
		base, err = tcpDial(n.Host, n.Port, timeout)
	}
	if err != nil {
		return nil, err
	}

	switch n.Proto {
	case "vless":
		return vlessEstablish(base, n, dstHost, dstPort)
	case "trojan":
		return trojanEstablish(base, n, dstHost, dstPort)
	case "ss":
		return ssEstablish(base, n, dstHost, dstPort)
	}
	base.Close()
	return nil, fmt.Errorf("unsupported proto %s", n.Proto)
}

// uuidBytes decodes a UUID string (with or without dashes) to 16 bytes.
func uuidBytes(s string) []byte {
	t := strings.ReplaceAll(strings.TrimSpace(s), "-", "")
	if len(t) == 32 {
		if b, err := hex.DecodeString(t); err == nil {
			return b
		}
	}
	return b64d(s)
}

func encodeAddr(host string, port int) []byte {
	out := []byte{}
	if isIPv4(host) {
		out = append(out, 1)
		for _, p := range strings.Split(host, ".") {
			v, _ := strconv.Atoi(p)
			out = append(out, byte(v))
		}
	} else {
		hb := []byte(host)
		out = append(out, 3, byte(len(hb)))
		out = append(out, hb...)
	}
	out = append(out, byte(port>>8), byte(port))
	return out
}

func isIPv4(s string) bool {
	parts := strings.Split(s, ".")
	if len(parts) != 4 {
		return false
	}
	for _, p := range parts {
		v, err := strconv.Atoi(p)
		if err != nil || v < 0 || v > 255 {
			return false
		}
	}
	return true
}

func vlessEstablish(base net.Conn, n *Node, dstHost string, dstPort int) (net.Conn, error) {
	var ws *wsConn
	isWs := n.Net == "ws"
	if isWs {
		var err error
		ws, err = wsDial(base, n.Host, n.Port, n.Path, n.HostHeader)
		if err != nil {
			base.Close()
			return nil, err
		}
	}
	var w io.Writer = base
	var r io.Reader = base
	if isWs {
		c := &wsNetConn{ws: ws, base: base}
		w = c
		r = c
	}
	uid := uuidBytes(n.UUID)
	if len(uid) != 16 {
		base.Close()
		return nil, errors.New("bad vless uuid")
	}
	req := []byte{0}
	req = append(req, uid...)
	req = append(req, 0) // addon len = 0
	req = append(req, 1) // cmd tcp
	req = append(req, encodeAddr(dstHost, dstPort)...)
	if _, err := w.Write(req); err != nil {
		base.Close()
		return nil, err
	}
	head := make([]byte, 2)
	if _, err := io.ReadFull(r, head); err != nil || head[0] != 0 {
		base.Close()
		return nil, fmt.Errorf("vless hs: %v", err)
	}
	if addon := int(head[1]); addon > 0 {
		skip := make([]byte, addon)
		if _, err := io.ReadFull(r, skip); err != nil {
			base.Close()
			return nil, err
		}
	}
	if isWs {
		return &wsNetConn{ws: ws, base: base}, nil
	}
	return base, nil
}

func trojanEstablish(base net.Conn, n *Node, dstHost string, dstPort int) (net.Conn, error) {
	sum := sha256.New224()
	sum.Write([]byte(n.Password))
	hexs := hex.EncodeToString(sum.Sum(nil))
	req := []byte(hexs + "\r\n\x01")
	req = append(req, encodeAddr(dstHost, dstPort)...)
	req = append(req, '\r', '\n')
	base.SetDeadline(time.Now().Add(15 * time.Second))
	if _, err := base.Write(req); err != nil {
		base.Close()
		return nil, err
	}
	crlf := make([]byte, 2)
	if _, err := io.ReadFull(base, crlf); err != nil || crlf[0] != '\r' || crlf[1] != '\n' {
		base.Close()
		return nil, errors.New("trojan hs failed")
	}
	base.SetDeadline(time.Time{})
	return base, nil
}

/* --------- Shadowsocks AES-GCM --------- */

func ssEstablish(base net.Conn, n *Node, dstHost string, dstPort int) (net.Conn, error) {
	klen := 32
	if strings.Contains(n.Method, "128") {
		klen = 16
	}
	if strings.Contains(n.Method, "192") {
		klen = 24
	}
	master := evpBytesToKey(n.Password, klen)
	salt := make([]byte, klen)
	if _, err := crandRead(salt); err != nil {
		base.Close()
		return nil, err
	}
	sub := hkdfSHA1(master, salt, "ss-subkey", klen)
	block, err := aes.NewCipher(sub)
	if err != nil {
		base.Close()
		return nil, err
	}
	aead, err := cipher.NewGCM(block)
	if err != nil {
		base.Close()
		return nil, err
	}
	enc := &ssAEAD{aead: aead, nonce: make([]byte, 12)}
	base.Write(salt)
	first := encodeAddr(dstHost, dstPort)
	buf := []byte{}
	buf = append(buf, enc.seal([]byte{byte(len(first) >> 8), byte(len(first))})...)
	buf = append(buf, enc.seal(first)...)
	if _, err := base.Write(buf); err != nil {
		base.Close()
		return nil, err
	}
	return &ssConn{Conn: base, aead: aead, decNonce: make([]byte, 12), key: sub, enc: enc}, nil
}

type ssAEAD struct {
	aead  cipher.AEAD
	nonce []byte
}

func (e *ssAEAD) seal(p []byte) []byte {
	out := e.aead.Seal(nil, e.nonce, p, nil)
	increment(e.nonce)
	return out
}

func increment(n []byte) {
	for i := len(n) - 1; i >= 0; i-- {
		n[i]++
		if n[i] != 0 {
			break
		}
	}
}

type ssConn struct {
	net.Conn
	aead     cipher.AEAD
	decNonce []byte
	key      []byte
	rbuf     []byte
	enc      *ssAEAD
}

func (c *ssConn) Read(p []byte) (int, error) {
	if len(c.rbuf) > 0 {
		n := copy(p, c.rbuf)
		c.rbuf = c.rbuf[n:]
		return n, nil
	}
	head := make([]byte, 18)
	if _, err := io.ReadFull(c.Conn, head); err != nil {
		return 0, err
	}
	plain, err := c.aead.Open(nil, c.decNonce, head, nil)
	if err != nil {
		return 0, err
	}
	increment(c.decNonce)
	l := int(binary.BigEndian.Uint16(plain))
	if l <= 0 || l > 0x3FFF {
		return 0, errors.New("ss bad chunk len")
	}
	body := make([]byte, l+16)
	if _, err := io.ReadFull(c.Conn, body); err != nil {
		return 0, err
	}
	data, err := c.aead.Open(nil, c.decNonce, body, nil)
	if err != nil {
		return 0, err
	}
	increment(c.decNonce)
	c.rbuf = data
	n := copy(p, c.rbuf)
	c.rbuf = c.rbuf[n:]
	return n, nil
}

func (c *ssConn) Write(p []byte) (int, error) {
	if c.enc == nil {
		return 0, errors.New("ss enc not initialized")
	}
	total := 0
	for len(p) > 0 {
		chunk := p
		if len(chunk) > 0x3F00 {
			chunk = chunk[:0x3F00]
		}
		out := c.enc.seal([]byte{byte(len(chunk) >> 8), byte(len(chunk))})
		if _, err := c.Conn.Write(out); err != nil {
			return total, err
		}
		out2 := c.enc.seal(chunk)
		if _, err := c.Conn.Write(out2); err != nil {
			return total, err
		}
		total += len(chunk)
		p = p[len(chunk):]
	}
	return total, nil
}

func hkdfSHA1(ikm, salt []byte, info string, l int) []byte {
	h := hmac.New(sha1.New, salt)
	h.Write(ikm)
	prk := h.Sum(nil)
	okm := []byte{}
	t := []byte{}
	counter := byte(1)
	for len(okm) < l {
		h = hmac.New(sha1.New, prk)
		h.Write(t)
		h.Write([]byte(info))
		h.Write([]byte{counter})
		t = h.Sum(nil)
		okm = append(okm, t...)
		counter++
	}
	return okm[:l]
}

func evpBytesToKey(pass string, klen int) []byte {
	var prev []byte
	out := []byte{}
	for len(out) < klen {
		h := md5.New()
		h.Write(prev)
		h.Write([]byte(pass))
		prev = h.Sum(nil)
		out = append(out, prev...)
	}
	return out[:klen]
}

/* testNode: full protocol handshake + real TCP DNS roundtrip through the node */
func testNode(n *Node) (bool, string) {
	c, err := dialNode(n, "1.1.1.1", 53)
	if err != nil {
		return false, err.Error()
	}
	c.SetDeadline(time.Now().Add(6 * time.Second))
	q := []byte{0x12, 0x34, 0x01, 0x00, 0, 1, 0, 0, 0, 0, 0, 0}
	for _, lbl := range strings.Split("example.com", ".") {
		q = append(q, byte(len(lbl)))
		q = append(q, []byte(lbl)...)
	}
	q = append(q, 0, 0, 1, 0, 1)
	if _, err := c.Write(q); err != nil {
		c.Close()
		return false, "write: " + err.Error()
	}
	buf := make([]byte, 512)
	c.SetReadDeadline(time.Now().Add(5 * time.Second))
	_, err = c.Read(buf)
	c.Close()
	if err != nil {
		return false, "read: " + err.Error()
	}
	return true, ""
}
