package com.mehrveil.app;

import android.annotation.SuppressLint;
import android.app.Activity;
import android.content.ClipData;
import android.content.ClipboardManager;
import android.content.Context;
import android.content.Intent;
import android.graphics.Color;
import android.net.VpnService;
import android.net.Uri;
import android.os.Build;
import android.os.Bundle;
import android.os.Handler;
import android.os.Looper;
import android.view.View;
import android.view.Window;
import android.webkit.JavascriptInterface;
import android.webkit.WebResourceRequest;
import android.webkit.WebSettings;
import android.webkit.WebView;
import android.webkit.WebViewClient;
import android.widget.Toast;

import java.util.List;

import org.json.JSONArray;
import org.json.JSONObject;

/**
 * WebView host for the web-app UI + native bridge to the real VPN core.
 * All bridge actions run off the main thread; results post back to JS callbacks.
 */
public class MainActivity extends Activity {
    private static final int REQ_VPN = 41;
    private WebView web;
    private final Handler main = new Handler(Looper.getMainLooper());

    private Node pendingNode;
    private String pendingCb;

    @SuppressLint({"SetJavaScriptEnabled", "AddJavascriptInterface"})
    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        Window w = getWindow();
        if (w != null) {
            // light liquid-glass theme: light status/nav bars with dark icons
            w.setStatusBarColor(Color.parseColor("#f0f0f5"));
            w.setNavigationBarColor(Color.parseColor("#f0f0f5"));
            if (Build.VERSION.SDK_INT >= 26) {
                View d = w.getDecorView();
                d.setSystemUiVisibility(d.getSystemUiVisibility() | View.SYSTEM_UI_FLAG_LIGHT_STATUS_BAR
                        | View.SYSTEM_UI_FLAG_LIGHT_NAVIGATION_BAR);
            }
        }

        VpnData.setSink(arr -> main.post(() -> pushServers(arr)));
        android.content.SharedPreferences pf = getSharedPreferences("veil_prefs", 0);
        VpnController.get().killSwitch = pf.getBoolean("f_kill", true);
        VpnController.get().autoConnect = pf.getBoolean("f_auto", true);
        VpnController.get().adblock = pf.getBoolean("f_adblock", false);
        VpnData.setCustomSource(pf.getString("src", null));

        web = new WebView(this);
        web.setBackgroundColor(Color.parseColor("#f0f0f5"));
        WebSettings s = web.getSettings();
        s.setJavaScriptEnabled(true);
        s.setDomStorageEnabled(true);
        s.setAllowFileAccess(true);
        s.setCacheMode(WebSettings.LOAD_NO_CACHE);
        s.setTextZoom(100);
        try { WebView.setWebContentsDebuggingEnabled(false); } catch (Throwable ignored) {}
        web.addJavascriptInterface(new Bridge(), "VeilNative");
        web.setWebViewClient(new WebViewClient() {
            @Override
            public boolean shouldOverrideUrlLoading(WebView view, WebResourceRequest request) {
                Uri u = request.getUrl();
                if ("file".equals(u.getScheme())) return false;
                try { startActivity(new Intent(Intent.ACTION_VIEW, u)); } catch (Throwable ignored) {}
                return true;
            }
        });
        setContentView(web);
        web.loadUrl("file:///android_asset/index.html");

        VpnController.get().setListener(st -> main.post(() -> pushStatus(st)));
        new Thread(() -> {
            VpnData.loadSeed(MainActivity.this);
            VpnData.startVerifyAsync(60);
        }).start();
    }

    @Override
    public void onBackPressed() { /* single-view app */ }

    /* ---------------- pushes ---------------- */

    private void pushServers(JSONArray arr) {
        if (arr == null || web == null) return;
        eval("window.__nEvent && __nEvent('servers'," + arr.toString() + ")");
    }

    private void pushStatus(JSONObject st) {
        if (st == null || web == null) return;
        eval("window.__nEvent && __nEvent('status'," + st.toString() + ")");
    }

    private void eval(final String js) {
        main.post(() -> { if (web != null) web.evaluateJavascript(js, null); });
    }

    /* ---------------- bridge ---------------- */

    private class Bridge {

        private void reply(String cb, JSONObject o) {
            final String js = "window.__nCb && __nCb(" + jq(cb) + "," + (o == null ? "{}" : o.toString()) + ")";
            eval(js);
        }

        private void fail(String cb, String msg) {
            eval("window.__nErr && __nErr(" + jq(cb) + "," + jq(msg) + ")");
        }

        private String jq(String s) {
            return s == null ? "null" : "\"" + s.replace("\\", "\\\\").replace("\"", "\\\"") + "\"";
        }

        @JavascriptInterface
        public void api(final String action, final String payloadJson, final String cbId) {
            new Thread(() -> {
                JSONObject p = null;
                try { p = new JSONObject(payloadJson == null ? "{}" : payloadJson); } catch (Throwable ignored) {}
                final JSONObject pay = p;
                try {
                    switch (action) {
                        case "fetchServers": {
                            final JSONArray arr = VpnData.fetchServers(MainActivity.this);
                            VpnData.startVerifyAsync(80);
                            reply(cbId, new JSONObject().put("servers", arr));
                            break;
                        }
                        case "pingBatch": {
                            JSONObject r = VpnData.pingBatch(pay.optJSONArray("ids"));
                            reply(cbId, new JSONObject().put("pings", r));
                            break;
                        }
                        case "stats": {
                            // REAL session stats: tunnel byte counters + live TCP ping
                            // to the connected server. No simulated numbers.
                            JSONObject o = new JSONObject();
                            boolean conn = VpnController.get().state().equals(VpnController.CONNECTED);
                            o.put("connected", conn);
                            o.put("up", TunPump.upBytes());
                            o.put("down", TunPump.downBytes());
                            if (conn) {
                                Node n = VpnController.get().currentNode();
                                if (n != null) {
                                    long ms = VpnData.tcpPing(n.host, n.port);
                                    if (ms > 0) o.put("ping", ms);
                                }
                            }
                            reply(cbId, o);
                            break;
                        }
                        case "connect": {
                            handleConnect(pay, cbId);
                            break;
                        }
                        case "disconnect": {
                            VpnController.get().setState(VpnController.DISCONNECTED);
                            Intent i = new Intent(MainActivity.this, VeilVpnService.class)
                                    .setAction(VeilVpnService.ACTION_DISCONNECT);
                            startService(i);
                            reply(cbId, new JSONObject().put("ok", true));
                            break;
                        }
                        case "getState": {
                            JSONObject o = new JSONObject();
                            o.put("status", VpnController.get().state());
                            o.put("platform", "android");
                            String ver;
            try { ver = getPackageManager().getPackageInfo(getPackageName(), 0).versionName; }
            catch (Throwable t) { ver = "1.6.7"; }
            o.put("version", ver);
                            o.put("features", new JSONObject()
                                    .put("kill", VpnController.get().killSwitch)
                                    .put("auto", VpnController.get().autoConnect)
                                    .put("adblock", VpnController.get().adblock));
                            String savedLang = getSharedPreferences("veil_prefs", 0).getString("lang", null);
                            if (savedLang != null) o.put("lang", savedLang);
                            Node n = VpnController.get().currentNode();
                            if (n != null && VpnController.get().state().equals(VpnController.CONNECTED)) {
                                o.put("server", new JSONObject().put("id", n.id)
                                        .put("cc", n.cc == null ? "XX" : n.cc).put("name", n.name()));
                            }
                            reply(cbId, o);
                            break;
                        }
                        case "setLang": {
                            getSharedPreferences("veil_prefs", 0).edit()
                                    .putString("lang", pay.optString("lang")).apply();
                            reply(cbId, new JSONObject().put("ok", true));
                            break;
                        }
                        case "setSource": {
                            String src = pay.optString("url");
                            VpnData.setCustomSource(src);
                            getSharedPreferences("veil_prefs", 0).edit().putString("src", src).apply();
                            reply(cbId, new JSONObject().put("ok", true));
                            break;
                        }
                        case "setFeature": {
                            String k = pay.optString("key");
                            boolean on = pay.optBoolean("on");
                            if ("kill".equals(k)) VpnController.get().killSwitch = on;
                            if ("auto".equals(k)) VpnController.get().autoConnect = on;
                            if ("adblock".equals(k)) VpnController.get().adblock = on;
                            getSharedPreferences("veil_prefs", 0).edit()
                                    .putBoolean("f_" + k, on).apply();
                            reply(cbId, new JSONObject().put("ok", true));
                            break;
                        }
                        case "getIpInfo": {
                            // HTTPS-only: never send user IP lookup over cleartext
                            JSONObject o = new JSONObject();
                            String body = VpnData.httpGet("https://api.ip.sb/geoip", 15_000, 65536);
                            if (body == null) body = VpnData.httpGet("https://ipwho.is/", 15_000, 65536);
                            if (body != null) {
                                try {
                                    JSONObject j = new JSONObject(body);
                                    o.put("ip", j.optString("query", j.optString("ip", "")));
                                    String cc = j.optString("countryCode",
                                            j.optString("country_code", j.optString("cc", "")));
                                    o.put("cc", cc);
                                    o.put("city", j.optString("city", ""));
                                } catch (Throwable ignored) {}
                            }
                            reply(cbId, o);
                            break;
                        }
                        case "copyText": {
                            ClipboardManager cm = (ClipboardManager) getSystemService(Context.CLIPBOARD_SERVICE);
                            cm.setPrimaryClip(ClipData.newPlainText("veil", pay.optString("text")));
                            reply(cbId, new JSONObject().put("ok", true));
                            break;
                        }
                        case "openUrl": {
                            try {
                                startActivity(new Intent(Intent.ACTION_VIEW, Uri.parse(pay.optString("url"))));
                            } catch (Throwable ignored) {}
                            reply(cbId, new JSONObject().put("ok", true));
                            break;
                        }
                        default:
                            fail(cbId, "unknown action " + action);
                    }
                } catch (final Throwable t) {
                    fail(cbId, t.getMessage() == null ? "error" : t.getMessage());
                }
            }, "bridge-" + action).start();
        }

        private void handleConnect(JSONObject pay, String cbId) {
            String id = pay.optString("id");
            Node node = id == null || id.isEmpty() ? null : VpnData.fullById(id);
            if (node == null && VpnData.NODES.size() > 0) node = VpnData.NODES.get(0);
            if (node == null) {
                try { reply(cbId, new JSONObject().put("ok", false).put("error", "no servers")); }
                catch (Throwable ignored) {}
                return;
            }
            // serialize sessions: tear down any previous TUN before starting a new one
            // (double-establish left two pumps running and broke the tunnel)
            String cur = VpnController.get().state();
            if (cur.equals(VpnController.CONNECTED) || cur.equals(VpnController.CONNECTING)
                    || cur.equals(VpnController.RECONNECTING)) {
                Intent stop = new Intent(MainActivity.this, VeilVpnService.class)
                        .setAction(VeilVpnService.ACTION_DISCONNECT);
                try { startService(stop); } catch (Throwable ignored) {}
                try { Thread.sleep(600); } catch (Throwable ignored) {}
            }
            pendingNode = node;
            pendingCb = cbId;
            final boolean[] needPerm = {false};
            final Intent[] prepare = {null};
            try {
                prepare[0] = VpnService.prepare(MainActivity.this);
                needPerm[0] = prepare[0] != null;
            } catch (Throwable t) { /* fallthrough */ }
            if (needPerm[0]) {
                // VpnService.prepare dialog MUST be launched from the main thread
                main.post(() -> {
                    try { startActivityForResult(prepare[0], REQ_VPN); }
                    catch (Throwable t) { startCoreConnect(cbId); }
                });
                return;                      // reply sent in onActivityResult
            }
            startCoreConnect(cbId);
        }
    }

    private void startCoreConnect(String cbId) {
        Node node = pendingNode;
        pendingNode = null;
        if (node == null) return;
        List<Node> cands = VpnController.get().buildCandidates(node);
        VeilVpnService.QUEUE = cands;
        Intent i = new Intent(this, VeilVpnService.class).setAction(VeilVpnService.ACTION_CONNECT);
        if (Build.VERSION.SDK_INT >= 26) startForegroundService(i);
        else startService(i);
        try {
            JSONObject sv = new JSONObject();
            sv.put("id", node.id);
            sv.put("cc", node.cc == null ? "XX" : node.cc);
            sv.put("name", node.name());
            new Bridge().reply(cbId, new JSONObject().put("ok", true).put("async", true).put("server", sv));
        } catch (Throwable ignored) {}
    }

    @Override
    protected void onActivityResult(int requestCode, int resultCode, Intent data) {
        super.onActivityResult(requestCode, resultCode, data);
        if (requestCode == REQ_VPN) {
            if (resultCode == RESULT_OK) {
                startCoreConnect(pendingCb);
            } else {
                pendingNode = null;
                pendingCb = null;
                VpnController.get().setState(VpnController.DISCONNECTED, "vpn permission denied");
                try { Toast.makeText(this, "VPN permission denied", Toast.LENGTH_SHORT).show(); } catch (Throwable ignored) {}
            }
        }
    }

    @Override
    protected void onDestroy() {
        if (web != null) { web.destroy(); web = null; }
        super.onDestroy();
    }
}
