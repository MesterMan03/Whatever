use inquire::{Confirm, Select, Text, validator::Validation};
use owo_colors::OwoColorize;
use std::path::Path;

pub fn run(dir: &str) -> anyhow::Result<()> {
    println!("{}", "  Whatever Mod Toolkit".cyan().bold());
    println!();
    println!(
        "  {} {}",
        "Creating a new mod in".bright_white(),
        dir.bright_cyan().bold()
    );
    println!();

    let target = Path::new(dir);

    if target.join("mod.toml").exists() {
        anyhow::bail!(
            "{}",
            format!("  '{}' already contains a mod.toml — aborting.", dir).red()
        );
    }

    // --- Prompts ---

    let id = Text::new("Mod ID:")
        .with_help_message("lowercase letters, digits, and underscores only")
        .with_validator(|input: &str| {
            if input.is_empty() {
                return Ok(Validation::Invalid("Mod ID cannot be empty".into()));
            }
            if !input
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
            {
                return Ok(Validation::Invalid(
                    "Use only lowercase letters, digits, and underscores".into(),
                ));
            }
            Ok(Validation::Valid)
        })
        .prompt()?;

    let default_name = id_to_name(&id);
    let name = Text::new("Name:").with_default(&default_name).prompt()?;

    let version = Text::new("Version:")
        .with_default("0.1.0")
        .with_help_message("semver, e.g. 1.0.0")
        .prompt()?;

    let description = Text::new("Description:")
        .with_help_message("optional — press Enter to skip")
        .prompt()?;

    let author = Text::new("Author:")
        .with_help_message("optional — press Enter to skip")
        .prompt()?;

    let license = Select::new(
        "License:",
        vec!["MIT", "Apache-2.0", "GPL-3.0", "MPL-2.0", "Other"],
    )
    .with_starting_cursor(0)
    .prompt()?;

    let scripts = Confirm::new("Include scripts?")
        .with_default(true)
        .with_help_message("adds scripts/index.ts with a Bun/TypeScript starter")
        .prompt()?;

    println!();

    // --- Create directories ---

    std::fs::create_dir_all(target.join("assets"))?;
    if scripts {
        std::fs::create_dir_all(target.join("scripts"))?;
    }

    // --- Write files ---

    let mod_toml = generate_mod_toml(
        &id,
        &name,
        &version,
        &description,
        &author,
        license,
        scripts,
    );
    std::fs::write(target.join("mod.toml"), &mod_toml)?;
    std::fs::write(target.join("assets").join(".gitkeep"), "")?;

    let mut created = vec![
        format!("{}/mod.toml", dir),
        format!("{}/assets/.gitkeep", dir),
    ];

    if scripts {
        let starter = generate_script_starter();
        std::fs::write(target.join("scripts").join("index.ts"), starter)?;
        created.push(format!("{}/scripts/index.ts", dir));
    }

    // --- Success summary ---

    println!(
        "  {} {} {}",
        "Mod".green().bold(),
        name.bright_white().bold(),
        "created!".green().bold()
    );
    println!();
    println!("  {}", "Files created:".bright_white());
    for f in &created {
        println!("    {}  {}", "·".bright_black(), f.cyan());
    }
    println!();
    println!("  {}", "Next steps:".bright_white());
    println!(
        "    {}  move '{}' into mods/ or mods_user/",
        "1.".bright_black(),
        dir.cyan()
    );
    println!("    {}  run the engine", "2.".bright_black());
    println!("       {}", "cargo run".bright_black());
    println!();

    Ok(())
}

fn id_to_name(id: &str) -> String {
    id.split('_')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn generate_mod_toml(
    id: &str,
    name: &str,
    version: &str,
    description: &str,
    author: &str,
    license: &str,
    scripts: bool,
) -> String {
    let mut out = String::new();

    out += "[mod]\n";
    out += &format!("id          = \"{id}\"\n");
    out += &format!("name        = \"{name}\"\n");
    out += &format!("version     = \"{version}\"\n");
    if !description.is_empty() {
        out += &format!("description = \"{description}\"\n");
    }
    if !author.is_empty() {
        out += &format!("authors     = [\"{author}\"]\n");
    }
    out += &format!("license     = \"{license}\"\n");

    out += "\n[assets]\nroot = \"assets\"\n";

    if scripts {
        out += "\n[script]\nentry = \"scripts/index.ts\"\n";
    }

    out += "\n# [dependencies]\n# other_mod = \"^1.0\"\n";

    out
}

fn generate_script_starter() -> String {
    format!(
        r#"import {{ Engine }} from "@whatever-engine/api";

Engine.on("init", ({{ mod_id }}) => {{
  Engine.log("info", `${{mod_id}} loaded`);
}});

Engine.on("exit", () => {{
  Engine.log("info", "goodbye");
}});
"#
    )
}
