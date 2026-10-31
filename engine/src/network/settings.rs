use anyhow::{Context, Result, ensure};
use std::path::PathBuf;

/// Operator-owned paths. Script requests can never select files to read.
#[derive(Clone, Debug, Default)]
pub struct NetworkSettings {
    pub directory: PathBuf,
    pub certificate: Option<PathBuf>,
    pub private_key: Option<PathBuf>,
    pub authority: Option<PathBuf>,
}
impl NetworkSettings {
    pub fn for_game(game: &str) -> Result<Self> {
        ensure!(
            crate::sandbox::valid_storage_id(game),
            "invalid game storage ID"
        );
        Ok(Self {
            // Outside every mod's broker-accessible data directory.
            directory: dirs::data_local_dir()
                .context("locate local data directory")?
                .join("WhateverNetwork")
                .join(game),
            ..Self::default()
        })
    }
}
