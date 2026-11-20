Name:           zyvor-fabricd
Version:        0.1.0
Release:        1%{?dist}
Summary:        Virtual Machine Management Daemon

License:        LicenseRef-Zyvor-Production-1.0
URL:            https://github.com/zyvorai/fabric
Source0:        %{name}-%{version}.tar.gz

BuildRequires:  cargo
BuildRequires:  rust
BuildRequires:  nodejs
BuildRequires:  npm
Recommends:     nftables
Recommends:     wireguard-tools

%description
zyvor-fabricd is a daemon for managing virtual machines, with VM lifecycle
handled by FluxVM (https://github.com/zyvorai/fluxvm), a standalone
disposable-VM control plane with no systemd dependency of its own — see
driver.fluxvm_url in zyvor-fabricd.toml. Provides REST API, WebSocket
console, VNC proxy, and comprehensive VM lifecycle management. Runs under
systemd or any other supervisor — nothing in this package requires it.

Includes fabricctl CLI tool.

%package web
Summary:        Web UI for zyvor-fabricd
Requires:       %{name} = %{version}-%{release}
BuildArch:      noarch

%description web
Web-based management interface for zyvor-fabricd. Provides a React dashboard
with 37+ pages for managing VMs, storage, network security, and more.

%prep
%autosetup

%build
%make_build

%install
%make_install PREFIX=%{_prefix}

%pre
getent group zyvor-fabricd >/dev/null || groupadd -r zyvor-fabricd
exit 0

%post
# Directories are also created defensively by the daemon itself at startup
# (see backend/zyvor-fabricd/src/daemon.rs::ensure_runtime_dirs) — created
# here too so they exist with the right ownership/mode from first boot,
# without a systemd-tmpfiles dependency.
mkdir -p /var/lib/zyvor-fabricd/images
mkdir -p /var/lib/zyvor-fabricd/state && chmod 0750 /var/lib/zyvor-fabricd/state
mkdir -p /run/zyvor-fabricd
mkdir -p /var/log/zyvor-fabricd
exit 0

%files
%license LICENSE
%doc README.md
%{_bindir}/zyvor-fabricd
%{_bindir}/fabricctl
# Optional: for operators who choose to run zyvor-fabricd under systemd.
# Nothing in this package enables, starts, or otherwise wires this up —
# that's a manual `systemctl enable --now zyvor-fabricd.service`.
/usr/lib/systemd/system/zyvor-fabricd.service
%{_libexecdir}/%{name}/backup-vms
%{_libexecdir}/%{name}/cleanup-store
%{_libexecdir}/%{name}/health-check
%dir %{_sysconfdir}/zyvor-fabricd
%config(noreplace) %{_sysconfdir}/zyvor-fabricd/zyvor-fabricd.toml
%config(noreplace) %{_sysconfdir}/zyvor-fabricd/zyvor-fabricd.env
%config(noreplace) %{_sysconfdir}/modules-load.d/zyvor-fabricd.conf
%dir %attr(0755,root,root) /var/lib/zyvor-fabricd
%dir %attr(0755,root,root) /var/lib/zyvor-fabricd/images

%files web
%{_datadir}/zyvor-fabricd/web/

%changelog
* Mon Mar 03 2026 ZyvorAI Labs Private Limited <ssahani@gmail.com> - 0.1.0-1
- Initial package
- zyvor-fabricd daemon with REST API and WebSocket
- fabricctl CLI with JSON/YAML output and 15+ subcommand groups
- Network security (policies, firewall, service mesh, QoS, DNS, VPN, mirror, NAT, monitor)
- Ceph/RBD storage support
- systemd units are optional; nothing in this package requires, enables, or
  auto-starts them (no hard systemd dependency, no sysusers/tmpfiles/preset)
