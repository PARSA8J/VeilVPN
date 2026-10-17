package com.mehrveil.app;

import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.net.SocketException;
import java.util.concurrent.atomic.AtomicBoolean;

/**
 * User-space TCP server-side flow: terminates the phone's TCP handshake inside the TUN,
 * pipes payload bytes to a remote channel (proxy client), writes responses back as IP/TCP.
 */
public class TcpFlow {
    public static final int FIN = 0x01, SYN = 0x02, RST = 0x04, PSH = 0x08, ACK = 0x10;

    public final long srcIp, dstIp;
    public final int sport, dport;
    public final TunPump pump;

    private long ackNum;
    private long seqNum;
    private boolean synSent, established, finSent, closed;
    private int peerWindow = 65535;
    private OutputStream remoteOut;
    private InputStream remoteIn;
    private Thread readThread;
    private final AtomicBoolean running = new AtomicBoolean(true);
    private String dstHost;
    private boolean dialFailed;
    public volatile long lastActive;

    public TcpFlow(TunPump pump, long srcIp, int sport, long dstIp, int dport) {
        this.pump = pump; this.srcIp = srcIp; this.sport = sport; this.dstIp = dstIp; this.dport = dport;
        this.lastActive = System.currentTimeMillis();
    }

    private void send(long seq, long ack, int flags, byte[] payload) {
        byte[] pkt = Ip4.tcpPacket(dstIp, srcIp, dport, sport, seq, ack, flags, 65535, payload);
        pump.write(pkt);
    }

    private void rst() {
        send(seqNum, ackNum, RST | ACK, null);
    }

    /** Called by pump for every packet of this flow. */
    public synchronized void onPacket(byte[] pkt, int tcpOff, int totalLen) {
        if (closed) return;
        lastActive = System.currentTimeMillis();
        int ipHdrLen = (pkt[0] & 0xF) * 4;
        int dataOff = (pkt[tcpOff + 12] >> 4) * 4;
        int flags = pkt[tcpOff + 13] & 0xFF;
        long seq = Ip4.u32(pkt, tcpOff + 4) & 0xFFFFFFFFL;
        int win = Ip4.u16(pkt, tcpOff + 14);
        if (win > 0) peerWindow = win;
        int payloadLen = totalLen - ipHdrLen - dataOff;
        if (payloadLen < 0) payloadLen = 0;

        if ((flags & SYN) != 0 && !synSent) {
            synSent = true;
            ackNum = seq + 1;
            seqNum = 0x1000L + (long) (Math.random() * 0x100000L);
            send(seqNum, ackNum, SYN | ACK, null);
            seqNum++;
            if (payloadLen > 0) feedRemote(pkt, tcpOff + dataOff, payloadLen);
            return;
        }
        if ((flags & RST) != 0) { close(true); return; }
        if ((flags & ACK) != 0 && payloadLen == 0) {
            if (!established && synSent) established = true;
            return;
        }
        if (payloadLen > 0) {
            if (!synSent) { rst(); close(true); return; }
            established = true;
            boolean inOrder = seq == ackNum || ((ackNum - seq) & 0xFFFFFFFFL) < 0x7FFFFFFFL;
            if (inOrder) {
                ackNum = (seq + payloadLen) & 0xFFFFFFFFL;
                feedRemote(pkt, tcpOff + dataOff, payloadLen);
                send(0, ackNum, ACK, null);
            } else {
                send(0, ackNum, ACK, null);
            }
        }
        if ((flags & FIN) != 0) {
            ackNum = (ackNum + 1) & 0xFFFFFFFFL;
            send(0, ackNum, ACK, null);
            halfClose();
        }
    }

    private void feedRemote(byte[] pkt, int off, int len) {
        try {
            ensureRemote();
            if (remoteOut != null) {
                byte[] data = new byte[len];
                System.arraycopy(pkt, off, data, 0, len);
                remoteOut.write(data);
                remoteOut.flush();
            } else {
                ackNum = (ackNum + len) & 0xFFFFFFFFL;
                send(0, ackNum, RST | ACK, null);
                close(true);
            }
        } catch (Throwable t) {
            dialFailed = true;
            send(0, ackNum, RST | ACK, null);
            close(true);
        }
    }

    /** Lazily dial through the selected proxy protocol. */
    private void ensureRemote() throws IOException {
        if (remoteOut != null) return;
        if (dialFailed) throw new SocketException("dial failed");
        if (dstHost == null) dstHost = Ip4.ipStr(dstIp);
        String real = DnsAgent.reverse(dstIp);
        if (real != null) dstHost = real;
        VpnController.ClientPair ch;
        try {
            ch = VpnController.get().dial(new Dst(dstHost, dport));
        } catch (Exception e) {
            throw new IOException(e.getMessage() == null ? "dial failed" : e.getMessage());
        }
        remoteOut = ch.out;
        remoteIn = ch.in;
        readThread = new Thread(this::pumpRemote, "flow-" + sport);
        readThread.setDaemon(true);
        readThread.start();
    }

    private void pumpRemote() {
        // MTU is 8500 (set in VeilVpnService): an IP packet written to the TUN must be
        // <= MTU, otherwise the write fails silently and ALL download traffic dies.
        // 20B IP + 20B TCP headers -> max payload 8000. Larger remote reads are split.
        final int CHUNK = 8000;
        byte[] buf = new byte[CHUNK];
        try {
            int n;
            while (running.get() && !closed && (n = remoteIn.read(buf)) > 0) {
                int off = 0;
                while (off < n) {
                    int len = Math.min(CHUNK, n - off);
                    byte[] chunk = new byte[len];
                    System.arraycopy(buf, off, chunk, 0, len);
                    send(seqNum, ackNum, PSH | ACK, chunk);
                    seqNum += len;
                    off += len;
                }
            }
        } catch (Throwable ignored) {}
        if (running.get() && !closed) {
            try { send(seqNum, ackNum, FIN | ACK, null); seqNum++; } catch (Throwable ignored) {}
            finSent = true;
            scheduleSelfClose(4000);
        }
    }

    private void halfClose() {
        if (!finSent) {
            try { send(seqNum, ackNum, FIN | ACK, null); seqNum++; finSent = true; } catch (Throwable ignored) {}
        }
        closeRemote();
        scheduleSelfClose(1500);
    }

    private void scheduleSelfClose(int ms) {
        new Thread(() -> {
            try { Thread.sleep(ms); } catch (Throwable ignored) {}
            close(true);
        }).start();
    }

    public void closeRemote() {
        running.set(false);
        try { if (remoteOut != null) remoteOut.close(); } catch (Throwable ignored) {}
        try { if (remoteIn != null) remoteIn.close(); } catch (Throwable ignored) {}
        if (readThread != null) readThread.interrupt();
    }

    public void close(boolean teardown) {
        if (closed) return;
        closed = true;
        closeRemote();
        pump.removeFlow(sport, dport);
    }

    /** simple destination holder */
    public static class Dst {
        public final String host; public final int port;
        public Dst(String h, int p) { host = h; port = p; }
    }
}
