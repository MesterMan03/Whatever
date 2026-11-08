use std::{
    env, fs,
    io::{self, Write},
    path::{Path, PathBuf},
    process::Command,
};

fn run_watcom(directory: &Path, program: &str, args: &[&str]) -> io::Result<()> {
    let status = Command::new(program)
        .args(args)
        .current_dir(directory)
        .status()
        .map_err(|error| {
            io::Error::new(
                error.kind(),
                format!("could not run {program}: {error}. Set up Open Watcom for DOS builds."),
            )
        })?;
    if !status.success() {
        return Err(io::Error::other(format!("{program} exited with {status}")));
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut cargo = io::stdout();
    writeln!(cargo, "cargo::rerun-if-changed=build.rs")?;
    if env::var("CARGO_CFG_TARGET_OS")? != "windows" || env::var("CARGO_CFG_TARGET_ENV")? != "msvc"
    {
        return Ok(());
    }

    let out_dir = PathBuf::from(env::var_os("OUT_DIR").ok_or("Cargo did not set OUT_DIR")?);
    for name in ["main.c", "snake.c", "snake.h", "stub.lnk"] {
        let source = Path::new("../dosstub").join(name);
        writeln!(cargo, "cargo::rerun-if-changed={}", source.display())?;
        fs::copy(source, out_dir.join(name))?;
    }
    for name in ["WATCOM", "INCLUDE", "PATH"] {
        writeln!(cargo, "cargo::rerun-if-env-changed={name}")?;
    }

    // Reserve the PE pointer before the DOS relocation table.
    run_watcom(
        &out_dir,
        "wcl",
        &["-bcl=dos", "-ms", "-c", "main.c", "snake.c"],
    )?;
    run_watcom(&out_dir, "wlink", &["@stub.lnk"])?;
    writeln!(
        cargo,
        "cargo::rustc-link-arg-bin=whatever-launcher=/STUB:{}",
        out_dir.join("snake-stub.exe").display()
    )?;
    Ok(())
}
