# clipfarm project format

A project is a single JSON document. The desktop app writes it to
`<userData>/projects/<id>.json` and the MCP server reads and writes the same file,
so an agent and the UI always share one source of truth.

```jsonc
{
  "schemaVersion": 1,
  "id": "d5b4…",
  "name": "Studio Session — Mara Chen",
  "createdAt": "2026-11-18T18:00:00Z",
  "updatedAt": "2026-11-18T18:12:04Z",
  "fps": 30,
  "width": 1920,
  "height": 1080,
  "assets": [
    {
      "id": "a-interview",
      "name": "Mara Chen — Interview.mp4",
      "path": "/Users/you/Footage/interview.mp4",
      "kind": "video",          // video | audio | image
      "duration": 186.4,        // seconds
      "width": 1920,
      "height": 1080,
      "fps": 24,
      "hasAudio": true,
      "thumbnailPath": "/…/thumbnails/a-interview.jpg"
    }
  ],
  "tracks": [
    { "id": "v2", "name": "Overlay", "kind": "overlay",  "height": 58, "muted": false, "locked": false },
    { "id": "v1", "name": "Video",   "kind": "video",    "height": 74, "muted": false, "locked": false },
    { "id": "cc", "name": "Captions","kind": "captions", "height": 46, "muted": false, "locked": false }
  ],
  "clips": [
    {
      "id": "c1",
      "assetId": "a-interview",
      "trackId": "v1",
      "start": 0.0,        // timeline position, seconds
      "duration": 21.8,    // timeline length, seconds
      "inPoint": 4.2,      // source offset, seconds
      "speed": 1.0,
      "volume": 1.0,
      "label": "Mara Chen — Interview.mp4"
    }
  ],
  "captions": [
    {
      "id": "cap-1",
      "start": 0.8,
      "end": 4.2,
      "text": "Everyone says the edit is the easy part.",
      "words": [{ "text": "Everyone", "start": 0.8, "end": 1.2, "confidence": 0.94 }]
    }
  ],
  "scenes": [{ "index": 0, "assetId": "a-interview", "start": 4.2, "end": 42.6 }],
  "silences": [{ "start": 2.1, "end": 3.4 }],
  "settings": {
    "captionsEnabled": true,
    "captionPreset": "clean",   // clean | bold | minimal
    "silencePadding": 0.12,
    "autoRipple": true
  }
}
```

## Semantics an agent must know

- **All times are seconds**, floats, on the timeline. `inPoint` is the only
  source-relative value.
- A clip occupies `[start, start + duration)` on its track. Overlapping clips on
  the same video track render in start order; the `overlay` track composites over
  the program.
- `scenes[].start/end` and `silences[].start/end` are **source-media** times.
  The renderer maps them onto each clip through `inPoint` and `speed`.
- Removing silence rewrites clips (splitting them into audible fragments) and
  shifts later clips on the same track. Captions are compacted with the timeline.
- The captions track is derived from `captions[]`; there are no caption clips.

## MCP tools

Start the server with the project path:

```bash
python -m clipfarm_core mcp --project ~/Library/Application\ Support/clipfarm/projects/<id>.json
```

| Tool | Purpose |
| --- | --- |
| `clipfarm_get_timeline` | Read tracks, clips, captions, scenes, silences |
| `clipfarm_list_assets` | List imported media |
| `clipfarm_place_clip` | Place an asset at `at` on `track_id` |
| `clipfarm_trim_clip` | Move start, change duration or in-point |
| `clipfarm_split_clip` | Split at an absolute timeline time |
| `clipfarm_delete_clip` | Remove a clip |
| `clipfarm_add_caption` | Add `{start, end, text}` |
| `clipfarm_remove_silences` | Detect + ripple-cut dead air |
| `clipfarm_detect_scenes` | PySceneDetect boundaries, stored on the project |
| `clipfarm_transcribe` | faster-whisper word-timed captions |
| `clipfarm_export` | FFmpeg render to `output` |

Mutations are written atomically (temp file + rename) and return a compact summary
of the new state, so a client can verify each step without a second read.

## Engine RPC (internal)

The Electron main process keeps a Python sidecar alive and speaks newline-delimited
JSON-RPC over stdio:

```json
{"id": 7, "method": "analyze.silence", "params": {"jobId": "…", "path": "…", "options": {"thresholdDb": -34, "minSilence": 0.35, "padding": 0.12}}}
{"id": 7, "ok": true, "result": {"silences": [{"start": 2.1, "end": 3.4}]}}
```

Progress arrives as notifications (`{"event":"progress","id":7,"progress":0.42,"stage":"…"}`)
and can be cancelled with `{"method":"cancel","params":{"jobId":"…"}}`.
