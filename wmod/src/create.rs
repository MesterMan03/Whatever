use inquire::{Confirm, Select, Text, validator::Validation};
use std::path::Path;
use whatever_engine::project::{ModOptions, create_mod};

/// Preserve the interactive command, using the same scaffold as the editor.
pub fn run(directory: &str) -> anyhow::Result<()> {
    let id = Text::new("Mod ID:")
        .with_help_message("lowercase letters, digits, underscores or hyphens")
        .with_validator(|input: &str| {
            Ok(
                if whatever_engine::sandbox::valid_storage_id(input) && input != "core" {
                    Validation::Valid
                } else {
                    Validation::Invalid("Choose a valid mod ID other than core".into())
                },
            )
        })
        .prompt()?;
    let name = Text::new("Name:").with_default(&id).prompt()?;
    let mut options = ModOptions::new(&id, &name);
    options.version = Text::new("Version:").with_default("0.1.0").prompt()?;
    options.description = Text::new("Description:").prompt()?;
    let author = Text::new("Author:").prompt()?;
    if !author.is_empty() {
        options.authors.push(author);
    }
    options.license = Select::new(
        "License:",
        vec!["MIT", "Apache-2.0", "GPL-3.0", "MPL-2.0", "Other"],
    )
    .prompt()?
    .into();
    options.scripts = Confirm::new("Include scripts and TypeScript configuration?")
        .with_default(true)
        .prompt()?;
    let root = create_mod(Path::new(directory), &options)?;
    tracing::info!(path=%root.display(), "created mod");
    Ok(())
}
