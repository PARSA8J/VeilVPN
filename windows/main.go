// VeilVPN 1.5.1 for Windows — single-file EXE.
// Serves the glassmorphism web UI on 127.0.0.1, runs a real proxy core
// (VLESS / Trojan / Shadowsocks-AEAD), and toggles the Windows system proxy.
package main

import (
	_ "embed"
	"fmt"
	"io"
	"log"
	"net"
	"net/http"
	"os"
	"path/filepath"
	"time"
)

//go:embed web/index.html
var indexHTML []byte

//go:embed web/app.js
var appJS []byte

//go:embed web/glass.css
var glassCSS []byte

//go:embed web/i18n.js
var i18nJS []byte

//go:embed web/seed.txt
var seedTXT []byte

const version = "1.5.1"
const botURL = "https://t.me/veilvpnverifybot"

var uiPort = 28443

func main() {
	logDir := stateDir()
	os.MkdirAll(logDir, 0o755)
	lf, err := os.OpenFile(filepath.Join(logDir, "veil.log"), os.O_CREATE|os.O_WRONLY|os.O_APPEND, 0o644)
	if err == nil {
		log.SetOutput(lf)
	} else {
		log.SetOutput(io.Discard)
	}
	log.Printf("==== VeilVPN %s start ====", version)

	// single instance: if our UI port is already served, just open browser
	if portBusy(fmt.Sprintf("127.0.0.1:%d", uiPort)) {
		log.Printf("port busy -> existing instance, open browser")
		openBrowser(fmt.Sprintf("http://127.0.0.1:%d", uiPort))
		return
	}

	licenseInit()
	go loadSeedAndRefresh()

	mux := http.NewServeMux()
	mux.HandleFunc("/", func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/" && r.URL.Path != "/index.html" {
			http.NotFound(w, r)
			return
		}
		w.Header().Set("Content-Type", "text/html; charset=utf-8")
		w.Write(indexHTML)
	})
	mux.HandleFunc("/app.js", func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "text/javascript; charset=utf-8")
		w.Write(appJS)
	})
	mux.HandleFunc("/glass.css", func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "text/css; charset=utf-8")
		w.Write(glassCSS)
	})
	mux.HandleFunc("/i18n.js", func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "text/javascript; charset=utf-8")
		w.Write(i18nJS)
	})
	mux.HandleFunc("/api/", apiDispatch)
	mux.HandleFunc("/api/events", apiEvents)

	addr := fmt.Sprintf("127.0.0.1:%d", uiPort)
	go func() {
		time.Sleep(700 * time.Millisecond)
		openBrowser("http://" + addr)
	}()
	log.Printf("UI on %s", addr)
	if err := http.ListenAndServe(addr, mux); err != nil {
		log.Printf("listen: %v", err)
	}
}

func portBusy(addr string) bool {
	ln, err := net.Listen("tcp", addr)
	if err != nil {
		return true
	}
	ln.Close()
	return false
}
