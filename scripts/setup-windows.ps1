# Verify the portable Windows / RTX 4070 layout. Does not download CUDA, TRT, or models.
$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
if (-not (Test-Path (Join-Path $Root "portable\PORTABLE"))) {
    $Root = Get-Location
}

function Ok($msg) { Write-Host "[ok]  $msg" -ForegroundColor Green }
function Bad($msg) { Write-Host "[!!]  $msg" -ForegroundColor Yellow }

Write-Host "Anime Player — Windows 4070 setup check"
Write-Host "root: $Root"

$nvidia = Get-Command nvidia-smi -ErrorAction SilentlyContinue
if ($nvidia) {
    nvidia-smi -L
    Ok "nvidia-smi found"
} else {
    Bad "nvidia-smi missing. Install an NVIDIA Game Ready/Studio driver before TRT/NVENC."
}

$need = @(
    @{ Path = "portable\mpv\mpv.exe"; Hint = "Copy mpv windows build here (includes mpv-2.dll / libmpv)" },
    @{ Path = "portable\ffmpeg\ffmpeg.exe"; Hint = "ffmpeg with hevc_nvenc" },
    @{ Path = "portable\ffmpeg\ffprobe.exe"; Hint = "ffprobe next to ffmpeg" },
    @{ Path = "portable\vapoursynth\VSPipe.exe"; Hint = "Portable VapourSynth + vsmlrt" },
    @{ Path = "pipeline\run_job.py"; Hint = "Repo pipeline scripts" },
    @{ Path = "portable\mpv\shaders\line_enhance.glsl"; Hint = "GLSL presets" },
    @{ Path = "portable\mpv\shaders\deband.glsl"; Hint = "GLSL presets" }
)

foreach ($n in $need) {
    $p = Join-Path $Root $n.Path
    if (Test-Path $p) { Ok $n.Path } else { Bad "$($n.Path) — $($n.Hint)" }
}

$rife = Get-ChildItem (Join-Path $Root "portable\models\rife") -File -ErrorAction SilentlyContinue | Where-Object { $_.Name -ne ".gitkeep" }
$esr = Get-ChildItem (Join-Path $Root "portable\models\realesrgan") -File -ErrorAction SilentlyContinue | Where-Object { $_.Name -ne ".gitkeep" }
if ($rife) { Ok "RIFE models present" } else { Bad "Drop vsmlrt RIFE v4.6 models into portable\models\rife (wrap vs-mlrt; do not fork)" }
if ($esr) { Ok "Real-ESRGAN models present" } else { Bad "Drop RealESRGAN animevideov3 into portable\models\realesrgan" }

$ff = Join-Path $Root "portable\ffmpeg\ffmpeg.exe"
if (Test-Path $ff) {
    $enc = & $ff -hide_banner -encoders 2>$null | Select-String "hevc_nvenc"
    if ($enc) { Ok "hevc_nvenc in ffmpeg" } else { Bad "ffmpeg has no hevc_nvenc — need an NVIDIA Windows build" }
}

Write-Host ""
Write-Host "Audio/subs: the pipeline asserts -c:a copy -c:s copy. Never pass an audio encoder."
Write-Host "First full job on a 4070 compiles TensorRT engines into portable\models\trt-engines (slow, once per resolution)."
