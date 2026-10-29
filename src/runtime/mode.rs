use serde::{Deserialize, Serialize};
use std::fmt;
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RuntimeMode {
    #[default]
    Client,
    Server,
}
impl fmt::Display for RuntimeMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Client => "client",
            Self::Server => "server",
        })
    }
}
impl std::str::FromStr for RuntimeMode {
    type Err = anyhow::Error;
    fn from_str(value: &str) -> anyhow::Result<Self> {
        match value {
            "client" => Ok(Self::Client),
            "server" => Ok(Self::Server),
            _ => anyhow::bail!("invalid runtime mode '{value}'; expected client or server"),
        }
    }
}
