package com.mehrveil.app.proto;

import com.mehrveil.app.Node;

/** Base for protocol clients holding the node. */
public abstract class BaseClient implements Client {
    protected Node n;
    public BaseClient(Node n) { this.n = n; }
}
