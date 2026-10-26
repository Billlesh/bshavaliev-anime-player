mod player;
mod runner;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use player_core::{
    probe_layout, EnvProbe, PathLayout, PipelineParams, QueueEvent, QueueHandle, QueueSnapshot,
};
use runner::PythonRunner;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::player::MpvPlayer;

const VIDEO_EXT: &[&str] = &[
    "mkv", "mp4", "m2ts", "mts", "ts", "avi", "webm", "mov", "wmv", "m4v", "mpg", "mpeg",
];

pub struct AppState {
    layout: PathLayout,
    queue: QueueHandle,
    player: MpvPlayer,
    now: Mutex<Option<NowPlaying>>,
    shaders_on: AtomicBool,
}

#[derive(Debug, Clone, Serialize)]
pub struct NowPlaying {
    pub source: PathBuf,
    pub playing: PathBuf,
    pub kind: String,
    pub paused: bool,
}

#[tauri::command]
fn probe(state: State<AppState>) -> EnvProbe {
    probe_layout(&state.layout)
}

#[tauri::command]
async fn snapshot(state: State<'_, AppState>) -> Result<QueueSnapshot, String> {
    Ok(state.queue.snapshot().await)
}

#[tauri::command]
fn now_playing(state: State<AppState>) -> Option<NowPlaying> {
    state.now.lock().ok().and_then(|g| g.clone())
}

#[tauri::command]
async fn open_media(app: AppHandle, state: State<'_, AppState>, path: String) -> Result<NowPlaying, String> {
    let path = PathBuf::from(path);
    if path.is_dir() {
        let files = list_videos(&path);
        if files.is_empty() {
            return Err("no video files in that folder".into());
        }
        let first = files[0].clone();
        for f in files {
            enqueue(&state, f).await;
        }
        play_original(&app, &state, first).await
    } else {
        enqueue(&state, path.clone()).await;
        play_original(&app, &state, path).await
    }
}

#[tauri::command]
async fn toggle_pause(app: AppHandle, state: State<'_, AppState>) -> Result<bool, String> {
    let paused = state.player.toggle_pause().await?;
    if let Ok(mut g) = state.now.lock() {
        if let Some(n) = g.as_mut() {
            n.paused = paused;
        }
    }
    let _ = app.emit("now-playing", now_playing_clone(&state));
    Ok(paused)
}

#[tauri::command]
async fn set_shaders(state: State<'_, AppState>, enabled: bool) -> Result<bool, String> {
    state.shaders_on.store(enabled, Ordering::SeqCst);
    let files = state.layout.shader_files();
    state.player.set_shaders(&files, enabled).await?;
    Ok(enabled)
}

#[tauri::command]
fn shaders_on(state: State<AppState>) -> bool {
    state.shaders_on.load(Ordering::SeqCst)
}

async fn enqueue(state: &AppState, path: PathBuf) {
    let params = PipelineParams::default();
    let _ = state.queue.enqueue(path, params).await;
}

async fn play_original(
    app: &AppHandle,
    state: &AppState,
    path: PathBuf,
) -> Result<NowPlaying, String> {
    state.player.ensure().await?;
    state.player.load(&path).await?;
    let on = state.shaders_on.load(Ordering::SeqCst);
    let _ = state.player.set_shaders(&state.layout.shader_files(), on).await;
    let np = NowPlaying {
        source: path.clone(),
        playing: path,
        kind: "original".into(),
        paused: false,
    };
    if let Ok(mut g) = state.now.lock() {
        *g = Some(np.clone());
    }
    let _ = app.emit("now-playing", np.clone());
    Ok(np)
}

async fn switch_if_current(app: &AppHandle, state: &AppState, source: &Path, output: PathBuf) {
    let matches = state
        .now
        .lock()
        .ok()
        .and_then(|g| g.as_ref().map(|n| n.source == source))
        .unwrap_or(false);
    if !matches {
        return;
    }
    if let Err(e) = state.player.switch_to_cache(&output).await {
        let _ = app.emit("player-error", e);
        return;
    }
    let paused = state
        .now
        .lock()
        .ok()
        .and_then(|g| g.as_ref().map(|n| n.paused))
        .unwrap_or(false);
    let np = NowPlaying {
        source: source.to_path_buf(),
        playing: output,
        kind: "cache".into(),
        paused,
    };
    if let Ok(mut g) = state.now.lock() {
        *g = Some(np.clone());
    }
    let _ = app.emit("now-playing", np);
}

fn now_playing_clone(state: &AppState) -> Option<NowPlaying> {
    state.now.lock().ok().and_then(|g| g.clone())
}

fn list_videos(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(dir) else {
        return out;
    };
    for e in rd.flatten() {
        let p = e.path();
        if !p.is_file() {
            continue;
        }
        let ext = p
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if VIDEO_EXT.contains(&ext.as_str()) {
            out.push(p);
        }
    }
    out.sort();
    out
}

fn pump_queue(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut rx = app.state::<AppState>().queue.subscribe();
        loop {
            match rx.recv().await {
                Ok(ev) => {
                    let _ = app.emit("queue-event", &ev);
                    if let QueueEvent::Ready {
                        source, output, ..
                    } = ev
                    {
                        let state = app.state::<AppState>();
                        switch_if_current(&app, &*state, &source, output).await;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let layout = PathLayout::from_dev_or_exe().unwrap_or_else(|_| {
        PathLayout::discover(std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
    });
    let _ = std::fs::create_dir_all(&layout.cache_dir);
    let queue = QueueHandle::start(layout.clone(), PythonRunner);
    let player = MpvPlayer::new(layout.clone());

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState {
            layout,
            queue,
            player,
            now: Mutex::new(None),
            shaders_on: AtomicBool::new(true),
        })
        .invoke_handler(tauri::generate_handler![
            probe,
            snapshot,
            now_playing,
            open_media,
            toggle_pause,
            set_shaders,
            shaders_on
        ])
        .setup(|app| {
            pump_queue(app.handle().clone());
            let probe = probe_layout(&app.state::<AppState>().layout);
            let _ = app.handle().emit("env-probe", probe);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("tauri run");
}
