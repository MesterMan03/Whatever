mod loader;
mod manifest;
pub(crate) mod meta;
mod registry;

pub use loader::discover_and_load;
pub use manifest::ModManifest;
pub use meta::GameMeta;
pub use registry::ModRegistry;
