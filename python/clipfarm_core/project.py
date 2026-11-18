"""Project document helpers shared by the RPC sidecar, the MCP server and tests."""

from __future__ import annotations

import json
import os
import tempfile
import uuid
from datetime import datetime, timezone
from typing import Any, Dict, Iterable, List, Optional

SCHEMA_VERSION = 1


class EngineError(RuntimeError):
    """An error safe to surface in the clipfarm UI."""


def now_iso() -> str:
    return datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")


def new_id() -> str:
    return str(uuid.uuid4())


def default_tracks() -> List[Dict[str, Any]]:
    return [
        {"id": "v2", "name": "Overlay", "kind": "overlay", "height": 58, "muted": False, "locked": False},
        {"id": "v1", "name": "Video", "kind": "video", "height": 74, "muted": False, "locked": False},
        {"id": "cc", "name": "Captions", "kind": "captions", "height": 46, "muted": False, "locked": False},
    ]


def new_project(name: str = "Untitled Project") -> Dict[str, Any]:
    stamp = now_iso()
    return {
        "schemaVersion": SCHEMA_VERSION,
        "id": new_id(),
        "name": name or "Untitled Project",
        "createdAt": stamp,
        "updatedAt": stamp,
        "fps": 30,
        "width": 1920,
        "height": 1080,
        "assets": [],
        "tracks": default_tracks(),
        "clips": [],
        "captions": [],
        "scenes": [],
        "silences": [],
        "settings": {
            "captionsEnabled": True,
            "captionPreset": "clean",
            "silencePadding": 0.12,
            "autoRipple": True,
        },
    }


def normalize(project: Dict[str, Any]) -> Dict[str, Any]:
    project.setdefault("schemaVersion", SCHEMA_VERSION)
    for key in ("assets", "clips", "captions", "scenes", "silences"):
        if not isinstance(project.get(key), list):
            project[key] = []
    if not isinstance(project.get("tracks"), list) or not project["tracks"]:
        project["tracks"] = default_tracks()
    settings = {
        "captionsEnabled": True,
        "captionPreset": "clean",
        "silencePadding": 0.12,
        "autoRipple": True,
    }
    settings.update(project.get("settings") or {})
    project["settings"] = settings
    project.setdefault("fps", 30)
    project.setdefault("width", 1920)
    project.setdefault("height", 1080)
    project.setdefault("name", "Untitled Project")
    project.setdefault("id", new_id())
    project.setdefault("createdAt", now_iso())
    project.setdefault("updatedAt", now_iso())
    return project


def load_project(path: str) -> Dict[str, Any]:
    if not path:
        raise EngineError("No project file was provided. Pass --project /path/to/project.json")
    try:
        with open(path, "r", encoding="utf-8") as handle:
            data = json.load(handle)
    except FileNotFoundError as error:
        raise EngineError(f"Project not found: {path}") from error
    except json.JSONDecodeError as error:
        raise EngineError(f"Project file is not valid JSON: {error}") from error
    if not isinstance(data, dict):
        raise EngineError("Project file must contain a JSON object")
    return normalize(data)


def save_project(project: Dict[str, Any], path: str) -> Dict[str, Any]:
    project["updatedAt"] = now_iso()
    directory = os.path.dirname(os.path.abspath(path))
    os.makedirs(directory, exist_ok=True)
    handle, temporary = tempfile.mkstemp(dir=directory, suffix=".tmp")
    try:
        with os.fdopen(handle, "w", encoding="utf-8") as stream:
            json.dump(normalize(project), stream, indent=2)
        os.replace(temporary, path)
    finally:
        if os.path.exists(temporary):
            os.remove(temporary)
    return project


def find_asset(project: Dict[str, Any], asset_id: str) -> Dict[str, Any]:
    for asset in project.get("assets", []):
        if asset.get("id") == asset_id:
            return asset
    raise EngineError(f"Unknown asset: {asset_id}")


def find_clip(project: Dict[str, Any], clip_id: str) -> Dict[str, Any]:
    for clip in project.get("clips", []):
        if clip.get("id") == clip_id:
            return clip
    raise EngineError(f"Unknown clip: {clip_id}")


def track_ids(project: Dict[str, Any]) -> Iterable[str]:
    return (track.get("id") for track in project.get("tracks", []))


def total_duration(project: Dict[str, Any]) -> float:
    clip_end = max((clip["start"] + clip["duration"] for clip in project.get("clips", [])), default=0.0)
    caption_end = max((caption["end"] for caption in project.get("captions", [])), default=0.0)
    return float(max(clip_end, caption_end, 0.0))


def timeline_summary(project: Dict[str, Any]) -> Dict[str, Any]:
    return {
        "id": project.get("id"),
        "name": project.get("name"),
        "duration": round(total_duration(project), 3),
        "fps": project.get("fps", 30),
        "size": [project.get("width", 1920), project.get("height", 1080)],
        "assets": [
            {
                "id": asset.get("id"),
                "name": asset.get("name"),
                "duration": asset.get("duration", 0),
                "kind": asset.get("kind", "video"),
            }
            for asset in project.get("assets", [])
        ],
        "tracks": [
            {"id": track.get("id"), "name": track.get("name"), "kind": track.get("kind")}
            for track in project.get("tracks", [])
        ],
        "clips": sorted(
            (
                {
                    "id": clip.get("id"),
                    "assetId": clip.get("assetId"),
                    "trackId": clip.get("trackId"),
                    "start": round(clip.get("start", 0.0), 3),
                    "duration": round(clip.get("duration", 0.0), 3),
                    "inPoint": round(clip.get("inPoint", 0.0), 3),
                    "speed": clip.get("speed", 1.0),
                }
                for clip in project.get("clips", [])
            ),
            key=lambda item: (item["trackId"], item["start"]),
        ),
        "captions": [
            {
                "id": caption.get("id"),
                "start": round(caption.get("start", 0.0), 3),
                "end": round(caption.get("end", 0.0), 3),
                "text": caption.get("text", ""),
            }
            for caption in sorted(project.get("captions", []), key=lambda item: item["start"])
        ],
        "scenes": project.get("scenes", []),
        "silences": project.get("silences", []),
    }
