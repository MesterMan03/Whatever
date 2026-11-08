mod api;
mod host;
pub mod ipc;
mod render_updates;
mod runtime;

pub use whatever_script_vm::{EmbeddedLimits, EmbeddedRuntime};

pub use api::{EngineContext, dispatch, mod_data_root};
pub use host::ScriptHost;
pub use render_updates::RenderCommand;
pub use runtime::{compile_sources, inspect_components, load_component_types};
