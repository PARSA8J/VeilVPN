package com.veilvpn.app;

import android.net.VpnService;
import android.os.ParcelFileDescriptor;

import java.io.FileInputStream;
import java.io.FileOutputStream;
import java.util.Map;
import java.util.concurrent.ConcurrentHashMap;

/** Reads IP packets from TUN, dispatches TCP flows / DNS UDP, writes replies. */
public class TunPump implements Runnable {
    private final VpnService svc;
    private final ParcelFileDescriptor tun;
    private final FileInputStream in;
    private final FileOutputStream out;
    private final Map<Long, TcpFlow> flows = new ConcurrentHashMap<>();
    private volatile boolean running = true;

    public TunPump(VpnService svc, ParcelFileDescriptor tun) {
        this.svc = svc; this.tun = tun;
        this.in = new FileInputStream(tun.getFileDescriptor());
        this.out = new FileOutputStream(tun.getFileDescriptor());
    }

    public void write(byte[] pkt) {
        try { out.write(pkt); out.flush(); } catch (Throwable ignored) {}
    }

    private static long key(int sport, int dport) { return ((long) sport << 16) | dport; }

    public void removeFlow(int sport, int dport) { flows.remove(key(sport, dport)); }

    @Override
    public void run() {
        byte[] buf = new byte[65536];
        try {
            while (running) {
                int n = in.read(buf);
                if (n <= 0) continue;
                if ((buf[0] >> 4) == 4 && n >= 20) handleIp4(buf, n);
                // IPv6 dropped (AAAA filtered by DnsAgent -> apps fall back to v4)
            }
        } catch (Throwable ignored) {}
    }

    private void handleIp4(byte[] b, int len) {
        int ihl = (b[0] & 0xF) * 4;
        int proto = b[9] & 0xFF;
        long src = Ip4.u32(b, 12), dst = Ip4.u32(b, 16);
        if (proto == 6) {
            int sport = Ip4.u16(b, ihl), dport = Ip4.u16(b, ihl + 2);
            if (sport == 0 || dport == 0) return;
            Long k = key(sport, dport);
            TcpFlow f = flows.get(k);
            if (f == null) {
                if (dport == 25 || dport == 587) { reject(b, ihl, len); return; }
                f = new TcpFlow(this, src, sport, dst, dport);
                flows.put(k, f);
            }
            f.onPacket(b, ihl, len);
        } else if (proto == 17) {
            int udpLen = Ip4.u16(b, ihl + 4);
            int dport = Ip4.u16(b, ihl + 2);
            int sport = Ip4.u16(b, ihl);
            if (dport == 53) {
                byte[] dns = new byte[udpLen - 8];
                System.arraycopy(b, ihl + 8, dns, 0, dns.length);
                DnsAgent.handle(svc, this, dns, src, sport, dst);
            }
            // other UDP: silent drop
        } else if (proto == 1) {
            if (len >= ihl + 8 && (b[ihl] & 0xFF) == 8) {
                byte[] reply = new byte[len];
                System.arraycopy(b, 0, reply, 0, len);
                reply[ihl] = 0;
                Ip4.p16(reply, 2, len);
                long t = src; Ip4.p32(reply, 12, dst); Ip4.p32(reply, 16, t);
                Ip4.p16(reply, 10, 0);
                Ip4.p16(reply, 10, Ip4.checksum(reply, 0, 20));
                Ip4.p16(reply, ihl + 2, 0);
                Ip4.p16(reply, ihl + 2, Ip4.checksum(reply, ihl, len - ihl));
                write(reply);
            }
        }
    }

    private void reject(byte[] b, int ihl, int len) {
        byte[] orig = new byte[len];
        System.arraycopy(b, 0, orig, 0, len);
        write(Ip4.icmpUnreachable(Ip4.u32(orig, 16), Ip4.u32(orig, 12), orig));
    }

    public void stop() {
        running = false;
        for (TcpFlow f : flows.values()) f.close(true);
        flows.clear();
        try { in.close(); } catch (Throwable ignored) {}
        try { out.close(); } catch (Throwable ignored) {}
        try { tun.close(); } catch (Throwable ignored) {}
    }
}
