"""Build the NVENC encode argv. Audio, subtitles, and attachments are always stream-copied."""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

from params import PipelineParams

# Hard lock: these tokens must never appear as stream codecs for audio/subs.
FORBIDDEN_A_S_CODECS = {
    "aac",
    "libopus",
    "libmp3lame",
    "ac3",
    "eac3",
    "flac",
    "pcm_s16le",
    "pcm_s24le",
    "ass",
    "srt",
    "webvtt",
    "mov_text",
    "dvdsub",
    "pgssub",
}


def build_ffmpeg_args(
    ffmpeg: str | Path,
    source: Path,
    output: Path,
    params: PipelineParams,
    *,
    mode: str = "full",
) -> list[str]:
    params.validate()
    args: list[str] = [
        str(ffmpeg),
        "-hide_banner",
        "-nostdin",
        "-y",
        "-progress",
        "pipe:1",
        "-nostats",
    ]

    if mode == "remux":
        # Dev/smoke only: copy every stream, including video. Still never transcodes a/s.
        args += [
            "-i",
            str(source),
            "-map",
            "0:v:0?",
            "-map",
            "0:a?",
            "-map",
            "0:s?",
            "-map",
            "0:t?",
            "-c:v",
            "copy",
            "-c:a",
            "copy",
            "-c:s",
            "copy",
            "-c:t",
            "copy",
            str(output),
        ]
        assert_stream_copy(args)
        return args

    # Product path: y4m from vspipe on stdin, original file for a/s/t.
    args += [
        "-fflags",
        "+genpts",
        "-i",
        "pipe:0",
        "-i",
        str(source),
        "-map",
        "0:v:0",
        "-map",
        "1:a?",
        "-map",
        "1:s?",
        "-map",
        "1:t?",
        "-c:v",
        "hevc_nvenc",
        "-preset",
        params.nvenc_preset,
        "-tune",
        "hq",
        "-rc",
        "vbr",
        "-cq",
        str(params.nvenc_cq),
        "-b:v",
        "0",
        "-profile:v",
        params.nvenc_profile,
        "-pix_fmt",
        "p010le",
        "-spatial-aq",
        "1",
        "-temporal-aq",
        "1",
        "-rc-lookahead",
        "16",
        "-gpu",
        "0",
        "-color_primaries",
        "bt709",
        "-color_trc",
        "bt709",
        "-colorspace",
        "bt709",
        "-c:a",
        "copy",
        "-c:s",
        "copy",
        "-c:t",
        "copy",
        str(output),
    ]
    assert_stream_copy(args)
    return args


def _flag_value(args: list[str], flag: str) -> str | None:
    try:
        i = args.index(flag)
    except ValueError:
        return None
    if i + 1 >= len(args):
        return None
    return args[i + 1]


def assert_stream_copy(args: list[str]) -> None:
    audio = _flag_value(args, "-c:a")
    subs = _flag_value(args, "-c:s")
    if audio != "copy":
        raise AssertionError(f"audio codec must be copy, got {audio!r}")
    if subs != "copy":
        raise AssertionError(f"subtitle codec must be copy, got {subs!r}")
    # Video may be hevc_nvenc or copy (remux). Never a forbidden a/s codec anywhere
    # in the *codec value* slots for -c:a / -c:s.
    lower = [a.lower() for a in args]
    for i, tok in enumerate(lower):
        if tok in ("-c:a", "-acodec", "-c:s", "-scodec"):
            val = lower[i + 1] if i + 1 < len(lower) else ""
            if val in FORBIDDEN_A_S_CODECS:
                raise AssertionError(f"forbidden transcode {tok} {val}")


def main() -> int:
    p = argparse.ArgumentParser(description="Print ffmpeg argv for the v1 cache encode")
    p.add_argument("--source", required=True)
    p.add_argument("--output", required=True)
    p.add_argument("--ffmpeg", default="ffmpeg")
    p.add_argument("--mode", default="full", choices=("full", "remux"))
    p.add_argument("--params-json", default=None)
    args = p.parse_args()
    params = PipelineParams()
    if args.params_json:
        params = PipelineParams.from_mapping(json.loads(Path(args.params_json).read_text()))
    cmd = build_ffmpeg_args(args.ffmpeg, Path(args.source), Path(args.output), params, mode=args.mode)
    json.dump(cmd, sys.stdout)
    sys.stdout.write("\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
