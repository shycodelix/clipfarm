"""End-to-end checks against a real FFmpeg binary.

Skipped automatically when FFmpeg is not installed, so the fast unit suite stays
dependency-free. Run after `npm run python:setup` (or with FFmpeg on PATH).
"""

from __future__ import annotations

import os
import shutil
import subprocess
import sys
import tempfile
import unittest

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from clipfarm_core.project import new_project  # noqa: E402
from clipfarm_core.render import render_project  # noqa: E402
from clipfarm_core.rpc import probe_media  # noqa: E402
from clipfarm_core.silence import detect_silences  # noqa: E402

FFMPEG = shutil.which("ffmpeg")
FFPROBE = shutil.which("ffprobe")


@unittest.skipUnless(FFMPEG and FFPROBE, "FFmpeg is not installed")
class FfmpegPipelineTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.directory = tempfile.TemporaryDirectory()
        cls.media = os.path.join(cls.directory.name, "take.mp4")
        # 8s clip: tone 0-2s, dead air 2-4s, tone 4-8s.
        command = [
            FFMPEG,
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=640x360:rate=30:duration=8",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:sample_rate=48000:duration=8",
            "-filter_complex",
            "[1:a]volume='if(between(t,2,4),0,1)':eval=frame[a]",
            "-map",
            "0:v",
            "-map",
            "[a]",
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
            "-pix_fmt",
            "yuv420p",
            "-c:a",
            "aac",
            "-shortest",
            cls.media,
        ]
        subprocess.run(command, check=True, timeout=120)

    @classmethod
    def tearDownClass(cls) -> None:
        cls.directory.cleanup()

    def project(self) -> dict:
        project = new_project("FFmpeg integration")
        project["width"] = 640
        project["height"] = 360
        project["assets"] = [
            {
                "id": "asset-1",
                "name": "take.mp4",
                "path": self.media,
                "kind": "video",
                "duration": 8.0,
                "width": 640,
                "height": 360,
                "fps": 30,
                "hasAudio": True,
            }
        ]
        project["clips"] = [
            {
                "id": "clip-1",
                "assetId": "asset-1",
                "trackId": "v1",
                "start": 0.0,
                "duration": 8.0,
                "inPoint": 0.0,
                "speed": 1.0,
                "volume": 1.0,
                "label": "take.mp4",
            }
        ]
        project["captions"] = [
            {"id": "cap-1", "start": 0.5, "end": 2.0, "text": "clipfarm renders captions locally", "words": []}
        ]
        return project

    def test_probe_reads_media_metadata(self) -> None:
        metadata = probe_media(self.media)
        self.assertAlmostEqual(metadata["duration"], 8.0, places=1)
        self.assertEqual(metadata["width"], 640)
        self.assertEqual(metadata["height"], 360)
        self.assertTrue(metadata["hasAudio"])

    def test_silence_detection_finds_the_gap(self) -> None:
        result = detect_silences(self.media, threshold_db=-34, min_silence=0.35, padding=0.05)
        silences = result["silences"]
        self.assertTrue(silences, "expected at least one silent stretch")
        longest = max(silences, key=lambda item: item["end"] - item["start"])
        self.assertGreater(longest["end"] - longest["start"], 1.4)
        self.assertLess(abs(longest["start"] - 2.0), 0.4)
        self.assertGreater(longest["end"], 3.4)

    def test_render_with_captions(self) -> None:
        output = os.path.join(self.directory.name, "render.mp4")
        result = render_project(
            self.project(),
            output,
            resolution="source",
            quality="small",
            burn_captions=True,
            fps=30,
        )
        self.assertTrue(os.path.exists(output))
        self.assertGreater(result["bytes"], 10_000)
        self.assertEqual(result["captionsBurned"], 1)
        metadata = probe_media(output)
        self.assertAlmostEqual(metadata["duration"], 8.0, places=1)
        self.assertTrue(metadata["hasAudio"])


if __name__ == "__main__":
    unittest.main()
