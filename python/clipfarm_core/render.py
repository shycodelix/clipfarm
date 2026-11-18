"""Timeline rendering with FFmpeg: trim, concat, overlay and burn-in captions."""

from __future__ import annotations

import os
import re
import shutil
import subprocess
import tempfile
import threading
from typing import Any, Callable, Dict, List, Optional, Sequence, Tuple

from .project import EngineError, total_duration
from .silence import ffmpeg_binary

OUT_TIME = re.compile(r"out_time_us=(\d+)")

RESOLUTIONS = {
    "1080p": (1920, 1080),
    "4k": (3840, 2160),
    "vertical": (1080, 1920),
}

QUALITY = {
    "high": {"crf": "18", "preset": "slow", "audio": "192k"},
    "balanced": {"crf": "21", "preset": "medium", "audio": "160k"},
    "small": {"crf": "26", "preset": "faster", "audio": "128k"},
}

CAPTION_STYLES = {
    "clean": "FontName=Helvetica,FontSize=15,PrimaryColour=&H00FFFFFF,OutlineColour=&H66000000,BorderStyle=1,Outline=1,Shadow=0,Alignment=2,MarginV=42",
    "bold": "FontName=Helvetica,FontSize=17,Bold=1,PrimaryColour=&H00FFFFFF,OutlineColour=&H99000000,BorderStyle=1,Outline=2,Shadow=0,Alignment=2,MarginV=52",
    "minimal": "FontName=Helvetica,FontSize=13,PrimaryColour=&H00FFFFFF,OutlineColour=&H44000000,BorderStyle=1,Outline=0,Shadow=0,Alignment=2,MarginV=32",
}


def _even(value: int) -> int:
    return int(value) - (int(value) % 2)


def _asset_path(project: Dict[str, Any], clip: Dict[str, Any]) -> str:
    asset = next((item for item in project.get("assets", []) if item["id"] == clip.get("assetId")), None)
    if asset is None:
        raise EngineError("A clip points at an asset that is no longer in the project")
    path = asset.get("path")
    if not path or not os.path.exists(path):
        raise EngineError(f"Missing media file for `{asset.get('name')}`. Re-import it and try again.")
    return path


def _speed_filters(speed: float) -> Tuple[str, str]:
    """Return (video, audio) speed filters, safe outside 0.5x–2x atempo limits."""
    speed = max(0.1, float(speed or 1.0))
    video = "" if abs(speed - 1.0) < 1e-3 else f",setpts=(PTS-STARTPTS)/{speed:.6f}"
    audio = ""
    if abs(speed - 1.0) >= 1e-3:
        remaining = speed
        parts = []
        while remaining > 2.0:
            parts.append("atempo=2.0")
            remaining /= 2.0
        while remaining < 0.5:
            parts.append("atempo=0.5")
            remaining /= 0.5
        parts.append(f"atempo={remaining:.6f}")
        audio = "," + ",".join(parts)
    return video, audio


def _srt_timestamp(seconds: float) -> str:
    milliseconds = int(round(max(0.0, seconds) * 1000))
    hours, milliseconds = divmod(milliseconds, 3_600_000)
    minutes, milliseconds = divmod(milliseconds, 60_000)
    whole_seconds, milliseconds = divmod(milliseconds, 1000)
    return f"{hours:02d}:{minutes:02d}:{whole_seconds:02d},{milliseconds:03d}"


def write_srt(captions: Sequence[Dict[str, Any]], path: str) -> int:
    written = 0
    with open(path, "w", encoding="utf-8") as handle:
        for caption in sorted(captions, key=lambda item: item["start"]):
            text = " ".join(str(caption.get("text", "")).split())
            if not text or float(caption["end"]) <= float(caption["start"]):
                continue
            written += 1
            handle.write(f"{written}\n")
            handle.write(f"{_srt_timestamp(caption['start'])} --> {_srt_timestamp(caption['end'])}\n")
            handle.write(f"{text}\n\n")
    return written


def render_project(
    project: Dict[str, Any],
    output_path: str,
    resolution: str = "source",
    quality: str = "balanced",
    burn_captions: bool = False,
    fps: Optional[float] = None,
    progress: Optional[Callable[[float, str], None]] = None,
    cancel: Optional[threading.Event] = None,
) -> Dict[str, Any]:
    clips = [clip for clip in project.get("clips", []) if float(clip.get("duration", 0)) > 0]
    if not clips:
        raise EngineError("Add at least one clip to the timeline before exporting")

    width, height = RESOLUTIONS.get(resolution, (project.get("width") or 1920, project.get("height") or 1080))
    if resolution == "source":
        width, height = project.get("width") or 1920, project.get("height") or 1080
    width, height = _even(width), _even(height)
    output_fps = int(round(float(fps or project.get("fps") or 30)))
    settings = QUALITY.get(quality, QUALITY["balanced"])

    video_track = next(
        (track["id"] for track in project.get("tracks", []) if track.get("kind") == "video"),
        "v1",
    )
    overlay_tracks = {
        track["id"] for track in project.get("tracks", []) if track.get("kind") == "overlay"
    }

    base_clips = sorted(
        (clip for clip in clips if clip.get("trackId") == video_track), key=lambda item: item["start"]
    )
    overlay_clips = sorted(
        (clip for clip in clips if clip.get("trackId") in overlay_tracks), key=lambda item: item["start"]
    )

    if not base_clips:
        raise EngineError("The video track is empty. Drag a clip onto the timeline first.")

    # Gather unique inputs, remembering their ffmpeg input order.
    inputs: List[str] = []
    input_indices: Dict[str, int] = {}

    def input_index(path: str) -> int:
        if path not in input_indices:
            input_indices[path] = len(inputs)
            inputs.append(path)
        return input_indices[path]

    for clip in base_clips + overlay_clips:
        input_index(_asset_path(project, clip))

    assets = {asset["id"]: asset for asset in project.get("assets", [])}
    fit = (
        f"scale={width}:{height}:force_original_aspect_ratio=decrease,"
        f"pad={width}:{height}:(ow-iw)/2:(oh-ih)/2:color=0x0b0b0c,setsar=1"
    )

    filters: List[str] = []
    video_labels: List[str] = []
    audio_labels: List[str] = []

    cursor = 0.0
    for position, clip in enumerate(base_clips):
        start = float(clip["start"])
        duration = float(clip["duration"])
        speed = float(clip.get("speed", 1.0) or 1.0)
        in_point = float(clip.get("inPoint", 0.0))
        source_end = in_point + duration * speed
        asset = assets.get(clip.get("assetId"), {})

        if start - cursor > 0.02:
            gap = start - cursor
            filters.append(f"color=c=0x0b0b0c:s={width}x{height}:r={output_fps}:d={gap:.6f}[gapv{position}]")
            filters.append(
                "anullsrc=channel_layout=stereo:sample_rate=48000,"
                f"atrim=duration={gap:.6f}[gapa{position}]"
            )
            video_labels.append(f"[gapv{position}]")
            audio_labels.append(f"[gapa{position}]")

        video_speed, audio_speed = _speed_filters(speed)
        index = input_indices[_asset_path(project, clip)]
        filters.append(
            f"[{index}:v]trim=start={in_point:.6f}:end={source_end:.6f},setpts=PTS-STARTPTS{video_speed},"
            f"fps={output_fps},{fit}[v{position}]"
        )
        video_labels.append(f"[v{position}]")

        has_audio = bool(asset.get("hasAudio")) and asset.get("kind") != "image"
        if has_audio:
            volume = max(0.0, min(2.0, float(clip.get("volume", 1.0))))
            filters.append(
                f"[{index}:a]atrim=start={in_point:.6f}:end={source_end:.6f},asetpts=PTS-STARTPTS,"
                f"volume={volume:.3f}{audio_speed},aresample=48000[a{position}]"
            )
        else:
            filters.append(
                "anullsrc=channel_layout=stereo:sample_rate=48000,"
                f"atrim=duration={duration:.6f}[a{position}]"
            )
        audio_labels.append(f"[a{position}]")
        cursor = max(cursor, start + duration)

    pairs = "".join(
        f"{video_labels[index]}{audio_labels[index]}" for index in range(len(video_labels))
    )
    filters.append(f"{pairs}concat=n={len(video_labels)}:v=1:a=1[basev][basea]")

    last_video = "[basev]"
    for position, clip in enumerate(overlay_clips):
        start = float(clip["start"])
        duration = float(clip["duration"])
        speed = float(clip.get("speed", 1.0) or 1.0)
        in_point = float(clip.get("inPoint", 0.0))
        source_end = in_point + duration * speed
        index = input_indices[_asset_path(project, clip)]
        video_speed, _ = _speed_filters(speed)
        filters.append(
            f"[{index}:v]trim=start={in_point:.6f}:end={source_end:.6f},setpts=PTS-STARTPTS{video_speed},"
            f"fps={output_fps},scale={width}:{height}:force_original_aspect_ratio=decrease,"
            f"pad={width}:{height}:(ow-iw)/2:(oh-ih)/2:color=0x00000000,setsar=1,"
            f"setpts=PTS+{start:.6f}/TB[ov{position}]"
        )
        filters.append(
            f"{last_video}[ov{position}]overlay=enable='between(t,{start:.6f},{start + duration:.6f})':"
            f"eof_action=pass:shortest=0[vov{position}]"
        )
        last_video = f"[vov{position}]"

    workdir = tempfile.mkdtemp(prefix="clipfarm-render-")
    subtitle_file = os.path.join(workdir, "captions.srt")
    burned = 0
    if burn_captions and project.get("captions"):
        burned = write_srt(project["captions"], subtitle_file)

    if burned:
        style = CAPTION_STYLES.get(project.get("settings", {}).get("captionPreset", "clean"), CAPTION_STYLES["clean"])
        filters.append(f"{last_video}subtitles=captions.srt:force_style='{style}'[vout]")
    else:
        filters.append(f"{last_video}null[vout]")

    filters.append(
        "[basea]loudnorm=I=-16:TP=-1.5:LRA=11,aresample=48000,"
        "aformat=channel_layouts=stereo[abase]"
    )

    graph = ";".join(filters)
    command: List[str] = [ffmpeg_binary(), "-hide_banner", "-nostdin", "-nostats", "-y"]

    image_paths = {
        asset.get("path")
        for asset in project.get("assets", [])
        if asset.get("kind") == "image"
    }
    for path in inputs:
        if path in image_paths:
            command.extend(["-loop", "1", "-framerate", str(output_fps)])
        command.extend(["-i", path])

    command.extend(
        [
            "-filter_complex",
            graph,
            "-map",
            "[vout]",
            "-map",
            "[abase]",
            "-c:v",
            "libx264",
            "-preset",
            settings["preset"],
            "-crf",
            settings["crf"],
            "-pix_fmt",
            "yuv420p",
            "-profile:v",
            "high",
            "-c:a",
            "aac",
            "-b:a",
            settings["audio"],
            "-movflags",
            "+faststart",
            "-progress",
            "pipe:1",
        ]
    )
    if output_path.lower().endswith(".mov"):
        command.extend(["-f", "mov"])
    command.append(output_path)

    os.makedirs(os.path.dirname(os.path.abspath(output_path)) or ".", exist_ok=True)
    duration = total_duration(project)
    process = subprocess.Popen(
        command,
        cwd=workdir,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        bufsize=1,
    )
    tail: List[str] = []
    try:
        assert process.stdout is not None
        for line in process.stdout:
            if cancel is not None and cancel.is_set():
                process.terminate()
                raise EngineError("Export was cancelled")
            match = OUT_TIME.search(line)
            if match and duration > 0 and progress is not None:
                progress(min(float(match.group(1)) / 1_000_000 / duration, 0.99), "Rendering")
            elif line.strip() and not line.startswith(("frame=", "fps=", "bitrate=", "total_size=", "out_time", "progress=", "speed=")):
                tail.append(line.rstrip())
                tail = tail[-20:]
        process.wait(timeout=60)
    finally:
        if process.stdout is not None:
            process.stdout.close()
        if process.poll() is None:
            process.terminate()
        shutil.rmtree(workdir, ignore_errors=True)

    if process.returncode != 0:
        detail = "\n".join(tail[-8:]) or "FFmpeg exited without details"
        raise EngineError(f"Export failed. {detail}")

    if progress is not None:
        progress(1.0, "Finishing")
    return {
        "outputPath": output_path,
        "bytes": os.path.getsize(output_path),
        "durationSeconds": round(duration, 3),
        "captionsBurned": burned,
    }
