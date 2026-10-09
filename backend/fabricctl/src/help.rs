// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

//! Cilium-style grouped root help (clap 4 cannot section subcommands natively).

use crate::style::{self, paint};

struct CmdEntry {
    emoji: &'static str,
    name: &'static str,
    about: &'static str,
}

struct CmdGroup {
    emoji: &'static str,
    title: &'static str,
    commands: &'static [CmdEntry],
}

const GROUPS: &[CmdGroup] = &[
    CmdGroup {
        emoji: "🖥️",
        title: "Basic Commands:",
        commands: &[
            CmdEntry {
                emoji: "📋",
                name: "list",
                about: "List all VMs",
            },
            CmdEntry {
                emoji: "ℹ️",
                name: "info",
                about: "Get VM information",
            },
            CmdEntry {
                emoji: "🍴",
                name: "live-fork",
                about: "Fork a running VM into copies of its memory (flux-vm KVM engine)",
            },
            CmdEntry {
                emoji: "🧠",
                name: "memory",
                about: "Show a VM's real memory footprint (PSS) and balloon",
            },
            CmdEntry {
                emoji: "🎈",
                name: "balloon",
                about: "Show or set a VM's memory balloon (Beta)",
            },
            CmdEntry {
                emoji: "✨",
                name: "create",
                about: "Create a new VM",
            },
            CmdEntry {
                emoji: "▶️",
                name: "start",
                about: "Start a VM",
            },
            CmdEntry {
                emoji: "⏹️",
                name: "stop",
                about: "Stop a VM",
            },
            CmdEntry {
                emoji: "🔄",
                name: "restart",
                about: "Restart a VM",
            },
            CmdEntry {
                emoji: "🗑️",
                name: "delete",
                about: "Delete a VM",
            },
            CmdEntry {
                emoji: "📊",
                name: "metrics",
                about: "Get VM metrics",
            },
        ],
    },
    CmdGroup {
        emoji: "📦",
        title: "Config:",
        commands: &[
            CmdEntry {
                emoji: "📥",
                name: "apply",
                about: "Apply configuration from a JSON or YAML file",
            },
            CmdEntry {
                emoji: "📤",
                name: "export",
                about: "Export current config to JSON or YAML",
            },
        ],
    },
    CmdGroup {
        emoji: "⚡",
        title: "Dataplane (FluxVM edge):",
        commands: &[CmdEntry {
            emoji: "🧭",
            name: "dataplane",
            about: "Status, policy, flows, hubble, services, CNP, …",
        }],
    },
    CmdGroup {
        emoji: "🔐",
        title: "Networking & Security:",
        commands: &[
            CmdEntry {
                emoji: "📜",
                name: "policy",
                about: "Manage Fabric SDN network policies",
            },
            CmdEntry {
                emoji: "🧱",
                name: "firewall",
                about: "Manage VM firewall profiles and zones",
            },
            CmdEntry {
                emoji: "🕸️",
                name: "service",
                about: "Manage Fabric SDN service mesh",
            },
            CmdEntry {
                emoji: "🎚️",
                name: "qos",
                about: "Manage QoS / traffic shaping policies",
            },
            CmdEntry {
                emoji: "🌍",
                name: "dns",
                about: "Manage DNS zones and policies",
            },
            CmdEntry {
                emoji: "🔒",
                name: "vpn",
                about: "Manage VPN tunnels and networks",
            },
            CmdEntry {
                emoji: "🪞",
                name: "mirror",
                about: "Manage packet mirror sessions",
            },
            CmdEntry {
                emoji: "🔀",
                name: "nat",
                about: "Manage NAT rules, pools, and gateways",
            },
            CmdEntry {
                emoji: "🔌",
                name: "net",
                about: "Manage networkd bridges, VLANs, bonds, taps",
            },
        ],
    },
    CmdGroup {
        emoji: "👁️",
        title: "Observability:",
        commands: &[
            CmdEntry {
                emoji: "📡",
                name: "monitor",
                about: "Manage network monitoring policies and alerts",
            },
            CmdEntry {
                emoji: "✋",
                name: "approval",
                about: "Review and decide agent approval requests",
            },
            CmdEntry {
                emoji: "🧰",
                name: "skill",
                about: "Publish and inspect agent skills",
            },
            CmdEntry {
                emoji: "📜",
                name: "agent-audit",
                about: "Show the tamper-evident agent action journal",
            },
        ],
    },
    CmdGroup {
        emoji: "💾",
        title: "Storage:",
        commands: &[CmdEntry {
            emoji: "🗄️",
            name: "ceph",
            about: "Manage Ceph storage pools and RBD images",
        }],
    },
    CmdGroup {
        emoji: "🧰",
        title: "Workloads:",
        commands: &[
            CmdEntry {
                emoji: "⚙️",
                name: "runtime",
                about: "FluxVM runtime capabilities and migration",
            },
            CmdEntry {
                emoji: "📦",
                name: "container-group",
                about: "Manage ContainerGroup workloads",
            },
            CmdEntry {
                emoji: "🤖",
                name: "ai",
                about: "AI Workloads — models, deployments, endpoints (preview)",
            },
        ],
    },
    CmdGroup {
        emoji: "🛠️",
        title: "Meta:",
        commands: &[
            CmdEntry {
                emoji: "❤️",
                name: "status",
                about: "Display Fabric / dataplane status",
            },
            CmdEntry {
                emoji: "⚙️",
                name: "config",
                about: "Show effective CLI configuration",
            },
            CmdEntry {
                emoji: "⌨️",
                name: "completion",
                about: "Generate shell completion scripts",
            },
            CmdEntry {
                emoji: "🏷️",
                name: "version",
                about: "Print version information",
            },
            CmdEntry {
                emoji: "❓",
                name: "help",
                about: "Help about any command",
            },
        ],
    },
];

/// Render Cilium-style grouped root help with emoji section markers.
pub fn render_root_help(color: bool) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "✨ {} {}\n\n",
        paint(color, style::BOLD, "fabricctl"),
        paint(color, style::DIM, "— Zyvor Fabric CLI")
    ));
    out.push_str(&format!(
        "📘 {}\n  fabricctl [flags] [command]\n\n",
        paint(color, style::BOLD, "Usage:")
    ));

    for group in GROUPS {
        let title = format!("{} {}", group.emoji, group.title);
        out.push_str(&paint(color, style::BOLD, &title));
        out.push('\n');
        for cmd in group.commands {
            let padded = format!("{:<16}", cmd.name);
            let name = paint(color, style::CYAN, &padded);
            out.push_str(&format!("  {} {name} {}\n", cmd.emoji, cmd.about));
        }
        out.push('\n');
    }

    out.push_str(&format!("🚩 {}\n", paint(color, style::BOLD, "Flags:")));
    out.push_str(&flag_line(
        color,
        "-o, --output",
        "Output format: table|json|yaml (default table)",
    ));
    out.push_str(&flag_line(
        color,
        "    --color",
        "Colorize output: auto|always|never (default auto)",
    ));
    out.push_str(&flag_line(
        color,
        "    --server",
        "Fabric API URL (overrides ZYVOR_FABRIC_URL)",
    ));
    out.push_str(&flag_line(
        color,
        "    --token",
        "Bearer token (overrides ZYVOR_FABRIC_TOKEN)",
    ));
    out.push_str(&flag_line(color, "-h, --help", "Help for fabricctl"));
    out.push_str(&flag_line(color, "-V, --version", "Print version"));
    out.push('\n');
    out.push_str("💡 Use \"fabricctl [command] --help\" for more information about a command.\n");
    out
}

fn flag_line(color: bool, flag: &str, about: &str) -> String {
    let padded = format!("  {:<16}", flag);
    format!("{} {}\n", paint(color, style::GREEN, &padded), about)
}

pub fn print_root_help(color: bool) {
    print!("{}", render_root_help(color));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_help_has_groups_plain() {
        let h = render_root_help(false);
        assert!(h.contains("Basic Commands:"));
        assert!(h.contains("🖥️"));
        assert!(h.contains("Dataplane (FluxVM edge):"));
        assert!(h.contains("Networking & Security:"));
        assert!(h.contains("Meta:"));
        assert!(h.contains("status"));
        assert!(h.contains("completion"));
        assert!(h.contains("✨"));
        assert!(!h.contains('\u{1b}'));
    }

    #[test]
    fn root_help_color_has_ansi() {
        let h = render_root_help(true);
        assert!(h.contains('\u{1b}'));
        assert!(h.contains("Basic Commands:"));
        assert!(h.contains("⚡"));
    }
}
