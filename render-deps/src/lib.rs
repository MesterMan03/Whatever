//! Shared graphics dependencies for the co-built engine, editor and player.
pub use {egui, egui_wgpu, egui_winit, glyphon, wgpu, winit};

// Reuse the VM library's common Rust dependencies throughout the shared-library graph.
use whatever_script_vm as _;
