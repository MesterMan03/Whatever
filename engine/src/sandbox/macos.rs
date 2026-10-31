use super::SandboxConfig;
use std::ffi::CString;
use std::os::unix::process::CommandExt;
use std::process::Command;

unsafe extern "C" {
    fn sandbox_init(
        profile: *const libc::c_char,
        flags: u64,
        errorbuf: *mut *mut libc::c_char,
    ) -> libc::c_int;
    fn sandbox_free_error(errorbuf: *mut libc::c_char);
}

pub fn apply_pre_spawn(cmd: &mut Command, cfg: &SandboxConfig) -> anyhow::Result<()> {
    let executable = cfg.runtime_executable.canonicalize()?;
    let mod_root = cfg.mod_root.canonicalize()?;
    let literal = |path: &std::path::Path| -> anyhow::Result<String> {
        let value = path
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("sandbox path is not UTF-8"))?;
        if value.contains(['"', '\\', '\0', '\n', '\r']) {
            anyhow::bail!("unsupported sandbox path");
        }
        Ok(value.to_owned())
    };
    let profile = CString::new(format!(
        r#"(version 1)
(deny default)
(allow file-read* (subpath "/usr") (subpath "/System") (subpath "/Library") (subpath "/private/etc") (subpath "/private/tmp") (subpath "/private/var/folders"))
(allow file-read* (literal "{executable}") (subpath "{mod_root}"))
(allow file-write* (subpath "/private/tmp"))
(allow process-exec (literal "{executable}"))
(deny network*)
(allow ipc-posix-shm)
(allow mach-lookup)
(allow process-info* (target self))
(allow signal (target self))
"#,
        executable = literal(&executable)?,
        mod_root = literal(&mod_root)?,
    ))?;
    let mod_id = cfg.mod_id.clone();
    // The profile is owned by the closure and applied in the child before exec.
    unsafe {
        cmd.pre_exec(move || {
            let mut error: *mut libc::c_char = std::ptr::null_mut();
            if sandbox_init(profile.as_ptr(), 0, &mut error) != 0 {
                if !error.is_null() {
                    sandbox_free_error(error);
                }
                eprintln!("[sandbox:{mod_id}] Seatbelt unavailable; native runtime policy remains required");
            }
            Ok(())
        });
    }
    Ok(())
}
