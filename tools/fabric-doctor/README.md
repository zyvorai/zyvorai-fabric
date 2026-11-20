# Fabric Doctor

`fabric-doctor` is the production preflight and safe support-bundle utility for Zyvor Fabric.

It answers two operator questions before a VM host is admitted into service:

1. **Can this Linux host safely run Fabric/FluxVM workloads?**
2. **If something is wrong, can I collect a useful diagnostic bundle without casually leaking secrets?**

## Checks

- Linux host and effective privilege
- Intel VT-x / AMD-V CPU virtualization flag
- `/dev/kvm` existence and read/write access
- `kvm`, `kvm_intel` / `kvm_amd` modules
- `/dev/net/tun` and `vhost_net`
- cgroup v2
- `ip`, `nft`, `bridge`, `tc`
- QEMU presence (warning only because FluxVM may use another backend)
- Fabric data directory writability
- configurable minimum free space
- NTP/time synchronization
- active Linux Security Modules
- Fabric `/health` (liveness) and `/readyz` (readiness: store + FluxVM) reachability
- FluxVM TCP reachability and HTTP `/readyz` (state dir + dataplane when required)
- HTTPS certificate lifetime when the Fabric health URL uses TLS

Flags: `--fabric-url`, `--fabric-ready-url`, `--fluxvm-address`, `--fluxvm-ready-url`.

## Build

```bash
cd tools/fabric-doctor
make check
make build VERSION=v0.1.0
./bin/fabric-doctor check --skip-service-ping
```

The tool intentionally uses only the Go standard library, so there are no third-party runtime dependencies or Go module supply-chain additions.

## Usage

```bash
# Pre-install host validation
sudo fabric-doctor check --skip-service-ping

# Running production host; services are mandatory
sudo fabric-doctor check --strict-services --min-free-gib 50

# Automation / inventory systems
fabric-doctor check --output json

# Support bundle. Config contents are NOT included by default.
sudo fabric-doctor bundle

# Add a redacted config and recent service journal output
sudo fabric-doctor bundle --include-config --include-logs
```

Exit codes:

- `0`: no failed checks
- `1`: one or more failed checks
- `2`: invalid invocation or bundle/rendering error

Warnings do not fail the command, which makes pre-install checks useful before `zyvor-fabricd` or FluxVM are running. Use `--strict-services` on active production nodes.

## JSON contract

The report is versioned with `schema_version: "v1"`. Each check has a stable ID, category, status, message, optional remediation and duration.

```json
{
  "schema_version": "v1",
  "tool_version": "v0.1.0",
  "summary": {"passed": 12, "warned": 2, "failed": 0, "info": 0},
  "checks": [
    {
      "id": "compute.kvm_device",
      "category": "compute",
      "status": "pass",
      "message": "/dev/kvm exists and is accessible read/write",
      "duration_ms": 0
    }
  ]
}
```

## Support-bundle privacy model

By default the bundle includes:

- the doctor report
- OS/runtime summary
- CPU summary
- memory and filesystem usage
- interface and route state
- nftables state
- loaded kernel modules
- **config metadata only** (path, size, mode, modified time, SHA-256)

Raw config is excluded unless `--include-config` is supplied. When included, common password/token/JWT/API-key/Bearer/credential URL patterns are replaced with `[REDACTED]`. Journal output is excluded unless `--include-logs` is supplied and is passed through the same redactor.

Redaction is best-effort. Operators should inspect support archives before sharing them outside their organization.
