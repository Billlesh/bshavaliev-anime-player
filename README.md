# Anime Player v1

Windows player for local anime files. The original plays immediately in mpv. A single GPU worker interpolates to 60 FPS at source resolution (vsmlrt TensorRT RIFE, FP16), upscales with Real-ESRGAN anime to 2560×1440, and writes an NVENC HEVC MKV. When that cache file is ready, playback switches to it at the current timestamp.

This is not a library, not a catalog, and not a live RIFE filter. AMD/Intel are out of v1.

## Locked target

| | |
| --- | --- |
| OS | Windows 10/11 |
| GPU | NVIDIA only (RTX 4070, 12 GB) |
| Shell | Tauri + portable mpv / libmpv JSON IPC |
| Interpolate | VapourSynth + vsmlrt `Backend.TRT(fp16=True)`, RIFE v4.6, **source resolution** |
| Upscale | Real-ESRGAN `animevideov3` → 2560×1440 (aspect preserved, letterboxed) |
| Encode | `hevc_nvenc` in MKV |
| Audio / subs | **always** `-c:a copy -c:s copy` (attachments copied too). Never transcoded or mixed. |
| Playback shaders | mpv GLSL: light deband + line enhance (Anime4K-style presets, original GLSL) |

## Run on Windows (4070)

### 1. Portable folder

Zip layout next to the exe (this is how v1 ships):

```
AnimePlayer/
  AnimePlayer.exe
  pipeline/                 (Python job runner — already in this repo)
  portable/
    PORTABLE
    mpv/                    drop mpv.exe + mpv-2.dll here
      mpv.conf
      shaders/              already in this repo
    ffmpeg/                 drop ffmpeg.exe + ffprobe.exe (NVENC build)
    vapoursynth/            portable VS + VSPipe.exe + vsmlrt
    cuda/                   CUDA runtime DLLs
    tensorrt/               TensorRT nvinfer DLLs
    models/
      rife/                 vsmlrt RIFE v4.6 weights
      realesrgan/           RealESRGAN animevideov3 weights
      trt-engines/          created on first run
  cache/                    portable cache (also used when PORTABLE marker exists)
```

Installed (non-portable) cache is `%LOCALAPPDATA%\AnimePlayer\cache`. Delete files there by hand in v1.

### 2. Drop-in binaries (do not fork)

Wrap and parameterize; do not vendor RIFE/vsmlrt/ESRGAN source.

1. [mpv windows build](https://mpv.io/) → `portable/mpv/` (`mpv.exe`, `mpv-2.dll`).
2. ffmpeg **with** `hevc_nvenc` → `portable/ffmpeg/`.
3. Portable [VapourSynth](https://www.vapoursynth.com/) + [vs-mlrt](https://github.com/AmusementClub/vs-mlrt) → `portable/vapoursynth/`.
4. Matching CUDA runtime + TensorRT into `portable/cuda` and `portable/tensorrt`.
5. vsmlrt model files for RIFE v4.6 and RealESRGAN animevideov3 into `portable/models/…`.

Then:

```powershell
powershell -File scripts\setup-windows.ps1
```

### 3. Start the app

**Prebuilt portable:** double-click `AnimePlayer.exe`.

**From this repo (dev):**

```powershell
# Need Node 20+, Rust 1.90+ (stable), WebView2 (already on Win10/11)
npm install
npm run tauri dev
```

Release:

```powershell
npm run tauri build
powershell -File scripts\pack-portable.ps1
```

Open a file or folder. Original playback starts at once. The queue runs **one GPU job at a time**. When the current file’s cache is ready, mpv loads the HEVC and seeks to the same time (pause state is kept). Shaders on/off toggles the GLSL pair.

First 4070 job at a given resolution builds TensorRT engines into `portable/models/trt-engines`. That pass is slow; later jobs reuse the engines.

## Pipeline (what the worker runs)

```
vspipe -c y4m job.vpy -  |  ffmpeg -i pipe:0 -i SOURCE
  -map 0:v:0 -map 1:a? -map 1:s? -map 1:t?
  -c:v hevc_nvenc -preset p5 -cq 23 -profile:v main10
  -c:a copy -c:s copy -c:t copy
  cache\<key>.mkv
```

`pipeline/ffmpeg_cmd.py` refuses to emit a non-`copy` audio or subtitle codec. Cache key is source fingerprint + pipeline params; a hit skips the GPU.

Dev-only (no NVIDIA): `ANIME_PLAYER_PIPELINE=remux` remuxes with `-c copy` so the queue, manifest, and switch-to-cache path can be tested. That is not the product encode.

## UI (v1)

Open file, open folder, queue + progress, play/pause, shaders on/off. No library.

## What still needs the 4070

This repo was authored on a Linux cloud VM. The following cannot be proven here:

- TensorRT engine build and vsmlrt RIFE/ESRGAN at 1080p/720p
- `hevc_nvenc` encode and NVDEC playback
- Real episode timing (RIFE → ESRGAN → NVENC) and VRAM headroom at 12 GB
- mpv `gpu-api=d3d11` + `hwdec=nvdec`
- Tauri + WebView2 windowing (GTK/WebKit dev packages are not on this VM)

Linux checks that **do** run: `scripts/smoke-linux.sh` (Python argv lock, player-core queue/cache tests, optional ffmpeg remux).
