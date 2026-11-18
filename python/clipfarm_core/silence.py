"""Dead-air detection built on FFmpeg's silencedetect filter.

The strategy mirrors what auto-editor does: measure the audio envelope, treat
anything under a noise floor for longer than the minimum duration as dead air,
then add a little padding so cuts never clip the first or last phoneme.
"""

from __future__ import annotations

import re
import shutil
import subprocess
import threading
from typing import Any, Callable, Dict, List, Optional

from .project import EngineError

SILENCE_START = re.compile(r"silence_start:\s*(-?\d+(?:\.\d+)?)")
SILENCE_END = re.compile(r"silence_end:\s*(-?\d+(?:\.\d+)?)")
OUT_TIME = re.compile(r"out_time_us=(\d+)")


def ffmpeg_binary() -> str:
    binary = shutil.which("ffmpeg")
    if not binary:
        raise EngineError(
            "FFmpeg was not found. Install it with `brew install ffmpeg` (macOS), "
            "`winget install Gyan.FFmpeg` (Windows) or `sudo apt install ffmpeg` (Linux), "
            "then restart clipfarm."
        )
    return binary


def ffprobe_binary() -> Optional[str]:
    return shutil.which("ffprobe")


def _duration_seconds(path: str) -> Optional[float]:
    probe = ffprobe_binary()
    if not probe:
        return None
    try:
        output = subprocess.run(
            [
                probe,
                "-v",
                "error",
                "-show_entries",
                "format=duration",
                "-of",
                "default=noprint_wrappers=1:nokey=1",
                path,
            ],
            capture_output=True,
            text=True,
            timeout=30,
            check=False,
        )
        return float(output.stdout.strip())
    except (ValueError, OSError, subprocess.SubprocessError):
        return None


def _merge(intervals: List[Dict[str, float]]) -> List[Dict[str, float]]:
    ordered = sorted(intervals, key=lambda item: item["start"])
    merged: List[Dict[str, float]] = []
    for interval in ordered:
        if merged and interval["start"] <= merged[-1]["end"] + 1e-3:
            merged[-1]["end"] = max(merged[-1]["end"], interval["end"])
        else:
            merged.append(dict(interval))
    return merged


def detect_silences(
    path: str,
    threshold_db: float = -34.0,
    min_silence: float = 0.35,
    padding: float = 0.12,
    progress: Optional[Callable[[float, str], None]] = None,
    cancel: Optional[threading.Event] = None,
) -> Dict[str, Any]:
    ffmpeg = ffmpeg_binary()
    duration = _duration_seconds(path) or 0.0
    command = [
        ffmpeg,
        "-hide_banner",
        "-nostdin",
        "-nostats",
        "-i",
        path,
        "-af",
        f"silencedetect=noise={threshold_db}dB:d={min_silence}",
        "-f",
        "null",
        "-",
        "-progress",
        "pipe:1",
    ]

    process = subprocess.Popen(
        command,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        bufsize=1,
    )

    starts: List[float] = []
    raw: List[Dict[str, float]] = []
    try:
        assert process.stdout is not None
        for line in process.stdout:
            if cancel is not None and cancel.is_set():
                process.terminate()
                raise EngineError("Silence detection was cancelled")
            time_match = OUT_TIME.search(line)
            if time_match and duration > 0 and progress is not None:
                progress(min(float(time_match.group(1)) / 1_000_000 / duration, 0.99), "Listening for dead air")
            start_match = SILENCE_START.search(line)
            if start_match:
                starts.append(float(start_match.group(1)))
            end_match = SILENCE_END.search(line)
            if end_match and starts:
                start = starts.pop(0)
                end = float(end_match.group(1))
                if end > start:
                    raw.append({"start": start, "end": end})
        process.wait(timeout=30)
    finally:
        if process.stdout is not None:
            process.stdout.close()
        if process.poll() is None:
            process.terminate()

    if process.returncode not in (0, None) and not raw:
        raise EngineError("FFmpeg could not analyse this file. Check that it is a readable media file.")

    padded: List[Dict[str, float]] = []
    for interval in raw:
        start = interval["start"] + padding
        end = interval["end"] - padding
        if duration:
            start = min(start, duration)
            end = min(end, duration)
        if end - start >= 0.05:
            padded.append({"start": round(start, 3), "end": round(end, 3)})

    if progress is not None:
        progress(1.0, f"Found {len(padded)} silent stretches")
    return {"silences": _merge(padded), "duration": duration}
