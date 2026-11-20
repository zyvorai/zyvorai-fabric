# Naming and clone URL

Zyvor Fabric is the product name. Technical identifiers use `zyvor-fabricd`.

| Item | Value |
|------|-------|
| Product | **Zyvor Fabric** |
| GitHub | [zyvorai/fabric](https://github.com/zyvorai/fabric) |
| systemd unit | `zyvor-fabricd.service` |
| Binary | `zyvor-fabricd`, `fabricctl` (was `zyvorctl` before 0.3.0), `zyvor-fabricd-ctl` |
| Config | `/etc/zyvor-fabricd/zyvor-fabricd.toml` |
| State | `/var/lib/zyvor-fabricd/` |
| Env vars | `ZYVOR_FABRICD_*` |
| Terraform | `zyvorai/zyvor-fabricd` |
| API paths | `/api/*` |

```bash
git clone https://github.com/zyvorai/fabric.git
cd fabric
```

`scripts/deploy-remote.sh` rsyncs to `~/zyvor-fabric` by default. Override with `DEPLOY_DIR` if needed.

See also: [POSITIONING.md](POSITIONING.md), [ARCHITECTURE.md](../ARCHITECTURE.md).
