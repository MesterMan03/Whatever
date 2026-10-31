use serde::{Deserialize, Serialize};
/// Native CPU timings in milliseconds. GPU execution is deliberately not inferred from submission.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct FrameTimings {
    pub frame_number: u64,
    pub frame_ms: f64,
    pub cpu_ms: f64,
    pub acquire_ms: f64,
    pub encode_ms: f64,
    pub submit_ms: f64,
    pub present_ms: f64,
    pub draw_calls: u32,
    pub width: u32,
    pub height: u32,
    pub vsync: bool,
    pub fps_cap: Option<f64>,
    pub adapter: String,
    pub gpu_ms: Option<f64>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct TickTimings {
    pub tick_number: u64,
    pub tick_rate: f64,
    pub total_ms: f64,
    pub scripts_ms: f64,
    pub commands_ms: f64,
    pub physics_ms: f64,
    pub presentation_ms: f64,
    pub snapshot_ms: f64,
    pub snapshot_bytes: usize,
    pub commands: usize,
    pub entities: usize,
    pub hosts: usize,
    pub build: String,
    pub platform: String,
    pub frame: Option<FrameTimings>,
}
pub fn elapsed(start: std::time::Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1000.0
}
