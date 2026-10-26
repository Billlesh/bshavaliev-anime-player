use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, mpsc, Mutex};
use uuid::Uuid;

use crate::cache::{record_for, CacheIndex};
use crate::hash::cache_key;
use crate::params::{PipelineMode, PipelineParams};
use crate::paths::PathLayout;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct JobId(pub Uuid);

impl JobId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum JobStage {
    Queued,
    Probing,
    Encoding,
    Manifest,
    Ready,
    Failed,
    CacheHit,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobProgress {
    pub stage: JobStage,
    pub percent: f32,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum JobStatus {
    Queued,
    Running { progress: JobProgress },
    Ready { output: PathBuf, cache_hit: bool },
    Failed { error: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    pub id: JobId,
    pub source: PathBuf,
    pub display_name: String,
    pub params: PipelineParams,
    pub status: JobStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum QueueEvent {
    Snapshot { jobs: Vec<Job> },
    Progress { id: JobId, progress: JobProgress },
    Ready { id: JobId, source: PathBuf, output: PathBuf, cache_hit: bool },
    Failed { id: JobId, error: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueSnapshot {
    pub jobs: Vec<Job>,
    pub running: Option<JobId>,
}

pub trait GpuRunner: Send + Sync + 'static {
    fn run(
        &self,
        job: Job,
        layout: PathLayout,
        mode: PipelineMode,
        progress: mpsc::UnboundedSender<JobProgress>,
        cancel: Arc<AtomicBool>,
    ) -> impl std::future::Future<Output = Result<PathBuf, String>> + Send;
}

struct Inner {
    jobs: Vec<Job>,
    pending: VecDeque<JobId>,
    running: Option<JobId>,
}

#[derive(Clone)]
pub struct QueueHandle {
    inner: Arc<Mutex<Inner>>,
    events: broadcast::Sender<QueueEvent>,
    wake: mpsc::UnboundedSender<()>,
}

impl QueueHandle {
    pub fn start<R: GpuRunner>(layout: PathLayout, runner: R) -> Self {
        let (events, _) = broadcast::channel(64);
        let (wake_tx, mut wake_rx) = mpsc::unbounded_channel();
        let inner = Arc::new(Mutex::new(Inner {
            jobs: Vec::new(),
            pending: VecDeque::new(),
            running: None,
        }));
        let handle = Self {
            inner: inner.clone(),
            events: events.clone(),
            wake: wake_tx,
        };
        let worker_handle = handle.clone();
        tokio::spawn(async move {
            let runner = Arc::new(runner);
            let mut cache = CacheIndex::open(layout.cache_dir.clone()).ok();
            loop {
                let next = {
                    let mut g = inner.lock().await;
                    if g.running.is_some() {
                        None
                    } else {
                        g.pending.pop_front()
                    }
                };
                if let Some(id) = next {
                    run_one(&worker_handle, &runner, &layout, &mut cache, id).await;
                    continue;
                }
                if wake_rx.recv().await.is_none() {
                    break;
                }
            }
        });
        handle
    }

    pub fn subscribe(&self) -> broadcast::Receiver<QueueEvent> {
        self.events.subscribe()
    }

    pub async fn snapshot(&self) -> QueueSnapshot {
        let g = self.inner.lock().await;
        QueueSnapshot {
            jobs: g.jobs.clone(),
            running: g.running,
        }
    }

    pub async fn enqueue(&self, source: PathBuf, params: PipelineParams) -> JobId {
        let id = JobId::new();
        let display_name = source
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| source.to_string_lossy().into_owned());
        let job = Job {
            id,
            source,
            display_name,
            params,
            status: JobStatus::Queued,
        };
        {
            let mut g = self.inner.lock().await;
            // Skip duplicate in-flight/queued source with same params.
            if let Some(existing) = g.jobs.iter().find(|j| {
                j.source == job.source
                    && j.params == job.params
                    && !matches!(j.status, JobStatus::Failed { .. })
            }) {
                return existing.id;
            }
            g.pending.push_back(id);
            g.jobs.push(job);
        }
        let _ = self.wake.send(());
        let _ = self.events.send(QueueEvent::Snapshot {
            jobs: self.snapshot().await.jobs,
        });
        id
    }
}

async fn run_one<R: GpuRunner>(
    handle: &QueueHandle,
    runner: &Arc<R>,
    layout: &PathLayout,
    cache: &mut Option<CacheIndex>,
    id: JobId,
) {
    let job = {
        let mut g = handle.inner.lock().await;
        g.running = Some(id);
        g.jobs.iter().find(|j| j.id == id).cloned()
    };
    let Some(job) = job else {
        handle.inner.lock().await.running = None;
        return;
    };

    let key = match cache_key(&job.source, &job.params) {
        Ok(k) => k,
        Err(e) => {
            fail(handle, id, e.to_string()).await;
            return;
        }
    };

    if let Some(idx) = cache.as_ref() {
        if let Some(hit) = idx.lookup(&key) {
            let output = hit.output.clone();
            set_ready(handle, id, output.clone(), true).await;
            return;
        }
    }

    set_running(
        handle,
        id,
        JobProgress {
            stage: JobStage::Probing,
            percent: 0.0,
            message: "starting".into(),
        },
    )
    .await;

    let (tx, mut rx) = mpsc::unbounded_channel();
    let h2 = handle.clone();
    tokio::spawn(async move {
        while let Some(p) = rx.recv().await {
            set_running(&h2, id, p).await;
        }
    });

    let cancel = Arc::new(AtomicBool::new(false));
    let result = runner
        .run(
            job.clone(),
            layout.clone(),
            PipelineMode::from_env(),
            tx,
            cancel,
        )
        .await;

    match result {
        Ok(output) => {
            if let Some(idx) = cache.as_mut() {
                let rec = record_for(&job.source, &key, output.clone(), job.params.clone());
                let _ = idx.insert(&key, rec);
            }
            set_ready(handle, id, output, false).await;
        }
        Err(e) => fail(handle, id, e).await,
    }
}

async fn set_running(handle: &QueueHandle, id: JobId, progress: JobProgress) {
    {
        let mut g = handle.inner.lock().await;
        if let Some(j) = g.jobs.iter_mut().find(|j| j.id == id) {
            j.status = JobStatus::Running {
                progress: progress.clone(),
            };
        }
    }
    let _ = handle.events.send(QueueEvent::Progress { id, progress });
}

async fn set_ready(handle: &QueueHandle, id: JobId, output: PathBuf, cache_hit: bool) {
    let source = {
        let mut g = handle.inner.lock().await;
        g.running = None;
        let src = g
            .jobs
            .iter_mut()
            .find(|j| j.id == id)
            .map(|j| {
                j.status = JobStatus::Ready {
                    output: output.clone(),
                    cache_hit,
                };
                j.source.clone()
            });
        src
    };
    let _ = handle.wake.send(());
    if let Some(source) = source {
        let _ = handle.events.send(QueueEvent::Ready {
            id,
            source,
            output,
            cache_hit,
        });
    }
}

async fn fail(handle: &QueueHandle, id: JobId, error: String) {
    {
        let mut g = handle.inner.lock().await;
        g.running = None;
        if let Some(j) = g.jobs.iter_mut().find(|j| j.id == id) {
            j.status = JobStatus::Failed { error: error.clone() };
        }
    }
    let _ = handle.wake.send(());
    let _ = handle.events.send(QueueEvent::Failed { id, error });
}

/// Test helper: records overlapping runs. Production runner lives in src-tauri.
#[cfg(test)]
struct FnRunner<F> {
    f: F,
}

#[cfg(test)]
impl<F, Fut> GpuRunner for FnRunner<F>
where
    F: Fn(Job) -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = Result<PathBuf, String>> + Send + 'static,
{
    fn run(
        &self,
        job: Job,
        _layout: PathLayout,
        _mode: PipelineMode,
        _progress: mpsc::UnboundedSender<JobProgress>,
        _cancel: Arc<AtomicBool>,
    ) -> impl std::future::Future<Output = Result<PathBuf, String>> + Send {
        (self.f)(job)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    #[tokio::test]
    async fn one_gpu_job_at_a_time() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("portable")).unwrap();
        std::fs::write(dir.path().join("portable").join("PORTABLE"), b"x").unwrap();
        let layout = PathLayout::discover(dir.path().to_path_buf());
        let current = Arc::new(AtomicUsize::new(0));
        let max = Arc::new(AtomicUsize::new(0));
        let c2 = current.clone();
        let m2 = max.clone();
        let runner = FnRunner {
            f: move |job: Job| {
                let c2 = c2.clone();
                let m2 = m2.clone();
                async move {
                    let n = c2.fetch_add(1, Ordering::SeqCst) + 1;
                    m2.fetch_max(n, Ordering::SeqCst);
                    tokio::time::sleep(Duration::from_millis(80)).await;
                    c2.fetch_sub(1, Ordering::SeqCst);
                    Ok(job.source)
                }
            },
        };
        let q = QueueHandle::start(layout, runner);
        let params = PipelineParams::default();
        for i in 0..3 {
            let f = dir.path().join(format!("a{i}.mkv"));
            std::fs::write(&f, [i as u8; 32]).unwrap();
            q.enqueue(f, params.clone()).await;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
        assert_eq!(max.load(Ordering::SeqCst), 1, "GPU jobs overlapped");
        let snap = q.snapshot().await;
        assert!(snap
            .jobs
            .iter()
            .all(|j| matches!(j.status, JobStatus::Ready { .. })));
    }

    #[tokio::test]
    async fn cache_hit_skips_runner() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("portable")).unwrap();
        std::fs::write(dir.path().join("portable").join("PORTABLE"), b"x").unwrap();
        let src = dir.path().join("ep.mkv");
        std::fs::write(&src, b"hello-episode").unwrap();
        let layout = PathLayout::discover(dir.path().to_path_buf());
        std::fs::create_dir_all(&layout.cache_dir).unwrap();
        let params = PipelineParams::default();
        let key = cache_key(&src, &params).unwrap();
        let mut idx = CacheIndex::open(layout.cache_dir.clone()).unwrap();
        let out = idx.output_path(&key);
        std::fs::write(&out, b"cached").unwrap();
        idx.insert(&key, record_for(&src, &key, out.clone(), params.clone()))
            .unwrap();

        let ran = Arc::new(AtomicBool::new(false));
        let ran2 = ran.clone();
        let runner = FnRunner {
            f: move |job: Job| {
                let ran2 = ran2.clone();
                async move {
                    ran2.store(true, Ordering::SeqCst);
                    Ok(job.source)
                }
            },
        };
        let q = QueueHandle::start(layout, runner);
        q.enqueue(src, params).await;
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert!(!ran.load(Ordering::SeqCst), "runner must not start on cache hit");
        let snap = q.snapshot().await;
        match &snap.jobs[0].status {
            JobStatus::Ready { cache_hit, .. } => assert!(*cache_hit),
            other => panic!("{other:?}"),
        }
    }
}
