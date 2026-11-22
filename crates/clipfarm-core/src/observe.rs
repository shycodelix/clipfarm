//! Local-only, opt-in, off-by-default usage/error observation.
//!
//! This module is the privacy boundary for v0.3 observation. It does not talk to
//! any network. It records a fixed allowlisted set of coarse events into a bounded
//! local file in the user's data directory. When observation is disabled, it is a
//! no-op: no events are written and no file is created.
//!
//! What we record is deliberately small and coarse. See the design doc for the full
//! rationale and the explicit list of what we do NOT track.

use serde::Deserialize;
use std::fs;
use std::io::Write;
use std::path::PathBuf;

use crate::project::data_dir;

// ---------------------------------------------------------------------------
// Consent / store location
// ---------------------------------------------------------------------------

/// Name of the local observation file kept in the data directory.
const OBSERVE_FILE: &str = "observe.jsonl";

/// The app setting that controls whether observation is enabled.
///
/// This lives in `AppConfig` (see `crates/clipfarm-app/Cargo.toml` usage). The
/// storage detail is intentionally small: a single boolean in config.json.
pub const OBSERVE_SETTING_KEY: &str = "observe_enabled";

// ---------------------------------------------------------------------------
// Event schema
// ---------------------------------------------------------------------------

/// Which room the user is currently looking at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Farm,
    Clips,
    Trends,
    Tuning,
}

impl Screen {
    pub const ALL: &[Screen] = &[Screen::Farm, Screen::Clips, Screen::Trends, Screen::Tuning];

    pub fn label(&self) -> &'static str {
        match self {
            Screen::Farm => "farm",
            Screen::Clips => "clips",
            Screen::Trends => "trends",
            Screen::Tuning => "tuning",
        }
    }
}

/// High-level source kind for a harvest start. We do not store the URL or path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceKind {
    Local,
    Remote,
}

impl SourceKind {
    pub fn label(&self) -> &'static str {
        match self {
            SourceKind::Local => "local",
            SourceKind::Remote => "remote",
        }
    }
}

/// Clip count bucket for a completed harvest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipCountBucket {
    Small,
    Large,
}

impl ClipCountBucket {
    /// "Small" = few clips; "large" = lots of clips. The boundary is intentionally
    /// coarse for v0.3.
    pub fn of_clip_count(count: usize) -> Self {
        if count <= 6 {
            ClipCountBucket::Small
        } else {
            ClipCountBucket::Large
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            ClipCountBucket::Small => "small",
            ClipCountBucket::Large => "large",
        }
    }
}

/// Why a harvest/job failed. This is the bounded failure-category enum, not the
/// underlying error text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureCategory {
    SourceMissingOrUnreadable,
    FfmpegMissing,
    FfprobeMissing,
    YtDlpMissing,
    YtDlpFetchFailed,
    NoSpeechDetected,
    VadFailed,
    TranscriptionSkipped,
    ExportFailedEncoder,
    ExportFailedCeiling,
    Unknown,
}

impl FailureCategory {
    pub fn label(&self) -> &'static str {
        match self {
            FailureCategory::SourceMissingOrUnreadable => "source_missing_or_unreadable",
            FailureCategory::FfmpegMissing => "ffmpeg_missing",
            FailureCategory::FfprobeMissing => "ffprobe_missing",
            FailureCategory::YtDlpMissing => "yt_dlp_missing",
            FailureCategory::YtDlpFetchFailed => "yt_dlp_fetch_failed",
            FailureCategory::NoSpeechDetected => "no_speech_detected",
            FailureCategory::VadFailed => "vad_failed",
            FailureCategory::TranscriptionSkipped => "transcription_skipped",
            FailureCategory::ExportFailedEncoder => "export_failed_encoder",
            FailureCategory::ExportFailedCeiling => "export_failed_ceiling",
            FailureCategory::Unknown => "unknown",
        }
    }

    /// Best-effort mapping from a free-form error string to a bounded category.
    /// We do not store the string itself; this exists so the app can translate the
    /// errors it already sees into our allowlisted categories.
    pub fn from_error_text(text: &str) -> Self {
        let lower = text.to_lowercase();
        if lower.contains("ffmpeg") && lower.contains("not found") {
            return FailureCategory::FfmpegMissing;
        }
        if lower.contains("ffprobe") && lower.contains("not found") {
            return FailureCategory::FfprobeMissing;
        }
        if lower.contains("yt-dlp") || lower.contains("yt_dlp") || lower.contains("youtube-dl") {
            if lower.contains("not found") || lower.contains("missing") {
                return FailureCategory::YtDlpMissing;
            }
            return FailureCategory::YtDlpFetchFailed;
        }
        if lower.contains("no speech") || lower.contains("silence") {
            return FailureCategory::NoSpeechDetected;
        }
        if lower.contains("vad") {
            return FailureCategory::VadFailed;
        }
        if lower.contains("transcri") {
            return FailureCategory::TranscriptionSkipped;
        }
        if lower.contains("encoder") {
            return FailureCategory::ExportFailedEncoder;
        }
        if lower.contains("ceiling") || lower.contains("duration") {
            return FailureCategory::ExportFailedCeiling;
        }
        FailureCategory::Unknown
    }
}

/// Coarse shape of a refine instruction, not the instruction text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefineShape {
    DurationRequest,
    StartShift,
    CaptionsToggle,
    VerticalToggle,
    Tighten,
    TitleOrCaptionSet,
    Other,
}

impl RefineShape {
    pub fn label(&self) -> &'static str {
        match self {
            RefineShape::DurationRequest => "duration_request",
            RefineShape::StartShift => "start_shift",
            RefineShape::CaptionsToggle => "captions_toggle",
            RefineShape::VerticalToggle => "vertical_toggle",
            RefineShape::Tighten => "tighten",
            RefineShape::TitleOrCaptionSet => "title_or_caption_set",
            RefineShape::Other => "other",
        }
    }

    /// Best-effort coarse shape from the refine change descriptions the app already
    /// produces. We look at keywords in the change text, not the structured ops.
    pub fn from_refine_changes(changes: &[String]) -> Self {
        let mut has_duration = false;
        let mut has_start_shift = false;
        let mut has_captions_toggle = false;
        let mut has_vertical_toggle = false;
        let mut has_tighten = false;
        let mut has_title_or_caption = false;

        for change in changes {
            let lower = change.to_lowercase();
            if lower.contains("duration") || lower.contains("second") || lower.contains("shorten") {
                has_duration = true;
            }
            if lower.contains("start") || lower.contains("begin") || lower.contains("shift") {
                has_start_shift = true;
            }
            if lower.contains("caption") || lower.contains("subtitle") || lower.contains("closed") {
                has_captions_toggle = true;
            }
            if lower.contains("vertical") || lower.contains("portrait") || lower.contains("9:16") {
                has_vertical_toggle = true;
            }
            if lower.contains("tighten") || lower.contains("shrink") || lower.contains("cut") || lower.contains("trim") {
                has_tighten = true;
            }
            if lower.contains("title") || lower.contains("caption") || lower.contains("name") {
                has_title_or_caption = true;
            }
        }

        // Prefer the most specific shape we can identify; fall back to Other.
        if has_title_or_caption {
            return RefineShape::TitleOrCaptionSet;
        }
        if has_duration {
            return RefineShape::DurationRequest;
        }
        if has_start_shift {
            return RefineShape::StartShift;
        }
        if has_captions_toggle {
            return RefineShape::CaptionsToggle;
        }
        if has_vertical_toggle {
            return RefineShape::VerticalToggle;
        }
        if has_tighten {
            return RefineShape::Tighten;
        }
        RefineShape::Other
    }
}

/// Coarse export encoder bucket, not the exact encoder string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncoderBucket {
    Libx264,
    Hardware,
    StreamCopy,
    Other,
}

impl EncoderBucket {
    pub fn label(&self) -> &'static str {
        match self {
            EncoderBucket::Libx264 => "libx264",
            EncoderBucket::Hardware => "hardware",
            EncoderBucket::StreamCopy => "stream_copy",
            EncoderBucket::Other => "other",
        }
    }

    /// Best-effort classification from an encoder name the app already knows.
    pub fn from_encoder_name(name: &str) -> Self {
        let lower = name.to_lowercase();
        if lower.contains("x264") || lower.contains("libx264") {
            return EncoderBucket::Libx264;
        }
        if lower.contains("nvenc")
            || lower.contains("qsv")
            || lower.contains("amf")
            || lower.contains("videotoolbox")
            || lower.contains("h264_")
        {
            return EncoderBucket::Hardware;
        }
        if lower.contains("copy") {
            return EncoderBucket::StreamCopy;
        }
        EncoderBucket::Other
    }
}

/// One observation event, serialized as one JSON line.
#[derive(Debug, Clone)]
pub struct Event {
    pub kind: EventKind,
    pub screen: Option<Screen>,
    pub source_kind: Option<SourceKind>,
    pub clip_count_bucket: Option<ClipCountBucket>,
    pub whisper_used: Option<bool>,
    pub llm_enabled: Option<bool>,
    pub failure_category: Option<FailureCategory>,
    pub refine_shape: Option<RefineShape>,
    pub export_kind: Option<String>,
    pub caption_on: Option<bool>,
    pub vertical: Option<bool>,
    pub encoder_bucket: Option<EncoderBucket>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventKind {
    AppStarted,
    ScreenViewed,
    HarvestStarted,
    HarvestCompleted,
    HarvestFailed,
    ClipRefined,
    ClipExported,
}

impl EventKind {
    pub fn label(&self) -> &'static str {
        match self {
            EventKind::AppStarted => "app_started",
            EventKind::ScreenViewed => "screen_viewed",
            EventKind::HarvestStarted => "harvest_started",
            EventKind::HarvestCompleted => "harvest_completed",
            EventKind::HarvestFailed => "harvest_failed",
            EventKind::ClipRefined => "clip_refined",
            EventKind::ClipExported => "clip_exported",
        }
    }
}

// ---------------------------------------------------------------------------
// Store
// ---------------------------------------------------------------------------

/// A local, bounded, append-only observation store.
pub struct Store {
    path: PathBuf,
    enabled: bool,
    max_events: usize,
}

impl Store {
    /// Create (or open) the store for the current user.
    pub fn new(enabled: bool) -> anyhow::Result<Self> {
        let path = data_dir().join(OBSERVE_FILE);
        if enabled {
            fs::create_dir_all(data_dir())?;
        }
        Ok(Self {
            path,
            enabled,
            max_events: 256,
        })
    }
}

impl Store {
    /// Append an event if observation is enabled.
    pub fn append(&self, event: Event) -> anyhow::Result<()> {
        if !self.enabled {
            return Ok(());
        }
        let line = serde_json::to_string(&event)?;
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        writeln!(file, "{line}")?;
        self.cull()?;
        Ok(())
    }

    /// Delete the local observation file. This is the "clear local usage data" action.
    pub fn clear(&self) -> anyhow::Result<()> {
        if self.path.exists() {
            fs::remove_file(&self.path)?;
        }
        Ok(())
    }

    /// Returns `true` if the local observation file exists.
    pub fn has_data(&self) -> bool {
        self.path.exists()
    }

    /// Read recent events for the local usage summary, if observation is enabled.
    pub fn recent_events(&self) -> anyhow::Result<Vec<Event>> {
        if !self.enabled {
            return Ok(Vec::new());
        }
        let text = match fs::read_to_string(&self.path) {
            Ok(t) => t,
            Err(_) => return Ok(Vec::new()),
        };
        let mut events = Vec::new();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            if let Ok(event) = serde_json::from_str::<Event>(line) {
                events.push(event);
            }
        }
        Ok(events)
    }

    fn cull(&self) -> anyhow::Result<()> {
        if !self.enabled {
            return Ok(());
        }
        let events = self.recent_events()?;
        if events.len() <= self.max_events {
            return Ok(());
        }
        let kept = events.split_at(events.len() - self.max_events).1.to_vec();
        let mut file = fs::File::create(&self.path)?;
        use std::io::Write;
        for event in kept {
            writeln!(file, "{}", serde_json::to_string(&event)?)?;
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Serialization helpers
// ---------------------------------------------------------------------------

impl serde::Serialize for Event {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(Some(12))?;
        map.serialize_entry("kind", &self.kind.label())?;
        map.serialize_entry("screen", &self.screen.map(|s| s.label()))?;
        map.serialize_entry("source_kind", &self.source_kind.map(|s| s.label()))?;
        map.serialize_entry("clip_count_bucket", &self.clip_count_bucket.map(|s| s.label()))?;
        map.serialize_entry("whisper_used", &self.whisper_used)?;
        map.serialize_entry("llm_enabled", &self.llm_enabled)?;
        map.serialize_entry("failure_category", &self.failure_category.map(|s| s.label()))?;
        map.serialize_entry("refine_shape", &self.refine_shape.map(|s| s.label()))?;
        map.serialize_entry("export_kind", &self.export_kind)?;
        map.serialize_entry("caption_on", &self.caption_on)?;
        map.serialize_entry("vertical", &self.vertical)?;
        map.serialize_entry("encoder_bucket", &self.encoder_bucket.map(|s| s.label()))?;
        map.end()
    }
}

impl<'de> serde::Deserialize<'de> for Event {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Raw {
            kind: String,
            screen: Option<String>,
            source_kind: Option<String>,
            clip_count_bucket: Option<String>,
            whisper_used: Option<bool>,
            llm_enabled: Option<bool>,
            failure_category: Option<String>,
            refine_shape: Option<String>,
            export_kind: Option<String>,
            caption_on: Option<bool>,
            vertical: Option<bool>,
            encoder_bucket: Option<String>,
        }

        let raw = Raw::deserialize(deserializer)?;

        let kind = match raw.kind.as_str() {
            "app_started" => EventKind::AppStarted,
            "screen_viewed" => EventKind::ScreenViewed,
            "harvest_started" => EventKind::HarvestStarted,
            "harvest_completed" => EventKind::HarvestCompleted,
            "harvest_failed" => EventKind::HarvestFailed,
            "clip_refined" => EventKind::ClipRefined,
            "clip_exported" => EventKind::ClipExported,
            _ => return Err(serde::de::Error::unknown_variant(&raw.kind, &["see EventKind"])),
        };

        let screen = raw.screen.and_then(|s| match s.as_str() {
            "farm" => Some(Screen::Farm),
            "clips" => Some(Screen::Clips),
            "trends" => Some(Screen::Trends),
            "tuning" => Some(Screen::Tuning),
            _ => None,
        });

        let source_kind = raw.source_kind.and_then(|s| match s.as_str() {
            "local" => Some(SourceKind::Local),
            "remote" => Some(SourceKind::Remote),
            _ => None,
        });

        let clip_count_bucket = raw.clip_count_bucket.and_then(|s| match s.as_str() {
            "small" => Some(ClipCountBucket::Small),
            "large" => Some(ClipCountBucket::Large),
            _ => None,
        });

        let failure_category = raw.failure_category.and_then(|s| match s.as_str() {
            "source_missing_or_unreadable" => Some(FailureCategory::SourceMissingOrUnreadable),
            "ffmpeg_missing" => Some(FailureCategory::FfmpegMissing),
            "ffprobe_missing" => Some(FailureCategory::FfprobeMissing),
            "yt_dlp_missing" => Some(FailureCategory::YtDlpMissing),
            "yt_dlp_fetch_failed" => Some(FailureCategory::YtDlpFetchFailed),
            "no_speech_detected" => Some(FailureCategory::NoSpeechDetected),
            "vad_failed" => Some(FailureCategory::VadFailed),
            "transcription_skipped" => Some(FailureCategory::TranscriptionSkipped),
            "export_failed_encoder" => Some(FailureCategory::ExportFailedEncoder),
            "export_failed_ceiling" => Some(FailureCategory::ExportFailedCeiling),
            "unknown" => Some(FailureCategory::Unknown),
            _ => None,
        });

        let refine_shape = raw.refine_shape.and_then(|s| match s.as_str() {
            "duration_request" => Some(RefineShape::DurationRequest),
            "start_shift" => Some(RefineShape::StartShift),
            "captions_toggle" => Some(RefineShape::CaptionsToggle),
            "vertical_toggle" => Some(RefineShape::VerticalToggle),
            "tighten" => Some(RefineShape::Tighten),
            "title_or_caption_set" => Some(RefineShape::TitleOrCaptionSet),
            "other" => Some(RefineShape::Other),
            _ => None,
        });

        let encoder_bucket = raw.encoder_bucket.and_then(|s| match s.as_str() {
            "libx264" => Some(EncoderBucket::Libx264),
            "hardware" => Some(EncoderBucket::Hardware),
            "stream_copy" => Some(EncoderBucket::StreamCopy),
            "other" => Some(EncoderBucket::Other),
            _ => None,
        });

        Ok(Event {
            kind,
            screen,
            source_kind,
            clip_count_bucket,
            whisper_used: raw.whisper_used,
            llm_enabled: raw.llm_enabled,
            failure_category,
            refine_shape,
            export_kind: raw.export_kind,
            caption_on: raw.caption_on,
            vertical: raw.vertical,
            encoder_bucket,
        })
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    fn temp_dir() -> PathBuf {
        let mut path = env::temp_dir();
        let id = std::process::id();
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        path.push(format!("clipfarm-observe-test-{id}-{seed}"));
        std::fs::create_dir_all(&path).ok();
        path
    }

    fn enabled_store(temp: &PathBuf) -> Store {
       Store {
            path: temp.join(OBSERVE_FILE),
            enabled: true,
            max_events: 4,
        }
    }

    fn disabled_store(temp: &PathBuf) -> Store {
        Store {
            path: temp.join(OBSERVE_FILE),
            enabled: false,
            max_events: 4,
        }
    }

    #[test]
    fn disabled_store_is_no_op() {
        let temp = temp_dir();
        let store = disabled_store(&temp);
        store
            .append(Event {
                kind: EventKind::AppStarted,
                screen: None,
                source_kind: None,
                clip_count_bucket: None,
                whisper_used: None,
                llm_enabled: None,
                failure_category: None,
                refine_shape: None,
                export_kind: None,
                caption_on: None,
                vertical: None,
                encoder_bucket: None,
            })
            .unwrap();
        assert!(!store.path.exists());
        store.clear().unwrap();
    }

    #[test]
    fn enabled_store_appends_and_reads_back() {
        let temp = temp_dir();
        let store = enabled_store(&temp);
        store.clear().unwrap();

        store.append(make_event(EventKind::AppStarted)).unwrap();
        store.append(make_event(EventKind::ScreenViewed)).unwrap();

        let events = store.recent_events().unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].kind, EventKind::AppStarted);
        assert_eq!(events[1].kind, EventKind::ScreenViewed);
    }

    #[test]
    fn clear_removes_local_file() {
        let temp = temp_dir();
        let store = enabled_store(&temp);
        store.clear().unwrap();
        store.append(make_event(EventKind::HarvestStarted)).unwrap();
        assert!(store.path.exists());
        store.clear().unwrap();
        assert!(!store.path.exists());
    }

    #[test]
    fn bounded_store_culls_oldest() {
        let temp = temp_dir();
        let store = enabled_store(&temp);
        store.clear().unwrap();

        for _ in 0..6 {
            store.append(make_event(EventKind::ClipExported)).unwrap();
        }

        let events = store.recent_events().unwrap();
        assert_eq!(events.len(), 4);
        assert_eq!(events[0].kind, EventKind::ClipExported);
        assert_eq!(events[3].kind, EventKind::ClipExported);
    }

    #[test]
    fn failure_category_mapping_is_coarse() {
        assert_eq!(
            FailureCategory::from_error_text("ffmpeg not found"),
            FailureCategory::FfmpegMissing
        );
        assert_eq!(
            FailureCategory::from_error_text("ffprobe was not found"),
            FailureCategory::FfprobeMissing
        );
        assert_eq!(
            FailureCategory::from_error_text("some weird thing happened"),
            FailureCategory::Unknown
        );
    }

    #[test]
    fn encoder_bucket_classification_is_coarse() {
        assert_eq!(EncoderBucket::from_encoder_name("libx264"), EncoderBucket::Libx264);
        assert_eq!(EncoderBucket::from_encoder_name("h264_nvenc"), EncoderBucket::Hardware);
        assert_eq!(EncoderBucket::from_encoder_name("copy"), EncoderBucket::StreamCopy);
        assert_eq!(EncoderBucket::from_encoder_name("something_else"), EncoderBucket::Other);
    }

    fn make_event(kind: EventKind) -> Event {
        Event {
            kind,
            screen: None,
            source_kind: None,
            clip_count_bucket: None,
            whisper_used: None,
            llm_enabled: None,
            failure_category: None,
            refine_shape: None,
            export_kind: None,
            caption_on: None,
            vertical: None,
            encoder_bucket: None,
        }
    }
}
