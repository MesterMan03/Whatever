#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

/// The same small launcher is renamed for each game. It carries no engine or mod runtime code.
fn launch() -> std::io::Result<std::process::ExitStatus> {
    let executable = std::env::current_exe()?;
    let directory = executable
        .parent()
        .ok_or_else(|| std::io::Error::other("launcher has no directory"))?;
    let engine = directory
        .join("engine")
        .join(format!("Whatever{}", std::env::consts::EXE_SUFFIX));
    let mut command = std::process::Command::new(engine);
    command
        .args(std::env::args_os().skip(1))
        .current_dir(directory);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000); // The engine creates its own window.
    }
    command.status()
}

fn main() {
    match launch() {
        Ok(status) => std::process::exit(status.code().unwrap_or(1)),
        Err(error) => {
            use std::io::Write;
            let _ = writeln!(std::io::stderr(), "Could not launch Whatever: {error}");
            std::process::exit(1);
        }
    }
}
