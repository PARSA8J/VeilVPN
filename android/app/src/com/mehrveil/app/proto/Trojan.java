package com.mehrveil.app.proto;

import com.mehrveil.app.Node;
import com.mehrveil.app.net.Tls;

import java.io.InputStream;
import java.io.OutputStream;
import java.net.Socket;

/** Trojan: hex(SHA224(password)) CRLF cmd ATYP addr port CRLF (always TLS). */
public final class Trojan extends BaseClient {

    public Trojan(Node n) { super(n); }

    @Override
    public Channel dial(Node node, String dstHost, int dstPort) throws Exception {
        Node nn = node != null ? node : n;
        Socket sock = Tls.tls(nn.host, nn.port, 12_000, firstNonEmpty(nn.sni, nn.host));
        try {
            OutputStream out = sock.getOutputStream();
            InputStream in = sock.getInputStream();

            java.security.MessageDigest md = java.security.MessageDigest.getInstance("SHA-224");
            byte[] h = md.digest(String.valueOf(nn.password == null ? nn.uuid : nn.password).getBytes("UTF-8"));
            StringBuilder hex = new StringBuilder();
            for (byte b : h) hex.append(String.format("%02x", b));

            byte[] head = (hex + "\r\n" + "\u0001").getBytes("ISO-8859-1");
            out.write(head);
            byte[] addr = encodeAddr(dstHost);
            out.write(addr);
            out.write(new byte[]{(byte) ((dstPort >> 8) & 0xFF), (byte) (dstPort & 0xFF), (byte) 0x0D, (byte) 0x0A});
            out.flush();

            int c1 = in.read(), c2 = in.read();
            if (c1 != 0x0D || c2 != 0x0A) throw new Exception("trojan handshake failed");
            return new Channel(in, out);
        } catch (Exception e) {
            try { sock.close(); } catch (Throwable ignored) {}
            throw e;
        }
    }

    private static String firstNonEmpty(String a, String b) { return a != null && !a.isEmpty() ? a : b; }

    private static byte[] encodeAddr(String host) {
        java.io.ByteArrayOutputStream b = new java.io.ByteArrayOutputStream();
        if (host.matches("^[0-9.]+$")) {
            String[] p = host.split("\\.");
            b.write(1);
            for (String x : p) b.write(Integer.parseInt(x) & 0xFF);
        } else {
            byte[] hb = host.getBytes();
            b.write(3);
            b.write(hb.length);
            try { b.write(hb); } catch (Exception ignored) {}
        }
        return b.toByteArray();
    }
}
