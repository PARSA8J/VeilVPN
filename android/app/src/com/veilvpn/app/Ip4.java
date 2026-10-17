package com.veilvpn.app;

/** Minimal IPv4 packet build/parse for the user-space TCP/IP layer. */
public final class Ip4 {
    public static int u16(byte[] b, int o) { return ((b[o] & 0xFF) << 8) | (b[o + 1] & 0xFF); }
    public static long u32(byte[] b, int o) { return ((b[o] & 0xFFL) << 24) | ((b[o+1] & 0xFFL) << 16) | ((b[o+2] & 0xFFL) << 8) | (b[o+3] & 0xFFL); }
    public static void p16(byte[] b, int o, int v) { b[o] = (byte) (v >> 8); b[o + 1] = (byte) v; }
    public static void p32(byte[] b, int o, long v) {
        b[o] = (byte) (v >> 24); b[o+1] = (byte) (v >> 16); b[o+2] = (byte) (v >> 8); b[o+3] = (byte) v;
    }
    public static String ipStr(long ip) {
        return ((ip >> 24) & 255) + "." + ((ip >> 16) & 255) + "." + ((ip >> 8) & 255) + "." + (ip & 255);
    }
    public static long ipVal(String s) {
        String[] p = s.split("\\.");
        return p.length != 4 ? 0 : (Long.parseLong(p[0]) << 24) | (Long.parseLong(p[1]) << 16)
                | (Long.parseLong(p[2]) << 8) | Long.parseLong(p[3]);
    }

    public static int checksum(byte[] buf, int off, int len) {
        long sum = 0;
        for (int i = 0; i < len - 1; i += 2) sum += u16(buf, off + i) & 0xFFFFL;
        if (len % 2 == 1) sum += (buf[off + len - 1] & 0xFF) << 8;
        while ((sum >> 16) != 0) sum = (sum & 0xFFFF) + (sum >> 16);
        return (int) (~sum & 0xFFFF);
    }

    /** Build full IPv4+TCP packet with payload. */
    public static byte[] tcpPacket(long srcIp, long dstIp, int sport, int dport,
                                   long seq, long ack, int flags, int window, byte[] payload) {
        int dataLen = payload == null ? 0 : payload.length;
        int tcpLen = 20 + dataLen;
        byte[] out = new byte[20 + tcpLen];
        out[0] = 0x45; out[1] = 0;
        p16(out, 2, 20 + tcpLen);
        out[4] = (byte) (Math.random() * 255); out[5] = 0;
        out[6] = 0x40; out[7] = 0;
        out[8] = 64; out[9] = 6;
        p32(out, 12, srcIp); p32(out, 16, dstIp);
        long sum = (srcIp >> 16) + (srcIp & 0xFFFF) + (dstIp >> 16) + (dstIp & 0xFFFF) + 6 + tcpLen;
        p16(out, 20, sport); p16(out, 22, dport);
        p32(out, 24, seq); p32(out, 28, ack);
        out[32] = 0x50; out[33] = (byte) flags;
        p16(out, 34, window);
        out[36] = 0; out[37] = 0;
        p16(out, 38, 0);
        if (payload != null) System.arraycopy(payload, 0, out, 40, dataLen);
        for (int i = 20; i < out.length - 1; i += 2) sum += u16(out, i) & 0xFFFFL;
        if ((out.length - 20) % 2 == 1) sum += (out[out.length - 1] & 0xFF) << 8;
        while ((sum >> 16) != 0) sum = (sum & 0xFFFF) + (sum >> 16);
        p16(out, 36, (int) (~sum & 0xFFFF));
        p16(out, 10, checksum(out, 0, 20));
        return out;
    }

    public static byte[] icmpUnreachable(long srcIp, long dstIp, byte[] origIpHdr) {
        byte[] out = new byte[20 + 8 + Math.min(8, origIpHdr == null ? 0 : origIpHdr.length)];
        out[0] = 0x45; out[1] = 0;
        p16(out, 2, out.length);
        out[8] = 64; out[9] = 1;
        p32(out, 12, srcIp); p32(out, 16, dstIp);
        out[20] = 3; out[21] = 1;
        p16(out, 22, 0);
        if (origIpHdr != null) System.arraycopy(origIpHdr, 0, out, 28, Math.min(8, origIpHdr.length));
        p16(out, 10, checksum(out, 0, 20));
        p16(out, 24, checksum(out, 20, out.length - 20));
        return out;
    }
}
