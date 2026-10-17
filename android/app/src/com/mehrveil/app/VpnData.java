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
    private static final ExecutorService POOL = Executors.newFixedThreadPool(6);
    public interface Sink { void onServers(JSONArray arr); }
    private static volatile Sink sink;
    private static volatile String customSource;

    public static void setSink(Sink s) { sink = s; }
    public static void setCustomSource(String u) { customSource = (u == null || u.trim().isEmpty()) ? null : u.trim(); }

    private static final String[][] SOURCES = {
        {"https://raw.githubusercontent.com/Epodonios/v2ray-configs/main/All_Configs_Sub.txt",
         "https://cdn.jsdelivr.net/gh/Epodonios/v2ray-configs@main/All_Configs_Sub.txt"},
        {"https://raw.githubusercontent.com/mahdibland/V2RayAggregator/master/sub/sub_merge.txt",
         "https://cdn.jsdelivr.net/gh/mahdibland/V2RayAggregator@master/sub/sub_merge.txt"},
        {"https://raw.githubusercontent.com/barry-far/V2ray-Configs/main/All_Configs_Sub.txt",
         "https://cdn.jsdelivr.net/gh/barry-far/V2ray-Configs@main/All_Configs_Sub.txt"},
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
        for (String[] src : SOURCES) { urls.add(src[0]); urls.add(src[1]); }
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
        List<Thread> ts = new ArrayList<>();
        for (Node n : targets) {
            Thread t = new Thread(() -> {
                long ms = tcpPing(n.host, n.port);
                if (ms > 0) PINGS.put(n.id, ms);
            });
            t.start(); ts.add(t);
        }
        for (Thread t : ts) { try { t.join(4000); } catch (Throwable ignored) {} }
        for (Node n : targets) {
            Long p = PINGS.get(n.id);
            try { if (p != null) out.put(n.id, p); } catch (Throwable ignored) {}
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

    public static String httpGet(String url, int timeoutMs, int limit) {
        try {
            HttpURLConnection con = (HttpURLConnection) new URL(url).openConnection();
            con.setConnectTimeout(timeoutMs);
            con.setReadTimeout(timeoutMs);
            con.setRequestProperty("User-Agent", "Mozilla/5.0 (Linux; Android 13) Mehrveil/1.6.3");
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
