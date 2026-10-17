package com.veilvpn.app.net;

import java.io.IOException;
import java.net.InetSocketAddress;
import java.net.Socket;

import javax.net.ssl.SSLSocket;
import javax.net.ssl.SSLSocketFactory;

/** TCP/TLS socket helpers. TLS via standard SSLSocketFactory (system CAs + SNI). */
public final class Tls {
    public static Socket tcp(String host, int port, int timeoutMs) throws IOException {
        Socket s = new Socket();
        try { s.setTcpNoDelay(true); } catch (Throwable ignored) {}
        s.connect(new InetSocketAddress(host, port), timeoutMs);
        s.setSoTimeout(30_000);
        return s;
    }

    public static Socket tls(String host, int port, int timeoutMs, String sni) throws IOException {
        Socket plain = tcp(host, port, timeoutMs);
        try {
            SSLSocketFactory f = (SSLSocketFactory) SSLSocketFactory.getDefault();
            SSLSocket ssl = (SSLSocket) f.createSocket(plain, sni != null && !sni.isEmpty() ? sni : host, port, true);
            ssl.setSoTimeout(30_000);
            ssl.startHandshake();
            return ssl;
        } catch (IOException e) {
            try { plain.close(); } catch (Throwable ignored) {}
            throw e;
        }
    }
}
