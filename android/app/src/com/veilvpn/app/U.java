package com.veilvpn.app;

/** Small shared utilities. android.util.Base64 ONLY (minSdk 24 safe). */
public final class U {
    private static final char[] HEX = "0123456789abcdef".toCharArray();

    public static String hex(byte[] b) {
        if (b == null) return "";
        char[] out = new char[b.length * 2];
        for (int i = 0; i < b.length; i++) {
            int v = b[i] & 0xFF;
            out[i * 2] = HEX[v >>> 4];
            out[i * 2 + 1] = HEX[v & 0xF];
        }
        return new String(out);
    }

    public static byte[] unhex(String s) {
        int n = s.length() / 2;
        byte[] out = new byte[n];
        for (int i = 0; i < n; i++)
            out[i] = (byte) Integer.parseInt(s.substring(i * 2, i * 2 + 2), 16);
        return out;
    }

    public static byte[] b64decode(String s) {
        String t = s == null ? "" : s.trim().replace("-", "+").replace("_", "/");
        try { return android.util.Base64.decode(t, android.util.Base64.NO_WRAP); }
        catch (Throwable t1) {
            try { return android.util.Base64.decode(t, android.util.Base64.DEFAULT); }
            catch (Throwable t2) { return new byte[0]; }
        }
    }

    public static String b64encode(byte[] b) {
        try { return android.util.Base64.encodeToString(b, android.util.Base64.NO_WRAP); }
        catch (Throwable t) { return ""; }
    }

    public static String b32(byte[] b) {
        final String A = "ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
        StringBuilder sb = new StringBuilder();
        int bits = 0, val = 0;
        for (byte x : b) {
            val = (val << 8) | (x & 0xFF);
            bits += 8;
            while (bits >= 5) { sb.append(A.charAt((val >>> (bits - 5)) & 31)); bits -= 5; }
        }
        if (bits > 0) sb.append(A.charAt((val << (5 - bits)) & 31));
        return sb.toString();
    }

    public static byte[] b32decode(String s) {
        final String A = "ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
        StringBuilder bits = new StringBuilder();
        for (char c : s.toUpperCase().toCharArray()) {
            int i = A.indexOf(c);
            if (i < 0) continue;
            bits.append(String.format("%5s", Integer.toBinaryString(i)).replace(' ', '0'));
        }
        int n = bits.length() / 8;
        byte[] out = new byte[n];
        for (int i = 0; i < n; i++)
            out[i] = (byte) Integer.parseInt(bits.substring(i * 8, i * 8 + 8), 2);
        return out;
    }

    /** Random UUIDv4 string. */
    public static String uuid() {
        java.security.SecureRandom r = new java.security.SecureRandom();
        byte[] b = new byte[16];
        r.nextBytes(b);
        b[6] = (byte) ((b[6] & 0x0F) | 0x40);
        b[8] = (byte) ((b[8] & 0x3F) | 0x80);
        return hex(b[0], b[1], b[2], b[3]) + "-" + hex(b[4], b[5]) + "-" + hex(b[6], b[7])
                + "-" + hex(b[8], b[9]) + "-" + hex(b[10], b[11], b[12], b[13], b[14], b[15]);
    }

    private static String hex(int... parts) {
        StringBuilder sb = new StringBuilder();
        for (int p : parts) sb.append(String.format("%02x", p));
        return sb.toString();
    }

    public static String sha256hex(String s) {
        try {
            java.security.MessageDigest md = java.security.MessageDigest.getInstance("SHA-256");
            return hex(md.digest(s.getBytes("UTF-8")));
        } catch (Throwable t) { return "00000000"; }
    }

    public static byte[] hmacSha256(byte[] key, byte[] data) {
        try {
            javax.crypto.Mac mac = javax.crypto.Mac.getInstance("HmacSHA256");
            mac.init(new javax.crypto.spec.SecretKeySpec(key, "HmacSHA256"));
            return mac.doFinal(data);
        } catch (Throwable t) { return new byte[0]; }
    }
}
