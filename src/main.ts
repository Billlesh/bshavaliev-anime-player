import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";

type JobStatus =
  | { status: "queued" }
  | { status: "running"; progress: { stage: string; percent: number; message: string } }
  | { status: "ready"; output: string; cache_hit: boolean }
  | { status: "failed"; error: string };

type Job = {
  id: { "0"?: string } | string;
  source: string;
  display_name: string;
  status: JobStatus;
};

type Snapshot = { jobs: Job[]; running: unknown };
type NowPlaying = { source: string; playing: string; kind: string; paused: boolean };
type EnvProbe = { notes: string[]; cache_dir: string; pipeline_mode: string; windows: boolean };

const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;

function renderNow(np: NowPlaying | null) {
  const box = $("now-playing");
  const badge = $("source-badge");
  const pp = $("btn-pp") as HTMLButtonElement;
  if (!np) {
    badge.dataset.kind = "none";
    badge.textContent = "idle";
    pp.disabled = true;
    pp.textContent = "Play";
    return;
  }
  badge.dataset.kind = np.kind;
  badge.textContent = np.kind;
  pp.disabled = false;
  pp.textContent = np.paused ? "Play" : "Pause";
  const file = np.playing.split(/[\\/]/).pop() ?? np.playing;
  box.innerHTML = `<p class="title">${escapeHtml(file)}</p>
    <p class="muted">${np.kind === "cache" ? "Playing QHD 60 FPS cache" : "Playing original · preprocess running if queued"}</p>`;
}

function renderQueue(jobs: Job[]) {
  const ul = $("queue");
  if (!jobs.length) {
    ul.innerHTML = `<li class="empty">Nothing queued.</li>`;
    return;
  }
  ul.innerHTML = jobs
    .map((j) => {
      const { label, pct, cls, detail } = statusView(j.status);
      return `<li>
        <span class="name">${escapeHtml(j.display_name)}</span>
        <span class="meta">${escapeHtml(label)}${detail ? " · " + escapeHtml(detail) : ""}</span>
        <div class="bar ${cls}"><span style="width:${pct}%"></span></div>
      </li>`;
    })
    .join("");
}

function statusView(st: JobStatus): { label: string; pct: number; cls: string; detail: string } {
  if (st.status === "queued") return { label: "queued", pct: 0, cls: "", detail: "" };
  if (st.status === "running") {
    return {
      label: st.progress.stage,
      pct: Math.max(2, Math.min(99, st.progress.percent)),
      cls: "",
      detail: `${Math.round(st.progress.percent)}%`,
    };
  }
  if (st.status === "ready") {
    return { label: st.cache_hit ? "cache hit" : "ready", pct: 100, cls: "ready", detail: "" };
  }
  return { label: "failed", pct: 100, cls: "failed", detail: st.error };
}

function renderEnv(probe: EnvProbe) {
  const el = $("env");
  if (!probe.notes.length) {
    el.hidden = true;
    return;
  }
  el.hidden = false;
  el.innerHTML = `<div>Cache ${escapeHtml(probe.cache_dir)} · mode ${escapeHtml(probe.pipeline_mode)}</div>
    <ul>${probe.notes.map((n) => `<li>${escapeHtml(n)}</li>`).join("")}</ul>`;
}

function escapeHtml(s: string): string {
  return s.replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]!));
}

async function pick(directory: boolean) {
  const selected = await open({
    directory,
    multiple: false,
    title: directory ? "Open folder" : "Open file",
    filters: directory ? undefined : [{ name: "Video", extensions: ["mkv", "mp4", "m2ts", "ts", "avi", "webm", "mov"] }],
  });
  if (!selected || Array.isArray(selected)) return;
  const np = await invoke<NowPlaying>("open_media", { path: selected });
  renderNow(np);
  const snap = await invoke<Snapshot>("snapshot");
  renderQueue(snap.jobs);
}

async function boot() {
  $("btn-open").addEventListener("click", () => void pick(false));
  $("btn-folder").addEventListener("click", () => void pick(true));
  $("btn-pp").addEventListener("click", async () => {
    const paused = await invoke<boolean>("toggle_pause");
    const np = await invoke<NowPlaying | null>("now_playing");
    if (np) {
      np.paused = paused;
      renderNow(np);
    }
  });
  const shaders = $("shaders") as HTMLInputElement;
  shaders.addEventListener("change", () => {
    void invoke("set_shaders", { enabled: shaders.checked });
  });
  document.addEventListener("keydown", (e) => {
    if (e.code === "Space" && e.target === document.body) {
      e.preventDefault();
      $("btn-pp").click();
    }
  });

  try {
    const probe = await invoke<EnvProbe>("probe");
    renderEnv(probe);
    shaders.checked = await invoke<boolean>("shaders_on");
    renderNow(await invoke<NowPlaying | null>("now_playing"));
    const snap = await invoke<Snapshot>("snapshot");
    renderQueue(snap.jobs);
  } catch (err) {
    renderEnv({
      notes: [`Shell is not running inside Tauri (${String(err)}). Use npm run tauri dev on Windows.`],
      cache_dir: "",
      pipeline_mode: "unknown",
      windows: false,
    });
  }

  await listen<NowPlaying>("now-playing", (e) => renderNow(e.payload));
  await listen<EnvProbe>("env-probe", (e) => renderEnv(e.payload));
  await listen<{ type?: string; jobs?: Job[] }>("queue-event", async () => {
    const snap = await invoke<Snapshot>("snapshot");
    renderQueue(snap.jobs);
  });
}

void boot();
