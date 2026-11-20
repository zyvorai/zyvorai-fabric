# Zyvor Fabric Integrations

Third-party and companion products that connect to the Zyvor Fabric control plane (`zyvor-fabricd`).

| Integration | Status | Document |
|-------------|--------|----------|
| **OpenStack compatibility** | Experimental | [../openstack-compat.md](../openstack-compat.md) · [Tutorial](../tutorials/08-openstack-clients.md) |
| **Machina** (macOS AI infrastructure workbench) | Planned v0.1 | [machina.md](machina.md) |
| Kubernetes operator | Shipped | [../operator/README.md](../../operator/README.md) |
| Terraform | Shipped | [../terraform-provider/README.md](../../terraform-provider/README.md) |
| Ansible | Shipped | [../ansible/README.md](../../ansible/README.md) |
| Prometheus | Shipped | [../guides/operations/monitoring.md](../guides/operations/monitoring.md) |

Fabric's native API is documented in [api.md](../api.md) and
[backend/api-docs/openapi.yaml](../../backend/api-docs/openapi.yaml). The
OpenStack façade shares the same daemon listen port (`/identity`, `/compute`,
…) — see [openstack-compat.md](../openstack-compat.md).
