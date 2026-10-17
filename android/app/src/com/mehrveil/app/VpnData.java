package com.mehrveil.app;

import android.content.Context;

import org.json.JSONArray;
import org.json.JSONObject;

import java.io.ByteArrayOutputStream;
import java.io.InputStream;
import java.net.HttpURLConnection;
import java.net.URL;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;

/**
 * Server data: seed snapshot baked in assets (list NEVER empty on first open),
 * then live refresh from public aggregators with mirror fallbacks (direct -> jsdelivr).
 */
public final class VpnData {
    public static final List<Node> NODES = new ArrayList<>();
    public static final Map<String, Long> PINGS = new ConcurrentHashMap<>();
    /** Real end-to-end verification results: id -> alive (full protocol handshake + DNS through tunnel). */
    public static final Map<String, Boolean> VERIFIED = new ConcurrentHashMap<>();
    private static volatile boolean verifying = false;
    private static final ExecutorService POOL = Executors.newFixedThreadPool(6);
    public interface Sink { void onServers(JSONArray arr); }
    private static volatile Sink sink;
    private static volatile String customSource;

    public static void setSink(Sink s) { sink = s; }
    public static void setCustomSource(String u) { customSource = (u == null || u.trim().isEmpty()) ? null : u.trim(); }

    /** Fetch sources — ordered by reachability from Iran:
     *  1) our own GitHub Pages subscription (verified pool, same domain the APK ships from)
     *  2) gh-proxy mirrors of the big aggregators (raw.githubusercontent.com is blocked in Iran)
     *  3) jsdelivr CDN mirrors
     *  4) raw.githubusercontent last resort (works outside Iran) */
    private static final String[] SOURCES = {
        "https://parsa8j.github.io/VeilVPN/sub.txt",
        "https://ghproxy.net/https://raw.githubusercontent.com/Epodonios/v2ray-configs/main/All_Configs_Sub.txt",
        "https://gh-proxy.com/https://raw.githubusercontent.com/Epodonios/v2ray-configs/main/All_Configs_Sub.txt",
        "https://cdn.jsdelivr.net/gh/Epodonios/v2ray-configs@main/All_Configs_Sub.txt",
        "https://cdn.jsdelivr.net/gh/mahdibland/V2RayAggregator@master/sub/sub_merge.txt",
        "https://ghproxy.net/https://raw.githubusercontent.com/barry-far/V2ray-Configs/main/All_Configs_Sub.txt",
        "https://raw.githubusercontent.com/Epodonios/v2ray-configs/main/All_Configs_Sub.txt",
        "https://raw.githubusercontent.com/mahdibland/V2RayAggregator/master/sub/sub_merge.txt",
    };

    /** Seed nodes, instantly available. */
    public static void loadSeed(Context c) {
        synchronized (NODES) {
            if (!NODES.isEmpty()) return;
            try {
                InputStream in = c.getAssets().open("seed.txt");
                ByteArrayOutputStream bos = new ByteArrayOutputStream();
                byte[] buf = new byte[16384];
                int n;
                while ((n = in.read(buf)) > 0) bos.write(buf, 0, n);
                in.close();
                for (String line : new String(bos.toByteArray(), "UTF-8").split("\n")) {
                    line = line.trim();
                    if (line.isEmpty() || line.startsWith("vmess://")) continue;   // vmess unsupported in 1.5.1
                    Node nd = Node.parse(line);
                    if (nd != null) NODES.add(nd);
                }
            } catch (Throwable ignored) {}
        }
        push();
    }

    private static JSONArray snapshot() {
        JSONArray arr = new JSONArray();
        synchronized (NODES) {
            for (Node n : NODES) {
                JSONObject o = n.toJson();
                Long p = PINGS.get(n.id);
                try { if (p != null) o.put("ping", p); } catch (Throwable ignored) {}
                Boolean v = VERIFIED.get(n.id);
                try { if (v != null && v) o.put("ok", true); else if (v != null) o.put("ok", false); } catch (Throwable ignored) {}
                arr.put(o);
            }
        }
        return arr;
    }

    public static void push() {
        Sink s = sink;
        if (s != null) { try { s.onServers(snapshot()); } catch (Throwable ignored) {} }
    }

    /** Live refresh: fetch sources (mirrors), parse, merge, push. */
    public static JSONArray fetchServers(Context c) {
        loadSeed(c);
        final Map<String, String> found = new LinkedHashMap<>();
        List<String> urls = new ArrayList<>();
        if (customSource != null) urls.add(customSource);
        for (String src : SOURCES) urls.add(src);
        List<Thread> ts = new ArrayList<>();
        for (final String u : urls) {
            Thread t = new Thread(() -> {
                String txt = httpGet(u, 12_000, 2_000_000);
                if (txt == null) return;
                for (String line : decodeSub(txt).split("\n")) {
                    line = line.trim();
                    if (line.length() < 8 || line.startsWith("vmess://")) continue;
                    Node nd = Node.parse(line);
                    if (nd == null) continue;
                    synchronized (found) { found.put(nd.host + ":" + nd.port + ":" + nd.proto, line); }
                }
            });
            t.start();
            ts.add(t);
            if (ts.size() >= 3) { for (Thread x : ts) try { x.join(1500); } catch (Throwable ignored) {} ts.clear(); }
        }
        for (Thread x : ts) { try { x.join(25000); } catch (Throwable ignored) {} }
        synchronized (NODES) {
            NODES.clear();
            for (String uri : found.values()) {
                Node nd = Node.parse(uri);
                if (nd != null && NODES.size() < 600) NODES.add(nd);
            }
            if (NODES.isEmpty()) { loadSeed(c); }
        }
        push();
        return snapshot();
    }

    private static String decodeSub(String txt) {
        txt = txt.trim();
        if (txt.contains("://")) return txt;
        String dec = new String(U.b64decode(txt));
        return dec.contains("://") ? dec : txt;
    }

    public static Node fullById(String id) {
        synchronized (NODES) { for (Node n : NODES) if (id.equals(n.id)) return n; }
        return null;
    }

    public static JSONObject pingBatch(JSONArray ids) {
        JSONObject out = new JSONObject();
        List<Node> targets = new ArrayList<>();
        synchronized (NODES) {
            for (int i = 0; i < ids.length(); i++) {
                String id = ids.optString(i, null);
                if (id == null) continue;
                Node n = fullById(id);
                if (n != null && !PINGS.containsKey(id)) targets.add(n);
            }
        }
        // fixed-size worker pool — never one thread per node (thread-storm OOM on low-RAM phones)
        final ExecutorService ex = Executors.newFixedThreadPool(Math.min(24, Math.max(1, targets.size())));
        final Map<String, Long> got = new ConcurrentHashMap<>();
        for (final Node n : targets) {
            ex.execute(() -> {
                long ms = tcpPing(n.host, n.port);
                if (ms > 0) { PINGS.put(n.id, ms); got.put(n.id, ms); }
            });
        }
        ex.shutdown();
        try { ex.awaitTermination(4, java.util.concurrent.TimeUnit.SECONDS); } catch (Throwable ignored) {}
        ex.shutdownNow();
        for (String id : got.keySet()) {
            Long p = got.get(id);
            try { if (p != null) out.put(id, p); } catch (Throwable ignored) {}
        }
        return out;
    }

    public static long tcpPing(String host, int port) {
        long t0 = System.currentTimeMillis();
        try {
            java.net.Socket s = new java.net.Socket();
            s.connect(new java.net.InetSocketAddress(host, port), 3500);
            s.close();
            return System.currentTimeMillis() - t0;
        } catch (Throwable t) { return -1; }
    }

    /* ---------- real end-to-end verification ---------- */

    /**
     * Full protocol handshake + HTTP probe through the tunnel (same code path as connecting).
     * Probe = cp.cloudflare.com/generate_204 on port 80: most proxy operators BLOCK port-53
     * relaying (anti-abuse), so DNS-based checks fail on perfectly healthy nodes.
     * cp.cloudflare.com is Cloudflare's connectivity check — reachable from Iran too.
     */
    public static boolean verifyNode(Node n) {
        try {
            com.mehrveil.app.proto.Client c;
            switch (n.proto) {
                case "vless": c = new com.mehrveil.app.proto.Vless(n); break;
                case "trojan": c = new com.mehrveil.app.proto.Trojan(n); break;
                case "ss": c = new com.mehrveil.app.proto.SsAead(n); break;
                default: return false;
            }
            com.mehrveil.app.proto.Client.Channel ch = c.dial(n, "cp.cloudflare.com", 80);
            byte[] req = ("GET /generate_204 HTTP/1.1\r\nHost: cp.cloudflare.com\r\n"
                    + "User-Agent: Mozilla/5.0\r\nConnection: close\r\n\r\n").getBytes("UTF-8");
            ch.out.write(req);
            ch.out.flush();
            byte[] buf = new byte[256];
            int total = 0, r;
            long deadline = System.currentTimeMillis() + 8000;
            while (total < 12 && System.currentTimeMillis() < deadline) {
                try { r = ch.in.read(buf, total, buf.length - total); }
                catch (Throwable t) { break; }
                if (r <= 0) break;
                total += r;
            }
            try { ch.in.close(); } catch (Throwable ignored) {}
            try { ch.out.close(); } catch (Throwable ignored) {}
            String head = new String(buf, 0, total, "UTF-8");
            return head.startsWith("HTTP/1") &&
                    (head.contains(" 204") || head.contains(" 200") || head.contains(" 301") || head.contains(" 302"));
        } catch (Throwable t) {
            return false;
        }
    }

    /** Verify up to `limit` not-yet-verified nodes (fastest-pinged first), 8 at a time. */
    public static void startVerifyAsync(final int limit) {
        if (verifying) return;
        verifying = true;
        new Thread(() -> {
            try {
                List<Node> targets;
                synchronized (NODES) { targets = new ArrayList<>(NODES); }
                targets.sort((a, b) -> {
                    Long pa = PINGS.get(a.id), pb = PINGS.get(b.id);
                    long va = pa == null ? 8888 : pa, vb = pb == null ? 8888 : pb;
                    return Long.compare(va, vb);
                });
                int picked = 0;
                List<Thread> ts = new ArrayList<>();
                for (Node n : targets) {
                    if (picked >= limit) break;
                    if (VERIFIED.containsKey(n.id)) continue;
                    picked++;
                    Thread t = new Thread(() -> {
                        try { VERIFIED.put(n.id, verifyNode(n)); } catch (Throwable ignored) {}
                        push();
                    }, "verify");
                    t.start();
                    ts.add(t);
                    if (ts.size() >= 8) {
                        for (Thread x : ts) try { x.join(25_000); } catch (Throwable ignored) {}
                        ts.clear();
                    }
                }
                for (Thread x : ts) try { x.join(25_000); } catch (Throwable ignored) {}
            } catch (Throwable ignored) {
                // never let the verifier kill the process
            } finally {
                verifying = false;
                push();
            }
        }, "verify-batch").start();
    }

    public static String httpGet(String url, int timeoutMs, int limit) {
        try {
            HttpURLConnection con = (HttpURLConnection) new URL(url).openConnection();
            con.setConnectTimeout(timeoutMs);
            con.setReadTimeout(timeoutMs);
            con.setRequestProperty("User-Agent", "Mozilla/5.0 (Linux; Android 13) Mehrveil/1.6.6");
            int code = con.getResponseCode();
            if (code != 200) { con.disconnect(); return null; }
            InputStream in = con.getInputStream();
            ByteArrayOutputStream bos = new ByteArrayOutputStream();
            byte[] buf = new byte[16384];
            int n, total = 0;
            while ((n = in.read(buf)) > 0) {
                bos.write(buf, 0, n);
                total += n;
                if (total > limit) break;
            }
            in.close();
            con.disconnect();
            return new String(bos.toByteArray(), "UTF-8");
        } catch (Throwable t) { return null; }
    }
}
