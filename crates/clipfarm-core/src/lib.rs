//! clipfarm core: everything that turns a long video into short, exportable clips.
//!
//! The crate is deliberately synchronous and subprocess-based. FFmpeg, yt-dlp and
//! whisper.cpp are external tools that clipfarm orchestrates; the only "AI" that
//! requires a network is an explicitly configured, local-only Ollama endpoint.

pub mod analyze;
pub mod captions;
pub mod config;
pub mod export;
pub mod ingest;
pub mod llm;
pub mod media;
pub mod metadata;
pub mod model;
pub mod pipeline;
pub mod project;
pub mod refine;
pub mod tools;
pub mod transcribe;
pub mod trends;
pub mod util;
pub mod vad;

pub use analyze::{plan_clips, AnalyzeOptions, PlanOutcome};
pub use config::AppConfig;
pub use export::{export_clip, sidecar_subtitles, ExportOptions, ExportOutcome, Quality};
pub use model::{
    Caption, CaptionStyle, Clip, ClipSettings, ExportKind, MediaInfo, Niche, Platform, Project,
    Source, SourceKind, Transcript, Word, MAX_CLIP_SECONDS,
};
pub use project::{data_dir, load_project, new_project, save_project};
pub use tools::Tools;

/// Friendly one-line product promise used by the CLI and the UI.
pub const TAGLINE: &str = "Long video in. Shorts out. No cloud, no credits.";
