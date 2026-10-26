# Playback GLSL (v1)

Order when shaders are on:

1. `deband.glsl` — light luma deband + dither
2. `line_enhance.glsl` — adaptive unsharp on edges only

Anime4K-style intent, original shaders (not a copy of Anime4K). Toggle from the Tauri chrome; mpv.conf enables the pair by default.
