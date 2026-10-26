"""Integer RIFE multipliers that land on 60 fps without live interpolation."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Literal


@dataclass(frozen=True)
class RifePlan:
    kind: Literal["skip", "multiply", "multiply_then_thin"]
    src_fps: float
    multi: int | None = None
    cycle: int | None = None
    reason: str | None = None
    result_fps: float | None = None


def _approx(a: float, b: float) -> bool:
    return abs(a - b) < 0.08


def rife_plan(src_fps: float, target_fps: int = 60) -> RifePlan:
    target = float(target_fps)
    if src_fps <= 0:
        return RifePlan(kind="skip", src_fps=src_fps, reason="unknown source fps")
    if src_fps >= target - 0.5:
        return RifePlan(
            kind="skip",
            src_fps=src_fps,
            reason="already at or above target fps",
        )
    if _approx(src_fps, 24.0) or _approx(src_fps, 23.976):
        return RifePlan(
            kind="multiply_then_thin",
            src_fps=src_fps,
            multi=5,
            cycle=2,
            result_fps=src_fps * 5.0 / 2.0,
        )
    if _approx(src_fps, 30.0) or _approx(src_fps, 29.97):
        return RifePlan(kind="multiply", src_fps=src_fps, multi=2, result_fps=src_fps * 2.0)
    if _approx(src_fps, 25.0):
        return RifePlan(
            kind="multiply_then_thin",
            src_fps=src_fps,
            multi=12,
            cycle=5,
            result_fps=src_fps * 12.0 / 5.0,
        )
    multi = max(2, int(round(target / src_fps)))
    return RifePlan(kind="multiply", src_fps=src_fps, multi=multi, result_fps=src_fps * multi)
