package com.mehrveil.app;

import com.mehrveil.app.proto.Client;

import java.io.InputStream;
import java.io.OutputStream;
import java.util.ArrayList;
import java.util.List;

/** VPN state machine singleton. Listener receives status JSON pushed to the WebView. */
public final class VpnController {
    public static final String DISCONNECTED = "disconnected", CONNECTING = "connecting",
            CONNECTED = "connected", RECONNECTING = "reconnecting";

    public interface Listener { void onStatus(org.json.JSONObject st); }

    private static final VpnController I = new VpnController();
    public static VpnController get() { return I; }

    private volatile Listener listener;
    private volatile String state = DISCONNECTED;
    private volatile Node current;
    private volatile String error;
    public volatile boolean killSwitch = true, autoConnect = true, adblock = false;

    public static VpnController instance() { return get(); }

    public void setListener(Listener l) { listener = l; }

    public String state() { return state; }
    public Node currentNode() { return current; }
    public String lastError() { return error; }

    public void setState(String s) {
        state = s;
        push();
    }
    public void setState(String s, String err) {
        error = err;
        state = s;
        push();
    }
    public void setCurrentNode(Node n) {
        current = n;
        push();
    }

    public void push() {
        Listener l = listener;
        if (l == null) return;
        org.json.JSONObject o = new org.json.JSONObject();
        try {
            o.put("state", state);
            if (error != null && !state.equals(CONNECTED)) o.put("error", error);
            if (current != null) {
                org.json.JSONObject sv = new org.json.JSONObject();
                sv.put("id", current.id);
                sv.put("cc", current.cc == null ? "XX" : current.cc);
                sv.put("name", current.name());
                o.put("server", sv);
            }
        } catch (Throwable ignored) {}
        try { l.onStatus(o); } catch (Throwable ignored) {}
    }

    /* ---------------- dial dispatch ---------------- */

    /** Open a channel to dst through the current node (used by TcpFlow + DNS). */
    public ClientPair dial(TcpFlow.Dst dst) throws Exception {
        Node n = current;
        if (n == null) throw new IllegalStateException("no node");
        Client c = clientFor(n);
        Client.Channel ch = c.dial(n, dst.host, dst.port);
        return new ClientPair(ch.in, ch.out);
    }

    /** TCP DNS resolve host via 1.1.1.1 through the tunnel. */
    public ClientPair dialDns(String host) throws Exception {
        Node n = current;
        if (n == null) throw new IllegalStateException("no node");
        Client c = clientFor(n);
        Client.Channel ch = c.dial(n, "1.1.1.1", 53);
        return new ClientPair(ch.in, ch.out);
    }

    public static class ClientPair {
        public final InputStream in;
        public final OutputStream out;
        public ClientPair(InputStream i, OutputStream o) { in = i; out = o; }
    }

    public static Client clientFor(Node n) throws Exception {
        switch (n.proto) {
            case "vless": return new com.mehrveil.app.proto.Vless(n);
            case "trojan": return new com.mehrveil.app.proto.Trojan(n);
            case "ss": return new com.mehrveil.app.proto.SsAead(n);
            default: throw new Exception("unsupported proto: " + n.proto);
        }
    }

    /** Build failover candidates: chosen node first, then same-country, then best-ping others. */
    public List<Node> buildCandidates(Node chosen) {
        List<Node> out = new ArrayList<>();
        if (chosen != null) out.add(chosen);
        List<Node> pool;
        synchronized (VpnData.NODES) { pool = new ArrayList<>(VpnData.NODES); }
        if (current != null && chosen != null) {
            for (Node n : pool)
                if (n.cc != null && n.cc.equals(chosen.cc) && !n.id.equals(chosen.id) && out.size() < 4) out.add(n);
        }
        pool.sort((a, b) -> {
            Boolean va2 = VpnData.VERIFIED.get(a.id), vb2 = VpnData.VERIFIED.get(b.id);
            int av = Boolean.TRUE.equals(va2) ? 0 : 1, bv = Boolean.TRUE.equals(vb2) ? 0 : 1;
            if (av != bv) return Integer.compare(av, bv);
            Long pa = a.ping, pb = b.ping;
            long va = pa == null ? 9999 : pa, vb = pb == null ? 9999 : pb;
            return Long.compare(va, vb);
        });
        for (Node n : pool) {
            if (out.size() >= 6) break;
            boolean dup = false;
            for (Node x : out) if (x.id.equals(n.id)) { dup = true; break; }
            if (!dup) out.add(n);
        }
        return out;
    }
}
