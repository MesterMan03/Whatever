use super::SandboxConfig;
use landlock::{
    ABI, Access, AccessFs, AccessNet, PathBeneath, PathFd, Ruleset, RulesetAttr, RulesetCreatedAttr,
};
use seccompiler::{
    BpfProgram, SeccompAction, SeccompCmpArgLen, SeccompCmpOp, SeccompCondition, SeccompFilter,
    SeccompRule,
};
use std::collections::BTreeMap;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::Command;

// prctl constants not always exported by libc on every distro
const PR_SET_SECUREBITS: libc::c_int = 28;
const PR_CAP_AMBIENT: libc::c_int = 47;
const PR_CAP_AMBIENT_CLEAR_ALL: libc::c_ulong = 4;
const SECBIT_NOROOT: u64 = 1;
const SECBIT_NOROOT_LOCKED: u64 = 2;
const SECBIT_NO_SETUID_FIXUP: u64 = 4;
const SECBIT_NO_SETUID_FIXUP_LOCKED: u64 = 8;

pub fn apply_pre_spawn(cmd: &mut Command, cfg: &SandboxConfig) -> anyhow::Result<()> {
    let bun_dir = cfg
        .runtime_executable
        .parent()
        .ok_or_else(|| anyhow::anyhow!("runtime has no parent"))?
        .to_path_buf();
    let mod_root = cfg.mod_root.clone();
    let mod_id = cfg.mod_id.clone();

    // Safety: the closure runs in the forked child before exec. Landlock, seccomp,
    // and prctl are all syscall-based and safe in a post-fork pre-exec context.
    unsafe {
        cmd.pre_exec(move || {
            apply_landlock(&bun_dir, &mod_root);
            apply_seccomp();
            drop_capabilities(&mod_id);
            Ok(())
        });
    }
    Ok(())
}

fn apply_landlock(bun_dir: &Path, mod_root: &Path) {
    // Ruleset::default() already uses BestEffort compatibility mode in landlock 0.4 —
    // unsupported rules are silently skipped rather than causing an error.
    let abi = ABI::V4;
    let read_only = AccessFs::ReadFile | AccessFs::ReadDir;
    let read_write = AccessFs::from_all(abi);

    // System paths that executables (dynamic linker, libc) live in — need Execute so
    // the kernel can load the ELF interpreter when execve-ing Bun.
    let exec_sys = AccessFs::ReadFile | AccessFs::ReadDir | AccessFs::Execute;

    let result: anyhow::Result<()> = (|| {
        let mut created = Ruleset::default()
            .handle_access(AccessFs::from_all(abi))?
            .handle_access(AccessNet::from_all(abi))?
            .create()?;

        // System paths: read + execute (dynamic linker /lib64/ld-linux-x86-64.so.2 needs
        // Execute, and it resolves to a path under /usr on this distro).
        let sys_paths: &[&str] = &[
            "/usr",
            "/lib",
            "/lib64",
            "/lib32",
            "/etc",
            "/dev",
            "/proc/self",
            "/run",
            "/sys",
        ];
        for &path_str in sys_paths {
            let path = Path::new(path_str);
            if path.exists() {
                created = created.add_rule(PathBeneath::new(PathFd::new(path)?, exec_sys))?;
            }
        }

        // Bun install dir: read + execute (bun binary + its internal runtime files).
        // Subprocesses spawned by a mod inherit the same Landlock restrictions, so
        // even if the mod runs another binary it stays within the sandbox.
        if bun_dir.exists() {
            created = created.add_rule(PathBeneath::new(PathFd::new(bun_dir)?, exec_sys))?;
        }

        // Mod root: read-only (script entry + sibling .ts/.js imports)
        if mod_root.exists() {
            created = created.add_rule(PathBeneath::new(PathFd::new(mod_root)?, read_only))?;
        }

        // /tmp: read-write (Bun needs this for its transpilation cache)
        let tmp = Path::new("/tmp");
        if tmp.exists() {
            created = created.add_rule(PathBeneath::new(PathFd::new(tmp)?, read_write))?;
        }

        // No AccessNet allow-rules added → all TCP bind/connect is blocked.

        let status = created.restrict_self()?;
        eprintln!("[sandbox] Landlock applied: ruleset={:?}", status.ruleset);
        Ok(())
    })();

    if let Err(e) = result {
        eprintln!("[sandbox] Landlock setup failed (sandbox may be weaker): {e:#}");
    }
}

fn apply_seccomp() {
    let result: anyhow::Result<()> = (|| {
        // Denylist: block specific dangerous syscalls; all others are allowed.
        // socket() is filtered by address family to block IPv4/IPv6 while
        // preserving AF_UNIX (needed by Bun's internal IPC).
        let rules: BTreeMap<i64, Vec<SeccompRule>> = BTreeMap::from([
            (libc::SYS_ptrace, vec![]),
            (libc::SYS_mount, vec![]),
            (libc::SYS_umount2, vec![]),
            (libc::SYS_pivot_root, vec![]),
            (libc::SYS_chroot, vec![]),
            (libc::SYS_kexec_load, vec![]),
            (libc::SYS_kexec_file_load, vec![]),
            (libc::SYS_reboot, vec![]),
            // Block IPv4 and IPv6 socket creation (belt-and-suspenders alongside Landlock net rules)
            (
                libc::SYS_socket,
                vec![
                    SeccompRule::new(vec![SeccompCondition::new(
                        0,
                        SeccompCmpArgLen::Dword,
                        SeccompCmpOp::Eq,
                        libc::AF_INET as u64,
                    )?])?,
                    SeccompRule::new(vec![SeccompCondition::new(
                        0,
                        SeccompCmpArgLen::Dword,
                        SeccompCmpOp::Eq,
                        libc::AF_INET6 as u64,
                    )?])?,
                ],
            ),
        ]);

        let arch = std::env::consts::ARCH.try_into().map_err(|_| {
            anyhow::anyhow!("seccomp: unsupported arch '{}'", std::env::consts::ARCH)
        })?;

        let filter = SeccompFilter::new(
            rules,
            SeccompAction::Allow,
            SeccompAction::Errno(libc::EPERM as u32),
            arch,
        )?;

        let prog: BpfProgram = filter.try_into()?;
        seccompiler::apply_filter(&prog)?;
        Ok(())
    })();

    if let Err(e) = result {
        eprintln!("[sandbox] seccomp setup failed (continuing without syscall filter): {e:#}");
    }
}

fn drop_capabilities(mod_id: &str) {
    unsafe {
        let securebits = SECBIT_NOROOT
            | SECBIT_NOROOT_LOCKED
            | SECBIT_NO_SETUID_FIXUP
            | SECBIT_NO_SETUID_FIXUP_LOCKED;
        if libc::prctl(PR_SET_SECUREBITS, securebits, 0u64, 0u64, 0u64) != 0 {
            eprintln!(
                "[sandbox:{mod_id}] PR_SET_SECUREBITS failed (errno {})",
                *libc::__errno_location()
            );
        }
        if libc::prctl(PR_CAP_AMBIENT, PR_CAP_AMBIENT_CLEAR_ALL, 0u64, 0u64, 0u64) != 0 {
            eprintln!(
                "[sandbox:{mod_id}] PR_CAP_AMBIENT_CLEAR_ALL failed (errno {})",
                *libc::__errno_location()
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linux_restrictions_block_engine_files_and_network_sockets() {
        // Landlock cannot reliably identify Windows-mounted 9p files under WSL.
        // Keep both the allowed and forbidden fixtures on the Linux filesystem.
        let cache = dirs::cache_dir().expect("cache directory");
        let root = cache.join(format!("whatever-policy-test-{}", std::process::id()));
        let mod_root = root.join("mod");
        std::fs::create_dir_all(&mod_root).expect("mod fixture");
        std::fs::write(mod_root.join("mod.toml"), "[mod]\nid = 'fixture'\n")
            .expect("allowed fixture");
        std::fs::write(root.join("Cargo.toml"), "secret").expect("forbidden fixture");
        assert!(std::fs::read_to_string(root.join("Cargo.toml")).is_ok());
        let cfg = SandboxConfig {
            mod_id: "linux_policy_test".to_owned(),
            mod_root: mod_root.canonicalize().expect("mod root"),
            runtime_executable: std::path::PathBuf::from("/usr/bin/python3"),
        };
        let source = r#"
import socket, sys
assert open(sys.argv[1]).read().startswith("[mod]")
try:
    open(sys.argv[2]).read()
except PermissionError:
    pass
else:
    raise AssertionError("engine files were exposed")
try:
    socket.socket(socket.AF_INET, socket.SOCK_STREAM)
except PermissionError:
    pass
else:
    raise AssertionError("network sockets were exposed")
print("file and network denied")
"#;
        let mut command = Command::new(&cfg.runtime_executable);
        command
            .arg("-c")
            .arg(source)
            .arg(cfg.mod_root.join("mod.toml"))
            .arg(root.join("Cargo.toml"));
        apply_pre_spawn(&mut command, &cfg).expect("install restrictions");
        let output = command.output().expect("spawn restricted fixture");
        assert_eq!(
            String::from_utf8_lossy(&output.stdout).trim(),
            "file and network denied",
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        std::fs::remove_dir_all(&root).expect("remove kernel fixtures");
    }
}
