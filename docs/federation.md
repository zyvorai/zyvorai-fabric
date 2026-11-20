# Federation: one view across several Fabric daemons

Each Fabric daemon already models datacenters, clusters and hosts, with a maintenance mode per host. Federation adds a read-mostly
view across **separate daemons** (for example one per site). Console: **Federation** in the Core section.

## How it works

- An admin registers a peer: `POST /api/federation/peers` with `name`, `url` and an optional read token. The URL is reduced to its origin.
- `GET /api/federation/overview` asks every peer, in parallel with a 5-second limit, for `/api/v1/hosts` and the VM total, using that
  peer's token. A peer that is down, refuses the token, redirects or answers badly gets its own row (`unreachable`, `unauthorized`,
  `error`, `token_missing`); it never fails the others. Totals add up the reachable peers.
- **Maintenance is not done with a stored credential.** `POST /api/federation/peers/{id}/hosts/{host_id}/maintenance/{enter|exit}`
  forwards the caller's own `Authorization` header to the peer, so the peer's own RBAC decides. A cookie session cannot be forwarded
  and is refused; a peer that does not trust the caller's token answers 401/403 and that is shown.

## Rules

- Registering, changing a token, removing a peer and maintenance need the admin role; the list and overview need read.
- A peer URL must be `http` or `https`, with no user name, password, query or fragment, and not a link-local, unspecified or
  multicast literal address. Redirects are never followed. At most 64 peers.
- Peer tokens are kept in the secrets manager, which is **in memory**: after a restart a peer with a token shows `token_missing`
  until `PUT /api/federation/peers/{id}/token` sets it again. Tokens are never returned by any endpoint.

## Not done

Cross-cluster live migration, shared sign-in, configuration sync, persistent token storage, DNS-rebinding protection for peer
names (a name that resolves to a link-local address is not caught; only literal addresses are), and a check against two real daemons. The 5 Rust tests use stub peers on loopback.
