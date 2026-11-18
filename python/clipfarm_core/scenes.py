"""Scene splitting through PySceneDetect's content-aware detector."""

from __future__ import annotations

import threading
from typing import Any, Callable, Dict, List, Optional

from .project import EngineError

INSTALL_HINT = "Run `npm run python:setup` (or `pip install 'scenedetect[opencv]'`) to enable scene detection."


def detect_scenes(
    path: str,
    threshold: float = 27.0,
    min_scene_length: float = 0.6,
    progress: Optional[Callable[[float, str], None]] = None,
    cancel: Optional[threading.Event] = None,
) -> Dict[str, Any]:
    try:
        from scenedetect import ContentDetector, SceneManager, open_video
    except ImportError as error:
        raise EngineError(f"PySceneDetect is not installed. {INSTALL_HINT}") from error

    if progress is not None:
        progress(0.02, "Opening video")

    try:
        video = open_video(path)
    except Exception as error:  # PySceneDetect raises a variety of decoding errors.
        raise EngineError(f"Could not open this video for scene detection: {error}") from error

    fps = float(video.frame_rate or 30.0)
    duration = 0.0
    try:
        duration = float(video.duration.get_seconds())
    except Exception:
        duration = 0.0

    manager = SceneManager()
    manager.add_detector(
        ContentDetector(threshold=threshold, min_scene_len=max(1, int(min_scene_length * fps)))
    )

    def on_frame(frame_number: int, frame: Any = None) -> None:
        del frame
        if cancel is not None and cancel.is_set():
            raise EngineError("Scene detection was cancelled")
        if progress is not None and duration > 0:
            progress(min(frame_number / fps / duration, 0.98), "Looking for cuts")

    try:
        manager.detect_scenes(video=video, callback=on_frame, show_progress=False)
        scene_list = manager.get_scene_list()
    except EngineError:
        raise
    except Exception as error:
        raise EngineError(f"Scene detection failed: {error}") from error

    scenes: List[Dict[str, Any]] = []
    for index, (start, end) in enumerate(scene_list):
        start_seconds = float(start.get_seconds())
        end_seconds = float(end.get_seconds())
        if end_seconds - start_seconds <= 0.05:
            continue
        scenes.append(
            {
                "index": index,
                "start": round(start_seconds, 3),
                "end": round(end_seconds, 3),
            }
        )

    if progress is not None:
        progress(1.0, f"Found {len(scenes)} scenes")
    return {"scenes": scenes, "duration": duration, "fps": fps}
