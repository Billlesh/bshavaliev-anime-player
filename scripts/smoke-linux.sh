#!/usr/bin/env bash
# What this Linux VM can prove without CUDA, VS, mpv, or NVENC.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

echo "== python pipeline tests =="
python3 -m unittest discover -s pipeline/tests -p 'test_*.py' -v

echo "== player-core tests =="
cargo test --manifest-path crates/player-core/Cargo.toml --offline 2>/dev/null \
  || cargo test --manifest-path crates/player-core/Cargo.toml

echo "== ffmpeg argv lock (full mode) =="
python3 pipeline/ffmpeg_cmd.py --source /tmp/in.mkv --output /tmp/out.mkv --mode full >/tmp/ff.json
python3 - <<'PY'
import json
args = json.load(open("/tmp/ff.json"))
assert args[args.index("-c:a")+1] == "copy"
assert args[args.index("-c:s")+1] == "copy"
assert args[args.index("-c:v")+1] == "hevc_nvenc"
print("full mode:", " ".join(args[-16:]))
PY

echo "== remux smoke if ffmpeg exists =="
if command -v ffmpeg >/dev/null && command -v ffprobe >/dev/null; then
  SAMPLE="$ROOT/cache/smoke-src.mkv"
  OUT="$ROOT/cache/smoke-out.mkv"
  mkdir -p "$ROOT/cache"
  ffmpeg -hide_banner -y -f lavfi -i testsrc2=size=320x240:rate=24:duration=1 \
    -f lavfi -i sine=frequency=440:duration=1 \
    -c:v libx264 -pix_fmt yuv420p -c:a aac "$SAMPLE"
  python3 pipeline/run_job.py --params /dev/stdin <<JSON
{
  "source": "$SAMPLE",
  "output": "$OUT",
  "mode": "remux",
  "ffmpeg": "ffmpeg",
  "ffprobe": "ffprobe",
  "vspipe": "vspipe",
  "trt_cache": "$ROOT/portable/models/trt-engines",
  "params": {
    "rife_model": "v4.6",
    "rife_fp16": true,
    "target_fps": 60,
    "esrgan_model": "animevideov3",
    "target_width": 2560,
    "target_height": 1440,
    "nvenc_preset": "p5",
    "nvenc_cq": 23,
    "nvenc_profile": "main10",
    "trt_workspace_mb": 2048,
    "preserve_aspect": true,
    "copy_audio": true,
    "copy_subs": true,
    "copy_attachments": true
  }
}
JSON
  echo "remux wrote $(stat -c%s "$OUT") bytes"
  ffprobe -v error -show_entries stream=codec_type,codec_name -of compact "$OUT"
else
  echo "ffmpeg not in PATH; skipped remux"
fi

echo "Linux smoke finished. Full RIFE/TRT/NVENC still needs the 4070."
