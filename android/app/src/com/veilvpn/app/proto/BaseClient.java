package com.veilvpn.app.proto;

import com.veilvpn.app.Node;

/** Base for protocol clients holding the node. */
public abstract class BaseClient implements Client {
    protected Node n;
    public BaseClient(Node n) { this.n = n; }
}
