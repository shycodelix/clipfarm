"""Model Context Protocol server for clipfarm.

Agents talk JSON-RPC over stdio and operate on the same project file the desktop
app saves, so an assistant can place, trim and split clips, write captions, cut
dead air and export without a human touching the timeline.

Run it with:
    python -m clipfarm_core mcp --project /path/to/project.json

The server implements the MCP lifecycle (initialize / initialized / ping),
tools/list and tools/call. It intentionally has no third-party dependencies so it
works even before the AI extras are installed.
"""

from __future__ import annotations

import argparse
import json
import sys
from typing import Any, Callable, Dict, List, Optional

from . import timeline
from .project import (
    EngineError,
    find_asset,
    load_project,
    save_project,
    timeline_summary,
    total_duration,
)
from .render import render_project
from .scenes import detect_scenes
from .silence import detect_silences
from .transcribe import transcribe

SERVER_NAME = "clipfarm"
SERVER_VERSION = "0.1.0"
PROTOCOL_VERSION = "2025-06-18"

JsonRpcId = Optional[Any]


class McpError(RuntimeError):
    def __init__(self, message: str, code: int = -32000):
        super().__init__(message)
        self.code = code


TOOLS: List[Dict[str, Any]] = [
    {
        "name": "clipfarm_get_timeline",
        "description": "Read the current timeline: tracks, clips, captions, scenes and silences.",
        "inputSchema": {
            "type": "object",
            "properties": {"project": {"type": "string", "description": "Project JSON path (optional when the server was started with --project)"}},
            "additionalProperties": False,
        },
    },
    {
        "name": "clipfarm_list_assets",
        "description": "List the media assets imported into the project.",
        "inputSchema": {
            "type": "object",
            "properties": {"project": {"type": "string"}},
            "additionalProperties": False,
        },
    },
    {
        "name": "clipfarm_place_clip",
        "description": "Place an asset on a timeline track. Defaults to the end of the track.",
        "inputSchema": {
            "type": "object",
            "required": ["asset_id"],
            "properties": {
                "asset_id": {"type": "string"},
                "at": {"type": "number", "description": "Timeline position in seconds"},
                "duration": {"type": "number"},
                "in_point": {"type": "number", "description": "Seconds into the source media"},
                "track_id": {"type": "string", "description": "v1 (video) or v2 (overlay)"},
                "project": {"type": "string"},
            },
            "additionalProperties": False,
        },
    },
    {
        "name": "clipfarm_trim_clip",
        "description": "Trim a clip by moving its start or changing its duration (also moves the source in-point).",
        "inputSchema": {
            "type": "object",
            "required": ["clip_id"],
            "properties": {
                "clip_id": {"type": "string"},
                "start": {"type": "number"},
                "duration": {"type": "number"},
                "in_point": {"type": "number"},
                "project": {"type": "string"},
            },
            "additionalProperties": False,
        },
    },
    {
        "name": "clipfarm_split_clip",
        "description": "Split a clip at an absolute timeline position.",
        "inputSchema": {
            "type": "object",
            "required": ["clip_id", "at"],
            "properties": {
                "clip_id": {"type": "string"},
                "at": {"type": "number"},
                "project": {"type": "string"},
            },
            "additionalProperties": False,
        },
    },
    {
        "name": "clipfarm_delete_clip",
        "description": "Remove a clip from the timeline.",
        "inputSchema": {
            "type": "object",
            "required": ["clip_id"],
            "properties": {"clip_id": {"type": "string"}, "project": {"type": "string"}},
            "additionalProperties": False,
        },
    },
    {
        "name": "clipfarm_add_caption",
        "description": "Add a caption on the captions track.",
        "inputSchema": {
            "type": "object",
            "required": ["start", "end", "text"],
            "properties": {
                "start": {"type": "number"},
                "end": {"type": "number"},
                "text": {"type": "string"},
                "project": {"type": "string"},
            },
            "additionalProperties": False,
        },
    },
    {
        "name": "clipfarm_remove_silences",
        "description": "Detect dead air in an asset with FFmpeg and ripple-cut it out of the timeline.",
        "inputSchema": {
            "type": "object",
            "required": ["asset_id"],
            "properties": {
                "asset_id": {"type": "string"},
                "threshold_db": {"type": "number", "description": "Noise floor, default -34 dB"},
                "min_silence": {"type": "number", "description": "Minimum silence in seconds, default 0.35"},
                "padding": {"type": "number", "description": "Seconds kept around each cut, default 0.12"},
                "project": {"type": "string"},
            },
            "additionalProperties": False,
        },
    },
    {
        "name": "clipfarm_detect_scenes",
        "description": "Run PySceneDetect on an asset and store scene boundaries in the project.",
        "inputSchema": {
            "type": "object",
            "required": ["asset_id"],
            "properties": {
                "asset_id": {"type": "string"},
                "threshold": {"type": "number", "description": "Content detector threshold, default 27"},
                "project": {"type": "string"},
            },
            "additionalProperties": False,
        },
    },
    {
        "name": "clipfarm_transcribe",
        "description": "Transcribe an asset with faster-whisper and write word-timed captions.",
        "inputSchema": {
            "type": "object",
            "required": ["asset_id"],
            "properties": {
                "asset_id": {"type": "string"},
                "model": {"type": "string", "description": "Whisper model, default small.en"},
                "language": {"type": "string", "description": "ISO code or auto"},
                "project": {"type": "string"},
            },
            "additionalProperties": False,
        },
    },
    {
        "name": "clipfarm_export",
        "description": "Render the timeline to a video file with FFmpeg.",
        "inputSchema": {
            "type": "object",
            "required": ["output"],
            "properties": {
                "output": {"type": "string", "description": "Absolute output path (.mp4 or .mov)"},
                "resolution": {"type": "string", "enum": ["source", "1080p", "4k", "vertical"]},
                "quality": {"type": "string", "enum": ["high", "balanced", "small"]},
                "burn_captions": {"type": "boolean"},
                "project": {"type": "string"},
            },
            "additionalProperties": False,
        },
    },
]


class ClipfarmMcpServer:
    def __init__(self, project_path: Optional[str] = None):
        self.project_path = project_path

    # -- project plumbing -------------------------------------------------
    def _path(self, arguments: Dict[str, Any]) -> str:
        path = arguments.get("project") or self.project_path
        if not path:
            raise McpError(
                "No project file. Start the server with --project or pass the `project` argument."
            )
        return str(path)

    def _load(self, arguments: Dict[str, Any]):
        path = self._path(arguments)
        return load_project(path), path

    def _asset_path(self, project: Dict[str, Any], asset_id: str) -> str:
        asset = find_asset(project, asset_id)
        path = asset.get("path")
        if not path:
            raise McpError("That asset has no file on disk")
        return path

    # -- tools ------------------------------------------------------------
    def tool_get_timeline(self, arguments: Dict[str, Any]) -> Dict[str, Any]:
        project, _ = self._load(arguments)
        return timeline_summary(project)

    def tool_list_assets(self, arguments: Dict[str, Any]) -> Dict[str, Any]:
        project, _ = self._load(arguments)
        return {"assets": project.get("assets", [])}

    def tool_place_clip(self, arguments: Dict[str, Any]) -> Dict[str, Any]:
        project, path = self._load(arguments)
        clip = timeline.place_clip(
            project,
            arguments["asset_id"],
            at=arguments.get("at"),
            duration=arguments.get("duration"),
            track_id=arguments.get("track_id", "v1"),
            in_point=arguments.get("in_point", 0.0),
        )
        save_project(project, path)
        return {"clip": clip, "duration": total_duration(project)}

    def tool_trim_clip(self, arguments: Dict[str, Any]) -> Dict[str, Any]:
        project, path = self._load(arguments)
        clip = timeline.trim_clip(
            project,
            arguments["clip_id"],
            start=arguments.get("start"),
            duration=arguments.get("duration"),
            in_point=arguments.get("in_point"),
        )
        save_project(project, path)
        return {"clip": clip, "duration": total_duration(project)}

    def tool_split_clip(self, arguments: Dict[str, Any]) -> Dict[str, Any]:
        project, path = self._load(arguments)
        left, right = timeline.split_clip(project, arguments["clip_id"], float(arguments["at"]))
        save_project(project, path)
        return {"left": left, "right": right, "duration": total_duration(project)}

    def tool_delete_clip(self, arguments: Dict[str, Any]) -> Dict[str, Any]:
        project, path = self._load(arguments)
        clip = timeline.delete_clip(project, arguments["clip_id"])
        save_project(project, path)
        return {"deleted": clip["id"], "duration": total_duration(project)}

    def tool_add_caption(self, arguments: Dict[str, Any]) -> Dict[str, Any]:
        project, path = self._load(arguments)
        caption = timeline.add_caption(
            project, float(arguments["start"]), float(arguments["end"]), str(arguments["text"])
        )
        save_project(project, path)
        return {"caption": caption, "captionCount": len(project["captions"])}

    def tool_remove_silences(self, arguments: Dict[str, Any]) -> Dict[str, Any]:
        project, path = self._load(arguments)
        asset_id = str(arguments["asset_id"])
        detection = detect_silences(
            self._asset_path(project, asset_id),
            threshold_db=float(arguments.get("threshold_db", -34)),
            min_silence=float(arguments.get("min_silence", 0.35)),
            padding=float(arguments.get("padding", project.get("settings", {}).get("silencePadding", 0.12))),
        )
        result = timeline.remove_silences(
            project,
            detection["silences"],
            asset_id=asset_id,
            ripple=bool(project.get("settings", {}).get("autoRipple", True)),
        )
        save_project(project, path)
        return {
            "detectedSilences": len(detection["silences"]),
            "removedSeconds": result["removed"],
            "duration": result["duration"],
        }

    def tool_detect_scenes(self, arguments: Dict[str, Any]) -> Dict[str, Any]:
        project, path = self._load(arguments)
        asset_id = str(arguments["asset_id"])
        detection = detect_scenes(
            self._asset_path(project, asset_id), threshold=float(arguments.get("threshold", 27))
        )
        timeline.apply_scenes(project, detection["scenes"], asset_id)
        save_project(project, path)
        return {"scenes": project["scenes"], "count": len(project["scenes"])}

    def tool_transcribe(self, arguments: Dict[str, Any]) -> Dict[str, Any]:
        project, path = self._load(arguments)
        asset_id = str(arguments["asset_id"])
        result = transcribe(
            self._asset_path(project, asset_id),
            model=str(arguments.get("model", "small.en")),
            language=str(arguments.get("language", "auto")),
        )
        timeline.replace_captions(project, result["captions"])
        save_project(project, path)
        return {
            "language": result["language"],
            "captions": len(result["captions"]),
            "words": sum(len(caption.get("words", [])) for caption in result["captions"]),
        }

    def tool_export(self, arguments: Dict[str, Any]) -> Dict[str, Any]:
        project, _ = self._load(arguments)
        return render_project(
            project,
            str(arguments["output"]),
            resolution=str(arguments.get("resolution", "source")),
            quality=str(arguments.get("quality", "balanced")),
            burn_captions=bool(arguments.get("burn_captions", True)),
        )

    def call_tool(self, name: str, arguments: Dict[str, Any]) -> Dict[str, Any]:
        handlers: Dict[str, Callable[[Dict[str, Any]], Dict[str, Any]]] = {
            "clipfarm_get_timeline": self.tool_get_timeline,
            "clipfarm_list_assets": self.tool_list_assets,
            "clipfarm_place_clip": self.tool_place_clip,
            "clipfarm_trim_clip": self.tool_trim_clip,
            "clipfarm_split_clip": self.tool_split_clip,
            "clipfarm_delete_clip": self.tool_delete_clip,
            "clipfarm_add_caption": self.tool_add_caption,
            "clipfarm_remove_silences": self.tool_remove_silences,
            "clipfarm_detect_scenes": self.tool_detect_scenes,
            "clipfarm_transcribe": self.tool_transcribe,
            "clipfarm_export": self.tool_export,
        }
        handler = handlers.get(name)
        if handler is None:
            raise McpError(f"Unknown tool: {name}", code=-32602)
        return handler(arguments or {})

    # -- JSON-RPC ---------------------------------------------------------
    def handle(self, message: Dict[str, Any]) -> Optional[Dict[str, Any]]:
        method = message.get("method")
        request_id = message.get("id")
        params = message.get("params") or {}

        if method == "initialize":
            return {
                "jsonrpc": "2.0",
                "id": request_id,
                "result": {
                    "protocolVersion": PROTOCOL_VERSION,
                    "capabilities": {"tools": {"listChanged": False}},
                    "serverInfo": {"name": SERVER_NAME, "version": SERVER_VERSION},
                    "instructions": (
                        "clipfarm edits a desktop video project. Read the timeline first, then use "
                        "clipfarm_place_clip / trim / split to build an edit. Times are seconds."
                    ),
                },
            }
        if method == "ping":
            return {"jsonrpc": "2.0", "id": request_id, "result": {}}
        if method in ("notifications/initialized", "notifications/cancelled"):
            return None
        if method == "tools/list":
            return {"jsonrpc": "2.0", "id": request_id, "result": {"tools": TOOLS}}
        if method == "tools/call":
            name = str(params.get("name", ""))
            arguments = params.get("arguments") or {}
            try:
                result = self.call_tool(name, arguments)
                return {
                    "jsonrpc": "2.0",
                    "id": request_id,
                    "result": {
                        "content": [{"type": "text", "text": json.dumps(result, indent=2)}],
                        "isError": False,
                    },
                }
            except (McpError, EngineError) as error:
                return {
                    "jsonrpc": "2.0",
                    "id": request_id,
                    "result": {
                        "content": [{"type": "text", "text": str(error)}],
                        "isError": True,
                    },
                }
            except Exception as error:  # noqa: BLE001
                return {
                    "jsonrpc": "2.0",
                    "id": request_id,
                    "result": {
                        "content": [{"type": "text", "text": f"{type(error).__name__}: {error}"}],
                        "isError": True,
                    },
                }
        if method == "resources/list":
            return {"jsonrpc": "2.0", "id": request_id, "result": {"resources": []}}

        return {
            "jsonrpc": "2.0",
            "id": request_id,
            "error": {"code": -32601, "message": f"Method not found: {method}"},
        }


def serve_stdio(project_path: Optional[str]) -> int:
    server = ClipfarmMcpServer(project_path)
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            message = json.loads(line)
        except json.JSONDecodeError:
            response: Optional[Dict[str, Any]] = {
                "jsonrpc": "2.0",
                "id": None,
                "error": {"code": -32700, "message": "Parse error"},
            }
        else:
            try:
                response = server.handle(message)
            except Exception as error:  # noqa: BLE001
                response = {
                    "jsonrpc": "2.0",
                    "id": message.get("id"),
                    "error": {"code": -32603, "message": f"{type(error).__name__}: {error}"},
                }
        if response is not None:
            sys.stdout.write(json.dumps(response) + "\n")
            sys.stdout.flush()
    return 0


def main(argv: Optional[List[str]] = None) -> int:
    parser = argparse.ArgumentParser(prog="clipfarm mcp", description="clipfarm MCP server")
    parser.add_argument("--project", help="Path to the project JSON an agent should edit")
    arguments = parser.parse_args(argv)
    return serve_stdio(arguments.project)


if __name__ == "__main__":
    raise SystemExit(main())
