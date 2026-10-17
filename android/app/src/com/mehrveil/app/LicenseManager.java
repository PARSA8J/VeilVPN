package com.mehrveil.app;

import android.content.Context;
import android.provider.Settings;

import org.json.JSONObject;

/**
 * Offline HMAC license. v1.5.1: auto-provisions a 3-day free trial bound to the
 * device on first run — no Telegram bot needed to get started.
 *
 * Format: VEIL-<payloadB32>-<sigB32(8)>
 * payload = "T|" + type + "|" + expEpochSec + "|" + devHash8
 */
public final class LicenseManager {
    public static final String SECRET = "Veil::LT-2026::xK9mQ7wR2p";
    public static final long TRIAL_MS = 3L * 24 * 3600 * 1000;
    private static final String PREFS = "veil_lic";

    public static class Info {
        public String type = "free";
        public long exp;               // 0 = lifetime (premium)
        public String device = "";
        public boolean valid;
        public int daysLeft() {
            if (exp == 0) return 9999;
            long d = (exp - System.currentTimeMillis()) / 86400000L;
            return d < 0 ? 0 : (int) d + 1;
        }
        public boolean active() {
            return valid && (exp == 0 || System.currentTimeMillis() < exp);
        }
        public JSONObject json() {
            JSONObject o = new JSONObject();
            try {
                o.put("type", type);
                o.put("exp", exp);
                o.put("daysLeft", daysLeft());
                o.put("active", active());
                o.put("device", device);
            } catch (Throwable ignored) {}
            return o;
        }
    }

    private static String devHash(Context c) {
        String id = Settings.Secure.getString(c.getContentResolver(), Settings.Secure.ANDROID_ID);
        if (id == null || id.isEmpty()) id = "veil-" + android.os.Build.FINGERPRINT;
        return U.sha256hex("veildev:" + id).substring(0, 8);
    }

    public static synchronized Info info(Context c) {
        android.content.SharedPreferences p = c.getSharedPreferences(PREFS, 0);
        Info in = new Info();
        in.device = devHash(c);
        String key = p.getString("key", null);
        if (key != null) {
            Info k = verify(key, in.device);
            if (k != null) return k;
        }
        String trial = p.getString("trial", null);
        if (trial != null) {
            Info k = verify(trial, in.device);
            if (k != null) return k;
        }
        String t = make("free", System.currentTimeMillis() + TRIAL_MS, in.device);
        p.edit().putString("trial", t).apply();
        Info k = verify(t, in.device);
        return k != null ? k : in;
    }

    public static String currentKey(Context c) {
        android.content.SharedPreferences p = c.getSharedPreferences(PREFS, 0);
        return p.getString("key", p.getString("trial", null));
    }

    /** Activate a bot-issued key. Returns info or null if invalid. */
    public static synchronized Info activate(Context c, String key) {
        if (key == null) return null;
        key = key.trim().toUpperCase();
        Info in = new Info();
        in.device = devHash(c);
        Info k = verify(key, in.device);
        if (k == null) return null;
        c.getSharedPreferences(PREFS, 0).edit().putString("key", key).apply();
        return k;
    }

    public static synchronized void deactivate(Context c) {
        c.getSharedPreferences(PREFS, 0).edit().remove("key").apply();
    }

    public static String make(String type, long exp, String dev) {
        String payload = "T|" + type + "|" + (exp / 1000) + "|" + dev;
        byte[] pb = payload.getBytes();
        String b32 = U.b32(pb);
        byte[] sig = U.hmacSha256(SECRET.getBytes(), b32.getBytes());
        String sig32 = U.b32(java.util.Arrays.copyOf(sig, 5)).substring(0, 8);
        return "VEIL-" + b32 + "-" + sig32;
    }

    private static Info verify(String key, String dev) {
        try {
            String[] parts = key.split("-");
            if (parts.length != 3 || !parts[0].equals("VEIL")) return null;
            byte[] sig = U.hmacSha256(SECRET.getBytes(), parts[1].getBytes());
            String expect = U.b32(java.util.Arrays.copyOf(sig, 5)).substring(0, 8);
            if (!expect.equals(parts[2])) return null;
            String payload = new String(U.b32decode(parts[1]));
            String[] f = payload.split("\\|");
            if (f.length != 4 || !f[0].equals("T")) return null;
            if (!f[3].equals(dev) && !f[3].equals("00000000")) return null;
            Info in = new Info();
            in.valid = true;
            in.type = f[1];
            in.exp = Long.parseLong(f[2]) * 1000L;
            in.device = dev;
            return in;
        } catch (Throwable t) { return null; }
    }
}
