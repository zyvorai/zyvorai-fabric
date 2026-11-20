# Kernel

## Purpose

Kernel — a snapshot of the host's kernel configuration: version, hostname, architecture, boot command line, loaded kernel modules, and sysctl parameters. This is static configuration, not live activity — for that, see Debug Tools or Event Stream.

Read-only. Modules table is capped at the first 100 matching rows after filter.

## When to use it

- To confirm what kernel version, architecture, or boot parameters the host is running
- To check whether a specific kernel module is loaded and what's using it
- To look up the current value of a sysctl parameter without shelling in
- Before enabling Network Fabric eBPF / dataplane features that depend on kernel capabilities
- When filing a support report and you need kernel version + boot cmdline in one place

## How to get there

- Route / id: `/kernel`
- Nav: **Monitoring → Kernel** (sidebar, command palette, or desktop nav)
- Console: `http://127.0.0.1:<port>/kernel` or `https://<host>/kernel`

## Operate from the console (UX)

1. **Summary tiles** — Kernel Version, Hostname, Architecture, Modules Loaded count.
2. **Boot Command Line** — shown verbatim in a code block when the backend reports one.
3. **Kernel Modules table** — name, size, and "used by" for each loaded module (first 100 matches). **Filter modules** narrows by name live.
4. **Sysctl Parameters table** — key/value pairs reported by the backend (no filter).
5. Auto-refresh every 10 seconds; header refresh forces immediate reload.

Typical flow: confirm version/arch → search modules (e.g. networking/storage-related) → note relevant sysctls → continue live troubleshooting in [Debug Tools](debug.md) or [Processes](processes.md). You cannot load modules or change sysctls here.

**Empty / fail:** Error banner, empty table, or failed toast — confirm you are signed in, `zyvor-fabricd` is healthy (`/readyz` on `http://127.0.0.1:<port>` or `https://<host>`), and any backend this page needs (FluxVM, storage, network) is reachable. See [Admin basics](../../admin-basics.md).

**Success:** Live data loads without error; creates/updates appear in the list or detail view and any confirmation toast clears cleanly.

## Related pages

- [Debug Tools](debug.md)
- [Processes](processes.md)
- [System](../infrastructure/system.md)
- [System Health](../infrastructure/system-health.md)
- [VM Dataplane](../infrastructure/dataplane.md)
- [Getting Started](../../getting-started.md)
- [Page index](../../PAGE_INDEX.md)
