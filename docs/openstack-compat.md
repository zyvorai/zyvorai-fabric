# OpenStack Compatibility API

Fabric already manages VMs, networks, volumes, images, and identity through
its native `/api/v1` surface. The `openstack-compat` crate exposes those
concepts on the **OpenStack wire protocol** so `openstack`, Terraform
`openstack_*` resources, and Ansible `os_*` modules can drive Fabric.

It does **not** replace FluxVM. It is a façade mounted on the same
`zyvor-fabricd` process and listen port as the Fabric API and web UI.

**Hands-on walkthrough:** [Tutorial 08 — Drive Fabric with OpenStack Clients](tutorials/08-openstack-clients.md).

## Included

- `backend/openstack-compat/` — Keystone v3, Nova v2.1, Glance v2, Neutron
  v2.0, and Cinder v3 routers plus an in-memory `Cloud` store for day-one
  CLI/Terraform smoke tests.
- `backend/zyvor-fabricd/src/api/openstack.rs` — thin mount helper.
- Nest in `server.rs`: `/identity`, `/compute`, `/image`, `/network`,
  `/volume` (alongside `/api`, `/scim/v2`, `/ws`).

## Behavior

1. Clients authenticate against Keystone-compat at `/identity/v3/auth/tokens`.
2. The service catalog returns endpoints under the daemon's **public base URL**
   (see [Public URL](#public-url)).
3. Compute / image / network / volume calls use OpenStack JSON envelopes
   (`{"server":…}`, `{"flavors":…}`, etc.).
4. Default seeds: flavors `m1.tiny` … `m1.xlarge`, image `cirros`, network
   `private` (`10.0.0.0/24`).

v0 keeps its own in-memory cloud so the façade can be tested without calling
FluxVM. The next slice should implement `Cloud` methods via `AppState.driver`
and Fabric storage APIs.

## Endpoints

Mounted at the daemon root (not under `/api`):

| Service | Path prefix | OpenStack equivalent |
|---------|-------------|----------------------|
| Keystone v3 | `/identity/v3` | tokens, catalog, projects, users |
| Nova v2.1 | `/compute/v2.1` | flavors, servers, start/stop/reboot |
| Glance v2 | `/image/v2` | images |
| Neutron v2.0 | `/network/v2.0` | networks, subnets, ports |
| Cinder v3 | `/volume/v3/{project_id}` | volumes, attach/detach |

Cinder routes require a project id path segment (for example
`/volume/v3/admin/volumes`), matching common OpenStack clients.

### Not the same as SCIM

| Surface | Path | Purpose |
|---------|------|---------|
| Fabric SCIM | `/scim/v2`, `/api/v1/identity/scim/*` | Enterprise IdP user/group provisioning |
| OpenStack Keystone-compat | `/identity/v3` | OpenStack client auth + catalog |

See [scim-identity.md](scim-identity.md) for SCIM.

## Public URL

Catalog endpoints must match the URL clients actually call. Resolution order:

1. `ZYVOR_FABRICD_PUBLIC_URL` (env)
2. `daemon.public_url` in TOML
3. Derived from `daemon.listen` + TLS (`https` when TLS is enabled; `0.0.0.0` /
   `::` become `127.0.0.1`)

```toml
[daemon]
listen = "0.0.0.0:9095"
public_url = "https://fabric.example.com:9095"
```

```bash
export ZYVOR_FABRICD_LISTEN=0.0.0.0:9095
export ZYVOR_FABRICD_PUBLIC_URL=https://fabric.example.com:9095
```

Remote bare-metal deploy (`./scripts/deploy remote USER@HOST`) sets
`public_url=https://HOST:9095` when binding `0.0.0.0`. For Kubernetes
NodePort, set `ZYVOR_FABRICD_PUBLIC_URL=http://NODE_IP:30095` (or your
ingress URL).

Listen port defaults to **9095**, not 8080. Override with `daemon.listen` or
`ZYVOR_FABRICD_LISTEN`.

## Talk to it

```bash
export OS_AUTH_URL=https://127.0.0.1:9095/identity
export OS_IDENTITY_API_VERSION=3
export OS_USERNAME=admin
export OS_PASSWORD=any
export OS_PROJECT_NAME=admin
export OS_USER_DOMAIN_NAME=Default
export OS_PROJECT_DOMAIN_NAME=Default
# Self-signed lab certs:
export OS_INSECURE=true

openstack token issue
openstack flavor list
openstack server create --flavor m1.tiny --image cirros demo
openstack network list
openstack volume create --size 1 data
```

With curl (lab TLS):

```bash
BASE=https://127.0.0.1:9095
curl -sk -D /tmp/os-hdrs -o /tmp/tok.json -X POST "$BASE/identity/v3/auth/tokens" \
  -H 'Content-Type: application/json' \
  -d '{"auth":{"identity":{"methods":["password"],"password":{"user":{"name":"admin","password":"any"}}},"scope":{"project":{"name":"admin"}}}}'
TOKEN=$(awk -F': ' 'tolower($1)=="x-subject-token"{print $2}' /tmp/os-hdrs | tr -d '\r')
curl -sk "$BASE/compute/v2.1/flavors" -H "X-Auth-Token: $TOKEN"
```

## Mapping

| OpenStack | Fabric (intended) |
|-----------|-------------------|
| Keystone project | Fabric tenant |
| Nova server | FluxVM / `POST /api/v1/vms` |
| Nova flavor | CPU + memory + disk preset |
| Glance image | Content library / images API |
| Neutron network/subnet/port | Fabric networking |
| Cinder volume | Fabric volume + attach |

## Limitations (experimental)

Treat this as an **experimental compatibility surface**, not “Fabric is
OpenStack.”

- Password auth accepts any password in the drop-in so the CLI works on day
  one. Wire `issue_token` to Fabric `enterprise-identity` / JWT for production.
- Most mutating routes do not yet require a live token (catalog does).
- Tokens write `expires_at` but lookup does not yet enforce expiry.
- In-memory store: `os-stop` flips a string; FluxVM is unchanged until `Cloud`
  calls the real driver.
- Gaps vs full OpenStack clients: flavor show by id, keypairs, project-prefixed
  Nova paths, microversions, Glance image upload, security groups, floating IPs.

## Tests

```bash
cd backend
cargo test -p openstack-compat
```

## See also

- [Tutorial 08: OpenStack clients](tutorials/08-openstack-clients.md) — step-by-step CLI/curl
- [api.md](api.md) — OpenStack section in the API reference
- [getting-started/03-Configuration.md](getting-started/03-Configuration.md) — `listen` / `public_url`
- [scim-identity.md](scim-identity.md) — enterprise SCIM (separate from Keystone-compat)
- [DOCKER.md](DOCKER.md) / [KUBERNETES.md](KUBERNETES.md) — port and public URL overrides
