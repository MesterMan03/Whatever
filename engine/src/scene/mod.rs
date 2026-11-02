mod document;
mod references;
pub use references::{
    allows_namespace, rewrite_asset_references, validate_asset, validate_references,
};

pub use document::{LoadedScene, SceneDocument, SceneEntity, scene_path};
