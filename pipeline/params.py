"""Locked v1 preprocess identity. Keep in sync with crates/player-core/src/params.rs."""

from __future__ import annotations

from dataclasses import asdict, dataclass, fields
import json
from typing import Any


TARGET_WIDTH = 2560
TARGET_HEIGHT = 1440
TARGET_FPS = 60


@dataclass(frozen=True)
class PipelineParams:
    rife_model: str = "v4.6"
    rife_fp16: bool = True
    target_fps: int = TARGET_FPS
    esrgan_model: str = "animevideov3"
    target_width: int = TARGET_WIDTH
    target_height: int = TARGET_HEIGHT
    nvenc_preset: str = "p5"
    nvenc_cq: int = 23
    nvenc_profile: str = "main10"
    trt_workspace_mb: int = 2048
    preserve_aspect: bool = True
    copy_audio: bool = True
    copy_subs: bool = True
    copy_attachments: bool = True

    def validate(self) -> None:
        if not self.copy_audio:
            raise ValueError("v1 forbids transcoding audio: copy_audio must be true")
        if not self.copy_subs:
            raise ValueError("v1 forbids transcoding subtitles: copy_subs must be true")
        if not self.rife_fp16:
            raise ValueError("v1 TensorRT RIFE is FP16-only")
        if self.target_width != TARGET_WIDTH or self.target_height != TARGET_HEIGHT:
            raise ValueError(f"v1 target is {TARGET_WIDTH}x{TARGET_HEIGHT}")
        if self.target_fps != TARGET_FPS:
            raise ValueError(f"v1 target fps is {TARGET_FPS}")

    def to_json(self) -> dict[str, Any]:
        return asdict(self)

    @classmethod
    def from_mapping(cls, data: dict[str, Any]) -> "PipelineParams":
        allowed = {f.name for f in fields(cls)}
        filtered = {k: v for k, v in data.items() if k in allowed}
        p = cls(**filtered)
        p.validate()
        return p


def dumps_params(p: PipelineParams) -> str:
    return json.dumps(p.to_json(), sort_keys=True, separators=(",", ":"))
