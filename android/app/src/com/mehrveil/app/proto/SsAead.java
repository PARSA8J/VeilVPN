package com.mehrveil.app.proto;

import com.mehrveil.app.Node;
import com.mehrveil.app.net.Tls;

import java.io.ByteArrayOutputStream;
import java.io.InputStream;
import java.io.OutputStream;
import java.net.Socket;
import java.security.MessageDigest;
import java.security.spec.AlgorithmParameterSpec;
import java.security.SecureRandom;

import javax.crypto.Cipher;
import javax.crypto.spec.GCMParameterSpec;
import javax.crypto.spec.IvParameterSpec;
import javax.crypto.spec.SecretKeySpec;

/**
 * Shadowsocks AEAD (aes-128/256-gcm; chacha when the platform provides it).
 * Subkey = HKDF-SHA1(pass, salt, "ss-subkey", 16/32). TCP request: salt + [len|payload]*
 * each chunk encrypted with incrementing nonce, first target header inside first chunk.
 */
public final class SsAead extends BaseClient {
    private static final int TAG_BITS = 128;   // GCM tag length in BITS (GCMParameterSpec takes bits)

    public SsAead(Node n) { super(n); }

    @Override
    public Channel dial(Node node, String dstHost, int dstPort) throws Exception {
        Node nn = node != null ? node : n;
        Socket sock = Tls.tcp(nn.host, nn.port, 12_000);
        try {
            OutputStream out = sock.getOutputStream();
            InputStream in = sock.getInputStream();

            int keyLen = nn.method.contains("128") ? 16 : nn.method.contains("192") ? 24 : 32;
            boolean chacha = nn.method.contains("chacha");
            byte[] master = evpBytesToKey(String.valueOf(nn.password), keyLen);

            byte[] salt = new byte[keyLen];
            new SecureRandom().nextBytes(salt);
            byte[] sub = hkdfSha1(master, salt, "ss-subkey", keyLen);

            Cipher enc = Cipher.getInstance(chacha ? "ChaCha20-Poly1305" : "AES/GCM/NoPadding");
            SecretKeySpec encKey = new SecretKeySpec(sub, chacha ? "ChaCha20" : "AES");
            byte[] encNonce = new byte[12];

            ByteArrayOutputStream target = new ByteArrayOutputStream();
            writeAddr(target, dstHost, dstPort);
            byte[] first = target.toByteArray();

            ByteArrayOutputStream wire = new ByteArrayOutputStream();
            wire.write(salt);
            encryptChunk(enc, encKey, encNonce, first, wire);
            out.write(wire.toByteArray());
            out.flush();

            return new Channel(new SsIn(in, sub, chacha), new SsOut(out, sub, chacha));
        } catch (Exception e) {
            try { sock.close(); } catch (Throwable ignored) {}
            throw e;
        }
    }

    /* ---------- crypto helpers ---------- */

    private static byte[] evpBytesToKey(String pass, int keyLen) throws Exception {
        MessageDigest md5 = MessageDigest.getInstance("MD5");
        ByteArrayOutputStream bos = new ByteArrayOutputStream();
        byte[] prev = new byte[0];
        while (bos.size() < keyLen) {
            md5.reset();
            md5.update(prev);
            md5.update(pass.getBytes("UTF-8"));
            prev = md5.digest();
            bos.write(prev);
        }
        byte[] out = bos.toByteArray();
        byte[] key = new byte[keyLen];
        System.arraycopy(out, 0, key, 0, keyLen);
        return key;
    }

    private static byte[] hkdfSha1(byte[] ikm, byte[] salt, String info, int len) throws Exception {
        javax.crypto.Mac hmac = javax.crypto.Mac.getInstance("HmacSHA1");
        hmac.init(new javax.crypto.spec.SecretKeySpec(salt.length == 0 ? new byte[20] : salt, "HmacSHA1"));
        byte[] prk = hmac.doFinal(ikm);
        ByteArrayOutputStream okm = new ByteArrayOutputStream();
        byte[] t = new byte[0];
        byte counter = 1;
        while (okm.size() < len) {
            hmac.init(new javax.crypto.spec.SecretKeySpec(prk, "HmacSHA1"));
            hmac.update(t);
            hmac.update(info.getBytes("UTF-8"));
            hmac.update(counter++);
            t = hmac.doFinal();
            okm.write(t);
        }
        byte[] out = okm.toByteArray();
        byte[] res = new byte[len];
        System.arraycopy(out, 0, res, 0, len);
        return res;
    }

    private static void increment(byte[] nonce) {
        for (int i = nonce.length - 1; i >= 0; i--) {
            if (++nonce[i] != 0) break;
        }
    }

    private void encryptChunk(Cipher cipher, SecretKeySpec key, byte[] nonce, byte[] payload, ByteArrayOutputStream out) throws Exception {
        byte[] lenBuf = {(byte) ((payload.length >> 8) & 0xFF), (byte) (payload.length & 0xFF)};
        AlgorithmParameterSpec spec = (cipher.getAlgorithm().contains("ChaCha")) ? new IvParameterSpec(nonce) : new GCMParameterSpec(TAG_BITS, nonce);
        cipher.init(Cipher.ENCRYPT_MODE, key, spec);
        byte[] lenEnc = cipher.doFinal(lenBuf);
        increment(nonce);
        spec = (cipher.getAlgorithm().contains("ChaCha")) ? new IvParameterSpec(nonce) : new GCMParameterSpec(TAG_BITS, nonce);
        cipher.init(Cipher.ENCRYPT_MODE, key, spec);
        byte[] dataEnc = cipher.doFinal(payload);
        increment(nonce);
        out.write(lenEnc);
        out.write(dataEnc);
    }

    private static void writeAddr(ByteArrayOutputStream b, String host, int port) throws Exception {
        if (host.matches("^[0-9.]+$")) {
            String[] p = host.split("\\.");
            b.write(1);
            for (String x : p) b.write(Integer.parseInt(x) & 0xFF);
        } else {
            byte[] hb = host.getBytes("UTF-8");
            b.write(3);
            b.write(hb.length);
            b.write(hb);
        }
        b.write((port >> 8) & 0xFF);
        b.write(port & 0xFF);
    }

    /* ---------- streaming wrappers ---------- */

    static final class SsOut extends OutputStream {
        final OutputStream out;
        final SecretKeySpec key; final boolean chacha;
        final byte[] nonce;
        Cipher cipher;
        SsOut(OutputStream o, byte[] sub, boolean chacha) throws Exception {
            this.out = o; this.chacha = chacha;
            this.key = new SecretKeySpec(sub, chacha ? "ChaCha20" : "AES");
            this.nonce = new byte[12];
            this.cipher = Cipher.getInstance(chacha ? "ChaCha20-Poly1305" : "AES/GCM/NoPadding");
            this.specFor = chacha;
        }
        final boolean specFor;
        private AlgorithmParameterSpec spec(byte[] n) { return specFor ? new IvParameterSpec(n) : new GCMParameterSpec(128, n); }
        private void chunk(byte[] b, int off, int len) throws java.io.IOException {
            try {
                byte[] lenBuf = {(byte) ((len >> 8) & 0xFF), (byte) (len & 0xFF)};
                cipher.init(Cipher.ENCRYPT_MODE, key, spec(nonce));
                byte[] le = cipher.doFinal(lenBuf);
                increment(nonce);
                cipher.init(Cipher.ENCRYPT_MODE, key, spec(nonce));
                byte[] de = cipher.doFinal(b, off, len);
                increment(nonce);
                out.write(le); out.write(de);
            } catch (Exception e) { throw new java.io.IOException(e); }
        }
        @Override public void write(int b) throws java.io.IOException { chunk(new byte[]{(byte) b}, 0, 1); }
        @Override public void write(byte[] b, int off, int len) throws java.io.IOException {
            int off2 = off;
            while (len > 0) {
                int n = Math.min(0x3F00, len);
                chunk(b, off2, n);
                off2 += n; len -= n;
            }
        }
        @Override public void flush() throws java.io.IOException { out.flush(); }
        @Override public void close() throws java.io.IOException { out.close(); }
    }

    static final class SsIn extends InputStream {
        final InputStream in;
        final SecretKeySpec key; final boolean chacha;
        byte[] nonce; Cipher dec;
        byte[] pending; int pos;
        boolean eof;
        SsIn(InputStream i, byte[] sub, boolean chacha) throws Exception {
            this.in = i; this.chacha = chacha;
            this.key = new SecretKeySpec(sub, chacha ? "ChaCha20" : "AES");
            this.nonce = new byte[12];
            this.dec = Cipher.getInstance(chacha ? "ChaCha20-Poly1305" : "AES/GCM/NoPadding");
        }
        private AlgorithmParameterSpec spec() { return chacha ? new IvParameterSpec(nonce) : new GCMParameterSpec(128, nonce); }
        private boolean nextChunk() {
            if (eof) return false;
            try {
                byte[] et = new byte[18];
                if (!readFull(et)) { eof = true; return false; }
                dec.init(Cipher.DECRYPT_MODE, key, spec());
                byte[] t = dec.doFinal(et);
                increment(nonce);
                int len = ((t[0] & 0xFF) << 8) | (t[1] & 0xFF);
                if (len <= 0 || len > 0x3FFF) { eof = true; return false; }
                byte[] ed = new byte[len + 16];
                if (!readFull(ed)) { eof = true; return false; }
                dec.init(Cipher.DECRYPT_MODE, key, spec());
                pending = dec.doFinal(ed);
                increment(nonce);
                pos = 0;
                return true;
            } catch (Throwable t) { eof = true; return false; }
        }
        private boolean fill() { return (pending != null && pos < pending.length) || nextChunk(); }
        @Override public int read() {
            if (!fill()) return -1;
            return pending[pos++] & 0xFF;
        }
        @Override public int read(byte[] b, int off, int len) {
            if (!fill()) return -1;
            int n = Math.min(len, pending.length - pos);
            System.arraycopy(pending, pos, b, off, n);
            pos += n;
            return n;
        }
        private boolean readFull(byte[] buf) throws java.io.IOException {
            int off = 0;
            while (off < buf.length) {
                int r = in.read(buf, off, buf.length - off);
                if (r <= 0) return false;
                off += r;
            }
            return true;
        }
    }
}
