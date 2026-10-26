use serde::{Deserialize, Serialize};

pub const TARGET_WIDTH: u32 = 2560;
pub const TARGET_HEIGHT: u32 = 1440;
pub const TARGET_FPS: u32 = 60;

/// Locked v1 preprocess. Changing any field invalidates the cache key.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PipelineParams {
    pub rife_model: String,
    pub rife_fp16: bool,
    pub target_fps: u32,
    pub esrgan_model: String,
    pub target_width: u32,
    pub target_height: u32,
    pub nvenc_preset: String,
    pub nvenc_cq: u8,
    pub nvenc_profile: String,
    pub trt_workspace_mb: u32,
    pub preserve_aspect: bool,
    /// Always true in v1. Stored so a future encoder cannot silently drop it.
    pub copy_audio: bool,
    pub copy_subs: bool,
    pub copy_attachments: bool,
}

impl Default for PipelineParams {
    fn default() -> Self {
        Self {
            rife_model: "v4.6".into(),
            rife_fp16: true,
            target_fps: TARGET_FPS,
            esrgan_model: "animevideov3".into(),
            target_width: TARGET_WIDTH,
            target_height: TARGET_HEIGHT,
            nvenc_preset: "p5".into(),
            nvenc_cq: 23,
            nvenc_profile: "main10".into(),
            trt_workspace_mb: 2048,
            preserve_aspect: true,
            copy_audio: true,
            copy_subs: true,
            copy_attachments: true,
        }
    }
}

impl PipelineParams {
    pub fn validate(&self) -> Result<(), String> {
        if !self.copy_audio {
            return Err("v1 forbids transcoding audio: copy_audio must be true".into());
        }
        if !self.copy_subs {
            return Err("v1 forbids transcoding subtitles: copy_subs must be true".into());
        }
        if !self.rife_fp16 {
            return Err("v1 TensorRT RIFE is FP16-only".into());
        }
        if self.target_width != TARGET_WIDTH || self.target_height != TARGET_HEIGHT {
            return Err(format!(
                "v1 target is {TARGET_WIDTH}x{TARGET_HEIGHT}"
            ));
        }
        if self.target_fps != TARGET_FPS {
            return Err(format!("v1 target fps is {TARGET_FPS}"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PipelineMode {
    /// VapourSynth RIFE + Real-ESRGAN + NVENC HEVC. Product path.
    Full,
    /// ffmpeg remux, every stream copied. Linux/dev smoke only.
    Remux,
}

impl PipelineMode {
    pub fn from_env() -> Self {
        match std::env::var("ANIME_PLAYER_PIPELINE")
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str()
        {
            "remux" => Self::Remux,
            _ => Self::Full,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FfmpegVideo {
    pub codec: String,
    pub preset: String,
    pub cq: u8,
    pub profile: String,
    pub pix_fmt: String,
}

impl FfmpegVideo {
    pub fn nvenc(params: &PipelineParams) -> Self {
        Self {
            codec: "hevc_nvenc".into(),
            preset: params.nvenc_preset.clone(),
            cq: params.nvenc_cq,
            profile: params.nvenc_profile.clone(),
            pix_fmt: "p010le".into(),
        }
    }
}

/// How to get from source fps to 60 with integer RIFE `multi`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RifePlan {
    Skip { src_fps: f64, reason: String },
    /// `output_fps = src * multi`.
    Multiply { src_fps: f64, multi: u32 },
    /// RIFE `multi`, then `SelectEvery(cycle, 0)` so the result lands on 60.
    MultiplyThenThin {
        src_fps: f64,
        multi: u32,
        cycle: u32,
        result_fps: f64,
    },
}

pub fn rife_plan(src_fps: f64, target_fps: u32) -> RifePlan {
    let target = target_fps as f64;
    if src_fps <= 0.0 {
        return RifePlan::Skip {
            src_fps,
            reason: "unknown source fps".into(),
        };
    }
    if src_fps >= target - 0.5 {
        return RifePlan::Skip {
            src_fps,
            reason: "already at or above target fps".into(),
        };
    }

    // 23.976 / 24 → 2.5x = 5/2 (RIFE ×5 = 120, keep even frames → 60)
    if approx(src_fps, 24.0) || approx(src_fps, 23.976) {
        return RifePlan::MultiplyThenThin {
            src_fps,
            multi: 5,
            cycle: 2,
            result_fps: src_fps * 5.0 / 2.0,
        };
    }
    // 29.97 / 30 → 2x
    if approx(src_fps, 30.0) || approx(src_fps, 29.97) {
        return RifePlan::Multiply {
            src_fps,
            multi: 2,
        };
    }
    // 25 → 2.4x = 12/5. Heavy; still exact. Rare for anime.
    if approx(src_fps, 25.0) {
        return RifePlan::MultiplyThenThin {
            src_fps,
            multi: 12,
            cycle: 5,
            result_fps: src_fps * 12.0 / 5.0,
        };
    }

    let multi = ((target / src_fps).round() as u32).max(2);
    RifePlan::Multiply { src_fps, multi }
}

fn approx(a: f64, b: f64) -> bool {
    (a - b).abs() < 0.08
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_locked_v1() {
        let p = PipelineParams::default();
        p.validate().unwrap();
        assert!(p.copy_audio && p.copy_subs && p.rife_fp16);
        assert_eq!((p.target_width, p.target_height, p.target_fps), (2560, 1440, 60));
        assert_eq!(p.rife_model, "v4.6");
        assert_eq!(p.esrgan_model, "animevideov3");
    }

    #[test]
    fn rejects_audio_transcode_flag() {
        let mut p = PipelineParams::default();
        p.copy_audio = false;
        assert!(p.validate().unwrap_err().contains("audio"));
    }

    #[test]
    fn rife_24_to_60_is_5_then_thin() {
        match rife_plan(23.976, 60) {
            RifePlan::MultiplyThenThin { multi, cycle, .. } => {
                assert_eq!((multi, cycle), (5, 2));
            }
            other => panic!("{other:?}"),
        }
        match rife_plan(24.0, 60) {
            RifePlan::MultiplyThenThin { multi, cycle, .. } => {
                assert_eq!((multi, cycle), (5, 2));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn rife_30_to_60_is_2x() {
        match rife_plan(29.97, 60) {
            RifePlan::Multiply { multi, .. } => assert_eq!(multi, 2),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn rife_skips_60() {
        match rife_plan(59.94, 60) {
            RifePlan::Skip { .. } => {}
            other => panic!("{other:?}"),
        }
    }
}
