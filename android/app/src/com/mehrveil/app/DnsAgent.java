package com.mehrveil.app;

import android.content.Context;

import java.util.Map;
import java.util.concurrent.ConcurrentHashMap;

/**
 * DNS: intercepts queries from TUN, answers from cache, else resolves the query over the
 * tunnel (TCP DNS via proxy) and returns a fake IP mapped back for future TCP dials.
 * AAAA queries answered empty -> apps use IPv4 (our stack is v4 only).
 */
public final class DnsAgent {
    private static final long POOL_BASE = 0x0A0B0000L;        // 10.11.0.x
    private static final Map<Long, String> fake2host = new ConcurrentHashMap<>();
    private static final Map<String, Long> host2fake = new ConcurrentHashMap<>();
    private static final Map<String, byte[]> cache = new ConcurrentHashMap<>();
    private static int nextIdx = 1;

    public static String reverse(long fakeIp) {
        return fake2host.get(fakeIp);
    }

    public static void handle(Context ctx, TunPump pump, byte[] query, long srcIp, int sport, long dstIp) {
        try {
            int qd = ((query[4] & 0xFF) << 8) | (query[5] & 0xFF);
            if (qd != 1) return;
            int o = 12;
            StringBuilder host = new StringBuilder();
            while (o < query.length) {
                int l = query[o] & 0xFF;
                if (l == 0) { o++; break; }
                host.append(new String(query, o + 1, l, "UTF-8")).append('.');
                o += l + 1;
            }
            int qtype = ((query[o] & 0xFF) << 8) | (query[o + 1] & 0xFF);
            String h = host.toString().replaceAll("\\.$", "").toLowerCase();
            byte[] resp;
            if (qtype == 28) {                       // AAAA -> empty answer
                resp = buildResp(query, false, null, 0);
            } else if (qtype == 1) {                 // A
                byte[] c = cache.get(h);
                if (c != null) {
                    long fake = Ip4.u32(c, 0);
                    resp = buildResp(query, true, fake, 300);
                    sendUdp(pump, resp, srcIp, sport, dstIp);
                    return;
                }
                // cache miss: resolve off the pump thread, answer when ready
                final byte[] q = query;
                final TunPump fp = pump; final long fs = srcIp, fd = dstIp; final int fpo = sport; final String fh = h;
                new Thread(() -> {
                    try {
                        String ip = resolveOverTunnel(ctx, fh);
                        byte[] r;
                        if (ip == null) {
                            r = buildResp(q, false, null, 0);
                        } else {
                            long fake = allocFake(fh, (int) Ip4.ipVal(ip));
                            cache.put(fh, new byte[]{(byte)(fake>>24),(byte)(fake>>16),(byte)(fake>>8),(byte)fake});
                            r = buildResp(q, true, fake, 300);
                        }
                        sendUdp(fp, r, fs, fpo, fd);
                    } catch (Throwable ignored) {}
                }, "dns-resolve").start();
                return;
            } else {
                resp = buildResp(query, false, null, 0);
            }
            if (resp != null) sendUdp(pump, resp, srcIp, sport, dstIp);
        } catch (Throwable ignored) {}
    }

    private static long allocFake(String host, int realIpHint) {
        synchronized (DnsAgent.class) {
            Long existing = host2fake.get(host);
            if (existing != null) return existing;
            long fake = (POOL_BASE & 0xFFFFFF00L) | (nextIdx++ & 0xFF);
            if (nextIdx > 254) nextIdx = 1;
            fake2host.put(fake, host);
            host2fake.put(host, fake);
            return fake;
        }
    }

    /** Resolve via TCP DNS through the tunnel (dest 1.1.1.1:53 via proxy). */
    private static String resolveOverTunnel(Context ctx, String host) {
        if (VpnController.get().currentNode() == null) return null;
        try {
            VpnController.ClientPair c = VpnController.get().dialDns(host);
            if (c == null) return null;
            byte[] q = dnsQuery(host);
            byte[] framed = new byte[2 + q.length];
            framed[0] = (byte) (q.length >> 8); framed[1] = (byte) q.length;
            System.arraycopy(q, 0, framed, 2, q.length);
            c.out.write(framed); c.out.flush();
            byte[] head = new byte[2];
            if (!readFull(c.in, head)) return null;
            int len = ((head[0] & 0xFF) << 8) | (head[1] & 0xFF);
            if (len < 12 || len > 4096) return null;
            byte[] resp = new byte[len];
            if (!readFull(c.in, resp)) return null;
            try { c.out.close(); } catch (Throwable ignored) {}
            return parseA(resp, host);
        } catch (Throwable t) { return null; }
    }

    private static boolean readFull(java.io.InputStream in, byte[] b) {
        int off = 0;
        try {
            while (off < b.length) {
                int n = in.read(b, off, b.length - off);
                if (n <= 0) return false;
                off += n;
            }
            return true;
        } catch (Throwable t) { return false; }
    }

    private static byte[] dnsQuery(String host) {
        java.io.ByteArrayOutputStream bos = new java.io.ByteArrayOutputStream();
        byte[] id = new byte[2];
        new java.security.SecureRandom().nextBytes(id);
        bos.write(id[0]); bos.write(id[1]);
        bos.write(0x01); bos.write(0x00);
        bos.write(0); bos.write(1);
        bos.write(0); bos.write(0); bos.write(0); bos.write(0); bos.write(0); bos.write(0);
        for (String p : host.split("\\.")) {
            bos.write(p.length());
            try { bos.write(p.getBytes("UTF-8")); } catch (Throwable ignored) {}
        }
        bos.write(0);
        bos.write(0); bos.write(1);
        bos.write(0); bos.write(1);
        return bos.toByteArray();
    }

    private static String parseA(byte[] r, String host) {
        try {
            int qd = ((r[4] & 0xFF) << 8) | (r[5] & 0xFF);
            int an = ((r[6] & 0xFF) << 8) | (r[7] & 0xFF);
            int o = 12;
            for (int i = 0; i < qd; i++) {
                while (o < r.length && (r[o] & 0xFF) != 0) o += (r[o] & 0xFF) + 1;
                o += 5;
            }
            for (int i = 0; i < an; i++) {
                if ((r[o] & 0xC0) == 0xC0) o += 2; else { while (o < r.length && (r[o] & 0xFF) != 0) o += (r[o] & 0xFF) + 1; o++; }
                int type = ((r[o] & 0xFF) << 8) | (r[o + 1] & 0xFF);
                int rdlen = ((r[o + 8] & 0xFF) << 8) | (r[o + 9] & 0xFF);
                o += 10;
                if (type == 1 && rdlen == 4) {
                    return (r[o] & 0xFF) + "." + (r[o+1] & 0xFF) + "." + (r[o+2] & 0xFF) + "." + (r[o+3] & 0xFF);
                }
                o += rdlen;
            }
        } catch (Throwable ignored) {}
        return null;
    }

    private static byte[] buildResp(byte[] q, boolean hasA, Long ip, int ttl) {
        java.io.ByteArrayOutputStream bos = new java.io.ByteArrayOutputStream();
        try {
            bos.write(q, 0, q.length);
            byte[] out = bos.toByteArray();
            out[2] |= (byte) 0x80;
            out[3] |= (byte) 0x80;
            java.io.ByteArrayOutputStream bos2 = new java.io.ByteArrayOutputStream();
            bos2.write(out, 0, out.length);
            bos2.write(0); bos2.write(hasA ? 1 : 0);
            if (hasA) {
                bos2.write(0xC0); bos2.write(12);
                bos2.write(0); bos2.write(1);
                bos2.write(0); bos2.write(1);
                bos2.write((ttl >> 24) & 0xFF); bos2.write((ttl >> 16) & 0xFF); bos2.write((ttl >> 8) & 0xFF); bos2.write(ttl & 0xFF);
                bos2.write(0); bos2.write(4);
                long v = ip;
                bos2.write((int) (v >> 24) & 0xFF); bos2.write((int) (v >> 16) & 0xFF); bos2.write((int) (v >> 8) & 0xFF); bos2.write((int) v & 0xFF);
            }
            return bos2.toByteArray();
        } catch (Throwable t) { return null; }
    }

    private static void sendUdp(TunPump pump, byte[] dns, long srcIp, int sport, long dstIp) {
        if (dns == null) return;
        byte[] udp = new byte[8 + dns.length];
        Ip4.p16(udp, 0, sport);
        Ip4.p16(udp, 2, 53);
        Ip4.p16(udp, 4, udp.length);
        Ip4.p16(udp, 6, 0);
        System.arraycopy(dns, 0, udp, 8, dns.length);
        byte[] pkt = new byte[20 + udp.length];
        pkt[0] = 0x45; pkt[8] = 64; pkt[9] = 17;
        Ip4.p16(pkt, 2, pkt.length);
        Ip4.p32(pkt, 12, dstIp); Ip4.p32(pkt, 16, srcIp);
        System.arraycopy(udp, 0, pkt, 20, udp.length);
        Ip4.p16(pkt, 10, 0);
        Ip4.p16(pkt, 20 + 6, 0);                                // UDP checksum 0 = none (legal for IPv4)
        Ip4.p16(pkt, 10, Ip4.checksum(pkt, 0, 20));
        pump.write(pkt);
    }
}
