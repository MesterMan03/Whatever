use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct GameMeta {
    pub game: GameInfo,
}

#[derive(Debug, Deserialize)]
pub struct GameInfo {
    pub id: String,
    pub name: String,
    #[serde(default = "default_tick_rate")]
    pub tick_rate: f64,
}

fn default_tick_rate() -> f64 {
    60.0
}

impl Default for GameMeta {
    fn default() -> Self {
        GameMeta {
            game: GameInfo {
                id: "unknown".into(),
                name: "Whatever".into(),
                tick_rate: default_tick_rate(),
            },
        }
    }
}
