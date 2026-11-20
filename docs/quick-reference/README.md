# Quick Reference

This section provides quick-reference material for Zyvor Fabric operators and
developers.

---

## Documents

| Document                                      | Description                                    |
|-----------------------------------------------|------------------------------------------------|
| [Quick Reference](quick-reference.md)         | One-page cheat sheet with essential API calls, common curl commands, configuration reference, and troubleshooting tips. |
| [Glossary](glossary.md)                       | Definitions of 100+ terms related to Zyvor Fabric, systemd, KVM, QEMU, cloud-init, and virtualization. |
| [FAQ](faq.md)                                 | Frequently asked questions about Zyvor Fabric architecture, capabilities, operations, and troubleshooting. |

---

## At a Glance

- **Default listen address**: `127.0.0.1:9095`
- **Config file**: `/etc/zyvor-fabricd/zyvor-fabricd.toml`
- **Data directory**: `/var/lib/zyvor-fabricd/`
- **API base path**: `/api/v1/`
- **OpenStack façade**: `/identity`, `/compute`, `/image`, `/network`, `/volume` (see [openstack-compat.md](../openstack-compat.md))
- **SCIM**: `/scim/v2`
- **Public URL env**: `ZYVOR_FABRICD_PUBLIC_URL`
- **Listen override**: `ZYVOR_FABRICD_LISTEN`
- **Metrics endpoint**: `/metrics`
- **WebSocket console**: `/api/v1/ws/{vm_name}/console`
- **SSE event stream**: `/api/v1/events/stream`
- **Log level env var**: `ZYVOR_FABRICD_LOG_LEVEL`
