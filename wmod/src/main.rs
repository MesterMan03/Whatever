mod create;

use owo_colors::OwoColorize;

const LOGO: &str = r#"
 ___       ___  _____ ______   ________  ________
|\  \     |\  \|\   _ \  _   \|\   __  \|\   ___ \
\ \  \    \ \  \ \  \\\__\ \  \ \  \|\  \ \  \_|\ \
 \ \  \  __\ \  \ \  \\|__| \  \ \  \\\  \ \  \ \\ \
  \ \  \|\__\_\  \ \  \    \ \  \ \  \\\  \ \  \_\\ \
   \ \____________\ \__\    \ \__\ \_______\ \_______\
    \|____________|\|__|     \|__|\|_______|\|_______|
"#;

fn print_logo() {
    println!("{}", LOGO.cyan().bold());
    println!("  {}", "Whatever Mod Toolkit".bright_white().bold());
    println!();
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();

    match args.get(1).map(String::as_str) {
        Some("create") => {
            let dir = args.get(2).map(String::as_str).unwrap_or(".");
            create::run(dir)?;
        }
        _ => {
            print_logo();
            println!("  {}", "Usage:".bright_white().bold());
            println!(
                "    wmod {}  create a new mod in <directory>",
                "create <directory>".green()
            );
            println!();
        }
    }

    Ok(())
}
