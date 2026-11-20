# Upgrade / rollback (N → N+1)

Issue #16. Contract implemented by [`scripts/upgrade-rollback.sh`](../../scripts/upgrade-rollback.sh).

## What is covered

| Area | Snapshot | Rollback |
|------|----------|----------|
| State store (`/var/lib/zyvor-fabricd/`) | tar.gz | unpack |
| SQLite / JSON files in state | included in state tar | included |
| Config (`/etc/zyvor-fabricd/`) | copied | restored |
| Auth secrets (`.jwt_secret`, `.admin_password`) | copied with mode 0600 | restored |
| Helm values / CRDs | recorded as versions file | operator must `helm rollback` separately |
| FluxVM binary / images | version pin recorded | **not** auto-rolled; see notes |

## Procedure

```bash
# 1. Snapshot current install (N)
sudo ./scripts/upgrade-rollback.sh snapshot --tag before-0.3.0

# 2. Install N+1 (package, make install, or helm upgrade)
# 3. Verify
./scripts/upgrade-rollback.sh verify --base-url http://127.0.0.1:9095

# 4. On failure
sudo ./scripts/upgrade-rollback.sh rollback --tag before-0.3.0
./scripts/upgrade-rollback.sh verify --base-url http://127.0.0.1:9095
```

Dry-run (no root, temp dirs) is what CI and `scripts/test-upgrade-rollback.sh` execute:

```bash
./scripts/upgrade-rollback.sh snapshot --state-dir "$PWD/tmp/state" --config-dir "$PWD/tmp/cfg" --backup-root "$PWD/tmp/snaps" --tag ci
./scripts/upgrade-rollback.sh rollback --state-dir "$PWD/tmp/state" --config-dir "$PWD/tmp/cfg" --backup-root "$PWD/tmp/snaps" --tag ci
```

## Compatibility notes

- **State / SQLite**: forward compatible within 0.2.x. Always snapshot before crossing a minor.
- **API**: `/health` and `/readyz` must stay on the same paths.
- **CRDs**: operator chart bumps are not rolled back by this script — use `helm rollback zyvor-fabricd-operator`.
- **Auth**: JWT secret restore invalidates tokens issued by N+1; that is intended.
- **FluxVM**: if N+1 requires a newer FluxVM schema, roll FluxVM back first, then fabricd.
