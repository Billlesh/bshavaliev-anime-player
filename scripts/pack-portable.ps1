# Assemble the portable zip layout next to a cargo-built exe.
$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
$Exe = Join-Path $Root "src-tauri\target\release\anime-player.exe"
if (-not (Test-Path $Exe)) {
    Write-Error "Build first: npm run tauri build   (expected $Exe)"
}
$Dist = Join-Path $Root "dist-portable\AnimePlayer"
New-Item -ItemType Directory -Force -Path $Dist | Out-Null
Copy-Item $Exe (Join-Path $Dist "AnimePlayer.exe")
Copy-Item (Join-Path $Root "portable") (Join-Path $Dist "portable") -Recurse -Force
Copy-Item (Join-Path $Root "pipeline") (Join-Path $Dist "pipeline") -Recurse -Force
New-Item -ItemType Directory -Force -Path (Join-Path $Dist "cache") | Out-Null
Copy-Item (Join-Path $Root "README.md") (Join-Path $Dist "README.md") -Force
Write-Host "Portable tree at $Dist"
Write-Host "Drop mpv, ffmpeg+nvenc, VapourSynth+vsmlrt, CUDA, TensorRT, and models into portable\ then zip the folder."
