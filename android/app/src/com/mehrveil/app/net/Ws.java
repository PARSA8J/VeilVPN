package com.mehrveil.app.net;

import java.io.ByteArrayOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.io.PushbackInputStream;
import java.net.Socket;

/**
 * Minimal WebSocket stream (client, we mask our frames).
 * Wraps an established (TLS) socket; exposes plain-stream in/out for the protocol layer.
 */
public final class Ws {
    private final PushbackInputStream in;
    private final OutputStream out;
    private volatile boolean open = true;

    private Ws(InputStream i, OutputStream o) {
        this.in = new PushbackInputStream(i, 1);
        this.out = o;
    }

    public static Ws handshake(Socket sock, String host, int port, String path, String hostHeader) throws IOException {
        InputStream in = sock.getInputStream();
        OutputStream out = sock.getOutputStream();
        String key = com.mehrveil.app.U.b64encode(new byte[]{1, 35, 69, 103, (byte) 137, (byte) 171, (byte) 205, (byte) 239});
        String h = "GET " + (path == null || path.isEmpty() ? "/" : path) + " HTTP/1.1\r\n" +
                "Host: " + (hostHeader != null && !hostHeader.isEmpty() ? hostHeader : host + (port == 80 ? "" : ":" + port)) + "\r\n" +
                "Upgrade: websocket\r\nConnection: Upgrade\r\n" +
                "Sec-WebSocket-Key: " + key + "\r\n" +
                "Sec-WebSocket-Version: 13\r\n" +
                "User-Agent: Mozilla/5.0 (Linux; Android 13)\r\n\r\n";
        out.write(h.getBytes("UTF-8"));
        out.flush();
        ByteArrayOutputStream head = new ByteArrayOutputStream();
        int prev = -1, c;
        while ((c = in.read()) >= 0) {
            head.write(c);
            if (prev == '\r' && c == '\n' && head.size() >= 4) {
                byte[] b = head.toByteArray();
                if (b[b.length - 4] == '\r' && b[b.length - 3] == '\n') break;
            }
            prev = c;
            if (head.size() > 16384) break;
        }
        String resp = new String(head.toByteArray(), "UTF-8");
        if (!resp.startsWith("HTTP/1.1 101") && !resp.startsWith("HTTP/1.0 101"))
            throw new IOException("ws handshake failed: " + resp.split("\r\n")[0]);
        return new Ws(in, out);
    }

    public InputStream in() { return in; }
    public OutputStream out() { return out; }

    /** write a binary frame with the given payload (masked, client->server) */
    public void writeFrame(byte[] payload, int off, int len, boolean fin) throws IOException {
        ByteArrayOutputStream f = new ByteArrayOutputStream(len + 14);
        f.write(0x80 | 0x2);
        int maskBit = 0x80;
        if (len < 126) f.write(maskBit | len);
        else if (len < 65536) { f.write(maskBit | 126); f.write(len >> 8); f.write(len); }
        else { f.write(maskBit | 127); for (int i = 7; i >= 0; i--) f.write((len >> (i * 8)) & 0xFF); }
        byte[] mask = new byte[4];
        new java.security.SecureRandom().nextBytes(mask);
        f.write(mask);
        for (int i = 0; i < len; i++) f.write(payload[off + i] ^ mask[i & 3]);
        out.write(f.toByteArray());
        out.flush();
    }

    private static final int MAX_FRAME = 1 << 22;

    /** read next data frame payload (responds to ping, ignores pong/close) */
    public byte[] readFrame() throws IOException {
        while (open) {
            int b0 = in.read();
            if (b0 < 0) { open = false; return null; }
            int op = b0 & 0x0F;
            int b1 = in.read();
            if (b1 < 0) { open = false; return null; }
            boolean masked = (b1 & 0x80) != 0;
            long len = b1 & 0x7F;
            if (len == 126) len = ((in.read() & 0xFF) << 8) | (in.read() & 0xFF);
            else if (len == 127) {
                len = 0;
                for (int i = 0; i < 8; i++) len = (len << 8) | (in.read() & 0xFF);
            }
            if (len > MAX_FRAME) { open = false; return null; }
            byte[] mask = null;
            if (masked) {
                mask = new byte[4];
                if (in.read(mask) != 4) { open = false; return null; }
            }
            byte[] payload = new byte[(int) len];
            int off = 0;
            while (off < len) {
                int n = in.read(payload, off, (int) len - off);
                if (n <= 0) { open = false; return null; }
                off += n;
            }
            if (mask != null) for (int i = 0; i < (int) len; i++) payload[i] ^= mask[i & 3];
            if (op == 0x8) { close(); return null; }
            if (op == 0x9) { writePong(payload); continue; }
            if (op == 0xA) continue;
            if (op == 0x2 || op == 0x0 || op == 0x1) return payload;
        }
        return null;
    }

    private void writePong(byte[] payload) {
        try {
            ByteArrayOutputStream f = new ByteArrayOutputStream(payload.length + 10);
            f.write(0x8A);
            if (payload.length < 126) f.write(payload.length);
            else { f.write(126); f.write(payload.length >> 8); f.write(payload.length); }
            f.write(payload);
            out.write(f.toByteArray());
            out.flush();
        } catch (Throwable ignored) {}
    }

    public void close() {
        open = false;
        try { out.write(new byte[]{(byte) 0x88, 0}); out.flush(); } catch (Throwable ignored) {}
    }
}
