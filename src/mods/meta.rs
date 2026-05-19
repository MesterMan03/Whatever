use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct GameMeta {
    pub game: GameInfo,
}

#[derive(Debug, Deserialize)]
pub struct GameInfo {
    pub id: String,
    pub name: String,
}

impl Default for GameMeta {
    fn default() -> Self {
        GameMeta {
            game: GameInfo {
                id: "unknown".into(),
                name: "Whatever".into(),
            },
        }
    }
}
