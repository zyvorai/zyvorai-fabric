// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

//! Inner containment: run the worker (and any browser it launches) as an
//! unprivileged user in a bubblewrap container inside the sandbox VM.
//!
//! The launcher is shipped with the runtime and written into the guest at
//! provisioning, so the template only needs `bubblewrap` and `util-linux`
//! (`setpriv`). It fails closed if either is missing.

/// Guest path the launcher is written to.
pub const GUEST_PATH: &str = "/opt/zyvor/contain.sh";

pub const SCRIPT: &str = include_str!("../guest/contain.sh");

/// Guest path of the compiled seccomp filter that `contain.sh` hands to bubblewrap.
pub const SECCOMP_GUEST_PATH: &str = "/opt/zyvor/seccomp.bpf";

/// x86_64 syscalls the agent has no business making, each answered with `EPERM`.
///
/// The list is what a process needs a kernel bug or a privilege for: debugging other processes,
/// mounting, loading kernel code, eBPF, perf, user page-fault handlers, kernel keyrings, changing
/// the clock or hardware ports, and io_uring (a large, fast-moving attack surface). Left out on
/// purpose because Chromium, V8 and Node use them: `unshare`, `clone`, `clone3`, `setns`,
/// `memfd_create`, `seccomp`, `prctl`.
pub const DENIED_SYSCALLS: &[(&str, u32)] = &[
    ("ptrace", 101),
    ("process_vm_readv", 310),
    ("process_vm_writev", 311),
    ("kcmp", 312),
    ("mount", 165),
    ("umount2", 166),
    ("pivot_root", 155),
    ("chroot", 161),
    ("open_tree", 428),
    ("move_mount", 429),
    ("fsopen", 430),
    ("fsconfig", 431),
    ("fsmount", 432),
    ("fspick", 433),
    ("mount_setattr", 442),
    ("swapon", 167),
    ("swapoff", 168),
    ("reboot", 169),
    ("acct", 163),
    ("quotactl", 179),
    ("kexec_load", 246),
    ("kexec_file_load", 320),
    ("init_module", 175),
    ("finit_module", 313),
    ("delete_module", 176),
    ("bpf", 321),
    ("perf_event_open", 298),
    ("userfaultfd", 323),
    ("keyctl", 250),
    ("add_key", 248),
    ("request_key", 249),
    ("open_by_handle_at", 304),
    ("lookup_dcookie", 212),
    ("settimeofday", 164),
    ("clock_settime", 227),
    ("clock_adjtime", 305),
    ("adjtimex", 159),
    ("iopl", 172),
    ("ioperm", 173),
    ("io_uring_setup", 425),
    ("io_uring_enter", 426),
    ("io_uring_register", 427),
];

// Classic BPF opcodes and seccomp return values (linux/filter.h, linux/seccomp.h).
const BPF_LD_W_ABS: u16 = 0x20;
const BPF_JMP_JEQ_K: u16 = 0x15;
const BPF_JMP_JSET_K: u16 = 0x45;
const BPF_RET_K: u16 = 0x06;
const AUDIT_ARCH_X86_64: u32 = 0xC000_003E;
const X32_SYSCALL_BIT: u32 = 0x4000_0000;
const SECCOMP_RET_KILL_PROCESS: u32 = 0x8000_0000;
const SECCOMP_RET_ALLOW: u32 = 0x7fff_0000;
const SECCOMP_RET_ERRNO_EPERM: u32 = 0x0005_0000 | 1;
/// `offsetof(struct seccomp_data, nr)` and `arch`.
const OFF_NR: u32 = 0;
const OFF_ARCH: u32 = 4;

fn insn(code: u16, jt: u8, jf: u8, k: u32) -> [u8; 8] {
    let mut b = [0u8; 8];
    b[0..2].copy_from_slice(&code.to_le_bytes());
    b[2] = jt;
    b[3] = jf;
    b[4..8].copy_from_slice(&k.to_le_bytes());
    b
}

/// The filter as the kernel reads it (an array of `struct sock_filter`), for `bwrap --seccomp`.
///
/// Any architecture other than x86_64 is killed, so a 32-bit compat call cannot slip past the
/// x86_64 numbers. The x32 ABI (bit 30 set) is refused. Denied syscalls get `EPERM`, and
/// everything else is allowed.
pub fn seccomp_filter() -> Vec<u8> {
    let n = DENIED_SYSCALLS.len();
    // 0 ld arch | 1 jeq arch | 2 kill | 3 ld nr | 4 jset x32 | 5.. one jeq per syscall | allow | deny
    let first_check = 5;
    let allow_at = first_check + n;
    let deny_at = allow_at + 1;
    let mut prog: Vec<[u8; 8]> = Vec::with_capacity(deny_at + 1);
    prog.push(insn(BPF_LD_W_ABS, 0, 0, OFF_ARCH));
    prog.push(insn(BPF_JMP_JEQ_K, 1, 0, AUDIT_ARCH_X86_64));
    prog.push(insn(BPF_RET_K, 0, 0, SECCOMP_RET_KILL_PROCESS));
    prog.push(insn(BPF_LD_W_ABS, 0, 0, OFF_NR));
    prog.push(insn(
        BPF_JMP_JSET_K,
        (deny_at - 4 - 1) as u8,
        0,
        X32_SYSCALL_BIT,
    ));
    for (i, (_, nr)) in DENIED_SYSCALLS.iter().enumerate() {
        let at = first_check + i;
        prog.push(insn(BPF_JMP_JEQ_K, (deny_at - at - 1) as u8, 0, *nr));
    }
    prog.push(insn(BPF_RET_K, 0, 0, SECCOMP_RET_ALLOW));
    prog.push(insn(BPF_RET_K, 0, 0, SECCOMP_RET_ERRNO_EPERM));
    debug_assert!(
        prog.len() <= 4096,
        "seccomp programs hold at most 4096 instructions"
    );
    debug_assert!(deny_at - first_check < 256, "jump offsets are one byte");
    prog.concat()
}

/// What goes in front of `node` in the guest command line: the launcher when
/// strict, nothing otherwise. Includes the trailing space.
pub fn launcher_prefix(mode: crate::model::InnerContainer) -> String {
    match mode {
        crate::model::InnerContainer::Strict => format!("{GUEST_PATH} "),
        crate::model::InnerContainer::Off => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launcher_prefix_wraps_only_when_strict() {
        use crate::model::InnerContainer;
        assert_eq!(launcher_prefix(InnerContainer::Off), "");
        assert_eq!(
            launcher_prefix(InnerContainer::Strict),
            "/opt/zyvor/contain.sh "
        );
    }
    use std::{fs, os::unix::fs::PermissionsExt, path::Path, process::Command};

    fn write_exe(dir: &Path, name: &str, body: &str) {
        let path = dir.join(name);
        fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    }

    /// Run the script with fake `bwrap`/`setpriv` that record their arguments.
    fn run(fake_setpriv: bool, fake_bwrap: bool, args: &[&str]) -> (i32, String, String) {
        run_with(fake_setpriv, fake_bwrap, "x86_64", true, args)
    }

    /// `arch` is what the fake `uname -m` prints; `with_filter` says whether the compiled
    /// seccomp filter file exists.
    fn run_with(
        fake_setpriv: bool,
        fake_bwrap: bool,
        arch: &str,
        with_filter: bool,
        args: &[&str],
    ) -> (i32, String, String) {
        let dir = std::env::temp_dir().join(format!("zyvor-contain-{}", uuid::Uuid::new_v4()));
        let bin = dir.join("bin");
        fs::create_dir_all(&bin).unwrap();
        let script = dir.join("contain.sh");
        fs::write(&script, SCRIPT).unwrap();
        let log = dir.join("bwrap.args");
        if fake_bwrap {
            write_exe(
                &bin,
                "bwrap",
                &format!("printf '%s\\n' \"$@\" > {}", log.display()),
            );
        }
        if fake_setpriv {
            write_exe(&bin, "setpriv", "exit 0");
        }
        write_exe(&bin, "uname", &format!("echo {arch}"));
        let filter = dir.join("seccomp.bpf");
        if with_filter {
            fs::write(&filter, seccomp_filter()).unwrap();
        }
        // Hermetic PATH: only the fakes above plus the few basic tools the script
        // needs, linked in. A real bwrap or setpriv elsewhere on the machine must
        // not be visible, or the "missing tool" cases would depend on the host.
        for tool in ["id", "mkdir", "chown"] {
            let real = ["/usr/bin", "/bin", "/usr/sbin", "/sbin"]
                .iter()
                .map(|d| Path::new(d).join(tool))
                .find(|p| p.exists())
                .unwrap_or_else(|| panic!("{tool} not found"));
            std::os::unix::fs::symlink(real, bin.join(tool)).unwrap();
        }
        let path = bin.display().to_string();
        let output = Command::new("/bin/sh")
            .arg(&script)
            .args(args)
            .env("PATH", path)
            .env("ZYVOR_CONTAIN_USER", "nobody")
            .env("ZYVOR_CONTAIN_HOME", dir.join("home"))
            .env("ZYVOR_CONTAIN_SECCOMP_FILE", &filter)
            .output()
            .unwrap();
        let recorded = fs::read_to_string(&log).unwrap_or_default();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        let _ = fs::remove_dir_all(&dir);
        (output.status.code().unwrap_or(-1), recorded, stderr)
    }

    #[test]
    fn runs_the_command_unprivileged_in_a_read_only_container() {
        let (code, args, stderr) = run(true, true, &["node", "/opt/zyvor/worker.mjs"]);
        assert_eq!(code, 0, "{stderr}");
        let lines: Vec<&str> = args.lines().collect();
        let has = |flag: &str| lines.contains(&flag);
        for flag in [
            "--die-with-parent",
            "--new-session",
            "--unshare-pid",
            "--unshare-ipc",
            "--ro-bind",
        ] {
            assert!(has(flag), "missing {flag} in {lines:?}");
        }
        // Read-only root, private tmp, no leftover root data.
        let pos = lines.iter().position(|l| *l == "--ro-bind").unwrap();
        assert_eq!(&lines[pos + 1..pos + 3], ["/", "/"]);
        assert!(lines.windows(2).any(|w| w == ["--tmpfs", "/tmp"]));
        assert!(lines.windows(2).any(|w| w == ["--tmpfs", "/root"]));
        // Privileges are dropped inside, then the command runs.
        let tail: Vec<&str> = lines
            .iter()
            .copied()
            .skip_while(|l| *l != "setpriv")
            .collect();
        for flag in [
            "--clear-groups",
            "--inh-caps=-all",
            "--bounding-set=-all",
            "--no-new-privs",
        ] {
            assert!(tail.contains(&flag), "missing {flag} in {tail:?}");
        }
        assert!(tail.iter().any(|l| l.starts_with("--reuid=")));
        assert_eq!(&tail[tail.len() - 2..], ["node", "/opt/zyvor/worker.mjs"]);
        // Network is shared on purpose, never unshared.
        assert!(!has("--unshare-net") && !has("--unshare-all"));
    }

    #[test]
    fn fails_closed_without_bubblewrap() {
        let (code, args, stderr) = run(true, false, &["true"]);
        assert_eq!(code, 127);
        assert!(stderr.contains("bwrap is required"), "{stderr}");
        assert!(args.is_empty());
    }

    #[test]
    fn fails_closed_without_setpriv() {
        let (code, _, stderr) = run(false, true, &["true"]);
        assert_eq!(code, 127);
        assert!(stderr.contains("setpriv is required"), "{stderr}");
    }

    #[test]
    fn hands_the_syscall_filter_to_bubblewrap_on_fd_3() {
        let (code, args, stderr) = run(true, true, &["true"]);
        assert_eq!(code, 0, "{stderr}");
        let lines: Vec<&str> = args.lines().collect();
        assert!(
            lines.windows(2).any(|w| w == ["--seccomp", "3"]),
            "no --seccomp 3 in {lines:?}"
        );
        // It must come before the `--` that ends bubblewrap's own options.
        let seccomp = lines.iter().position(|l| *l == "--seccomp").unwrap();
        let end = lines.iter().position(|l| *l == "--").unwrap();
        assert!(seccomp < end);
    }

    #[test]
    fn fails_closed_without_the_filter_file() {
        let (code, args, stderr) = run_with(true, true, "x86_64", false, &["true"]);
        assert_eq!(code, 127);
        assert!(stderr.contains("syscall filter"), "{stderr}");
        assert!(args.is_empty(), "bwrap must not run: {args}");
    }

    #[test]
    fn fails_closed_on_a_cpu_the_filter_does_not_cover() {
        let (code, args, stderr) = run_with(true, true, "aarch64", true, &["true"]);
        assert_eq!(code, 127);
        assert!(stderr.contains("x86_64 only"), "{stderr}");
        assert!(args.is_empty(), "bwrap must not run: {args}");
    }

    #[test]
    fn refuses_to_run_with_no_command() {
        let (code, _, stderr) = run(true, true, &[]);
        assert_eq!(code, 64);
        assert!(stderr.contains("no command"), "{stderr}");
    }

    #[test]
    fn script_is_shell_syntax_clean() {
        let status = Command::new("sh")
            .arg("-n")
            .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/guest/contain.sh"))
            .status()
            .unwrap();
        assert!(status.success());
    }

    /// Just enough classic BPF to run the filter: load, jeq, jset, ret.
    fn bpf_run(prog: &[u8], arch: u32, nr: u32) -> u32 {
        let ins: Vec<(u16, u8, u8, u32)> = prog
            .chunks(8)
            .map(|c| {
                (
                    u16::from_le_bytes([c[0], c[1]]),
                    c[2],
                    c[3],
                    u32::from_le_bytes([c[4], c[5], c[6], c[7]]),
                )
            })
            .collect();
        let (mut a, mut pc) = (0u32, 0usize);
        loop {
            let (code, jt, jf, k) = ins[pc];
            pc += 1;
            match code {
                BPF_LD_W_ABS => a = if k == OFF_ARCH { arch } else { nr },
                BPF_JMP_JEQ_K => pc += if a == k { jt } else { jf } as usize,
                BPF_JMP_JSET_K => pc += if a & k != 0 { jt } else { jf } as usize,
                BPF_RET_K => return k,
                other => panic!("unexpected opcode {other:#x}"),
            }
        }
    }

    #[test]
    fn filter_is_whole_instructions() {
        assert_eq!(seccomp_filter().len() % 8, 0);
    }

    #[test]
    fn denied_syscalls_get_eperm_and_others_are_allowed() {
        let f = seccomp_filter();
        for (name, nr) in DENIED_SYSCALLS {
            assert_eq!(
                bpf_run(&f, AUDIT_ARCH_X86_64, *nr),
                SECCOMP_RET_ERRNO_EPERM,
                "{name} should be denied"
            );
        }
        // read, write, openat, mmap, execve, exit_group, socket, connect, clone, clone3,
        // unshare, memfd_create, prctl, seccomp
        for nr in [0, 1, 257, 9, 59, 231, 41, 42, 56, 435, 272, 319, 157, 317] {
            assert_eq!(
                bpf_run(&f, AUDIT_ARCH_X86_64, nr),
                SECCOMP_RET_ALLOW,
                "nr {nr}"
            );
        }
    }

    #[test]
    fn other_architectures_and_the_x32_abi_do_not_get_through() {
        let f = seccomp_filter();
        // AUDIT_ARCH_I386: a 32-bit compat call must not reuse the x86_64 numbers.
        assert_eq!(bpf_run(&f, 0x4000_0003, 0), SECCOMP_RET_KILL_PROCESS);
        // AUDIT_ARCH_AARCH64
        assert_eq!(bpf_run(&f, 0xC000_00B7, 0), SECCOMP_RET_KILL_PROCESS);
        // x32 ptrace (bit 30 | 101) and an x32 read
        assert_eq!(
            bpf_run(&f, AUDIT_ARCH_X86_64, X32_SYSCALL_BIT | 101),
            SECCOMP_RET_ERRNO_EPERM
        );
        assert_eq!(
            bpf_run(&f, AUDIT_ARCH_X86_64, X32_SYSCALL_BIT),
            SECCOMP_RET_ERRNO_EPERM
        );
    }

    #[test]
    fn syscall_numbers_are_unique() {
        let mut seen = std::collections::BTreeSet::new();
        for (name, nr) in DENIED_SYSCALLS {
            assert!(seen.insert(*nr), "{name} repeats syscall {nr}");
        }
    }

    #[test]
    fn launcher_requires_the_filter_and_x86_64() {
        assert!(SCRIPT.contains("--seccomp 3"));
        assert!(SCRIPT.contains(SECCOMP_GUEST_PATH));
        assert!(SCRIPT.contains("x86_64"));
    }
}
