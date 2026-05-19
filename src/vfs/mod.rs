mod disk;
mod layered;

pub use disk::DiskLayer;
pub use layered::LayeredVfs;

use std::sync::Arc;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct VfsPath {
    pub mod_id: String,
    pub path: String,
}

impl VfsPath {
    pub fn parse(s: &str) -> Option<Self> {
        let (mod_id, path) = s.split_once("://")?;
        Some(VfsPath { mod_id: mod_id.to_owned(), path: path.to_owned() })
    }
    pub fn to_string(&self) -> String {
        format!("{}://{}", self.mod_id, self.path)
    }
}

#[derive(Debug, Error)]
#[allow(dead_code)]
pub enum VfsError {
    #[error("path not found: {0}")]
    NotFound(String),
    #[error("invalid VFS path: {0}")]
    InvalidPath(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

pub trait Vfs: Send + Sync {
    fn read(&self, path: &VfsPath) -> Result<Vec<u8>, VfsError>;
    fn exists(&self, path: &VfsPath) -> bool;
    fn list(&self, mod_id: &str, prefix: &str) -> Result<Vec<String>, VfsError>;
}

pub type VfsHandle = Arc<dyn Vfs>;