//! Drive portable mpv (libmpv JSON IPC) as the playback engine.
//!
//! The distro ships `mpv.exe` + `mpv-2.dll`. We speak the same protocol as
//! libmpv's `mpv_command_node` over `--input-ipc-server`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{mpsc, oneshot, Mutex};
use tokio::time::timeout;

use player_core::PathLayout;

#[cfg(unix)]
use tokio::net::UnixStream;

#[cfg(windows)]
use tokio::net::windows::named_pipe::ClientOptions;

pub struct MpvPlayer {
    layout: PathLayout,
    inner: Mutex<Option<Live>>,
    ipc_path: PathBuf,
}

struct Live {
    _child: Child,
    req: mpsc::UnboundedSender<Pending>,
}

struct Pending {
    cmd: Value,
    tx: oneshot::Sender<Result<Value, String>>,
}

#[derive(Clone, Debug)]
pub struct PlaybackPos {
    pub time: f64,
    pub paused: bool,
}

impl MpvPlayer {
    pub fn new(layout: PathLayout) -> Self {
        let ipc_path = ipc_path();
        Self {
            layout,
            inner: Mutex::new(None),
            ipc_path,
        }
    }

    pub async fn ensure(&self) -> Result<(), String> {
        if self.inner.lock().await.is_some() {
            return Ok(());
        }
        self.spawn().await
    }

    async fn spawn(&self) -> Result<(), String> {
        let mpv = self.layout.mpv_bin();
        if cfg!(not(windows)) {
            // Still try PATH mpv on this Linux VM for IPC smoke tests.
        }
        let _ = std::fs::remove_file(&self.ipc_path);

        let mut cmd = Command::new(&mpv);
        cmd.arg(format!(
            "--input-ipc-server={}",
            ipc_server_arg(&self.ipc_path)
        ))
        .arg(format!("--config-dir={}", self.layout.mpv_dir.display()))
        .arg("--idle=yes")
        .arg("--force-window=yes")
        .arg("--keep-open=yes")
        .arg("--no-terminal")
        .arg("--osc=yes")
        .arg("--title=Anime Player")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

        let child = cmd
            .spawn()
            .map_err(|e| format!("failed to start mpv at {}: {e}", mpv.display()))?;

        let stream = connect_ipc(&self.ipc_path).await?;
        let (tx, rx) = mpsc::unbounded_channel::<Pending>();
        tokio::spawn(ipc_loop(stream, rx));
        *self.inner.lock().await = Some(Live { _child: child, req: tx });
        Ok(())
    }

    async fn rpc(&self, command: Value) -> Result<Value, String> {
        self.ensure().await?;
        let (tx, rx) = oneshot::channel();
        {
            let g = self.inner.lock().await;
            let live = g.as_ref().ok_or("mpv not running")?;
            live.req
                .send(Pending { cmd: command, tx })
                .map_err(|_| "mpv ipc closed".to_string())?;
        }
        timeout(Duration::from_secs(8), rx)
            .await
            .map_err(|_| "mpv ipc timeout".to_string())?
            .map_err(|_| "mpv ipc dropped".to_string())?
    }

    pub async fn load(&self, path: &Path) -> Result<(), String> {
        let p = path.to_string_lossy().into_owned();
        self.rpc(json!(["loadfile", p, "replace"])).await?;
        Ok(())
    }

    pub async fn pause(&self, paused: bool) -> Result<(), String> {
        self.rpc(json!(["set_property", "pause", paused])).await?;
        Ok(())
    }

    pub async fn toggle_pause(&self) -> Result<bool, String> {
        let cur = self
            .rpc(json!(["get_property", "pause"]))
            .await?
            .as_bool()
            .unwrap_or(false);
        let next = !cur;
        self.pause(next).await?;
        Ok(next)
    }

    pub async fn pos(&self) -> Result<PlaybackPos, String> {
        let time = self
            .rpc(json!(["get_property", "time-pos"]))
            .await?
            .as_f64()
            .unwrap_or(0.0);
        let paused = self
            .rpc(json!(["get_property", "pause"]))
            .await?
            .as_bool()
            .unwrap_or(false);
        Ok(PlaybackPos { time, paused })
    }

    pub async fn set_shaders(&self, files: &[PathBuf], on: bool) -> Result<(), String> {
        let list: Vec<String> = if on {
            files
                .iter()
                .filter(|p| p.exists())
                .map(|p| p.to_string_lossy().replace('\\', "/"))
                .collect()
        } else {
            Vec::new()
        };
        self.rpc(json!(["set_property", "glsl-shaders", list])).await?;
        Ok(())
    }

    /// Replace the playing file with the cache, keeping timestamp and pause.
    pub async fn switch_to_cache(&self, cache: &Path) -> Result<PlaybackPos, String> {
        let pos = self.pos().await.unwrap_or(PlaybackPos {
            time: 0.0,
            paused: false,
        });
        let p = cache.to_string_lossy().into_owned();
        self.rpc(json!(["loadfile", p, "replace"])).await?;
        tokio::time::sleep(Duration::from_millis(120)).await;
        let _ = self.rpc(json!(["set_property", "time-pos", pos.time])).await;
        self.pause(pos.paused).await?;
        Ok(pos)
    }
}

struct PendingMap {
    next: AtomicI64,
    wait: Mutex<HashMap<i64, oneshot::Sender<Result<Value, String>>>>,
}

#[cfg(unix)]
type Ipc = UnixStream;
#[cfg(windows)]
type Ipc = tokio::net::windows::named_pipe::NamedPipeClient;

async fn ipc_loop(stream: Ipc, mut rx: mpsc::UnboundedReceiver<Pending>) {
    let (reader, mut writer) = tokio::io::split(stream);
    let mut lines = BufReader::new(reader).lines();
    let map = Arc::new(PendingMap {
        next: AtomicI64::new(1),
        wait: Mutex::new(HashMap::new()),
    });
    let map_r = map.clone();

    let reader_task = tokio::spawn(async move {
        while let Ok(Some(line)) = lines.next_line().await {
            if let Ok(v) = serde_json::from_str::<Value>(&line) {
                if let Some(id) = v.get("request_id").and_then(|x| x.as_i64()) {
                    let mut g = map_r.wait.lock().await;
                    if let Some(tx) = g.remove(&id) {
                        let err = v.get("error").and_then(|e| e.as_str()).unwrap_or("success");
                        if err == "success" {
                            let _ = tx.send(Ok(v.get("data").cloned().unwrap_or(Value::Null)));
                        } else {
                            let _ = tx.send(Err(err.to_string()));
                        }
                    }
                }
            }
        }
    });

    while let Some(p) = rx.recv().await {
        let id = map.next.fetch_add(1, Ordering::SeqCst);
        map.wait.lock().await.insert(id, p.tx);
        let line = json!({"command": p.cmd, "request_id": id}).to_string() + "\n";
        if writer.write_all(line.as_bytes()).await.is_err() {
            break;
        }
    }
    let _ = reader_task.await;
}

fn ipc_path() -> PathBuf {
    let n = unique();
    if cfg!(windows) {
        PathBuf::from(format!(r"\\.\pipe\anime-player-mpv-{n}"))
    } else {
        std::env::temp_dir().join(format!("anime-player-mpv-{n}.sock"))
    }
}

fn ipc_server_arg(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn unique() -> u64 {
    static N: AtomicU64 = AtomicU64::new(0);
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    t.wrapping_add(N.fetch_add(1, Ordering::SeqCst))
}

async fn connect_ipc(path: &Path) -> Result<Ipc, String> {
    let mut last = "not attempted".to_string();
    for _ in 0..40 {
        tokio::time::sleep(Duration::from_millis(50)).await;
        match try_connect(path).await {
            Ok(s) => return Ok(s),
            Err(e) => last = e,
        }
    }
    Err(format!("could not connect to mpv ipc at {}: {last}", path.display()))
}

#[cfg(unix)]
async fn try_connect(path: &Path) -> Result<Ipc, String> {
    UnixStream::connect(path)
        .await
        .map_err(|e| e.to_string())
}

#[cfg(windows)]
async fn try_connect(path: &Path) -> Result<Ipc, String> {
    ClientOptions::new()
        .open(path)
        .map_err(|e| e.to_string())
}
