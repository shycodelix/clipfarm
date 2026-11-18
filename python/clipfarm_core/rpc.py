"""Newline-delimited JSON-RPC sidecar used by the Electron main process."""

from __future__ import annotations

import importlib.util
import json
import os
import platform
import shutil
import subprocess
import sys
import threading
import traceback
from typing import Any, Callable, Dict

from .project import EngineError
from .render import render_project
from .scenes import detect_scenes
from .silence import detect_silences
from .transcribe import transcribe

CANCELLATIONS: Dict[str, threading.Event] = {}
WRITE_LOCK = threading.Lock()


def _log(level: str, message: str) -> None:
    _write({"event": "log", "level": level, "message": message})


def _write(payload: Dict[str, Any]) -> None:
    with WRITE_LOCK:
        sys.stdout.write(json.dumps(payload) + "\n")
        sys.stdout.flush()


def _progress_emitter(request_id: Any) -> Callable[[float, str], None]:
    def emit(progress: float, stage: str = "") -> None:
        _write({"event": "progress", "id": request_id, "progress": progress, "stage": stage})

    return emit


def _module_available(module: str) -> bool:
    try:
        return importlib.util.find_spec(module) is not None
    except (ImportError, ValueError):
        return False


def engine_status() -> Dict[str, Any]:
    device = "cpu"
    try:
        import ctranslate2

        if ctranslate2.get_cuda_device_count() > 0:
            device = "cuda"
    except Exception:
        if sys.platform == "darwin" and platform.machine() == "arm64":
            device = "mps"

    ffmpeg = shutil.which("ffmpeg")
    ffprobe = shutil.which("ffprobe")
    faster_whisper = _module_available("faster_whisper")
    scenedetect = _module_available("scenedetect")

    if not ffmpeg:
        message = "FFmpeg is missing. Silence removal and export need it."
    elif not faster_whisper and not scenedetect:
        message = "Core engine ready. Install the AI extras for transcription and scene detection."
    else:
        message = "Engine ready. Everything runs on this machine."

    return {
        "ready": bool(ffmpeg and ffprobe),
        "python": sys.executable,
        "pythonVersion": platform.python_version(),
        "ffmpeg": ffmpeg,
        "ffprobe": ffprobe,
        "device": device,
        "packages": {
            "fasterWhisper": faster_whisper,
            "scenedetect": scenedetect,
            "autoEditor": _module_available("auto_editor"),
            "torch": _module_available("torch"),
        },
        "message": message,
    }


def probe_media(path: str) -> Dict[str, Any]:
    ffprobe = shutil.which("ffprobe")
    if not ffprobe:
        raise EngineError("FFprobe is missing, so clipfarm cannot read media metadata.")
    if not os.path.exists(path):
        raise EngineError(f"File not found: {path}")
    result = subprocess.run(
        [ffprobe, "-v", "error", "-print_format", "json", "-show_format", "-show_streams", path],
        capture_output=True,
        text=True,
        timeout=60,
        check=False,
    )
    if result.returncode != 0:
        raise EngineError("FFprobe could not read this file. Is it a supported media format?")
    payload = json.loads(result.stdout or "{}")
    streams = payload.get("streams", [])
    video_stream = next((stream for stream in streams if stream.get("codec_type") == "video"), None)
    audio_stream = next((stream for stream in streams if stream.get("codec_type") == "audio"), None)

    duration = 0.0
    try:
        duration = float(payload.get("format", {}).get("duration", 0.0))
    except (TypeError, ValueError):
        duration = 0.0
    if duration == 0.0 and video_stream:
        try:
            duration = float(video_stream.get("duration", 0.0))
        except (TypeError, ValueError):
            duration = 0.0

    fps = 0.0
    if video_stream and video_stream.get("avg_frame_rate"):
        try:
            numerator, denominator = video_stream["avg_frame_rate"].split("/")
            fps = round(float(numerator) / float(denominator or 1), 3)
        except (ValueError, ZeroDivisionError):
            fps = 0.0

    return {
        "duration": round(duration, 3),
        "width": int(video_stream.get("width", 0)) if video_stream else 0,
        "height": int(video_stream.get("height", 0)) if video_stream else 0,
        "fps": fps,
        "hasAudio": audio_stream is not None,
    }


def _silence_kwargs(options: Dict[str, Any]) -> Dict[str, Any]:
    return {
        "threshold_db": float(options.get("thresholdDb", -34)),
        "min_silence": float(options.get("minSilence", 0.35)),
        "padding": float(options.get("padding", 0.12)),
    }


def _scene_kwargs(options: Dict[str, Any]) -> Dict[str, Any]:
    return {
        "threshold": float(options.get("threshold", 27)),
        "min_scene_length": float(options.get("minSceneLength", 0.6)),
    }


def _transcribe_kwargs(options: Dict[str, Any]) -> Dict[str, Any]:
    return {
        "model": str(options.get("model", "small.en")),
        "language": str(options.get("language", "auto")),
        "compute_type": str(options.get("computeType", "int8")),
        "vad": bool(options.get("vad", True)),
    }


def _export_kwargs(options: Dict[str, Any]) -> Dict[str, Any]:
    if not options.get("outputPath"):
        raise EngineError("An export path is required")
    return {
        "output_path": str(options["outputPath"]),
        "resolution": str(options.get("resolution", "source")),
        "quality": str(options.get("quality", "balanced")),
        "burn_captions": bool(options.get("burnCaptions", False)),
        "fps": options.get("fps"),
    }


def dispatch(method: str, params: Dict[str, Any], request_id: Any) -> Any:
    progress = _progress_emitter(request_id)
    job_id = str(params.get("jobId") or request_id)
    cancel = CANCELLATIONS.get(job_id)
    options = params.get("options") or {}

    if method == "status":
        return engine_status()
    if method == "probe":
        return probe_media(str(params["path"]))
    if method == "analyze.silence":
        return detect_silences(
            str(params["path"]), **_silence_kwargs(options), progress=progress, cancel=cancel
        )
    if method == "analyze.scenes":
        return detect_scenes(
            str(params["path"]), **_scene_kwargs(options), progress=progress, cancel=cancel
        )
    if method == "transcribe":
        return transcribe(
            str(params["path"]), **_transcribe_kwargs(options), progress=progress, cancel=cancel
        )
    if method == "export":
        return render_project(
            params["project"], **_export_kwargs(options), progress=progress, cancel=cancel
        )
    raise EngineError(f"Unknown engine method: {method}")


def _handle(line: str) -> None:
    try:
        message = json.loads(line)
    except json.JSONDecodeError:
        _log("error", "Ignored a malformed engine request")
        return

    request_id = message.get("id")
    method = str(message.get("method", ""))
    params = message.get("params") or {}

    if method == "cancel":
        job_id = str(params.get("jobId", ""))
        event = CANCELLATIONS.pop(job_id, None)
        if event is not None:
            event.set()
        _write({"id": request_id, "ok": True, "result": {"cancelled": event is not None}})
        return

    job_id = str(params.get("jobId") or request_id)
    CANCELLATIONS[job_id] = threading.Event()

    def work() -> None:
        try:
            result = dispatch(method, params, request_id)
            _write({"id": request_id, "ok": True, "result": result})
        except EngineError as error:
            _write(
                {"id": request_id, "ok": False, "error": {"code": "engine_error", "message": str(error)}}
            )
        except Exception as error:  # noqa: BLE001 - never take the sidecar down for one job
            _log("error", traceback.format_exc(limit=3))
            _write(
                {
                    "id": request_id,
                    "ok": False,
                    "error": {
                        "code": "internal_error",
                        "message": f"{type(error).__name__}: {error}",
                    },
                }
            )
        finally:
            CANCELLATIONS.pop(job_id, None)

    threading.Thread(target=work, name=f"clipfarm-{method}", daemon=True).start()


def serve() -> int:
    os.environ.setdefault("PYTHONUNBUFFERED", "1")
    _log("ready", f"clipfarm engine {platform.python_version()} on {platform.system()}")
    try:
        for line in sys.stdin:
            if line.strip():
                _handle(line.strip())
    except KeyboardInterrupt:
        return 0
    return 0
