use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

/// Hard ceiling for every exported clip. Shorts platforms cap at 60s (TikTok now
/// allows longer, but retention and the brief both favor a strict ceiling).
pub const MAX_CLIP_SECONDS: f64 = 60.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Platform {
    YoutubeShorts,
    TikTok,
    InstagramReels,
}

impl Platform {
    pub const ALL: [Platform; 3] = [
        Platform::YoutubeShorts,
        Platform::TikTok,
        Platform::InstagramReels,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Platform::YoutubeShorts => "YouTube Shorts",
            Platform::TikTok => "TikTok",
            Platform::InstagramReels => "Instagram Reels",
        }
    }

    pub fn slug(self) -> &'static str {
        match self {
            Platform::YoutubeShorts => "youtube_shorts",
            Platform::TikTok => "tiktok",
            Platform::InstagramReels => "instagram_reels",
        }
    }

    pub fn parse(value: &str) -> Result<Self> {
        match value
            .trim()
            .to_ascii_lowercase()
            .replace([' ', '-'], "_")
            .as_str()
        {
            "youtube_shorts" | "shorts" | "youtube" => Ok(Platform::YoutubeShorts),
            "tiktok" | "tik_tok" => Ok(Platform::TikTok),
            "instagram_reels" | "reels" | "instagram" => Ok(Platform::InstagramReels),
            other => Err(anyhow!(
                "Unknown platform `{other}`. Use youtube_shorts, tiktok or instagram_reels."
            )),
        }
    }

    pub fn aspect(self) -> &'static str {
        "9:16"
    }

    pub fn target_seconds(self) -> (f64, f64) {
        match self {
            Platform::YoutubeShorts => (20.0, 58.0),
            Platform::TikTok => (15.0, 45.0),
            Platform::InstagramReels => (18.0, 50.0),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Niche {
    Podcast,
    Gaming,
    Fitness,
    Tech,
    Comedy,
    Education,
    Generic,
}

impl Niche {
    pub const ALL: [Niche; 7] = [
        Niche::Podcast,
        Niche::Gaming,
        Niche::Fitness,
        Niche::Tech,
        Niche::Comedy,
        Niche::Education,
        Niche::Generic,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Niche::Podcast => "Podcast / interview",
            Niche::Gaming => "Gaming",
            Niche::Fitness => "Fitness",
            Niche::Tech => "Tech",
            Niche::Comedy => "Comedy",
            Niche::Education => "Education",
            Niche::Generic => "Something else",
        }
    }

    pub fn slug(self) -> &'static str {
        match self {
            Niche::Podcast => "podcast",
            Niche::Gaming => "gaming",
            Niche::Fitness => "fitness",
            Niche::Tech => "tech",
            Niche::Comedy => "comedy",
            Niche::Education => "education",
            Niche::Generic => "generic",
        }
    }

    pub fn parse(value: &str) -> Result<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "podcast" | "interview" => Ok(Niche::Podcast),
            "gaming" | "game" => Ok(Niche::Gaming),
            "fitness" | "gym" => Ok(Niche::Fitness),
            "tech" | "technology" => Ok(Niche::Tech),
            "comedy" | "funny" => Ok(Niche::Comedy),
            "education" | "edu" => Ok(Niche::Education),
            "generic" | "other" | "" => Ok(Niche::Generic),
            other => Err(anyhow!(
                "Unknown niche `{other}`. Try podcast, gaming, fitness, tech, comedy, education or generic."
            )),
        }
    }

    /// Words that make a moment feel native to the niche. Used by the hook scorer
    /// and the title generator; it is intentionally small and editable.
    pub fn keywords(self) -> &'static [&'static str] {
        match self {
            Niche::Podcast => &[
                "story", "actually", "never", "nobody", "truth", "mistake", "learned", "years",
                "money", "failed",
            ],
            Niche::Gaming => &[
                "clutch", "insane", "nerf", "buff", "meta", "wiped", "carry", "rank", "speedrun",
                "glitch",
            ],
            Niche::Fitness => &[
                "form",
                "reps",
                "protein",
                "bulk",
                "cut",
                "injury",
                "mobility",
                "sets",
                "progressive",
                "recovery",
            ],
            Niche::Tech => &[
                "benchmark",
                "shipping",
                "latency",
                "database",
                "compiler",
                "model",
                "open source",
                "latency",
                "security",
                "tradeoff",
            ],
            Niche::Comedy => &[
                "joke",
                "bit",
                "crowd",
                "hell",
                "literally",
                "unhinged",
                "chaos",
                "bit",
                "punchline",
                "wait",
            ],
            Niche::Education => &[
                "because",
                "therefore",
                "example",
                "means",
                "step",
                "principle",
                "proof",
                "remember",
                "concept",
                "explain",
            ],
            Niche::Generic => &["how", "why", "never", "best", "worst", "secret", "change"],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    Local,
    Remote,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Source {
    pub kind: SourceKind,
    /// Absolute path on disk. For remote sources this is the downloaded file.
    pub path: PathBuf,
    /// Original URL, when the source came from yt-dlp.
    pub url: Option<String>,
    pub display_name: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MediaInfo {
    pub duration: f64,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub has_audio: bool,
    pub video_codec: String,
    pub audio_codec: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Word {
    pub text: String,
    pub start: f64,
    pub end: f64,
    #[serde(default)]
    pub probability: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Caption {
    /// Source-media time, seconds.
    pub start: f64,
    pub end: f64,
    pub text: String,
    #[serde(default)]
    pub words: Vec<Word>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Transcript {
    pub language: String,
    pub words: Vec<Word>,
    /// Audio duration reported by the transcriber; 0 when unknown.
    pub duration: f64,
}

impl Transcript {
    pub fn words_between(&self, start: f64, end: f64) -> Vec<Word> {
        self.words
            .iter()
            .filter(|word| word.end > start && word.start < end)
            .cloned()
            .collect()
    }

    pub fn text_between(&self, start: f64, end: f64) -> String {
        self.words_between(start, end)
            .into_iter()
            .map(|word| word.text)
            .collect::<Vec<_>>()
            .join(" ")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptionStyle {
    Clean,
    Bold,
    Karaoke,
}

impl CaptionStyle {
    pub const ALL: [CaptionStyle; 3] = [
        CaptionStyle::Clean,
        CaptionStyle::Bold,
        CaptionStyle::Karaoke,
    ];

    pub fn label(self) -> &'static str {
        match self {
            CaptionStyle::Clean => "Clean",
            CaptionStyle::Bold => "Bold",
            CaptionStyle::Karaoke => "Karaoke",
        }
    }

    /// libass force_style string for burned-in captions.
    pub fn force_style(self) -> &'static str {
        match self {
            CaptionStyle::Clean => {
                "FontName=Helvetica,FontSize=16,PrimaryColour=&H00FFFFFF,OutlineColour=&H66000000,BorderStyle=1,Outline=1,Shadow=0,Alignment=2,MarginV=80"
            }
            CaptionStyle::Bold => {
                "FontName=Helvetica,FontSize=20,Bold=1,PrimaryColour=&H00FFFFFF,OutlineColour=&H99000000,BorderStyle=1,Outline=2,Shadow=0,Alignment=2,MarginV=110"
            }
            CaptionStyle::Karaoke => {
                "FontName=Helvetica,FontSize=20,Bold=1,PrimaryColour=&H0000E5FF,OutlineColour=&HAA000000,BorderStyle=1,Outline=2,Shadow=0,Alignment=2,MarginV=110"
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipSettings {
    pub vertical: bool,
    pub captions_enabled: bool,
    pub caption_style: CaptionStyle,
    pub pad_start: f64,
    pub pad_end: f64,
    /// Center of the subject as a 0..1 fraction of the source width. Used to place
    /// the 9:16 window. 0.5 is dead center.
    pub focus_x: f64,
}

impl Default for ClipSettings {
    fn default() -> Self {
        Self {
            vertical: true,
            captions_enabled: true,
            caption_style: CaptionStyle::Bold,
            pad_start: 0.08,
            pad_end: 0.14,
            focus_x: 0.5,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportKind {
    Raw,
    Rendered,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportRecord {
    pub path: PathBuf,
    pub kind: ExportKind,
    pub preset: String,
    pub bytes: u64,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClipStatus {
    Planned,
    Kept,
    Rejected,
    Exported,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Clip {
    pub id: Uuid,
    pub start: f64,
    pub end: f64,
    pub score: f32,
    pub hook: String,
    pub reasons: Vec<String>,
    pub title: String,
    pub caption: String,
    pub hashtags: Vec<String>,
    #[serde(default)]
    pub captions: Vec<Caption>,
    pub settings: ClipSettings,
    pub status: ClipStatus,
    #[serde(default)]
    pub thumbnail: Option<PathBuf>,
    #[serde(default)]
    pub exports: Vec<ExportRecord>,
}

impl Clip {
    pub fn duration(&self) -> f64 {
        (self.end - self.start).max(0.0)
    }

    /// Returns an error when a clip exceeds the platform-agnostic 60s ceiling.
    pub fn validate_duration(&self) -> Result<()> {
        if self.duration() > MAX_CLIP_SECONDS + 1e-6 {
            return Err(anyhow!(
                "Clip is {:.1}s — the hard ceiling is {}s. Trim it first.",
                self.duration(),
                MAX_CLIP_SECONDS as i64
            ));
        }
        if self.duration() < 2.0 {
            return Err(anyhow!(
                "Clip is too short to export ({:.1}s).",
                self.duration()
            ));
        }
        Ok(())
    }

    pub fn enforce_cap(&mut self) {
        if self.end - self.start > MAX_CLIP_SECONDS {
            self.end = self.start + MAX_CLIP_SECONDS;
        }
        self.start = self.start.max(0.0);
        self.end = self.end.max(self.start + 2.0);
    }

    pub fn shift(&mut self, delta: f64) {
        self.start = (self.start + delta).max(0.0);
        self.end = self.end.max(self.start + 2.0);
        self.enforce_cap();
    }

    pub fn tighten(&mut self, seconds: f64) {
        let target = seconds.clamp(2.0, MAX_CLIP_SECONDS);
        if self.end - self.start > target {
            self.end = self.start + target;
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectSettings {
    pub target_clips: usize,
    pub max_clip_seconds: f64,
    pub captions_enabled: bool,
    pub vertical: bool,
    /// Local-only LLM assistance; disabled by default so nothing leaves the box.
    pub llm_enabled: bool,
}

impl Default for ProjectSettings {
    fn default() -> Self {
        Self {
            target_clips: 6,
            max_clip_seconds: MAX_CLIP_SECONDS,
            captions_enabled: true,
            vertical: true,
            llm_enabled: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub schema_version: u32,
    pub id: Uuid,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub niche: Niche,
    pub platform: Platform,
    pub source: Option<Source>,
    pub media: Option<MediaInfo>,
    pub transcript: Transcript,
    pub clips: Vec<Clip>,
    pub settings: ProjectSettings,
}

impl Project {
    pub fn total_clip_seconds(&self) -> f64 {
        self.clips.iter().map(Clip::duration).sum()
    }
}
