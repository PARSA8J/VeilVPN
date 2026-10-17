package com.mehrveil.app;

import android.app.Notification;
import android.app.NotificationChannel;
import android.app.NotificationManager;
import android.app.PendingIntent;
import android.content.Intent;
import android.net.VpnService;
import android.os.Build;
import android.os.ParcelFileDescriptor;

import java.io.IOException;
import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.atomic.AtomicBoolean;

/**
 * Real VPN core: TUN device + user-space TCP/IP + VLESS/Trojan/SS channels.
 * Failover: tries candidates in order (watchdog 8s per candidate, 10s health loop).
 */
public class VeilVpnService extends VpnService {
    public static final String ACTION_CONNECT = "com.mehrveil.CONNECT";
    public static final String ACTION_DISCONNECT = "com.mehrveil.DISCONNECT";
    private static final String CH = "veil";
    private static final int NOTIF_ID = 42;

    public static volatile List<Node> QUEUE;
    private TunPump pump;
    private Thread pumpThread;
    private final AtomicBoolean working = new AtomicBoolean(false);
    private Thread watchdog;

    @Override
    public int onStartCommand(Intent intent, int flags, int startId) {
        String action = intent == null ? ACTION_CONNECT : intent.getAction();
        if (ACTION_DISCONNECT.equals(action)) { stopAll("user"); return START_NOT_STICKY; }
        if (working.compareAndSet(false, true)) {
            startForeground(NOTIF_ID, buildNotification("Connecting…"));
            List<Node> q = QUEUE;
            if (q == null || q.isEmpty()) {
                working.set(false);
                stopAll("no servers");
                return START_NOT_STICKY;
            }
            new Thread(() -> runQueue(new ArrayList<>(q)), "veil-core").start();
        }
        return START_NOT_STICKY;
    }

    private void runQueue(List<Node> queue) {
        try {
            establishAndRun(queue.remove(0), queue);
        } catch (Throwable t) {
            working.set(false);
            stopAll(firstMessage(t));
        }
    }

    private void establishAndRun(Node node, List<Node> rest) throws Exception {
        VpnController.get().setCurrentNode(node);
        VpnController.get().setState(VpnController.CONNECTING);
        updateNotification("Connecting · " + node.name());

        Builder b = new Builder()
                .setMtu(8500)
                .addAddress("10.11.0.1", 24)
                .addDnsServer("10.11.0.1")
                .addRoute("0.0.0.0", 0)
                .setSession("VeilVPN");
        try { b.addDisallowedApplication(getPackageName()); } catch (Throwable ignored) {}
        if (Build.VERSION.SDK_INT >= 29) {
            b.setMetered(false);
            try { b.setUnderlyingNetworks(null); } catch (Throwable ignored) {}
        }
        ParcelFileDescriptor tun;
        try { tun = b.establish(); }
        catch (Throwable t) { tun = null; }
        if (tun == null) throw new IOException("tun establish failed");

        pump = new TunPump(this, tun);
        pumpThread = new Thread(pump, "tun-pump");
        pumpThread.start();

        // verify the node actually works: full protocol handshake via real dial
        long deadline = System.currentTimeMillis() + 8000;
        boolean ok = false;
        String lastErr = "timeout";
        while (System.currentTimeMillis() < deadline && VpnController.get().currentNode() != null) {
            try {
                Node n = VpnController.get().currentNode();
                com.mehrveil.app.proto.Client c = VpnController.clientFor(n);
                com.mehrveil.app.proto.Client.Channel ch = c.dial(n, "1.1.1.1", 53);
                try { ch.in.close(); } catch (Throwable ignored) {}
                try { ch.out.close(); } catch (Throwable ignored) {}
                ok = true;
                break;
            } catch (Throwable t) {
                lastErr = firstMessage(t);
            }
            Thread.sleep(400);
        }
        if (ok) {
            working.set(false);
            VpnController.get().setState(VpnController.CONNECTED);
            updateNotification("Connected · Protected");
            startWatchdog();
        } else {
            teardownTun();
            working.set(false);
            if (!rest.isEmpty()) {
                VpnController.get().setState(VpnController.RECONNECTING);
                establishAndRun(rest.remove(0), rest);
            } else {
                stopAll(lastErr);
            }
        }
    }

    private void startWatchdog() {
        stopWatchdog();
        watchdog = new Thread(() -> {
            while (VpnController.get().state().equals(VpnController.CONNECTED)) {
                try { Thread.sleep(10_000); } catch (Throwable t) { return; }
                try {
                    Node n = VpnController.get().currentNode();
                    if (n == null) continue;
                    com.mehrveil.app.proto.Client c = VpnController.clientFor(n);
                    com.mehrveil.app.proto.Client.Channel ch = c.dial(n, "1.1.1.1", 53);
                    try { ch.in.close(); } catch (Throwable ignored) {}
                    try { ch.out.close(); } catch (Throwable ignored) {}
                } catch (Throwable t) {
                    if (working.compareAndSet(false, true)) {
                        VpnController.get().setState(VpnController.RECONNECTING);
                        teardownTun();
                        stopWatchdog();
                        List<Node> q = VpnController.get().buildCandidates(VpnController.get().currentNode());
                        QUEUE = q;
                        if (!q.isEmpty()) {
                            new Thread(() -> {
                                try { establishAndRun(q.remove(0), q); }
                                catch (Throwable tt) { working.set(false); stopAll(firstMessage(tt)); }
                            }, "veil-reconnect").start();
                        } else {
                            working.set(false);
                            stopAll("reconnect failed");
                        }
                    }
                    return;
                }
            }
        }, "veil-watchdog");
        watchdog.setDaemon(true);
        watchdog.start();
    }

    private void stopWatchdog() {
        if (watchdog != null) { watchdog.interrupt(); watchdog = null; }
    }

    private void teardownTun() {
        stopWatchdog();
        if (pump != null) { pump.stop(); pump = null; }
    }

    private void stopAll(String err) {
        teardownTun();
        working.set(false);
        try { stopForeground(true); } catch (Throwable ignored) {}
        try { stopSelf(); } catch (Throwable ignored) {}
        VpnController.get().setState(VpnController.DISCONNECTED, err);
    }

    @Override
    public void onRevoke() { stopAll("revoked"); }

    @Override
    public void onDestroy() { teardownTun(); }

    private Notification buildNotification(String text) {
        NotificationManager nm = (NotificationManager) getSystemService(NOTIFICATION_SERVICE);
        if (Build.VERSION.SDK_INT >= 26 && nm.getNotificationChannel(CH) == null) {
            nm.createNotificationChannel(new NotificationChannel(CH, "VeilVPN", NotificationManager.IMPORTANCE_LOW));
        }
        Notification.Builder nb = Build.VERSION.SDK_INT >= 26
                ? new Notification.Builder(this, CH)
                : new Notification.Builder(this);
        PendingIntent pi = PendingIntent.getActivity(this, 0, new Intent(this, MainActivity.class),
                Build.VERSION.SDK_INT >= 23 ? PendingIntent.FLAG_IMMUTABLE : 0);
        nb.setSmallIcon(getApplicationInfo().icon)
                .setContentTitle("VeilVPN")
                .setContentText(text)
                .setOngoing(true)
                .setContentIntent(pi);
        return nb.build();
    }

    private void updateNotification(String text) {
        try {
            NotificationManager nm = (NotificationManager) getSystemService(NOTIFICATION_SERVICE);
            nm.notify(NOTIF_ID, buildNotification(text));
        } catch (Throwable ignored) {}
    }

    private static String firstMessage(Throwable t) {
        String m = t.getMessage();
        return m == null || m.isEmpty() ? t.getClass().getSimpleName() : m.length() > 80 ? m.substring(0, 80) : m;
    }
}
