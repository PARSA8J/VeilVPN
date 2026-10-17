package com.mehrveil.app.proto;

import com.mehrveil.app.Node;
import com.mehrveil.app.net.Tls;
import com.mehrveil.app.net.Ws;

import java.io.ByteArrayOutputStream;
import java.io.InputStream;
import java.io.OutputStream;
import java.net.Socket;

/** VLESS over TCP+TLS / WS+TLS. Request: ver(0) uuid(16) addons-len addons cmd addrType addr port. */
public final class Vless extends BaseClient {

    public Vless(Node n) { super(n); }

    @Override
    public Channel dial(Node node, String dstHost, int dstPort) throws Exception {
        Node nn = node != null ? node : n;
        Socket sock = null;
        Ws ws = null;
        try {
            if (nn.net.equals("ws")) {
                Socket raw = nn.tls
                        ? Tls.tls(nn.host, nn.port, 12_000, firstNonEmpty(nn.sni, nn.host))
                        : Tls.tcp(nn.host, nn.port, 12_000);
                ws = Ws.handshake(raw, nn.host, nn.port, nn.path, nn.hostHeader);
            } else if (nn.tls) {
                sock = Tls.tls(nn.host, nn.port, 12_000, firstNonEmpty(nn.sni, nn.host));
            } else {
                sock = Tls.tcp(nn.host, nn.port, 12_000);
            }

            OutputStream out;
            InputStream in;
            if (ws != null) { out = ws.out(); in = ws.in(); }
            else { out = sock.getOutputStream(); in = sock.getInputStream(); }

            byte[] uid = uuidBytes(nn.uuid);
            if (uid.length != 16) throw new Exception("bad vless uuid");

            ByteArrayOutputStream req = new ByteArrayOutputStream();
            req.write(0);                                  // version
            req.write(uid);                                // uuid
            req.write(0);                                  // addon len 0
            req.write(1);                                  // cmd tcp
            byte[] addr = encodeAddr(dstHost);
            req.write(addr, 0, addr.length);
            req.write((dstPort >> 8) & 0xFF);
            req.write(dstPort & 0xFF);
            out.write(req.toByteArray());
            out.flush();

            byte[] head = new byte[2];
            if (!readFull(in, head) || head[0] != 0) throw new Exception("vless handshake failed");
            int addon = head[1] & 0xFF;
            if (addon > 0) {
                byte[] skip = new byte[addon];
                if (!readFull(in, skip)) throw new Exception("vless addon read failed");
            }
            if (ws != null) return new Channel(new WsIn(ws), new WsOut(ws));
            return new Channel(in, out);
        } catch (Exception e) {
            try { if (ws != null) ws.close(); } catch (Throwable ignored) {}
            try { if (sock != null) sock.close(); } catch (Throwable ignored) {}
            throw e;
        }
    }

    private static String firstNonEmpty(String a, String b) { return a != null && !a.isEmpty() ? a : b; }

    /** UUID string (with or without dashes) -> 16 bytes. */
    private static byte[] uuidBytes(String s) {
        String t = s == null ? "" : s.trim().replace("-", "");
        if (t.length() == 32) {
            try { return com.mehrveil.app.U.unhex(t); } catch (Throwable ignored) {}
        }
        return com.mehrveil.app.U.b64decode(s);
    }

    private static byte[] encodeAddr(String host) {
        ByteArrayOutputStream b = new ByteArrayOutputStream();
        if (host.matches("^[0-9.]+$")) {
            String[] p = host.split("\\.");
            b.write(1);
            for (String x : p) b.write(Integer.parseInt(x) & 0xFF);
        } else {
            byte[] hb = host.getBytes();
            b.write(2);
            b.write(hb.length);
            try { b.write(hb); } catch (Exception ignored) {}
        }
        return b.toByteArray();
    }

    private static boolean readFull(InputStream in, byte[] buf) {
        int off = 0;
        try {
            while (off < buf.length) {
                int r = in.read(buf, off, buf.length - off);
                if (r <= 0) return false;
                off += r;
            }
            return true;
        } catch (Exception e) { return false; }
    }

    /** aggregates ws frames into a stream view */
    static final class WsIn extends InputStream {
        final Ws ws;
        byte[] cur; int pos;
        WsIn(Ws ws) { this.ws = ws; }
        private boolean fill() {
            if (cur != null && pos < cur.length) return true;
            try { cur = ws.readFrame(); pos = 0; } catch (Exception e) { cur = null; }
            return cur != null && cur.length > 0;
        }
        @Override public int read() {
            if (!fill()) return -1;
            return cur[pos++] & 0xFF;
        }
        @Override public int read(byte[] b, int off, int len) {
            if (!fill()) return -1;
            int n = Math.min(len, cur.length - pos);
            System.arraycopy(cur, pos, b, off, n);
            pos += n;
            return n;
        }
    }

    /** splits stream into ws frames */
    static final class WsOut extends OutputStream {
        final Ws ws;
        WsOut(Ws ws) { this.ws = ws; }
        @Override public void write(int b) throws java.io.IOException {
            ws.writeFrame(new byte[]{(byte) b}, 0, 1, true);
        }
        @Override public void write(byte[] b, int off, int len) throws java.io.IOException {
            ws.writeFrame(b, off, len, true);
        }
        @Override public void flush() {}
        @Override public void close() { ws.close(); }
    }
}
