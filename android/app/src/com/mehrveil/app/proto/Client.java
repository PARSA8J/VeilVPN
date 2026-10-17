package com.mehrveil.app.proto;

import com.mehrveil.app.Node;

import java.io.InputStream;
import java.io.OutputStream;

/** Proxy protocol client. dial() returns established bidirectional channel to dst. */
public interface Client {
    class Channel {
        public final InputStream in;
        public final OutputStream out;
        public Channel(InputStream i, OutputStream o) { in = i; out = o; }
    }

    /** Establish a channel to dstHost:dstPort through the node. Throws on failure. */
    Channel dial(Node node, String dstHost, int dstPort) throws Exception;
}
