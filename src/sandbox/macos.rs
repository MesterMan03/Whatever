use super::SandboxConfig;
use std::ffi::CString;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;

// Private Apple API — deprecated but functional on current macOS versions.
extern "C" {
    fn sandbox_init(
        profile: *const libc::c_char,
        flags: u64,
        errorbuf: *mut *mut libc::c_char,
    ) -> libc::c_int;
    fn sandbox_free_error(errorbuf: *mut libc::c_char);
}

pub fn apply_pre_spawn(cmd: &mut Command, cfg: &SandboxConfig) -> anyhow::Result<()> {
    let bun_dir = detect_bun_dir().unwrap_or_else(|| PathBuf::from("/usr/local"));

    let profile = build_profile(
        &bun_dir.to_string_lossy(),
        &cfg.engine_root.to_string_lossy(),
        &cfg.mod_root.to_string_lossy(),
        &cfg.mod_data_dir.to_string_lossy(),
    );

    let mod_id = cfg.mod_id.clone();

    // Safety: pre_exec runs in the forked child before exec. We call sandbox_init
    // which is a libc function safe to call after fork.
    unsafe {
        cmd.pre_exec(move || {
            let Ok(profile_cstr) = CString::new(profile.as_bytes()) else {
                eprintln!("[sandbox:{mod_id}] profile contained null bytes; skipping Seatbelt");
                return Ok(());
            };
            let mut errp: *mut libc::c_char = std::ptr::null_mut();
            let rc = sandbox_init(profile_cstr.as_ptr(), 0, &mut errp);
            if rc != 0 {
                if !errp.is_null() {
                    sandbox_free_error(errp);
                }
                // Seatbelt is deprecated; degrade gracefully rather than killing the mod.
                eprintln!("[sandbox:{mod_id}] sandbox_init failed (rc={rc}); running without Seatbelt");
            }
            Ok(())
        });
    }
    Ok(())
}

fn build_profile(bun_dir: &str, engine_root: &str, mod_root: &str, mod_data_dir: &str) -> String {
    // Escape paths: Seatbelt uses a Scheme-like syntax where paths appear as string literals.
    // We sanitise by refusing any path that contains '"' or '\' (extremely unlikely in practice).
    let safe = |p: &str| p.replace('\\', "").replace('"', "");

    format!(
        r#"(version 1)
(deny default)
(allow file-read* (subpath "/usr"))
(allow file-read* (subpath "/System"))
(allow file-read* (subpath "/Library"))
(allow file-read* (subpath "/private/etc"))
(allow file-read* (subpath "/private/tmp"))
(allow file-read* (subpath "/private/var/folders"))
(allow file-read* (subpath "{bun}"))
(allow file-read* (subpath "{engine}"))
(allow file-read* (subpath "{mod_root}"))
(allow file-write* (subpath "{data}"))
(allow file-write* (subpath "/private/tmp"))
(deny network*)
(deny process-exec*)
(allow ipc-posix-shm)
(allow mach-lookup)
(allow process-info* (target self))
(allow signal (target self))
"#,
        bun = safe(bun_dir),
        engine = safe(engine_root),
        mod_root = safe(mod_root),
        data = safe(mod_data_dir),
    )
}

fn detect_bun_dir() -> Option<PathBuf> {
    // 1. $BUN_INSTALL
    if let Ok(s) = std::env::var("BUN_INSTALL") {
        let p = PathBuf::from(s);
        if p.exists() {
            return Some(p);
        }
    }
    // 2. ~/.bun
    if let Some(home) = dirs::home_dir() {
        let p = home.join(".bun");
        if p.exists() {
            return Some(p);
        }
    }
    // 3. Homebrew locations
    for path in ["/opt/homebrew", "/usr/local"] {
        let p = PathBuf::from(path).join("bin").join("bun");
        if p.exists() {
            return Some(PathBuf::from(path));
        }
    }
    None
}
