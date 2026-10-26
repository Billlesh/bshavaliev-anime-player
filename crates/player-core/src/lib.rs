//! Shared engine for the Tauri shell: pipeline identity, cache, and a
//! single-slot GPU queue. No windowing, no libmpv — those stay in `src-tauri`.

mod cache;
mod hash;
mod params;
mod paths;
mod probe;
mod queue;

pub use cache::{CacheIndex, CacheRecord, Manifest};
pub use hash::{cache_key, fingerprint_file, pipeline_id, CacheKey};
pub use params::{
    rife_plan, FfmpegVideo, PipelineMode, PipelineParams, RifePlan, TARGET_FPS, TARGET_HEIGHT,
    TARGET_WIDTH,
};
pub use paths::{PathLayout, PortableError};
pub use probe::{probe_layout, EnvProbe};
pub use queue::{
    GpuRunner, Job, JobId, JobProgress, JobStage, JobStatus, QueueEvent, QueueHandle, QueueSnapshot,
};

pub const APP_NAME: &str = "AnimePlayer";
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
