# Operating `ContainerGroup` (FluxVM Secure Containers)

A step-by-step walkthrough for standing up `ContainerGroup` end to end: node
setup on the FluxVM side, registering that host with Fabric, applying a
`ContainerGroup` spec, and verifying it actually landed. See
[FLUXVM-FABRIC-BOUNDARY.md](FLUXVM-FABRIC-BOUNDARY.md) for how the two
projects divide ownership of this workload, and the gap-analysis notes in
this repo's PR history (#97-#110) for the full production-readiness list
this feature has worked through.

`ContainerGroup` is still developer-preview: Secure Containers itself is
developer-preview upstream in FluxVM (QEMU-only, no live migration, no
conformance results published — see `docs/secure-containers.md` in the
[fluxvm](https://github.com/zyvorai/fluxvm) repo), and this walkthrough
assumes a single target Kubernetes cluster (v1 has no multi-site
resolution).

## 1. Prerequisites

- A Kubernetes cluster reachable from `zyvor-fabricd`, with `RuntimeClass
  fluxvm` and the `containerd-shim-fluxvm-v2` + guest image installed on
  every node that should run Secure Containers Pods.

### Install FluxVM Secure Containers shim (node)

On each capable node (see FluxVM `docs/secure-containers.md` /
`docs/secure-containers-set3.md`):

1. Run FluxVM `scripts/install-secure-containers.sh` (installs shim +
   RuntimeClass + guest image paths).
2. Confirm FluxVM `/readyz.secure_containers` reports:
   `{ "available": true, "shim_installed": true, "guest_image_present": true }`.
3. Fabric host-agent probes that object on each heartbeat and stores both
   `secure_containers_ready` and the nested detail for the Datacenters UI.

There is still no fleet-wide Helm automation for the shim — DaemonSet
install remains FluxVM's responsibility; Fabric only consumes readiness.

- A kubeconfig fabric can use to reach that cluster (a `ServiceAccount`
  token is the usual choice for a long-running daemon).

## 2. Enable `ContainerGroup` support in fabric

Off by default. In `zyvor-fabricd.toml`:

```toml
[container_groups]
enabled = true
kubeconfig_path = "/etc/zyvor-fabricd/container-groups.kubeconfig"
# namespace = "default"                 # default shown
# namespace_per_tenant = true           # default shown
```

`kubeconfig_path` unset falls back to the ambient in-cluster or default
kubeconfig context. With `namespace_per_tenant` on (the default), a
ContainerGroup with a `tenant` gets its own `{namespace}-{tenant}`
namespace, auto-created on first `apply`; an untenanted group always lands
in the shared `namespace`.

## 3. Register the host with fabric

Fabric places `ContainerGroup` Pods the same way it places VMs —
`predictive_drs::DrsManager` scoring `datacenter::HostInfo` entries — filtered
to hosts that have reported `secure_containers_ready`. A host has to exist in
fabric's datacenter/cluster/host inventory before it can be picked.

```bash
# 1. A datacenter
curl -s -X POST "$FABRIC_URL/api/datacenters" \
  -H "Authorization: Bearer $TOKEN" -H 'content-type: application/json' \
  -d '{"name": "dc1"}'

# 2. A cluster in it
curl -s -X POST "$FABRIC_URL/api/clusters" \
  -H "Authorization: Bearer $TOKEN" -H 'content-type: application/json' \
  -d '{"name": "cluster1", "datacenter_id": "<dc-id-from-step-1>"}'

# 3. The host itself, already flagged Secure-Containers-capable
curl -s -X POST "$FABRIC_URL/api/hosts" \
  -H "Authorization: Bearer $TOKEN" -H 'content-type: application/json' \
  -d '{
    "hostname": "node-1",
    "address": "10.0.0.11",
    "cluster_id": "<cluster-id-from-step-2>",
    "cpus": 16,
    "memory_mb": 65536,
    "agent_version": "1.0.0",
    "secure_containers_ready": true
  }'
```

A host that registered with `secure_containers_ready: false` (or omitted
it) can flip it later via its regular heartbeat:

```bash
curl -s -X POST "$FABRIC_URL/api/hosts/<host-id>/heartbeat" \
  -H "Authorization: Bearer $TOKEN" -H 'content-type: application/json' \
  -d '{
    "cpu_usage_pct": 12.5,
    "memory_usage_pct": 30.0,
    "vm_count": 0,
    "uptime_secs": 3600,
    "secure_containers_ready": true
  }'
```

Hosts that go quiet for too long are marked `NotResponding` by fabric's own
stale-host detector and drop out of placement automatically — no manual
deregistration needed for a host that's just temporarily down.

## 4. Apply a `ContainerGroup`

```bash
curl -s -X POST "$FABRIC_URL/api/container-groups/apply" \
  -H "Authorization: Bearer $TOKEN" -H 'content-type: application/json' \
  -d '{
    "name": "web",
    "replicas": 2,
    "containers": [{
      "name": "app",
      "image": "nginx:latest",
      "resources": {"cpus": 1, "memory": "512M"}
    }]
  }'
```

Or via `fabricctl` from a spec file:

```yaml
# web.yaml
name: web
replicas: 2
containers:
  - name: app
    image: nginx:latest
    resources:
      cpus: 1
      memory: 512M
```

```bash
fabricctl container-group apply -f web.yaml
```

A tenant-scoped, private-registry, health-checked, network-isolated group
looks like this (every field beyond `name`/`containers` is optional):

```json
{
  "name": "web",
  "replicas": 2,
  "tenant": "acme",
  "image_pull_secrets": ["acme-registry-creds"],
  "containers": [{
    "name": "app",
    "image": "registry.acme.internal/web:1.4.0",
    "resources": {"cpus": 1, "memory": "512M"},
    "readiness_probe": {"type": "http", "path": "/healthz", "port": 8080},
    "liveness_probe": {"type": "tcp", "port": 8080}
  }],
  "network_policy": {
    "ingress": [{"from_container_groups": ["frontend"], "ports": [8080]}]
  }
}
```

If the caller's JWT carries a `tenant` claim, it must match `tenant` here (or
the field can be left unset and fabric stamps it in automatically) — see
`tenant_scope::apply_create_tenant`. A quota scoped to that tenant
(`POST /api/quotas` with `"tenant": "acme"`, admin-only) is enforced before
any Pod gets created; see [quotas below](#quotas-and-billing).

## 5. Verify it landed

```bash
fabricctl container-group list
fabricctl container-group info web
fabricctl container-group events   # audit trail: created/applied/deleted/quota_exceeded/placement_failed
```

Or directly against Kubernetes, in whichever namespace step 2 resolved to
(the shared `namespace`, or `{namespace}-{tenant}` if tenant-scoped):

```bash
kubectl get pods -n default -l fabric.zyvor.dev/container-group=web
```

Each Pod's `spec.nodeName` is pinned to the host fabric placed it on; Pod
creation itself goes straight to the Kubernetes API (`k8s-pod-client`),
never to FluxVM directly — FluxVM's shim and guest-agent only get invoked by
`kubelet`/`containerd` on that node via the normal CRI path once the Pod
lands.

## Troubleshooting

| Symptom | Likely cause |
| --- | --- |
| `503 ContainerGroup support is not enabled/configured` | `container_groups.enabled = false`, or the kubeconfig can't reach the cluster. |
| `400 no Secure-Containers-capable host is currently connected` | No registered host has `secure_containers_ready: true` and `status: Connected` — see step 3. |
| `403 token tenant '...' cannot create...` | The spec's `tenant` doesn't match the caller's JWT `tenant` claim. |
| `403 Quota '...' would be exceeded` | A quota scoped to this tenant/tag doesn't have room for the requested CPU/memory/replica count — see `GET /api/quotas/usage`. |
| Pods created but never reach `Running` | Check `kubectl describe pod` on the target node — this is almost always a Secure Containers/shim-level issue on the FluxVM side (missing `RuntimeClass`, shim not installed, guest image path), not a fabric placement problem. |

## Quotas and billing

Optional but recommended for a shared/multi-tenant cluster. Create an
admin-only quota scoped to a tenant:

```bash
curl -s -X POST "$FABRIC_URL/api/quotas" \
  -H "Authorization: Bearer $ADMIN_TOKEN" -H 'content-type: application/json' \
  -d '{
    "name": "acme-quota",
    "tenant": "acme",
    "max_cpus": 64,
    "max_memory": 131072,
    "max_disk": 2000,
    "max_vms": 20,
    "max_containers": 40
  }'
```

CPU/memory form one shared budget across a tenant's VMs and ContainerGroups;
`max_containers` is a separate cap on total ContainerGroup replica count.
Usage aggregates into the same tenant's invoice via
`POST /api/billing/invoice/{tenant_id}`.

## Backups

ContainerGroup volumes are hostPath bind mounts, not a VM disk image, so
backup/restore works on the mounted directories directly rather than
`qemu-img`:

```bash
fabricctl container-group backup create web --retention-days 30
fabricctl container-group backup list
fabricctl container-group backup restore <backup-id>
```

Restore extracts the archive back onto the exact host paths it was taken
from; it does not stop/start the group's Pods around the restore, so
coordinate that yourself if the target paths are actively mounted by a
running Pod.

## What's deliberately not here yet

- Multi-site placement, horizontal autoscaling, and `AffinityRule`
  cross-referencing (placement only understands `node_hint` today).
- Nested readiness is probed by host-agent from FluxVM `/readyz` and shown
  on the Datacenters host table; fleet-wide shim install automation still
  lives in FluxVM, not Fabric charts.
- Live Pod phase/node is exposed at `GET /api/container-groups/{name}/status`
  and in the Container Groups UI; Kubernetes Event objects are not mirrored
  yet (Fabric audit events cover apply/delete).
- A CRD-native path for backup/quota management — those stay Fabric REST/CLI
  operations, not part of the `ContainerGroup` custom resource the operator
  reconciles (see the Helm chart under `operator/charts/`).
- Replacing the FluxVM Secure Containers shim or guest image pipeline.