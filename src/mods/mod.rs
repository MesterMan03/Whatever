mod loader;
mod manifest;
mod registry;

pub use loader::discover_and_load;
pub use registry::ModRegistry;