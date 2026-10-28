# Public deployment hardening (EVM RPC, Stratum dashboard, DNS seeder)

Status 2026-06-27. This guide covers exposing MISAKA's operator-facing network surfaces to
untrusted networks. The node ships **fail-closed**: every public-facing listener binds to
loopback by default and refuses (or warns about) a non-loopback bind unless you opt in
explicitly. Opting in does **not** add authentication — you are expected to put a TLS +
auth + rate-limiting reverse proxy (or a private network / firewall) in front.

> Rule of thumb: if a listener is reachable from the internet, it is in front of either a
> reverse proxy you control or a cloud firewall allow-list — never raw.

## 1. Ethereum JSON-RPC (`--evm-rpc-listen`, EVM builds only)

- **Default:** loopback (`127.0.0.1:8545`). Unmodified Foundry/Hardhat/ethers/viem/MetaMask
  connect locally — see `../connecting-ethereum-tooling.md`.
- **Fail-closed:** a **non-loopback** bind makes the node **refuse to start** unless
  `MISAKA_ALLOW_PUBLIC_EVM_RPC=1` is set (`kaspad/src/daemon.rs`). A stray `0.0.0.0` cannot
  silently expose the RPC.
- **Threat model when public:** the adapter sends `Access-Control-Allow-Origin: *`
  (`rpc/eth/src/lib.rs`), so once reachable, **any web origin** can POST to it from a
  victim's browser. It has **no authentication of its own**. Treat a public EVM RPC as a
  fully untrusted, world-callable endpoint.
- **Required when exposing:**
  - Terminate TLS and authenticate at a reverse proxy (bearer token / mTLS / IP allow-list).
  - Rate-limit per IP (JSON-RPC batches amplify work).
  - Keep `--evm-rpc-listen` on `127.0.0.1` and let only the proxy reach it; set
    `MISAKA_ALLOW_PUBLIC_EVM_RPC=1` **only** if the node itself must bind a routable address.
  - Consider method allow-listing at the proxy (e.g. drop `debug_*`/`trace_*` for public users).

## 2. Stratum bridge dashboard / config API

- **Default:** the dashboard binds loopback; a public bind requires
  `RKSTRATUM_ALLOW_PUBLIC_DASHBOARD=1`.
- **Config writes** (`POST /api/config`) are **disabled** unless `RKSTRATUM_ALLOW_CONFIG_WRITE=1`.
  Never enable config-write on a public-facing dashboard.
- The HTTP server already enforces a global connection cap, a per-IP cap, request-body caps,
  and an `Origin`/`Referer` check; static assets are served only from the vendored
  `bridge/static` tree (path-traversal is rejected via component validation). These are
  defense-in-depth, **not** a substitute for auth — front the dashboard with proxy auth and
  expose `/metrics` only to your monitoring network.

## 3. DNS seeder (`misaka-dnsseeder`, UDP+TCP :53)

- The seeder is **meant** to be public (it answers `A` queries for live peers) and serves
  only publicly-routable peer IPs (bogon Sybil entries are dropped).
- The TCP face is bounded: a global concurrent-connection cap, a per-connection read/write
  deadline, and a 4096-byte message cap (`misaka-dnsseeder/src/main.rs`) defeat
  slowloris / FD-exhaustion. Still place it behind a cloud firewall / DDoS layer if the host
  runs other services, and run it as an unprivileged user with
  `cap_net_bind_service` rather than root.

## 4. Reverse-proxy examples

### nginx (TLS + bearer token + rate limit) for the EVM RPC

```nginx
limit_req_zone $binary_remote_addr zone=evmrpc:10m rate=20r/s;

server {
    listen 443 ssl;
    server_name rpc.example.com;
    ssl_certificate     /etc/letsencrypt/live/rpc.example.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/rpc.example.com/privkey.pem;

    location / {
        # Require a shared secret; reject browsers' preflight-less cross-origin abuse here.
        if ($http_authorization != "Bearer REPLACE_WITH_LONG_RANDOM_TOKEN") { return 401; }
        limit_req zone=evmrpc burst=40 nodelay;
        client_max_body_size 1m;
        proxy_pass http://127.0.0.1:8545;   # node stays loopback-only
    }
}
```

### Caddy (TLS + basic auth) for the dashboard

```caddy
dash.example.com {
    basicauth { admin JDJhJDE0... }   # bcrypt hash from `caddy hash-password`
    reverse_proxy 127.0.0.1:<dashboard-port>
}
```

## 5. Firewall / network

- Bind node admin surfaces to `127.0.0.1`; expose only `:443` (proxy) and the P2P port.
- Use a cloud firewall / security group to allow-list who can reach the proxy and `/metrics`.
- Never expose gRPC/wRPC admin RPC publicly without a proxy and auth.

## Quick checklist

- [ ] EVM RPC stays on `127.0.0.1`; public reach is only via a TLS+auth+rate-limited proxy.
- [ ] `MISAKA_ALLOW_PUBLIC_EVM_RPC` set **only** if the node must bind a routable address.
- [ ] Dashboard behind proxy auth; `RKSTRATUM_ALLOW_CONFIG_WRITE` **unset** in production.
- [ ] `/metrics` restricted to the monitoring network.
- [ ] DNS seeder runs unprivileged (`cap_net_bind_service`), fronted by a DDoS layer if shared.
- [ ] Cloud firewall allow-lists admin/proxy ports; only `:443` + P2P are world-reachable.
