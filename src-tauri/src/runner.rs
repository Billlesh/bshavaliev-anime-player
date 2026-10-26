use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use player_core::{
    cache_key, CacheIndex, GpuRunner, Job, JobProgress, JobStage, PipelineMode, PathLayout,
};
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;

pub struct PythonRunner;

impl GpuRunner for PythonRunner {
    fn run(
        &self,
        job: Job,
        layout: PathLayout,
        mode: PipelineMode,
        progress: mpsc::UnboundedSender<JobProgress>,
        _cancel: Arc<AtomicBool>,
    ) -> impl std::future::Future<Output = Result<PathBuf, String>> + Send {
        async move { run_python_job(job, layout, mode, progress).await }
    }
}

async fn run_python_job(
    job: Job,
    layout: PathLayout,
    mode: PipelineMode,
    progress: mpsc::UnboundedSender<JobProgress>,
) -> Result<PathBuf, String> {
    job.params
        .validate()
        .map_err(|e| e)?;
    std::fs::create_dir_all(&layout.cache_dir).map_err(|e| e.to_string())?;
    let key = cache_key(&job.source, &job.params).map_err(|e| e.to_string())?;
    let idx = CacheIndex::open(layout.cache_dir.clone()).map_err(|e| e.to_string())?;
    let output = idx.output_path(&key);

    let spec = json!({
        "source": job.source,
        "output": output,
        "mode": match mode {
            PipelineMode::Full => "full",
            PipelineMode::Remux => "remux",
        },
        "params": job.params,
        "ffmpeg": layout.ffmpeg_bin(),
        "ffprobe": layout.ffprobe_bin(),
        "vspipe": layout.vspipe_bin(),
        "trt_cache": layout.trt_engine_dir,
    });
    let spec_path = layout
        .cache_dir
        .join(format!("job-{}.json", job.id.0));
    tokio::fs::write(&spec_path, serde_json::to_vec_pretty(&spec).unwrap())
        .await
        .map_err(|e| e.to_string())?;

    let mut cmd = Command::new(layout.python_bin());
    cmd.arg(layout.run_job_py())
        .arg("--params")
        .arg(&spec_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    for (k, v) in layout.job_env() {
        cmd.env(k, v);
    }
    if matches!(mode, PipelineMode::Remux) {
        cmd.env("ANIME_PLAYER_PIPELINE", "remux");
    }

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("failed to start pipeline python: {e}"))?;
    let stdout = child.stdout.take().ok_or("no stdout from pipeline")?;
    let mut lines = BufReader::new(stdout).lines();
    let mut last_err: Option<String> = None;

    while let Ok(Some(line)) = lines.next_line().await {
        let Ok(v) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let stage = v.get("stage").and_then(|s| s.as_str()).unwrap_or("encoding");
        let percent = v.get("percent").and_then(|p| p.as_f64()).unwrap_or(0.0) as f32;
        let message = v
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or(stage)
            .to_string();
        let mapped = match stage {
            "probing" => JobStage::Probing,
            "done" => JobStage::Ready,
            "error" => JobStage::Failed,
            "writing_manifest" | "manifest" => JobStage::Manifest,
            _ => JobStage::Encoding,
        };
        if stage == "error" {
            last_err = Some(message.clone());
        }
        let _ = progress.send(JobProgress {
            stage: mapped,
            percent,
            message,
        });
    }

    let status = child.wait().await.map_err(|e| e.to_string())?;
    let _ = tokio::fs::remove_file(&spec_path).await;
    if !status.success() {
        let err = match child.stderr.take() {
            Some(mut err) => {
                let mut buf = String::new();
                use tokio::io::AsyncReadExt;
                let _ = err.read_to_string(&mut buf).await;
                buf
            }
            None => String::new(),
        };
        return Err(last_err
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| format!("pipeline exit {status}: {err}")));
    }
    if !output.exists() {
        return Err(format!("pipeline reported success but {} is missing", output.display()));
    }
    Ok(output)
}
