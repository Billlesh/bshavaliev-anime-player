#!/usr/bin/env python3
"""Offline preprocess worker.

Full mode (Windows 4070):
  vspipe y4m | ffmpeg hevc_nvenc  with  -c:a copy -c:s copy

Remux mode (dev smoke, no GPU):
  ffmpeg -c copy  (still never transcodes audio/subs)
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
import sys
import tempfile
import threading
import time
from pathlib import Path
from typing import Any

# Allow `python pipeline/run_job.py` from repo root.
sys.path.insert(0, str(Path(__file__).resolve().parent))

from ffmpeg_cmd import assert_stream_copy, build_ffmpeg_args
from params import PipelineParams
from vpy_template import write_vpy


def emit(obj: dict[str, Any]) -> None:
    sys.stdout.write(json.dumps(obj, ensure_ascii=False) + "\n")
    sys.stdout.flush()


def ffprobe_info(ffprobe: str, source: Path) -> dict[str, Any]:
    cmd = [
        ffprobe,
        "-v",
        "error",
        "-show_entries",
        "format=duration:stream=codec_type,codec_name,width,height,avg_frame_rate",
        "-of",
        "json",
        str(source),
    ]
    out = subprocess.run(cmd, capture_output=True, text=True)
    if out.returncode != 0:
        return {"duration": 0.0, "fps": 0.0, "width": 0, "height": 0, "raw_error": out.stderr}
    data = json.loads(out.stdout or "{}")
    duration = float((data.get("format") or {}).get("duration") or 0.0)
    fps = 0.0
    width = height = 0
    for st in data.get("streams") or []:
        if st.get("codec_type") == "video":
            width = int(st.get("width") or 0)
            height = int(st.get("height") or 0)
            afr = st.get("avg_frame_rate") or "0/1"
            if "/" in afr:
                n, d = afr.split("/", 1)
                try:
                    fps = float(n) / float(d) if float(d) else 0.0
                except ValueError:
                    fps = 0.0
            break
    return {"duration": duration, "fps": fps, "width": width, "height": height}


def _read_progress(proc: subprocess.Popen[str], duration: float) -> None:
    if proc.stdout is None:
        return
    for line in proc.stdout:
        line = line.strip()
        if not line:
            continue
        if "=" not in line:
            continue
        k, _, v = line.partition("=")
        if k == "out_time_ms":
            try:
                ms = float(v)
            except ValueError:
                continue
            pct = 0.0
            if duration > 0:
                pct = max(0.0, min(99.0, (ms / 1000.0) / duration * 100.0))
            emit({"stage": "encoding", "percent": round(pct, 2), "out_time_ms": ms})
        elif k == "progress" and v == "end":
            emit({"stage": "encoding", "percent": 99.0, "message": "flushing"})


def run_remux(ffmpeg: str, source: Path, output: Path, params: PipelineParams) -> None:
    args = build_ffmpeg_args(ffmpeg, source, output, params, mode="remux")
    assert_stream_copy(args)
    emit({"stage": "encoding", "percent": 1.0, "message": "remux copy", "argv_tail": args[-12:]})
    proc = subprocess.run(args, capture_output=True, text=True)
    if proc.returncode != 0:
        raise RuntimeError(proc.stderr[-4000:] or f"ffmpeg exited {proc.returncode}")


def run_full(
    *,
    vspipe: str,
    ffmpeg: str,
    source: Path,
    output: Path,
    params: PipelineParams,
    src_fps: float,
    duration: float,
    trt_cache: Path,
) -> None:
    tmp = Path(tempfile.mkdtemp(prefix="anime-player-"))
    vpy = tmp / "job.vpy"
    write_vpy(vpy, source, params, src_fps, trt_cache)
    ff_args = build_ffmpeg_args(ffmpeg, source, output, params, mode="full")
    assert_stream_copy(ff_args)
    # Drop the ffmpeg binary from args[0] when Popen uses it as executable? keep full argv.
    emit({"stage": "encoding", "percent": 0.5, "message": "vspipe | nvenc", "vpy": str(vpy)})

    vs = subprocess.Popen(
        [vspipe, "-c", "y4m", "--progress", str(vpy), "-"],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=False,
    )
    assert vs.stdout is not None
    ff = subprocess.Popen(
        ff_args,
        stdin=vs.stdout,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )
    vs.stdout.close()

    t = threading.Thread(target=_read_progress, args=(ff, duration), daemon=True)
    t.start()

    vs_err = vs.stderr.read() if vs.stderr else b""
    ff_err = ff.stderr.read() if ff.stderr else ""
    vs_code = vs.wait()
    ff_code = ff.wait()
    t.join(timeout=2)

    shutil.rmtree(tmp, ignore_errors=True)

    if vs_code != 0 or ff_code != 0:
        vs_txt = vs_err.decode("utf-8", "replace") if isinstance(vs_err, (bytes, bytearray)) else str(vs_err)
        raise RuntimeError(
            f"vspipe={vs_code} ffmpeg={ff_code}\n{vs_txt[-2000:]}\n{ff_err[-2000:]}"
        )


def main() -> int:
    p = argparse.ArgumentParser()
    p.add_argument("--params", required=True, help="JSON job spec from the Tauri worker")
    args = p.parse_args()
    spec = json.loads(Path(args.params).read_text(encoding="utf-8"))
    try:
        source = Path(spec["source"])
        output = Path(spec["output"])
        mode = spec.get("mode") or os.environ.get("ANIME_PLAYER_PIPELINE") or "full"
        params = PipelineParams.from_mapping(spec.get("params") or {})
        ffmpeg = spec.get("ffmpeg") or "ffmpeg"
        ffprobe = spec.get("ffprobe") or "ffprobe"
        vspipe = spec.get("vspipe") or "vspipe"
        trt_cache = Path(spec.get("trt_cache") or ".")

        output.parent.mkdir(parents=True, exist_ok=True)
        emit({"stage": "probing", "percent": 0.0, "source": str(source)})
        info = ffprobe_info(ffprobe, source)
        emit({"stage": "probing", "percent": 2.0, **info})

        t0 = time.time()
        if mode == "remux":
            run_remux(ffmpeg, source, output, params)
        else:
            run_full(
                vspipe=vspipe,
                ffmpeg=ffmpeg,
                source=source,
                output=output,
                params=params,
                src_fps=float(info.get("fps") or 0.0),
                duration=float(info.get("duration") or 0.0),
                trt_cache=trt_cache,
            )
        if not output.exists() or output.stat().st_size < 32:
            raise RuntimeError(f"encoder produced no output at {output}")
        emit(
            {
                "stage": "done",
                "percent": 100.0,
                "output": str(output),
                "seconds": round(time.time() - t0, 2),
            }
        )
        return 0
    except Exception as exc:
        emit({"stage": "error", "percent": 0.0, "message": str(exc)})
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
