# Architecture

clipfarm is three Rust crates and a set of external, replaceable tools. The engine is
synchronous, the UI owns one background job at a time, and every long operation reports
progress through a callback.

```
┌────────────────────────────── clipfarm-app (egui/eframe) ─────────────────────────────┐
│  farm / clips / trends / tuning                                                       │
│  project in memory · thumbnail textures · refine chat · export queue (one job)         │
└───────────────────────────────────────┬───────────────────────────────────────────────┘
                                        │ direct calls, one worker thread
┌───────────────────────────────────────▼───────────────────────────────────────────────┐
│ clipfarm-core                                                                         │
│  ingest ──► probe ──► VAD ──► transcribe ──► analyze/plan ──► metadata ──► export       │
│  project store (atomic JSON)        trends profiles        refine ops                  │
└───────────────┬───────────────────────────────┬───────────────────────────────┬────────┘
                │                               │                               │
           yt-dlp (optional)              whisper.cpp (optional)          FFmpeg / ffprobe
                                                                         (required)
```

## Crates

### `clipfarm-core`

| Module | Responsibility |
| --- | --- |
| `model` | `Project`, `Clip`, `Word`, `Caption`, enums, the 60s constant and invariants |
| `project` | data directory, atomic JSON save/load, listing, export folders |
| `config` | `config.json`, env overrides, loopback check for the LLM |
| `tools` | discovery and health of FFmpeg/ffprobe/yt-dlp/whisper, encoder probing |
| `media` | ffprobe normalization, thumbnails, smart-frame heuristic, FFmpeg progress runner |
| `vad` | streaming PCM energy VAD + pure `detect_pcm` for tests |
| `transcribe` | whisper.cpp invocation and defensive JSON parsing |
| `ingest` | local files, yt-dlp download with progress, title capture |
| `analyze` | candidate windows, scoring, clip construction, transcript-aware hooks |
| `captions` | line grouping, filler removal, SRT rendering |
| `metadata` | offline titles/captions/hashtags + optional local-LLM polish |
| `trends` | bundled niche/platform profiles, override feed, save |
| `refine` | natural-language instruction → explicit `RefineOp`s |
| `export` | raw stream copy, 9:16 render, encoder selection, ceiling fallback |
| `pipeline` | ingest → analyze orchestration, thumbnails, clip lookup, default paths |
| `llm` | minimal Ollama JSON client with a loopback guard |

### `clipfarm-cli`

A thin `clap` front end: `doctor`, `ingest`, `analyze`, `projects`, `clips`, `refine`,
`export`, `trends`, `paths`, `open`. It is the reference consumer of the engine and is
used in CI/integration tests.

### `clipfarm-app`

The Rust desktop UI. It owns no media logic: it calls `pipeline::create_project`,
`pipeline::analyze`, `refine::refine` and `export_clip` on a worker thread and consumes
`JobEvent`s on the UI thread. Thumbnails are decoded with `image` and uploaded as egui
textures.

## Project JSON

Stored at `<data>/projects/<uuid>.json`. Schema version 1:

```jsonc
{
  "schema_version": 1,
  "id": "…",
  "name": "Studio session",
  "created_at": "…", "updated_at": "…",
  "niche": "podcast",
  "platform": "tiktok",
  "source": { "kind": "local", "path": "/abs/file.mp4", "url": null, "display_name": "…" },
  "media": { "duration": 2718.4, "width": 1920, "height": 1080, "fps": 29.97,
             "has_audio": true, "video_codec": "h264", "audio_codec": "aac",
             "size_bytes": 123 },
  "transcript": { "language": "en", "duration": 2718.0,
                  "words": [{ "text": "…", "start": 12.31, "end": 12.62, "probability": 0.94 }] },
  "clips": [{
    "id": "…", "start": 12.0, "end": 42.0, "score": 78.0,
    "hook": "…", "reasons": ["platform-length", "question hook"],
    "title": "…", "caption": "…", "hashtags": ["shorts", "podcast"],
    "captions": [{ "start": 12.4, "end": 15.1, "text": "…", "words": [] }],
    "settings": { "vertical": true, "captions_enabled": true, "caption_style": "bold",
                  "pad_start": 0.08, "pad_end": 0.14, "focus_x": 0.5 },
    "status": "kept", "thumbnail": "/abs/thumb.jpg",
    "exports": [{ "path": "…", "kind": "raw", "preset": "stream-copy",
                  "bytes": 123, "created_at": "…" }]
  }],
  "settings": { "target_clips": 6, "max_clip_seconds": 60.0,
                "captions_enabled": true, "vertical": true, "llm_enabled": false }
}
```

Timestamps are source-media seconds (floating point). Captions inside a clip stay in
source time; SRT generation subtracts `clip.start`.

## Invariants

1. **≤60 seconds, everywhere.** `Clip::enforce_cap` runs after planning, refinement,
   project load and save; `validate_duration` refuses export. Raw stream-copy output is
   probed and re-encoded if keyframe alignment overshoots.
2. **No cloud inference.** `AppConfig::llm_allowed` requires `llm_enabled` *and* a
   loopback endpoint; `llm::complete_json` re-checks the host itself.
3. **Optional tools degrade, required tools fail loudly.** No whisper → VAD-only with a
   visible warning. No FFmpeg → explicit error with install hints.
4. **Source media is read-only.** Projects reference local files; downloads and
   generated assets live under the data directory.
5. **Refine cannot invent timeline state.** A fixed `RefineOp` enum is applied in order,
   clamped to source duration, and described back to the user.

## Concurrency

The UI spawns one `std::thread` per job and sends `JobEvent::{Progress, Harvest,
Exported, Failed}` over an `mpsc` channel. FFmpeg progress is parsed from
`-progress pipe:1` (`out_time_us` and `speed`). Cancellation for the CLI/engine is
process-level: the child FFmpeg is owned by the call, so dropping the job process ends
it. In-app cancellation is not exposed in v0.2; the UI stays responsive and the job bar
is honest about progress.

## Extension points

- **Face tracking:** replace `media::focus_center` with an ONNX Runtime model returning
  a crop center; the renderer already consumes `settings.focus_x`.
- **Real-time trends:** implement a producer that writes `<data>/trends.json`; no engine
  change needed.
- **More refine operations:** add a variant to `RefineOp`, apply it in `apply_ops`, and
  document it in the LLM prompt.
- **Batch exports:** `export_clip` is pure with respect to project state, so a queue can
  call it concurrently with distinct output paths.
- **MCP / agent bridge:** the JSON project format is the contract; a small server could
  reuse the same ops if agent control returns in a future version.
