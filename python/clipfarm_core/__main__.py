"""Command line entry point.

    python -m clipfarm_core serve                  # JSON-RPC sidecar for the app
    python -m clipfarm_core mcp --project <file>   # MCP server for agents
    python -m clipfarm_core status                 # show engine capabilities
    python -m clipfarm_core probe <file>
"""

from __future__ import annotations

import argparse
import json
import sys
from typing import List, Optional

from .mcp_server import main as mcp_main
from .project import EngineError
from .rpc import engine_status, probe_media, serve
from .scenes import detect_scenes
from .silence import detect_silences


def _print(payload: object) -> None:
    print(json.dumps(payload, indent=2))


def main(argv: Optional[List[str]] = None) -> int:
    parser = argparse.ArgumentParser(prog="clipfarm_core", description="clipfarm local media engine")
    subparsers = parser.add_subparsers(dest="command")

    subparsers.add_parser("serve", help="Run the JSON-RPC sidecar")
    mcp_parser = subparsers.add_parser("mcp", help="Run the MCP server for AI agents")
    mcp_parser.add_argument("--project", help="Project JSON to edit")
    subparsers.add_parser("status", help="Report FFmpeg, Whisper and PySceneDetect availability")

    probe_parser = subparsers.add_parser("probe", help="Read media metadata")
    probe_parser.add_argument("path")

    silence_parser = subparsers.add_parser("silence", help="Detect silent stretches")
    silence_parser.add_argument("path")
    silence_parser.add_argument("--threshold-db", type=float, default=-34)
    silence_parser.add_argument("--min-silence", type=float, default=0.35)
    silence_parser.add_argument("--padding", type=float, default=0.12)

    scenes_parser = subparsers.add_parser("scenes", help="Detect scene boundaries")
    scenes_parser.add_argument("path")
    scenes_parser.add_argument("--threshold", type=float, default=27)

    arguments = parser.parse_args(argv)

    try:
        if arguments.command in (None, "status"):
            _print(engine_status())
            return 0
        if arguments.command == "serve":
            return serve()
        if arguments.command == "mcp":
            return mcp_main(["--project", arguments.project] if arguments.project else [])
        if arguments.command == "probe":
            _print(probe_media(arguments.path))
            return 0
        if arguments.command == "silence":
            _print(
                detect_silences(
                    arguments.path,
                    threshold_db=arguments.threshold_db,
                    min_silence=arguments.min_silence,
                    padding=arguments.padding,
                )
            )
            return 0
        if arguments.command == "scenes":
            _print(detect_scenes(arguments.path, threshold=arguments.threshold))
            return 0
    except EngineError as error:
        print(f"error: {error}", file=sys.stderr)
        return 2

    parser.print_help()
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
