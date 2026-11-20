use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{anyhow, Context, Result};
use serde::Serialize;

use crate::config::AppConfig;
use crate::util::{run_with_timeout, which};

/// Where an external tool was found. Used by the UI to show whether clipfarm is
/// using a bundled copy or a user/PATH install.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolSource {
    Bundled,
    Path,
    Config,
}

impl ToolSource {
    pub fn label(self) -> &'static str {
        match self {
            ToolSource::Bundled => "bundled",
            ToolSource::Path => "PATH",
            ToolSource::Config => "config",
        }
    }
}


/// Discovered external tools. FFmpeg and ffprobe are required; yt-dlp, whisper.cpp
/// and the local LLM endpoint are optional.
///
/// The app prefers its own bundled copy when present (shipped inside the installer
/// on Linux, Windows and macOS), then an explicit config path, then PATH.
#[derive(Debug, Clone)]
pub struct Tools {
    pub ffmpeg: PathBuf,
    pub ffmpeg_source: ToolSource,
    pub ffprobe: PathBuf,
    pub ffprobe_source: ToolSource,
    pub yt_dlp: Option<(PathBuf, ToolSource)>,
    pub whisper_cli: Option<PathBuf>,
    pub whisper_model: Option<PathBuf>,
    pub llm_endpoint: String,
    pub llm_model: String,
    pub llm_enabled: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolStatus {
    pub name: &'static str,
    pub path: Option<String>,
    pub version: Option<String>,
    pub required: bool,
    pub hint: String,
    /// Where the tool was found: "bundled", "PATH", "config", or empty when
    /// unknown. Shown in the Tuning screen next to the tool name.
    #[serde(skip)]
    pub source: Option<&'static str>,
}

impl Tools {
    pub fn discover(config: &AppConfig) -> Result<Self> {
        let ffmpeg = config
            .resolve_ffmpeg()
            .ok_or_else(|| anyhow!("FFmpeg was not found. Install it or set CLIPFARM_FFMPEG."))?;
        let ffprobe = config.resolve_ffprobe().ok_or_else(|| {
            anyhow!("ffprobe was not found. It ships with FFmpeg; set CLIPFARM_FFPROBE if it is elsewhere.")
        })?;

        let ffmpeg_source = tool_source(&config.ffmpeg, &ffmpeg, true);
        let ffprobe_source = tool_source(&config.ffprobe, &ffprobe, true);

        let yt_dlp = match config.resolve_ytdlp() {
            Some(path) => {
                let src = tool_source(&config.yt_dlp, &path, false);
                Some((path, src))
            }
            None => None,
        };

        Ok(Self {
            ffmpeg,
            ffmpeg_source,
            ffprobe,
            ffprobe_source,
            yt_dlp,
            whisper_cli: config.resolve_whisper(),
            whisper_model: config.whisper_model.clone().filter(|path| path.is_file()),
            llm_endpoint: config.llm_endpoint.clone(),
            llm_model: config.llm_model.clone(),
            llm_enabled: config.llm_allowed(),
        })
    }

    /// Status for `clipfarm doctor` and the app's tuning screen.
    pub fn status(&self) -> Vec<ToolStatus> {
        let ffmpeg_version = version_of(&self.ffmpeg, &["-version"]);
        let ffprobe_version = version_of(&self.ffprobe, &["-version"]);
        let ffmpeg_tag = tool_tag(&self.ffmpeg_source);
        let ffprobe_tag = tool_tag(&self.ffprobe_source);
        let yt_dlp_tag = self
            .yt_dlp
            .as_ref()
            .map(|(_, src)| format!(" ({})", tool_tag(src)));
        vec![
            ToolStatus {
                name: "FFmpeg",
                path: Some(tool_display(&self.ffmpeg, ffmpeg_tag)),
                version: ffmpeg_version,
                required: true,
                hint: "Bundled in the installer. If missing, install FFmpeg (brew/winget/apt) or set CLIPFARM_FFMPEG.".to_string(),
                source: Some(ffmpeg_tag),
            },
            ToolStatus {
                name: "ffprobe",
                path: Some(tool_display(&self.ffprobe, ffprobe_tag)),
                version: ffprobe_version,
                required: true,
                hint: "Bundled with FFmpeg. If missing, reinstall FFmpeg from the same source or set CLIPFARM_FFPROBE.".to_string(),
                source: Some(ffprobe_tag),
            },
            ToolStatus {
                name: "yt-dlp",
                path: self.yt_dlp.as_ref().map(|(path, _)| path.display().to_string()),
                version: self
                    .yt_dlp
                    .as_ref()
                    .and_then(|(path, _)| version_of(path, &["--version"])),
                required: false,
                hint: format!(
                    "Bundled in the installer{}. Update it inside the app, or install from https://github.com/yt-dlp/yt-dlp/releases/latest.",
                    yt_dlp_tag.as_deref().unwrap_or(""),
                ),
                source: self.yt_dlp.as_ref().map(|(_, src)| tool_tag(src)),
            },
            ToolStatus {
                name: "whisper.cpp",
                path: self
                    .whisper_cli
                    .as_ref()
                    .map(|path| path.display().to_string()),
                version: self
                    .whisper_cli
                    .as_ref()
                    .and_then(|path| version_of(path, &["--help"]).map(|_| "found".to_string())),
                required: false,
                hint: "Optional. Build whisper.cpp (or set CLIPFARM_WHISPER) and download a ggml-*.bin model. Without it, clipfarm runs VAD-only.".to_string(),
                source: None,
            },
            ToolStatus {
                name: "whisper model",
                path: self
                    .whisper_model
                    .as_ref()
                    .map(|path| path.display().to_string()),
                version: None,
                required: false,
                hint: "Download ggml-tiny.en.bin or ggml-base.en.bin from the whisper.cpp HuggingFace repo and drop it into the clipfarm data dir, or let the app download it.".to_string(),
                source: None,
            },
            ToolStatus {
                name: "local LLM",
                path: if self.llm_enabled {
                    Some(self.llm_endpoint.clone())
                } else {
                    None
                },
                version: if self.llm_enabled {
                    Some(format!("{} (local)", self.llm_model))
                } else {
                    None
                },
                required: false,
                hint: "Optional. `ollama pull llama3.2`, then enable it in Tuning. Clipfarm only talks to loopback endpoints.".to_string(),
                source: None,
            },
        ]
    }

    pub fn supports_subtitles(&self) -> bool {
        let output = Command::new(&self.ffmpeg)
            .args(["-hide_banner", "-filters"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output();
        match output {
            Ok(output) => String::from_utf8_lossy(&output.stdout).contains(" subtitles "),
            Err(_) => false,
        }
    }

    /// Pick the best available H.264 encoder. Returns a friendly, human-readable
    /// name plus the encoder ID.
    ///
    /// Some FFmpeg builds advertise hardware encoders they cannot actually open
    /// (common in CI/container runners). We probe the chosen encoder with a
    /// trivial encode and fall back to `libx264` when it is not usable.
    pub fn hardware_encoder(&self) -> Option<(&'static str, &'static str)> {
        let output = Command::new(&self.ffmpeg)
            .args(["-hide_banner", "-encoders"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output()
            .ok()?;
        let list = String::from_utf8_lossy(&output.stdout);
        let candidates: [(&str, &str); 4] = [
            ("h264_videotoolbox", "VideoToolbox (macOS)"),
            ("h264_nvenc", "NVENC (NVIDIA)"),
            ("h264_qsv", "Quick Sync (Intel)"),
            ("h264_amf", "AMF (AMD)"),
        ];
        candidates
            .into_iter()
            .find(|(encoder, _)| list.contains(encoder))
            .and_then(|(encoder, label)| {
                if encoder_is_usable(&self.ffmpeg, encoder) {
                    Some((encoder, label))
                } else {
                    None
                }
            })
    }

    pub fn ffmpeg_version(&self) -> Option<String> {
        version_of(&self.ffmpeg, &["-version"])
    }

    pub fn whisper_ready(&self) -> bool {
        self.whisper_cli.is_some() && self.whisper_model.is_some()
    }

    /// Bundled copy of FFmpeg is present and usable. This is the signal the app
    /// uses to decide whether to show the "bundled" marker in Tuning and whether
    /// the first-run health check is satisfied for video.
    pub fn ffmpeg_is_bundled(&self) -> bool {
        matches!(self.ffmpeg_source, ToolSource::Bundled)
    }

    /// Bundled copy of yt-dlp is present. When `None`, the app can prompt the
    /// user to let it download the standalone yt-dlp for their platform.
    pub fn yt_dlp_is_bundled(&self) -> bool {
        matches!(self.yt_dlp.as_ref().map(|(_, src)| src), Some(ToolSource::Bundled))
    }

    /// Run a quick, quiet command, returning trimmed stdout. Handy for probes.
    pub fn run(&self, program: &Path, args: &[&str]) -> Result<String> {
        let mut command = Command::new(program);
        command.args(args);
        let output = run_with_timeout(&mut command, 20_000)?;
        if !output.status.success() {
            return Err(anyhow!(
                "{} failed: {}",
                program.display(),
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }
}

fn encoder_is_usable(ffmpeg: &Path, encoder: &str) -> bool {
    // One frame, one pixel, one second, explicit parameters. Failures here (not
    // just missing encoder entries) mean this machine cannot use that encoder.
    let output = Command::new(ffmpeg)
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=16x16:rate=1:duration=1",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=1000:sample_rate=8000:duration=1",
            "-c:v",
            encoder,
            "-preset",
            "fast",
            "-pix_fmt",
            "yuv420p",
            "-c:a",
            "aac",
            "-shortest",
            "-t",
            "1",
            "pipe:1",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .output();
    match output {
        Ok(out) => out.status.success(),
        Err(_) => false,
    }
}

fn version_of(program: &Path, args: &[&str]) -> Option<String> {
    let mut command = Command::new(program);
    command.args(args);
    let output = run_with_timeout(&mut command, 10_000).ok()?;
    let text = if output.stdout.is_empty() {
        String::from_utf8_lossy(&output.stderr).to_string()
    } else {
        String::from_utf8_lossy(&output.stdout).to_string()
    };
    text.lines()
        .next()
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty())
}

/// Resolve a tool by name and give a helpful error when it is missing.
pub fn require_tool(program: &str) -> Result<PathBuf> {
    which(program).ok_or_else(|| {
        anyhow!(
            "`{program}` is not installed or not on PATH. Run `clipfarm doctor` for install hints."
        )
    })
}

/// Capture-friendly wrapper for a binary's family (used in tests and diagnostics).
pub fn binary_family(path: &Path) -> String {
    path.file_stem()
        .map(|stem| stem.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".into())
        .to_ascii_lowercase()
}

/// Convenience for building a command with a working directory.
pub fn command_in(dir: &Path, program: &Path) -> Command {
    let mut command = Command::new(program);
    command.current_dir(dir);
    command
}

#[allow(dead_code)]
fn _assert_context_is_used() -> Result<()> {
    std::fs::metadata(".").context("metadata")?;
    Ok(())
}

/// Best-effort check: is `path` inside the app bundle / install root that cargo-
/// packager uses for externals? Used to tag a tool as "bundled" in the UI.
fn bundled_path(path: &Path) -> Option<PathBuf> {
    let exec = std::env::current_exe().ok()?;
    if let Some(app) = app_bundle_dir(&exec) {
        let macos_dir = app.join("Contents/MacOS");
        if path.starts_with(&macos_dir) {
            return Some(macos_dir);
        }
    }
    if cfg!(windows) {
        if let Some(parent) = exec.parent() {
            if path.starts_with(&parent) {
                return Some(parent.to_path_buf());
            }
        }
    }
    None
}

fn app_bundle_dir(exec: &Path) -> Option<PathBuf> {
    let mut dir = exec.parent()?.to_path_buf();
    while let Some(parent) = dir.parent() {
        if parent.file_name()?.to_str() == Some("MacOS") {
            return Some(dir.parent()?.to_path_buf());
        }
        dir = parent.to_path_buf();
    }
    None
}

fn tool_tag(src: &ToolSource) -> &'static str {
    match src {
        ToolSource::Bundled => "bundled",
        ToolSource::Path => "PATH",
        ToolSource::Config => "config",
    }
}

fn tool_display(path: &Path, tag: &str) -> String {
    if tag.is_empty() {
        return path.display().to_string();
    }
    format!("{} ({})", path.display(), tag)
}

fn tool_source(configured: &Option<PathBuf>, resolved: &Path, _required: bool) -> ToolSource {
    if configured.as_ref().map(|p| p == resolved).unwrap_or(false) {
        ToolSource::Config
    } else if bundled_path(resolved).is_some() {
        ToolSource::Bundled
    } else {
        ToolSource::Path
    }
}
