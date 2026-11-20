// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

use anyhow::{bail, Context, Result};
use futures::TryStreamExt;
use netlink_packet_route::address::{AddressAttribute, AddressScope};
use netlink_packet_route::link::{
    BondMode as NlBondMode, BridgeStpState, InfoBridge, InfoData, InfoKind, LinkAttribute,
    LinkFlags, LinkInfo, LinkLayerType, MacVtapMode as NlMacVtapMode,
};
use netlink_packet_route::AddressFamily;
use rtnetlink::{
    LinkBond, LinkBridge, LinkMacVtap, LinkUnspec, LinkVlan, LinkVxlan, LinkWireguard,
    RouteMessageBuilder,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

/// Network interface information retrieved via netlink.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetlinkInterface {
    pub index: u32,
    pub name: String,
    pub mac: String,
    pub mtu: u32,
    pub state: String,
    pub link_type: String,
    pub flags: Vec<String>,
    pub addresses: Vec<InterfaceAddress>,
    pub master_index: Option<u32>,
    pub master_name: Option<String>,
    pub kind: Option<String>,
    pub speed_mbps: Option<u32>,
}

/// IP address assigned to an interface.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterfaceAddress {
    pub address: String,
    pub prefix_len: u8,
    pub family: String,
    pub scope: String,
}

/// List all network interfaces via netlink.
pub async fn list_interfaces() -> Result<Vec<NetlinkInterface>> {
    let (conn, handle, _) = rtnetlink::new_connection()?;
    tokio::spawn(conn);

    let mut links = handle.link().get().execute();
    let mut ifaces: Vec<NetlinkInterface> = Vec::new();
    let mut index_to_name: HashMap<u32, String> = HashMap::new();

    while let Some(msg) = links.try_next().await? {
        let header = &msg.header;
        let index = header.index;
        let lf = header.flags;
        let state = if lf.contains(LinkFlags::Up) {
            "up"
        } else {
            "down"
        };

        let mut name = String::new();
        let mut mac = String::new();
        let mut mtu: u32 = 0;
        let mut kind: Option<String> = None;
        let mut master_index: Option<u32> = None;

        for attr in &msg.attributes {
            match attr {
                LinkAttribute::IfName(n) => name = n.clone(),
                LinkAttribute::Address(bytes) => {
                    mac = bytes
                        .iter()
                        .map(|b| format!("{:02x}", b))
                        .collect::<Vec<_>>()
                        .join(":");
                }
                LinkAttribute::Mtu(m) => mtu = *m,
                LinkAttribute::Controller(m) => master_index = Some(*m),
                LinkAttribute::LinkInfo(infos) => {
                    for info in infos {
                        if let LinkInfo::Kind(k) = info {
                            kind = Some(match k {
                                InfoKind::Bridge => "bridge".to_string(),
                                InfoKind::Bond => "bond".to_string(),
                                InfoKind::Vlan => "vlan".to_string(),
                                InfoKind::Vxlan => "vxlan".to_string(),
                                InfoKind::MacVlan => "macvlan".to_string(),
                                InfoKind::MacVtap => "macvtap".to_string(),
                                InfoKind::Tun => "tun".to_string(),
                                InfoKind::Veth => "veth".to_string(),
                                InfoKind::Dummy => "dummy".to_string(),
                                InfoKind::Other(s) => s.clone(),
                                _ => format!("{:?}", k),
                            });
                        }
                    }
                }
                _ => {}
            }
        }

        let link_type = match header.link_layer_type {
            LinkLayerType::Ether => "ether",
            LinkLayerType::Loopback => "loopback",
            _ => "other",
        };

        let mut flag_names = Vec::new();
        if lf.contains(LinkFlags::Up) {
            flag_names.push("UP".to_string());
        }
        if lf.contains(LinkFlags::Broadcast) {
            flag_names.push("BROADCAST".to_string());
        }
        if lf.contains(LinkFlags::Loopback) {
            flag_names.push("LOOPBACK".to_string());
        }
        if lf.contains(LinkFlags::Pointopoint) {
            flag_names.push("POINTOPOINT".to_string());
        }
        if lf.contains(LinkFlags::Multicast) {
            flag_names.push("MULTICAST".to_string());
        }
        if lf.contains(LinkFlags::Running) {
            flag_names.push("RUNNING".to_string());
        }
        if lf.contains(LinkFlags::LowerUp) {
            flag_names.push("LOWER_UP".to_string());
        }

        index_to_name.insert(index, name.clone());

        ifaces.push(NetlinkInterface {
            index,
            name,
            mac,
            mtu,
            state: state.to_string(),
            link_type: link_type.to_string(),
            flags: flag_names,
            addresses: Vec::new(),
            master_index,
            master_name: None,
            kind,
            speed_mbps: None,
        });
    }

    // Resolve master names
    for iface in &mut ifaces {
        if let Some(mi) = iface.master_index {
            iface.master_name = index_to_name.get(&mi).cloned();
        }
    }

    // Get addresses via netlink
    let mut addrs = handle.address().get().execute();
    while let Some(msg) = addrs.try_next().await? {
        let if_index = msg.header.index;
        let prefix_len = msg.header.prefix_len;
        let family = match msg.header.family {
            AddressFamily::Inet => "inet",
            AddressFamily::Inet6 => "inet6",
            _ => "unknown",
        };
        let scope = match msg.header.scope {
            AddressScope::Universe => "global",
            AddressScope::Link => "link",
            AddressScope::Host => "host",
            AddressScope::Site => "site",
            _ => "other",
        };

        let mut addr_str = String::new();
        for attr in &msg.attributes {
            if let AddressAttribute::Address(ip) = attr {
                addr_str = ip.to_string();
            }
        }

        if !addr_str.is_empty() {
            if let Some(iface) = ifaces.iter_mut().find(|i| i.index == if_index) {
                iface.addresses.push(InterfaceAddress {
                    address: addr_str,
                    prefix_len,
                    family: family.to_string(),
                    scope: scope.to_string(),
                });
            }
        }
    }

    // Get link speeds from sysfs
    for iface in &mut ifaces {
        if iface.link_type == "ether" {
            if let Ok(speed) =
                std::fs::read_to_string(format!("/sys/class/net/{}/speed", iface.name))
            {
                if let Ok(s) = speed.trim().parse::<i32>() {
                    if s > 0 {
                        iface.speed_mbps = Some(s as u32);
                    }
                }
            }
        }
    }

    Ok(ifaces)
}

/// List all non-loopback interfaces.
pub async fn list_physical_interfaces() -> Result<Vec<NetlinkInterface>> {
    let all = list_interfaces().await?;
    Ok(all.into_iter().filter(|i| i.name != "lo").collect())
}

/// List all non-loopback interfaces that are not already enslaved.
pub async fn list_available_interfaces() -> Result<Vec<NetlinkInterface>> {
    let all = list_interfaces().await?;
    Ok(all
        .into_iter()
        .filter(|i| i.name != "lo" && i.master_index.is_none())
        .collect())
}

// ============================================================================
// Write operations — replace NetworkdManager's write-a-.netdev/.network-file-
// then-`networkctl reload` two-step with direct, immediate netlink calls. See
// the systemd-removal migration plan, Phase 4.
// ============================================================================

pub async fn connect() -> Result<rtnetlink::Handle> {
    let (conn, handle, _) = rtnetlink::new_connection().context("failed to open netlink socket")?;
    tokio::spawn(conn);
    Ok(handle)
}

pub async fn link_index_by_name(handle: &rtnetlink::Handle, name: &str) -> Result<u32> {
    let mut links = handle.link().get().match_name(name.to_string()).execute();
    match links.try_next().await? {
        Some(msg) => Ok(msg.header.index),
        None => bail!("interface '{name}' not found"),
    }
}

/// Create a Linux bridge (`ip link add <name> type bridge`), brought up
/// immediately (`LinkBridge::new` sets the UP flag by default).
///
/// Retries a bounded number of times on failure: found live, several
/// bridges created in immediate back-to-back succession each failed with
/// "Received a netlink error message Numerical result out of range (os
/// error 34)" (ERANGE) -- a 300ms stagger between creates never failed,
/// confirming it's transient contention under rapid concurrent creation
/// rather than a real rejection. Root mechanism not fully pinned down
/// (kernel-side ifindex or netlink port allocation under burst churn are
/// the leading candidates); retrying with a short backoff is far cheaper
/// than serializing all bridge creation through a single connection/lock,
/// and safe since a failed create leaves nothing to double up on retry.
pub async fn create_bridge(name: &str) -> Result<()> {
    const MAX_ATTEMPTS: u32 = 5;
    let mut last_err = None;
    for attempt in 0..MAX_ATTEMPTS {
        let handle = connect().await?;
        match handle
            .link()
            .add(LinkBridge::new(name).build())
            .execute()
            .await
        {
            Ok(()) => return Ok(()),
            Err(e) => {
                if attempt + 1 < MAX_ATTEMPTS {
                    tokio::time::sleep(std::time::Duration::from_millis(50 * (attempt as u64 + 1)))
                        .await;
                }
                last_err = Some(e);
            }
        }
    }
    Err(last_err.unwrap())
        .with_context(|| format!("failed to create bridge '{name}' after {MAX_ATTEMPTS} attempts"))
}

/// Create a bridge only when it does not already exist (idempotent apply/update).
pub async fn ensure_bridge(name: &str) -> Result<()> {
    let handle = connect().await?;
    if link_index_by_name(&handle, name).await.is_ok() {
        return Ok(());
    }
    create_bridge(name).await
}

/// Create an 802.1Q VLAN sub-interface on `parent` (`ip link add <name> link
/// <parent> type vlan id <vlan_id>`).
pub async fn create_vlan(parent: &str, vlan_id: u16, name: &str) -> Result<()> {
    let handle = connect().await?;
    let parent_index = link_index_by_name(&handle, parent).await?;
    handle
        .link()
        .add(LinkVlan::new(name, parent_index, vlan_id).build())
        .execute()
        .await
        .with_context(|| format!("failed to create VLAN '{name}' (id={vlan_id}, parent={parent})"))
}

/// Create a macvtap device on `parent`. `mode` is one of
/// bridge/vepa/private/passthru/source (defaults to bridge on anything else,
/// matching `MacvtapConfig`'s own default).
pub async fn create_macvtap(parent: &str, name: &str, mode: &str) -> Result<()> {
    let handle = connect().await?;
    let parent_index = link_index_by_name(&handle, parent).await?;
    let nl_mode = match mode {
        "vepa" => NlMacVtapMode::Vepa,
        "private" => NlMacVtapMode::Private,
        "passthru" | "passthrough" => NlMacVtapMode::Passthrough,
        "source" => NlMacVtapMode::Source,
        _ => NlMacVtapMode::Bridge,
    };
    handle
        .link()
        .add(LinkMacVtap::new(name, parent_index, nl_mode).build())
        .execute()
        .await
        .with_context(|| format!("failed to create macvtap '{name}' (parent={parent})"))
}

/// Create a persistent TAP device. rtnetlink's generic link-add doesn't cover
/// tun/tap (the kernel creates those via the `/dev/net/tun` character device
/// plus a `TUNSETIFF` ioctl, not `RTM_NEWLINK`), so this is the one device
/// type still created via `ip tuntap` rather than a raw netlink call — a
/// pragmatic exception, not a step back toward systemd-networkd (which this
/// replaces `networkctl reload`, not `iproute2`, for).
pub async fn create_tap(name: &str) -> Result<()> {
    let name = input_guard::vet_component!(name, anyhow::anyhow!("invalid tap device name"));
    let output =
        input_guard::argv_checked!(name, anyhow::anyhow!("invalid tap device name"), |name| {
            tokio::process::Command::new("ip")
                .args(["tuntap", "add", "dev", name, "mode", "tap"])
                .output()
                .await
                .context("failed to run `ip tuntap add`")?
        });
    if !output.status.success() {
        bail!(
            "ip tuntap add dev {name} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}

/// Create a bond device with the given mode and enslave `slaves` to it.
pub async fn create_bond(name: &str, mode: &str, slaves: &[String]) -> Result<()> {
    let handle = connect().await?;
    let nl_mode = match mode {
        "active-backup" => NlBondMode::ActiveBackup,
        "balance-xor" => NlBondMode::BalanceXor,
        "broadcast" => NlBondMode::Broadcast,
        "802.3ad" => NlBondMode::Ieee8023Ad,
        "balance-tlb" => NlBondMode::BalanceTlb,
        "balance-alb" => NlBondMode::BalanceAlb,
        _ => NlBondMode::BalanceRr,
    };
    handle
        .link()
        .add(LinkBond::new(name).mode(nl_mode).up().build())
        .execute()
        .await
        .with_context(|| format!("failed to create bond '{name}'"))?;
    let bond_index = link_index_by_name(&handle, name).await?;
    for slave in slaves {
        set_master(slave, bond_index)
            .await
            .with_context(|| format!("failed to enslave '{slave}' to bond '{name}'"))?;
    }
    Ok(())
}

/// Create a VXLAN device. `remote` is `None` for a multicast/BUM-flooding
/// VXLAN (no fixed remote peer).
pub async fn create_vxlan(
    name: &str,
    vni: u32,
    local: Option<IpAddr>,
    remote: Option<IpAddr>,
    port: Option<u16>,
) -> Result<()> {
    let handle = connect().await?;
    let mut builder = LinkVxlan::new(name, vni).up();
    if let Some(IpAddr::V4(addr)) = local {
        builder = builder.local(addr);
    }
    if let Some(IpAddr::V4(addr)) = remote {
        builder = builder.remote(addr);
    }
    if let Some(p) = port {
        builder = builder.port(p);
    }
    handle
        .link()
        .add(builder.build())
        .execute()
        .await
        .with_context(|| format!("failed to create VXLAN '{name}' (vni={vni})"))
}

/// Create a WireGuard device (`ip link add <name> type wireguard`). Device
/// creation is the one part of WireGuard setup that's a normal rtnetlink
/// link type; private key/listen-port/peers/allowed-ips live in the
/// WireGuard *generic* netlink family (`NLA_WGDEVICE`/`NLA_WGPEER`, a
/// different subsystem from `rtnetlink`'s route-netlink messages this
/// module otherwise uses) — those are set with the `wg` CLI instead, the
/// same tool `host_wireguard.rs` already shells out to for discovery.
pub async fn create_wireguard_device(name: &str) -> Result<()> {
    let handle = connect().await?;
    handle
        .link()
        .add(LinkWireguard::new(name).build())
        .execute()
        .await
        .with_context(|| format!("failed to create WireGuard device '{name}'"))
}

/// Enslave `iface` to `master_index` (a bridge or bond) — `ip link set
/// <iface> master <master-device>`.
pub async fn set_master(iface: &str, master_index: u32) -> Result<()> {
    let handle = connect().await?;
    handle
        .link()
        .set(
            LinkUnspec::new_with_name(iface)
                .controller(master_index)
                .build(),
        )
        .execute()
        .await
        .with_context(|| format!("failed to attach '{iface}' to master index {master_index}"))
}

/// Detach `iface` from whatever bridge/bond it's currently enslaved to —
/// `ip link set <iface> nomaster`.
pub async fn unset_master(iface: &str) -> Result<()> {
    let handle = connect().await?;
    handle
        .link()
        .set(LinkUnspec::new_with_name(iface).nocontroller().build())
        .execute()
        .await
        .with_context(|| format!("failed to detach '{iface}' from its master"))
}

/// Assign an IP address in CIDR form (`192.168.1.10/24`) to `iface` — `ip
/// addr add <cidr> dev <iface>`.
pub async fn set_addr(iface: &str, cidr: &str) -> Result<()> {
    let (addr_str, prefix_str) = cidr.split_once('/').with_context(|| {
        format!("address '{cidr}' is not in CIDR form (expected e.g. 10.0.0.1/24)")
    })?;
    let addr: IpAddr = addr_str
        .parse()
        .with_context(|| format!("invalid IP address '{addr_str}'"))?;
    let prefix_len: u8 = prefix_str
        .parse()
        .with_context(|| format!("invalid prefix length '{prefix_str}'"))?;

    let handle = connect().await?;
    let index = link_index_by_name(&handle, iface).await?;
    handle
        .address()
        .add(index, addr, prefix_len)
        .execute()
        .await
        .with_context(|| format!("failed to add address {cidr} to '{iface}'"))
}

/// Bring `iface` administratively up — `ip link set <iface> up`.
pub async fn set_link_up(iface: &str) -> Result<()> {
    let handle = connect().await?;
    handle
        .link()
        .set(LinkUnspec::new_with_name(iface).up().build())
        .execute()
        .await
        .with_context(|| format!("failed to bring up '{iface}'"))
}

/// Set `iface`'s MTU — `ip link set <iface> mtu <mtu>`.
pub async fn set_mtu(iface: &str, mtu: u32) -> Result<()> {
    let handle = connect().await?;
    handle
        .link()
        .set(LinkUnspec::new_with_name(iface).mtu(mtu).build())
        .execute()
        .await
        .with_context(|| format!("failed to set mtu {mtu} on '{iface}'"))
}

/// Set `iface`'s MAC address — `ip link set <iface> address <mac>`.
pub async fn set_mac_address(iface: &str, mac: &str) -> Result<()> {
    let bytes = mac
        .split(':')
        .map(|b| u8::from_str_radix(b, 16))
        .collect::<std::result::Result<Vec<u8>, _>>()
        .with_context(|| format!("invalid MAC address '{mac}'"))?;
    if bytes.len() != 6 {
        bail!(
            "invalid MAC address '{mac}': expected 6 octets, got {}",
            bytes.len()
        );
    }
    let handle = connect().await?;
    handle
        .link()
        .set(LinkUnspec::new_with_name(iface).address(bytes).build())
        .execute()
        .await
        .with_context(|| format!("failed to set mac address {mac} on '{iface}'"))
}

/// STP on/off plus the finer bridge tuning knobs -- all optional, only the
/// ones present get sent. `forward_delay_sec`/`hello_time_sec`/
/// `max_age_sec` are whole seconds; the kernel's `IFLA_BR_*` attributes for
/// these are in centiseconds (confirmed live: `ip -d link show` prints
/// `forward_delay 1500 /* 15.00 s */` for the kernel default of 1500), so
/// `set_bridge_options` multiplies by 100 before sending.
#[derive(Debug, Default, Clone, Copy)]
pub struct BridgeOptions {
    pub stp: Option<bool>,
    pub forward_delay_sec: Option<u32>,
    pub hello_time_sec: Option<u32>,
    pub max_age_sec: Option<u32>,
    pub vlan_filtering: Option<bool>,
}

/// Apply bridge-specific options — `ip link set <iface> type bridge
/// stp_state <0|1> forward_delay <cs> hello_time <cs> max_age <cs>
/// vlan_filtering <0|1>` (whichever of these `opts` actually sets). Found
/// live via a byte-for-byte strace comparison against a working `ip link
/// set ... type bridge stp_state 1`: iproute2 sends this as `RTM_NEWLINK`
/// targeting the *existing* ifindex (with `IFLA_INFO_KIND=bridge`
/// alongside `IFLA_INFO_DATA`), not `RTM_SETLINK` -- rtnetlink's own
/// `.link().set(...)` (RTM_SETLINK) happily ACKs the same message with
/// error=0, and top-level attributes (mtu/mac/up) genuinely do apply
/// through it, but the kernel never actually invokes the bridge driver's
/// changelink path for nested `IFLA_INFO_DATA` attributes that way --
/// nothing in `opts` actually changes despite the clean ACK.
///
/// The flags matter too: iproute2's message carries plain
/// `NLM_F_REQUEST|NLM_F_ACK` only. `.link().add(...)` defaults to
/// `NLM_F_CREATE|NLM_F_EXCL` and `.replace()` swaps that for
/// `NLM_F_CREATE|NLM_F_REPLACE` -- both tried live, both got rejected
/// with `EOPNOTSUPP` (os error 95): the kernel's RTM_NEWLINK handler
/// only calls the bridge driver's changelink for an *existing* device
/// when neither CREATE flag is set. `set_flags` overrides to exactly
/// iproute2's flag set (hardcoded rather than pulling in
/// netlink-packet-core as a direct dependency just for two constants:
/// NLM_F_REQUEST=1, NLM_F_ACK=4).
pub async fn set_bridge_options(iface: &str, opts: &BridgeOptions) -> Result<()> {
    const NLM_F_REQUEST_ACK: u16 = 1 | 4;

    let mut attrs = Vec::new();
    if let Some(enable) = opts.stp {
        // netlink-packet-route 0.33+: StpState takes BridgeStpState, not u8.
        // 0=Disabled, 1=KernelStp (matches iproute2 / prior integer encoding).
        attrs.push(InfoBridge::StpState(if enable {
            BridgeStpState::KernelStp
        } else {
            BridgeStpState::Disabled
        }));
    }
    if let Some(sec) = opts.forward_delay_sec {
        attrs.push(InfoBridge::ForwardDelay(sec.saturating_mul(100)));
    }
    if let Some(sec) = opts.hello_time_sec {
        attrs.push(InfoBridge::HelloTime(sec.saturating_mul(100)));
    }
    if let Some(sec) = opts.max_age_sec {
        attrs.push(InfoBridge::MaxAge(sec.saturating_mul(100)));
    }
    if let Some(enable) = opts.vlan_filtering {
        attrs.push(InfoBridge::VlanFiltering(enable));
    }
    if attrs.is_empty() {
        return Ok(());
    }

    let handle = connect().await?;
    handle
        .link()
        .add(
            LinkBridge::new(iface)
                .set_info_data(InfoData::Bridge(attrs))
                .build(),
        )
        .set_flags(NLM_F_REQUEST_ACK)
        .execute()
        .await
        .with_context(|| format!("failed to set bridge options on '{iface}'"))
}

/// Add a default route (0.0.0.0/0 or ::/0, depending on `gateway`'s
/// family) via `gateway`, egressing through `iface` — `ip route add
/// default via <gateway> dev <iface>`.
pub async fn add_default_route(iface: &str, gateway: IpAddr) -> Result<()> {
    let handle = connect().await?;
    let index = link_index_by_name(&handle, iface).await?;
    let route = match gateway {
        IpAddr::V4(addr) => RouteMessageBuilder::<Ipv4Addr>::new()
            .gateway(addr)
            .output_interface(index)
            .build(),
        IpAddr::V6(addr) => RouteMessageBuilder::<Ipv6Addr>::new()
            .gateway(addr)
            .output_interface(index)
            .build(),
    };
    handle
        .route()
        .add(route)
        .execute()
        .await
        .with_context(|| format!("failed to add default route via {gateway} on '{iface}'"))
}

/// Rename `iface` — `ip link set <iface> name <new_name>`. The interface
/// must be administratively down for the kernel to accept a rename.
pub async fn rename_link(iface: &str, new_name: &str) -> Result<()> {
    let handle = connect().await?;
    handle
        .link()
        .set(LinkUnspec::new_with_name(iface).down().build())
        .execute()
        .await
        .with_context(|| format!("failed to bring down '{iface}' before rename"))?;
    handle
        .link()
        .set(
            LinkUnspec::new_with_name(iface)
                .name(new_name.to_string())
                .build(),
        )
        .execute()
        .await
        .with_context(|| format!("failed to rename '{iface}' to '{new_name}'"))?;
    handle
        .link()
        .set(LinkUnspec::new_with_name(new_name).up().build())
        .execute()
        .await
        .with_context(|| format!("failed to bring '{new_name}' back up after rename"))
}

/// Delete a link by name — `ip link del <name>`. Works for any device type
/// created above (bridge/vlan/macvtap/bond/vxlan); tap devices too, since
/// tun/tap deletion (unlike creation) does go through the standard
/// `RTM_DELLINK` netlink call.
pub async fn delete_link(name: &str) -> Result<()> {
    let handle = connect().await?;
    let index = link_index_by_name(&handle, name).await?;
    handle
        .link()
        .del(index)
        .execute()
        .await
        .with_context(|| format!("failed to delete link '{name}'"))
}
