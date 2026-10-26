# Portable runtime (Windows)

This folder is the v1 distro layout. The `PORTABLE` marker makes the app use `../cache` next to the exe instead of `%LOCALAPPDATA%\AnimePlayer\cache`.

Drop **binaries and models** here. Do not fork RIFE, vs-mlrt, or Real-ESRGAN into this tree — wrap the official builds.

| Path | What to put here |
| --- | --- |
| `mpv/mpv.exe`, `mpv/mpv-2.dll` | mpv / libmpv. `mpv.conf` and `shaders/` already ship. |
| `ffmpeg/ffmpeg.exe`, `ffprobe.exe` | Must list `hevc_nvenc`. |
| `vapoursynth/` | `VSPipe.exe`, VS Python, vsmlrt, source plugins (BestSource / lsmas / ffms2). |
| `cuda/` | CUDA runtime DLLs matching the vsmlrt wheel. |
| `tensorrt/` | `nvinfer*.dll` and friends. |
| `models/rife/` | vsmlrt RIFE v4.6 |
| `models/realesrgan/` | RealESRGAN animevideov3 |
| `models/trt-engines/` | Filled on first 4070 run |

`PATH` inside a job is prefixed with `cuda`, `tensorrt`, `vapoursynth`, `ffmpeg`, `mpv`. `VSMLRT_CACHE_PATH` points at `models/trt-engines`.
