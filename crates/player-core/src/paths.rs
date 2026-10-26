use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::APP_NAME;

#[derive(Debug, Error)]
pub enum PortableError {
    #[error("could not locate executable directory")]
    NoExeDir,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathLayout {
    pub exe_dir: PathBuf,
    pub portable_root: PathBuf,
    pub portable: bool,
    pub cache_dir: PathBuf,
    pub mpv_dir: PathBuf,
    pub shader_dir: PathBuf,
    pub models_dir: PathBuf,
    pub trt_engine_dir: PathBuf,
    pub pipeline_dir: PathBuf,
}

impl PathLayout {
    pub fn discover(exe_dir: PathBuf) -> Self {
        let portable_root = exe_dir.join("portable");
        let marker = portable_root.join("PORTABLE");
        let env_portable = std::env::var_os("ANIME_PLAYER_PORTABLE").is_some();
        let portable = marker.exists() || env_portable || portable_root.is_dir();

        let cache_dir = if portable {
            exe_dir.join("cache")
        } else {
            local_app_data().join(APP_NAME).join("cache")
        };

        Self {
            shader_dir: portable_root.join("mpv").join("shaders"),
            mpv_dir: portable_root.join("mpv"),
            models_dir: portable_root.join("models"),
            trt_engine_dir: portable_root.join("models").join("trt-engines"),
            pipeline_dir: pipeline_dir(&exe_dir),
            portable_root,
            portable,
            cache_dir,
            exe_dir,
        }
    }

    pub fn from_env() -> Result<Self, PortableError> {
        let exe = std::env::current_exe().map_err(|_| PortableError::NoExeDir)?;
        let exe_dir = exe.parent().ok_or(PortableError::NoExeDir)?.to_path_buf();
        Ok(Self::discover(exe_dir))
    }

    /// Dev checkout: `src-tauri` binary lives under target/, so walk up to the repo root.
    pub fn from_dev_or_exe() -> Result<Self, PortableError> {
        if let Ok(root) = std::env::var("ANIME_PLAYER_ROOT") {
            return Ok(Self::discover(PathBuf::from(root)));
        }
        let exe = std::env::current_exe().map_err(|_| PortableError::NoExeDir)?;
        let mut dir = exe.parent().ok_or(PortableError::NoExeDir)?.to_path_buf();
        for _ in 0..8 {
            if dir.join("portable").join("PORTABLE").exists()
                && dir.join("pipeline").join("run_job.py").exists()
            {
                return Ok(Self::discover(dir));
            }
            if !dir.pop() {
                break;
            }
        }
        Self::from_env()
    }

    pub fn mpv_bin(&self) -> PathBuf {
        first_existing(&[
            self.mpv_dir.join("mpv.exe"),
            self.mpv_dir.join("mpv"),
            self.portable_root.join("mpv").join("mpv.com"),
        ])
        .unwrap_or_else(|| PathBuf::from(if cfg!(windows) { "mpv.exe" } else { "mpv" }))
    }

    pub fn ffmpeg_bin(&self) -> PathBuf {
        first_existing(&[
            self.portable_root.join("ffmpeg").join("ffmpeg.exe"),
            self.portable_root.join("ffmpeg").join("ffmpeg"),
        ])
        .unwrap_or_else(|| PathBuf::from(if cfg!(windows) { "ffmpeg.exe" } else { "ffmpeg" }))
    }

    pub fn ffprobe_bin(&self) -> PathBuf {
        first_existing(&[
            self.portable_root.join("ffmpeg").join("ffprobe.exe"),
            self.portable_root.join("ffmpeg").join("ffprobe"),
        ])
        .unwrap_or_else(|| PathBuf::from(if cfg!(windows) { "ffprobe.exe" } else { "ffprobe" }))
    }

    pub fn vspipe_bin(&self) -> PathBuf {
        first_existing(&[
            self.portable_root.join("vapoursynth").join("VSPipe.exe"),
            self.portable_root.join("vapoursynth").join("vspipe"),
        ])
        .unwrap_or_else(|| PathBuf::from(if cfg!(windows) { "VSPipe.exe" } else { "vspipe" }))
    }

    pub fn python_bin(&self) -> PathBuf {
        first_existing(&[
            self.portable_root.join("vapoursynth").join("python.exe"),
            self.portable_root.join("python").join("python.exe"),
            self.portable_root.join("python").join("python"),
        ])
        .unwrap_or_else(|| PathBuf::from(if cfg!(windows) { "python.exe" } else { "python3" }))
    }

    pub fn run_job_py(&self) -> PathBuf {
        self.pipeline_dir.join("run_job.py")
    }

    pub fn shader_files(&self) -> Vec<PathBuf> {
        vec![
            self.shader_dir.join("deband.glsl"),
            self.shader_dir.join("line_enhance.glsl"),
        ]
    }

    pub fn job_env(&self) -> Vec<(String, String)> {
        let mut pairs = Vec::new();
        let extra = [
            self.portable_root.join("cuda"),
            self.portable_root.join("tensorrt"),
            self.portable_root.join("vapoursynth"),
            self.portable_root.join("ffmpeg"),
            self.mpv_dir.clone(),
        ];
        let extra_s = extra
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join(if cfg!(windows) { ";" } else { ":" });
        if let Some(path) = std::env::var_os("PATH") {
            pairs.push((
                "PATH".into(),
                format!("{extra_s}{}{}", if cfg!(windows) { ";" } else { ":" }, path.to_string_lossy()),
            ));
        } else {
            pairs.push(("PATH".into(), extra_s));
        }
        pairs.push((
            "CUDA_PATH".into(),
            self.portable_root.join("cuda").to_string_lossy().into_owned(),
        ));
        let py = self.portable_root.join("python");
        let vs = self.portable_root.join("vapoursynth");
        pairs.push((
            "PYTHONPATH".into(),
            [py, vs, self.pipeline_dir.clone()]
                .into_iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join(if cfg!(windows) { ";" } else { ":"}),
        ));
        pairs.push((
            "VSMLRT_CACHE_PATH".into(),
            self.trt_engine_dir.to_string_lossy().into_owned(),
        ));
        pairs
    }
}

fn pipeline_dir(exe_dir: &Path) -> PathBuf {
    if exe_dir.join("pipeline").join("run_job.py").exists() {
        exe_dir.join("pipeline")
    } else {
        // bundled next to portable/
        exe_dir.join("pipeline")
    }
}

fn local_app_data() -> PathBuf {
    if let Some(p) = std::env::var_os("LOCALAPPDATA") {
        return PathBuf::from(p);
    }
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home).join(".local").join("share");
    }
    PathBuf::from(".")
}

fn first_existing(cands: &[PathBuf]) -> Option<PathBuf> {
    cands.iter().find(|p| p.exists()).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn portable_marker_uses_local_cache() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("portable")).unwrap();
        fs::write(root.join("portable").join("PORTABLE"), "PORTABLE\n").unwrap();
        let layout = PathLayout::discover(root.to_path_buf());
        assert!(layout.portable);
        assert_eq!(layout.cache_dir, root.join("cache"));
    }
}
