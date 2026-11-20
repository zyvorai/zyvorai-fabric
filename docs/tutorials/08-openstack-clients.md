# Tutorial 08: Drive Fabric with OpenStack Clients

Use the standard `openstack` CLI (and curl) against Zyvor Fabric’s OpenStack
compatibility façade. Fabric stays the control plane; OpenStack is only the
wire dialect on the **same** daemon port as the Fabric API and web UI.

**Level:** Beginner  
**Time:** 25 minutes  
**Prerequisites:** Fabric running (bare metal, Docker, or lab deploy), `curl`,
`jq`. Optional: [python-openstackclient](https://docs.openstack.org/python-openstackclient/).

> **Experimental.** Today’s façade uses an in-memory cloud for smoke tests —
> `openstack server create` does **not** yet launch a FluxVM guest. Use this
> tutorial to wire clients and validate the catalog; native VM lifecycle still
> goes through `/api/v1` or the web UI. See
> [openstack-compat.md](../openstack-compat.md).

---

## What You Will Learn

1. Point OpenStack clients at Fabric (`OS_AUTH_URL`, public URL)
2. Issue a Keystone token and read the service catalog
3. List flavors and create/stop/start/delete a Nova-style server
4. Create Glance images, Neutron networks, and Cinder volumes
5. Know which paths are Fabric-native vs OpenStack-compat vs SCIM

---

## Architecture (one port)

```
  openstack CLI / Terraform openstack_* / Ansible os_*
                         │
         OS_AUTH_URL = https://HOST:9095/identity
                         │
    ┌────────────────────┴────────────────────┐
    │              zyvor-fabricd              │
    │  /identity  /compute  /image  /network  │
    │  /volume    /api/v1   /scim/v2   /ws    │
    └────────────────────┬────────────────────┘
                         │
                      FluxVM (native /api only today)
```

Default listen port is **9095** (HTTPS with a self-signed cert in lab deploys).

---

## Step 0: Confirm Fabric is up

Replace the host with yours (never commit real lab IPs).

```bash
export FABRIC_HOST="https://127.0.0.1:9095"
# Remote:
# export FABRIC_HOST="https://<host>:9095"

curl -skf "$FABRIC_HOST/health" && echo OK
curl -skf "$FABRIC_HOST/readyz" | jq '{ok, store, fluxvm_ok: .fluxvm.ok}'
```

If health fails, start or deploy Fabric first:

```bash
# Local (Linux)
cd backend && cargo run --bin zyvor-fabricd

# Remote lab
FABRIC_LAB_DEFAULTS=1 ./scripts/deploy remote USER@HOST --quick
```

### Public URL (required for remote clients)

The OpenStack **service catalog** advertises absolute URLs. They must match how
you reach the daemon.

```toml
# /etc/zyvor-fabricd/zyvor-fabricd.toml
[daemon]
listen = "0.0.0.0:9095"
public_url = "https://YOUR_HOST:9095"
```

Or:

```bash
export ZYVOR_FABRICD_PUBLIC_URL="https://YOUR_HOST:9095"
export ZYVOR_FABRICD_LISTEN="0.0.0.0:9095"
sudo systemctl restart zyvor-fabricd
```

`./scripts/deploy remote USER@HOST` sets `public_url=https://HOST:9095` for you.

---

## Step 1: Configure the OpenStack CLI

```bash
export OS_AUTH_URL="${FABRIC_HOST}/identity"
export OS_IDENTITY_API_VERSION=3
export OS_USERNAME=admin
export OS_PASSWORD=any          # façade accepts any password in v0
export OS_PROJECT_NAME=admin
export OS_USER_DOMAIN_NAME=Default
export OS_PROJECT_DOMAIN_NAME=Default
export OS_INSECURE=true         # lab self-signed TLS
```

Install the client if needed:

```bash
pip install python-openstackclient
# or: apt install python3-openstackclient
```

Sanity check:

```bash
openstack token issue
openstack catalog list
```

You should see endpoints under `${FABRIC_HOST}/identity`, `/compute/v2.1`,
`/image`, `/network`, `/volume/v3`.

---

## Step 2: Keystone with curl (no CLI)

Useful when debugging without python-openstackclient.

```bash
curl -sk -D /tmp/os-hdrs -o /tmp/tok.json -X POST \
  "$FABRIC_HOST/identity/v3/auth/tokens" \
  -H 'Content-Type: application/json' \
  -d '{
    "auth": {
      "identity": {
        "methods": ["password"],
        "password": {
          "user": {
            "name": "admin",
            "domain": {"name": "Default"},
            "password": "any"
          }
        }
      },
      "scope": {
        "project": {
          "name": "admin",
          "domain": {"name": "Default"}
        }
      }
    }
  }'

export OS_TOKEN=$(awk -F': ' 'tolower($1)=="x-subject-token"{print $2}' /tmp/os-hdrs | tr -d '\r')
echo "Token: ${OS_TOKEN:0:16}..."

jq '.token.catalog[].endpoints[0].url' /tmp/tok.json

# Catalog requires a live token
curl -sk "$FABRIC_HOST/identity/v3/auth/catalog" \
  -H "X-Auth-Token: $OS_TOKEN" | jq .
```

Also available:

```bash
curl -sk "$FABRIC_HOST/identity/v3/projects" -H "X-Auth-Token: $OS_TOKEN" | jq .
curl -sk "$FABRIC_HOST/identity/v3/users" -H "X-Auth-Token: $OS_TOKEN" | jq .
```

---

## Step 3: Nova — flavors and servers

### List flavors

```bash
openstack flavor list
# or
curl -sk "$FABRIC_HOST/compute/v2.1/flavors/detail" \
  -H "X-Auth-Token: $OS_TOKEN" | jq '.flavors[] | {name,vcpus,ram,disk}'
```

Defaults: `m1.tiny` … `m1.xlarge`. Show one:

```bash
curl -sk "$FABRIC_HOST/compute/v2.1/flavors/m1.tiny" \
  -H "X-Auth-Token: $OS_TOKEN" | jq .
```

### Create a server

```bash
openstack server create --flavor m1.tiny --image cirros demo

# or curl
curl -sk -X POST "$FABRIC_HOST/compute/v2.1/servers" \
  -H "X-Auth-Token: $OS_TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{
    "server": {
      "name": "demo",
      "flavorRef": "m1.tiny",
      "imageRef": "cirros"
    }
  }' | jq .
```

Save the server id:

```bash
export SERVER_ID=$(openstack server list -f value -c ID | head -1)
# or from jq: .server.id
```

### Lifecycle actions

```bash
openstack server stop "$SERVER_ID"
openstack server start "$SERVER_ID"
openstack server reboot --soft "$SERVER_ID"

# curl equivalents
curl -sk -X POST "$FABRIC_HOST/compute/v2.1/servers/$SERVER_ID/action" \
  -H "X-Auth-Token: $OS_TOKEN" -H 'Content-Type: application/json' \
  -d '{"os-stop": null}'

curl -sk -X POST "$FABRIC_HOST/compute/v2.1/servers/$SERVER_ID/action" \
  -H "X-Auth-Token: $OS_TOKEN" -H 'Content-Type: application/json' \
  -d '{"os-start": null}'

curl -sk -X POST "$FABRIC_HOST/compute/v2.1/servers/$SERVER_ID/action" \
  -H "X-Auth-Token: $OS_TOKEN" -H 'Content-Type: application/json' \
  -d '{"reboot": {"type": "SOFT"}}'

curl -sk -X POST "$FABRIC_HOST/compute/v2.1/servers/$SERVER_ID/action" \
  -H "X-Auth-Token: $OS_TOKEN" -H 'Content-Type: application/json' \
  -d '{"pause": null}'

curl -sk -X POST "$FABRIC_HOST/compute/v2.1/servers/$SERVER_ID/action" \
  -H "X-Auth-Token: $OS_TOKEN" -H 'Content-Type: application/json' \
  -d '{"unpause": null}'
```

### List / show / delete

```bash
openstack server list
openstack server show "$SERVER_ID"
openstack server delete "$SERVER_ID"
```

---

## Step 4: Glance — images

```bash
openstack image list

curl -sk -X POST "$FABRIC_HOST/image/v2/images" \
  -H "X-Auth-Token: $OS_TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{
    "name": "my-image",
    "disk_format": "qcow2",
    "container_format": "bare"
  }' | jq .

export IMAGE_ID=$(curl -sk "$FABRIC_HOST/image/v2/images" \
  -H "X-Auth-Token: $OS_TOKEN" | jq -r '.images[] | select(.name=="my-image") | .id')

curl -sk "$FABRIC_HOST/image/v2/images/$IMAGE_ID" \
  -H "X-Auth-Token: $OS_TOKEN" | jq .

curl -sk -X DELETE "$FABRIC_HOST/image/v2/images/$IMAGE_ID" \
  -H "X-Auth-Token: $OS_TOKEN" -w "%{http_code}\n"
```

Seeded image name: `cirros`. Binary upload (`PUT …/file`) is not implemented yet.

---

## Step 5: Neutron — networks, subnets, ports

```bash
openstack network list
openstack subnet list

# Create a network + subnet + port
NET=$(curl -sk -X POST "$FABRIC_HOST/network/v2.0/networks" \
  -H "X-Auth-Token: $OS_TOKEN" -H 'Content-Type: application/json' \
  -d '{"network":{"name":"tutorial-net"}}')
echo "$NET" | jq .
export NET_ID=$(echo "$NET" | jq -r '.network.id')

curl -sk -X POST "$FABRIC_HOST/network/v2.0/subnets" \
  -H "X-Auth-Token: $OS_TOKEN" -H 'Content-Type: application/json' \
  -d "{
    \"subnet\": {
      \"name\": \"tutorial-subnet\",
      \"network_id\": \"$NET_ID\",
      \"cidr\": \"10.99.0.0/24\",
      \"ip_version\": 4
    }
  }" | jq .

curl -sk -X POST "$FABRIC_HOST/network/v2.0/ports" \
  -H "X-Auth-Token: $OS_TOKEN" -H 'Content-Type: application/json' \
  -d "{
    \"port\": {
      \"name\": \"tutorial-port\",
      \"network_id\": \"$NET_ID\"
    }
  }" | jq .
```

Default seed: network `private` (`10.0.0.0/24`).

---

## Step 6: Cinder — volumes (project id in path)

Cinder URLs include a **project id** segment (OpenStack convention):

```text
/volume/v3/{project_id}/volumes
```

Use `admin` (or any string the façade accepts as project id):

```bash
export OS_PROJECT_ID=admin

openstack volume list
openstack volume create --size 1 data

# curl
curl -sk -X POST "$FABRIC_HOST/volume/v3/${OS_PROJECT_ID}/volumes" \
  -H "X-Auth-Token: $OS_TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{"volume":{"size":1,"name":"data"}}' | jq .

export VOL_ID=$(curl -sk "$FABRIC_HOST/volume/v3/${OS_PROJECT_ID}/volumes" \
  -H "X-Auth-Token: $OS_TOKEN" | jq -r '.volumes[] | select(.name=="data") | .id')

# Attach / detach to a Nova server id
curl -sk -X POST "$FABRIC_HOST/volume/v3/${OS_PROJECT_ID}/volumes/$VOL_ID/action" \
  -H "X-Auth-Token: $OS_TOKEN" -H 'Content-Type: application/json' \
  -d "{\"os-attach\":{\"instance_uuid\":\"$SERVER_ID\"}}"

curl -sk -X POST "$FABRIC_HOST/volume/v3/${OS_PROJECT_ID}/volumes/$VOL_ID/action" \
  -H "X-Auth-Token: $OS_TOKEN" -H 'Content-Type: application/json' \
  -d '{"os-detach":null}'
```

> Paths like `/volume/v3/volumes` (no project id) are **not** valid OpenStack
> Cinder routes on Fabric; the web UI may return HTML 200 (SPA fallback). Always
> include `{project_id}`.

---

## Step 7: Terraform / Ansible (outline)

### Terraform

```hcl
provider "openstack" {
  auth_url    = "https://YOUR_HOST:9095/identity/v3"
  user_name   = "admin"
  password    = "any"
  tenant_name = "admin"
  region      = "RegionOne"
  insecure    = true
}

resource "openstack_compute_instance_v2" "demo" {
  name      = "tf-demo"
  flavor_id = "m1.tiny"
  image_id  = "cirros"
}
```

Expect gaps vs full OpenStack (keypairs, microversions, Glance upload). Prefer
Fabric’s native Terraform provider for real VMs today:
[terraform-provider](../../terraform-provider/README.md).

### Ansible

```yaml
- hosts: localhost
  tasks:
    - name: List flavors via OpenStack modules
      openstack.cloud.compute_flavor_info:
        auth:
          auth_url: "https://YOUR_HOST:9095/identity"
          username: admin
          password: any
          project_name: admin
          user_domain_name: Default
          project_domain_name: Default
        validate_certs: false
```

---

## Step 8: Don’t confuse these three surfaces

| Goal | Use |
|------|-----|
| Real Fabric VMs / FluxVM | `/api/v1/vms`, web UI, `fabricctl` |
| OpenStack CLI / OS Terraform | `/identity`, `/compute`, … (this tutorial) |
| Entra ID / Okta user provisioning | `/scim/v2` — [scim-identity.md](../scim-identity.md) |

Fabric login (JWT) is separate from OpenStack tokens:

```bash
# Fabric JWT (dashboard /api)
curl -sk -X POST "$FABRIC_HOST/api/v1/auth/login" \
  -H 'Content-Type: application/json' \
  -d '{"username":"admin","password":"Admin@321"}' | jq -r .token
```

---

## Troubleshooting

| Symptom | Fix |
|---------|-----|
| Catalog shows `127.0.0.1` but you call a remote IP | Set `daemon.public_url` / `ZYVOR_FABRICD_PUBLIC_URL` to the URL clients use |
| TLS certificate errors | Lab: `OS_INSECURE=true` or `curl -sk` |
| `openstack volume …` 404 / HTML | Use `/volume/v3/{project}/volumes` (include project id) |
| Server create “works” but no KVM guest | Expected in v0 — façade is in-memory; use `/api/v1/vms` for real VMs |
| Port wrong | Fabric default is **9095**, not 8080 |

Verify the crate in CI or on the host:

```bash
cd backend && cargo test -p openstack-compat
```

---

## What You Accomplished

- Configured `OS_*` env vars against Fabric  
- Issued a Keystone token and inspected the catalog  
- Exercised Nova / Glance / Neutron / Cinder via CLI or curl  
- Learned how public URL, TLS, and project-scoped Cinder paths work  

## Next Steps

1. Reference: [OpenStack Compatibility](../openstack-compat.md)  
2. Native first VM: [Tutorial 01](01-first-vm.md)  
3. Configuration: [daemon.listen / public_url](../getting-started/03-Configuration.md)  
4. Deploy: [Deploy](../deploy.md) · [KUBERNETES.md](../KUBERNETES.md)  
5. SCIM (enterprise IdP): [scim-identity.md](../scim-identity.md)  
