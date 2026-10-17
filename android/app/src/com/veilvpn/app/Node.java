package com.veilvpn.app;

import org.json.JSONObject;

/** Proxy node model + URI parser (vless:// trojan:// ss:// vmess://). */
public class Node {
    public String id, uri, proto, host, cc, remark;
    public int port;
    public String uuid, password, method;
    public String net = "tcp";                  // tcp | ws
    public boolean tls;
    public String sni, path, hostHeader, flow;
    public Long ping;

    public String name() {
        if (remark != null && remark.length() > 1 && !remark.matches("[0-9a-zA-Z.:-]{1,20}")) {
            String r = remark.replaceAll("[\\p{So}\\p{Cn}]", "").trim();
            if (r.length() > 1) return r.length() > 26 ? r.substring(0, 26) : r;
        }
        return host;
    }

    public static String flagOf(String cc) {
        if (cc == null || cc.length() != 2) return "\uD83D\uDEE1";
        int a = Character.toUpperCase(cc.charAt(0)) - 'A' + 0x1F1E6;
        int b = Character.toUpperCase(cc.charAt(1)) - 'A' + 0x1F1E6;
        if (a < 0x1F1E6 || a > 0x1F1FF || b < 0x1F1E6 || b > 0x1F1FF) return "\uD83D\uDEE1";
        return new String(Character.toChars(a)) + new String(Character.toChars(b));
    }

    private static final java.util.regex.Pattern CC_PAT = java.util.regex.Pattern.compile("[\\uD83C\\uDDE6-\\uD83C\\uDDFF]{2}");

    public static String ccOf(String remark) {
        if (remark == null) return null;
        java.util.regex.Matcher m = CC_PAT.matcher(remark);
        if (m.find()) {
            StringBuilder sb = new StringBuilder();
            for (int i = 0; i < 2; i++) sb.append((char) (m.group().codePointAt(i * 2) - 0x1F1E6 + 'A'));
            return sb.toString();
        }
        m = java.util.regex.Pattern.compile("(?<![A-Za-z])([A-Z]{2})(?![A-Za-z])").matcher(remark.toUpperCase());
        if (m.find()) return m.group(1);
        return null;
    }

    public static Node parse(String uri) {
        try {
            uri = uri.trim();
            if (uri.startsWith("vless://") || uri.startsWith("trojan://"))
                return parseVlessTrojan(uri);
            if (uri.startsWith("ss://")) return parseSs(uri);
            if (uri.startsWith("vmess://")) return parseVmess(uri);
        } catch (Throwable ignored) {}
        return null;
    }

    private static java.util.Map<String, String> query(String q) {
        java.util.Map<String, String> m = new java.util.HashMap<>();
        if (q == null) return m;
        for (String p : q.split("&")) {
            int i = p.indexOf('=');
            if (i > 0) m.put(p.substring(0, i), urlDecode(p.substring(i + 1)));
        }
        return m;
    }

    private static String urlDecode(String s) {
        try { return java.net.URLDecoder.decode(s, "UTF-8"); } catch (Throwable t) { return s; }
    }

    private static Node parseVlessTrojan(String uri) {
        String proto = uri.startsWith("vless") ? "vless" : "trojan";
        String body = uri.substring(proto.length() + 3);
        int hash = body.indexOf('#');
        String remark = hash >= 0 ? urlDecode(body.substring(hash + 1)) : "";
        body = body.substring(0, hash < 0 ? body.length() : hash);
        int q = body.indexOf('?');
        java.util.Map<String, String> qs = query(q >= 0 ? body.substring(q + 1) : null);
        if (q >= 0) body = body.substring(0, q);
        int at = body.lastIndexOf('@');
        if (at < 0) return null;
        String userinfo = body.substring(0, at);
        String hostport = body.substring(at + 1);
        String host; int port;
        if (hostport.startsWith("[")) {
            host = hostport.substring(1, hostport.indexOf(']'));
            port = Integer.parseInt(hostport.substring(hostport.indexOf("]:") + 2));
        } else {
            int c = hostport.lastIndexOf(':');
            if (c < 0) return null;
            host = hostport.substring(0, c);
            port = Integer.parseInt(hostport.substring(c + 1));
        }
        Node n = new Node();
        n.proto = proto; n.host = host; n.port = port; n.remark = remark;
        n.uuid = userinfo;
        n.password = proto.equals("trojan") ? userinfo : null;
        String type = qs.containsKey("type") ? qs.get("type") : "tcp";
        if ("grpc".equals(type) || "kcp".equals(type) || "quic".equals(type) || "httpupgrade".equals(type)) return null;
        n.net = "ws".equals(type) ? "ws" : "tcp";
        n.path = qs.get("path");
        n.hostHeader = qs.get("host");
        String security = qs.get("security");
        n.tls = "tls".equals(security) || proto.equals("trojan");
        n.sni = qs.get("sni");
        if (n.sni == null) n.sni = qs.get("host");
        if (n.sni == null) n.sni = host;
        n.flow = qs.get("flow");
        if ("xtls-rprx-vision".equals(n.flow)) return null;
        if ("reality".equals(security)) return null;
        n.cc = ccOf(remark);
        n.id = Integer.toHexString((host + ":" + port + n.uuid).hashCode()) + proto.charAt(0);
        return n;
    }

    private static Node parseSs(String uri) {
        String body = uri.substring(5);
        int hash = body.indexOf('#');
        String remark = hash >= 0 ? urlDecode(body.substring(hash + 1)) : "";
        body = body.substring(0, hash < 0 ? body.length() : hash);
        int plugin = body.indexOf("/?plugin=");
        if (plugin >= 0) return null;
        String userinfo, hostport;
        if (body.contains("@")) {
            int at = body.lastIndexOf('@');
            userinfo = body.substring(0, at);
            hostport = body.substring(at + 1);
            if (userinfo.matches("[A-Za-z0-9+/=_-]+")) {
                byte[] d = U.b64decode(userinfo);
                if (d.length > 0 && new String(d).contains(":")) userinfo = new String(d);
            }
            userinfo = urlDecode(userinfo);
        } else {
            byte[] d = U.b64decode(body);
            if (d.length == 0) return null;
            String dec = new String(d);
            int at = dec.lastIndexOf('@');
            if (at < 0) return null;
            userinfo = dec.substring(0, at);
            hostport = dec.substring(at + 1);
        }
        int c = hostport.lastIndexOf(':');
        if (c < 0) return null;
        String host = hostport.substring(0, c).replace("[", "").replace("]", "");
        int port = Integer.parseInt(hostport.substring(c + 1).split("/")[0]);
        int cm = userinfo.indexOf(':');
        if (cm < 0) return null;
        String method = userinfo.substring(0, cm), pass = userinfo.substring(cm + 1);
        if (!method.contains("gcm") && !method.contains("chacha")) return null;
        Node n = new Node();
        n.proto = "ss"; n.host = host; n.port = port; n.remark = remark;
        n.method = method; n.password = pass; n.net = "tcp"; n.tls = false;
        n.cc = ccOf(remark);
        n.id = Integer.toHexString((host + ":" + port + pass).hashCode()) + "s";
        return n;
    }

    private static Node parseVmess(String uri) {
        byte[] d = U.b64decode(uri.substring(8));
        if (d.length == 0) return null;
        JSONObject j;
        try { j = new JSONObject(new String(d)); } catch (Throwable t) { return null; }
        String net = j.optString("net", "tcp");
        if ("grpc".equals(net) || "kcp".equals(net) || "quic".equals(net)) return null;
        if (j.optString("tls", "").equals("reality")) return null;
        Node n = new Node();
        n.proto = "vmess";
        n.host = j.optString("add");
        n.port = Integer.parseInt(j.optString("port", "443").trim());
        n.remark = j.optString("ps", "");
        n.uuid = j.optString("id");
        n.net = "ws".equals(net) ? "ws" : "tcp";
        n.path = j.optString("path", null);
        n.hostHeader = j.optString("host", null);
        n.tls = "tls".equals(j.optString("tls", ""));
        n.sni = j.optString("sni", n.hostHeader != null ? n.hostHeader : n.host);
        if (n.path == null || n.path.isEmpty()) n.path = "/";
        n.cc = ccOf(n.remark);
        n.id = Integer.toHexString((n.host + ":" + n.port + n.uuid).hashCode()) + "m";
        return n;
    }

    public JSONObject toJson() {
        JSONObject o = new JSONObject();
        try {
            o.put("id", id); o.put("p", proto); o.put("cc", cc == null ? "XX" : cc);
            o.put("name", name()); o.put("host", host);
            if (ping != null) o.put("ping", ping);
        } catch (Throwable ignored) {}
        return o;
    }
}
