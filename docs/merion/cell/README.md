# Agent cell

Untrusted agent runtime runs in a **Firecracker / FluxVM microVM**, not only
nspawn/bubblewrap on the host kernel.

- No raw secrets, no `CAP_NET_ADMIN`, no host filesystem.
- Admin plane is **vsock only** — no SSH to the agent.
- Optional `ttl_seconds` for throwaway research cells.
- Host network pin: FluxVM TC `deny_udp` + gateway-only broker/proxy ports
  ([confine.md](../confine.md)). Guest Chromium never enforces policy.
- Interim (fabric today): FluxVM sandbox + bubblewrap `InnerContainer::Strict`, with a seccomp syscall filter
  (x86_64 only: an arm64 guest with `strict` does not start). No Landlock layer, on purpose (see
  [REMAINING.md](../REMAINING.md)). Details: `agent-runtime/README.md`, "inner containment".
- Optional GPUs: `"gpus": N` in the agent manifest passes N free GPUs through to a QEMU cell (FluxVM picks them;
  needs `cell_backend: qemu`; not with `confidential`, a warm pool or hibernation). Tested against fakes only, not on a real GPU.
  Keep 0.1 target: Firecracker as the cell so escape still hits a hypervisor
  before the vault.
