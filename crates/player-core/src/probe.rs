use std::path::PathBuf;
use std::process::Command;

use serde::{Deserialize, Serialize};

use crate::paths::PathLayout;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvProbe {
    pub windows: bool,
    pub portable: bool,
    pub cache_dir: PathBuf,
    pub mpv: Option<PathBuf>,
    pub ffmpeg: Option<PathBuf>,
    pub ffprobe: Option<PathBuf>,
    pub vspipe: Option<PathBuf>,
    pub python: Option<PathBuf>,
    pub run_job: Option<PathBuf>,
    pub shaders: Vec<PathBuf>,
    pub rife_models: bool,
    pub esrgan_models: bool,
    pub nvidia_smi: bool,
    pub nvenc_hevc: Option<bool>,
    pub pipeline_mode: String,
    pub notes: Vec<String>,
}

pub fn probe_layout(layout: &PathLayout) -> EnvProbe {
    let mpv = existing_bin(&layout.mpv_bin());
    let ffmpeg = existing_bin(&layout.ffmpeg_bin());
    let ffprobe = existing_bin(&layout.ffprobe_bin());
    let vspipe = existing_bin(&layout.vspipe_bin());
    let python = existing_bin(&layout.python_bin());
    let run_job = layout.run_job_py().exists().then(|| layout.run_job_py());
    let shaders = layout
        .shader_files()
        .into_iter()
        .filter(|p| p.exists())
        .collect();
    let rife_models = dir_has_files(&layout.models_dir.join("rife"));
    let esrgan_models = dir_has_files(&layout.models_dir.join("realesrgan"));
    let nvidia_smi = Command::new("nvidia-smi").arg("-L").output().ok().map(|o| o.status.success()).unwrap_or(false);
    let nvenc_hevc = ffmpeg.as_ref().map(|ff| encoder_listed(ff, "hevc_nvenc"));

    let mut notes = Vec::new();
    if !cfg!(windows) {
        notes.push("This binary is not on Windows. Full RIFE/TRT/NVENC needs the RTX 4070 machine.".into());
    }
    if mpv.is_none() {
        notes.push("mpv not found. Drop mpv.exe into portable/mpv/ (ships libmpv as mpv-2.dll).".into());
    }
    if ffmpeg.is_none() {
        notes.push("ffmpeg not found. Drop a build with hevc_nvenc into portable/ffmpeg/.".into());
    }
    if vspipe.is_none() {
        notes.push("VSPipe not found. Install portable VapourSynth + vsmlrt into portable/vapoursynth/.".into());
    }
    if !rife_models || !esrgan_models {
        notes.push("Drop vsmlrt RIFE v4.6 and RealESRGAN animevideov3 models into portable/models/ (do not fork the repos; wrap them).".into());
    }
    if matches!(nvenc_hevc, Some(false)) {
        notes.push("This ffmpeg build has no hevc_nvenc. Need an NVIDIA-enabled Windows build.".into());
    }

    EnvProbe {
        windows: cfg!(windows),
        portable: layout.portable,
        cache_dir: layout.cache_dir.clone(),
        mpv,
        ffmpeg,
        ffprobe,
        vspipe,
        python,
        run_job,
        shaders,
        rife_models,
        esrgan_models,
        nvidia_smi,
        nvenc_hevc,
        pipeline_mode: format!("{:?}", crate::params::PipelineMode::from_env()).to_ascii_lowercase(),
        notes,
    }
}

fn existing_bin(path: &PathBuf) -> Option<PathBuf> {
    if path.exists() {
        return Some(path.clone());
    }
    // PATH lookup by asking the shell would be racy; check `which` via Command.
    let name = path.file_name()?.to_string_lossy().into_owned();
    which_cmd(&name)
}

fn which_cmd(name: &str) -> Option<PathBuf> {
    let tool = if cfg!(windows) { "where" } else { "which" };
    let out = Command::new(tool).arg(name).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let line = String::from_utf8_lossy(&out.stdout);
    line.lines().next().map(|s| PathBuf::from(s.trim())).filter(|p| p.as_os_str().len() > 0)
}

fn encoder_listed(ffmpeg: &PathBuf, name: &str) -> bool {
    let out = Command::new(ffmpeg).args(["-hide_banner", "-encoders"]).output();
    match out {
        Ok(o) => String::from_utf8_lossy(&o.stdout).contains(name),
        Err(_) => false,
    }
}

fn dir_has_files(dir: &std::path::Path) -> bool {
    fs_has_non_gitkeep(dir)
}

fn fs_has_non_gitkeep(dir: &std::path::Path) -> bool {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return false;
    };
    rd.flatten().any(|e| {
        let n = e.file_name();
        n != ".gitkeep" && n != "." && n != ".."
    })
}
