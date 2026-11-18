"""Standard-library tests for the timeline, project IO and the MCP tool surface."""

from __future__ import annotations

import json
import os
import sys
import tempfile
import unittest

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from clipfarm_core import timeline  # noqa: E402
from clipfarm_core.mcp_server import ClipfarmMcpServer  # noqa: E402
from clipfarm_core.project import (  # noqa: E402
    EngineError,
    load_project,
    new_project,
    save_project,
    timeline_summary,
)


def project_with_asset(duration: float = 30.0) -> dict:
    project = new_project("Test Edit")
    project["assets"].append(
        {
            "id": "asset-1",
            "name": "Interview.mp4",
            "path": "/tmp/interview.mp4",
            "kind": "video",
            "duration": duration,
            "width": 1920,
            "height": 1080,
            "fps": 30,
            "hasAudio": True,
        }
    )
    return project


class ProjectTests(unittest.TestCase):
    def test_round_trip(self) -> None:
        project = project_with_asset()
        with tempfile.TemporaryDirectory() as directory:
            path = os.path.join(directory, "project.json")
            save_project(project, path)
            loaded = load_project(path)
            self.assertEqual(loaded["id"], project["id"])
            self.assertEqual(loaded["name"], "Test Edit")
            self.assertEqual(loaded["tracks"][1]["id"], "v1")

    def test_missing_project_is_reported(self) -> None:
        with self.assertRaises(EngineError):
            load_project("/nope/missing.json")

    def test_summary_shape(self) -> None:
        project = project_with_asset()
        timeline.place_clip(project, "asset-1", at=2.0, duration=5.0)
        summary = timeline_summary(project)
        self.assertEqual(summary["duration"], 7.0)
        self.assertEqual(len(summary["clips"]), 1)


class TimelineTests(unittest.TestCase):
    def test_place_and_trim(self) -> None:
        project = project_with_asset()
        clip = timeline.place_clip(project, "asset-1", at=4.0, duration=10.0)
        self.assertEqual(clip["start"], 4.0)

        trimmed = timeline.trim_clip(project, clip["id"], start=6.0)
        self.assertEqual(trimmed["start"], 6.0)
        self.assertEqual(trimmed["duration"], 8.0)
        self.assertEqual(trimmed["inPoint"], 2.0)

    def test_split(self) -> None:
        project = project_with_asset()
        clip = timeline.place_clip(project, "asset-1", at=0.0, duration=10.0)
        left, right = timeline.split_clip(project, clip["id"], 4.0)
        self.assertEqual(left["duration"], 4.0)
        self.assertEqual(right["duration"], 6.0)
        self.assertEqual(right["inPoint"], 4.0)
        self.assertNotEqual(left["id"], right["id"])
        self.assertEqual(len(project["clips"]), 2)

    def test_split_outside_clip_fails(self) -> None:
        project = project_with_asset()
        clip = timeline.place_clip(project, "asset-1", at=0.0, duration=4.0)
        with self.assertRaises(EngineError):
            timeline.split_clip(project, clip["id"], 6.0)

    def test_remove_silences_ripples_and_keeps_source_alignment(self) -> None:
        project = project_with_asset()
        first = timeline.place_clip(project, "asset-1", at=0.0, duration=10.0)
        second = timeline.place_clip(project, "asset-1", at=10.0, duration=10.0, in_point=10.0)

        result = timeline.remove_silences(
            project,
            [{"start": 2.0, "end": 4.0}, {"start": 12.0, "end": 13.0}],
            asset_id="asset-1",
            ripple=True,
        )

        self.assertAlmostEqual(result["removed"], 3.0)
        clips = sorted(project["clips"], key=lambda item: item["start"])
        # First clip is split around 2s-4s; the tail keeps its source offset.
        self.assertEqual(len(clips), 4)
        self.assertAlmostEqual(clips[0]["start"], 0.0)
        self.assertAlmostEqual(clips[0]["duration"], 2.0)
        self.assertAlmostEqual(clips[1]["start"], 2.0)
        self.assertAlmostEqual(clips[1]["inPoint"], 4.0)
        # The second clip moves left by the first cut, then loses 1s itself.
        self.assertAlmostEqual(clips[2]["start"], 8.0)
        self.assertAlmostEqual(clips[3]["start"], 10.0)
        self.assertAlmostEqual(clips[3]["duration"], 7.0)
        self.assertAlmostEqual(clips[3]["inPoint"], 13.0)
        self.assertAlmostEqual(max(clip["start"] + clip["duration"] for clip in project["clips"]), 17.0)

    def test_captions_compact_with_the_timeline(self) -> None:
        project = project_with_asset()
        timeline.place_clip(project, "asset-1", at=0.0, duration=10.0)
        caption = timeline.add_caption(project, 5.0, 6.0, "after the cut")
        timeline.remove_silences(project, [{"start": 2.0, "end": 4.0}], asset_id="asset-1")

        shifted = next(item for item in project["captions"] if item["id"] == caption["id"])
        self.assertAlmostEqual(shifted["start"], 3.0)
        self.assertAlmostEqual(shifted["end"], 4.0)

    def test_captions_split_by_words(self) -> None:
        words = [
            {"text": "Hello", "start": 0.0, "end": 0.4},
            {"text": "world,", "start": 0.4, "end": 0.9},
            {"text": "this", "start": 2.5, "end": 2.8},
            {"text": "is", "start": 2.8, "end": 3.0},
            {"text": "clipfarm.", "start": 3.0, "end": 3.6},
        ]
        captions = timeline.captions_from_words(words, max_chars=64, gap=0.5)
        self.assertEqual(len(captions), 2)
        self.assertEqual(captions[0]["text"], "Hello world,")
        self.assertEqual(captions[1]["text"], "this is clipfarm.")
        self.assertEqual(captions[1]["words"][0]["start"], 2.5)

    def test_caption_validation(self) -> None:
        project = project_with_asset()
        with self.assertRaises(EngineError):
            timeline.add_caption(project, 4.0, 4.0, "nope")


class McpTests(unittest.TestCase):
    def setUp(self) -> None:
        self.directory = tempfile.TemporaryDirectory()
        self.path = os.path.join(self.directory.name, "project.json")
        project = project_with_asset()
        timeline.place_clip(project, "asset-1", at=0.0, duration=12.0)
        save_project(project, self.path)
        self.server = ClipfarmMcpServer(self.path)

    def tearDown(self) -> None:
        self.directory.cleanup()

    def test_lifecycle(self) -> None:
        response = self.server.handle({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}})
        assert response is not None
        self.assertEqual(response["result"]["serverInfo"]["name"], "clipfarm")
        self.assertIn("tools", response["result"]["capabilities"])

        listed = self.server.handle({"jsonrpc": "2.0", "id": 2, "method": "tools/list"})
        assert listed is not None
        names = [tool["name"] for tool in listed["result"]["tools"]]
        self.assertIn("clipfarm_place_clip", names)
        self.assertIn("clipfarm_export", names)
        self.assertEqual(len(names), 11)

        self.assertIsNone(
            self.server.handle(
                {"jsonrpc": "2.0", "method": "notifications/initialized", "params": {}}
            )
        )

    def test_tools_mutate_the_saved_project(self) -> None:
        response = self.server.handle(
            {
                "jsonrpc": "2.0",
                "id": 3,
                "method": "tools/call",
                "params": {
                    "name": "clipfarm_place_clip",
                    "arguments": {"asset_id": "asset-1", "at": 12.0, "duration": 4.0},
                },
            }
        )
        assert response is not None
        self.assertFalse(response["result"]["isError"])
        placed = json.loads(response["result"]["content"][0]["text"])
        self.assertEqual(placed["clip"]["start"], 12.0)

        reloaded = load_project(self.path)
        self.assertEqual(len(reloaded["clips"]), 2)

        caption = self.server.handle(
            {
                "jsonrpc": "2.0",
                "id": 4,
                "method": "tools/call",
                "params": {
                    "name": "clipfarm_add_caption",
                    "arguments": {"start": 1.0, "end": 2.0, "text": "A caption"},
                },
            }
        )
        assert caption is not None
        self.assertFalse(caption["result"]["isError"])
        self.assertEqual(len(load_project(self.path)["captions"]), 1)

    def test_unknown_tool_is_a_tool_error(self) -> None:
        response = self.server.handle(
            {
                "jsonrpc": "2.0",
                "id": 5,
                "method": "tools/call",
                "params": {"name": "clipfarm_nope", "arguments": {}},
            }
        )
        assert response is not None
        self.assertTrue(response["result"]["isError"])


if __name__ == "__main__":
    unittest.main()
