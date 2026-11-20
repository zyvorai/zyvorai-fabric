# Decision Support

This section helps teams decide whether Zyvor Fabric fits their environment.

## When to Choose Zyvor Fabric

Zyvor Fabric is a strong fit when your environment meets these criteria:

- **No systemd requirement** -- VM lifecycle is managed by [FluxVM](https://github.com/zyvorai/fluxvm), a disposable-VM engine with no systemd dependency of its own; host networking uses direct netlink calls (not systemd-networkd). systemd itself is entirely optional -- only as a way to supervise the `zyvor-fabricd` daemon process, for operators who choose that supervisor.
- **API-first automation** -- You need a comprehensive REST API (780+ endpoints) for infrastructure-as-code workflows, CI/CD pipelines, or custom tooling.
- **Single-host or small-cluster deployments** -- You want lightweight VM management without the operational overhead of full cluster orchestration platforms.
- **Security-conscious environments** -- You value PAM-based authentication, role-based access control, audit logging, and network policy enforcement built into the platform.

## When Fabric Is Not Yet the Right Fit

- **Large multi-host clusters with live migration** -- Fabric's native live-migration transport is still preview; the disk-copy path is GA.
- **Existing libvirt ecosystem** -- If your tooling already depends on libvirt's XML domain definitions and virsh, migrating to Zyvor Fabric requires API adaptation.
- **Windows guest workloads** -- Zyvor Fabric focuses on Linux guests via its QEMU/KVM-based VM drivers. Mixed Windows/Linux environments are not yet a good fit.
