## Fabric Doctor / Production Preflight

Added a standalone production-readiness and support-bundle utility for Fabric hosts.

### Features
- KVM/CPU/TUN/vhost/cgroup/networking/storage/time/LSM checks
- Fabric API and FluxVM reachability checks
- optional strict production-service mode
- human and versioned JSON output
- safe-by-default support bundle
- opt-in redacted config and journal collection
- Linux amd64 + arm64 cross-build CI
- unit, race, vet, formatting and smoke-test gates
