use super::SandboxConfig;
use landlock::{
    ABI, Access, AccessFs, AccessNet, PathBeneath, PathFd, Ruleset, RulesetAttr,
    RulesetCreatedAttr,
};
use seccompiler::{
    BpfProgram, SeccompAction, SeccompCmpArgLen, SeccompCmpOp, SeccompCondition, SeccompFilter,
    SeccompRule,
};
use std::collections::BTreeMap;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
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
    let bun_bin = match detect_bun_binary() {
        Some(p) => p,
        None => {
            tracing::warn!(
                mod_id = %cfg.mod_id,
                "sandbox: could not detect bun binary; Landlock will not be applied"
            );
            return Ok(());
        }
    };

    // Derive the bun home directory from the binary path:
    // ~/.bun/bin/bun → ~/.bun   or   /usr/local/bin/bun → /usr/local
    let bun_dir = bun_bin
        .parent()  // .../bin/
        .and_then(|p| p.parent())  // install root
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"));

    let mod_root = cfg.mod_root.clone();
    let mod_data_dir = cfg.mod_data_dir.clone();
    let engine_root = cfg.engine_root.clone();
    let mod_id = cfg.mod_id.clone();

    // Safety: the closure runs in the forked child before exec. Landlock, seccomp,
    // and prctl are all syscall-based and safe in a post-fork pre-exec context.
    unsafe {
        cmd.pre_exec(move || {
            apply_landlock(&bun_dir, &mod_root, &mod_data_dir, &engine_root);
            apply_seccomp();
            drop_capabilities(&mod_id);
            Ok(())
        });
    }
    Ok(())
}

fn apply_landlock(
    bun_dir: &Path,
    mod_root: &Path,
    mod_data_dir: &Path,
    engine_root: &Path,
) {
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
            "/usr", "/lib", "/lib64", "/lib32", "/etc", "/dev", "/proc/self", "/run", "/sys",
        ];
        for &path_str in sys_paths {
            let path = Path::new(path_str);
            if path.exists() {
                created =
                    created.add_rule(PathBeneath::new(PathFd::new(path)?, exec_sys))?;
            }
        }

        // Bun install dir: read + execute (bun binary + its internal runtime files).
        // Subprocesses spawned by a mod inherit the same Landlock restrictions, so
        // even if the mod runs another binary it stays within the sandbox.
        if bun_dir.exists() {
            created = created.add_rule(PathBeneath::new(PathFd::new(bun_dir)?, exec_sys))?;
        }

        // Engine root: read-only (node_modules/@whatever/api lives here)
        if engine_root.exists() {
            created =
                created.add_rule(PathBeneath::new(PathFd::new(engine_root)?, read_only))?;
        }

        // Mod root: read-only (script entry + sibling .ts/.js imports)
        if mod_root.exists() {
            created =
                created.add_rule(PathBeneath::new(PathFd::new(mod_root)?, read_only))?;
        }

        // Mod data dir: full read-write (the only place a mod can persist data)
        if mod_data_dir.exists() {
            created =
                created.add_rule(PathBeneath::new(PathFd::new(mod_data_dir)?, read_write))?;
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
        if libc::prctl(
            PR_CAP_AMBIENT,
            PR_CAP_AMBIENT_CLEAR_ALL,
            0u64,
            0u64,
            0u64,
        ) != 0
        {
            eprintln!(
                "[sandbox:{mod_id}] PR_CAP_AMBIENT_CLEAR_ALL failed (errno {})",
                *libc::__errno_location()
            );
        }
    }
}

fn detect_bun_binary() -> Option<PathBuf> {
    // 1. Check $BUN_INSTALL env var (set by the official bun installer)
    if let Ok(bun_install) = std::env::var("BUN_INSTALL") {
        let p = PathBuf::from(bun_install).join("bin").join("bun");
        if p.exists() {
            return std::fs::canonicalize(&p).ok().or(Some(p));
        }
    }
    // 2. Default user-install path: ~/.bun/bin/bun
    if let Some(home) = dirs::home_dir() {
        let p = home.join(".bun").join("bin").join("bun");
        if p.exists() {
            return std::fs::canonicalize(&p).ok().or(Some(p));
        }
    }
    // 3. Fall back to PATH lookup via shell
    let output = std::process::Command::new("sh")
        .args(["-c", "which bun 2>/dev/null"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let s = std::str::from_utf8(&output.stdout).ok()?.trim().to_owned();
    if s.is_empty() {
        return None;
    }
    let p = PathBuf::from(s);
    Some(std::fs::canonicalize(&p).unwrap_or(p))
}
