use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{anyhow, Context, Result};
use serde::Serialize;

use crate::config::AppConfig;
use crate::util::{run_with_timeout, which};

/// Discovered external tools. clipfarm never bundles FFmpeg/yt-dlp itself; it
/// finds them, checks them, and tells you exactly what is missing.
#[derive(Debug, Clone)]
pub struct Tools {
    pub ffmpeg: PathBuf,
    pub ffprobe: PathBuf,
    pub yt_dlp: Option<PathBuf>,
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
    pub hint: &'static str,
}

impl Tools {
    pub fn discover(config: &AppConfig) -> Result<Self> {
        let ffmpeg = config
            .resolve_ffmpeg()
            .ok_or_else(|| anyhow!("FFmpeg was not found. Install it or set CLIPFARM_FFMPEG."))?;
        let ffprobe = config.resolve_ffprobe().ok_or_else(|| {
            anyhow!("ffprobe was not found. It ships with FFmpeg; set CLIPFARM_FFPROBE if it is elsewhere.")
        })?;
        Ok(Self {
            ffmpeg,
            ffprobe,
            yt_dlp: config.resolve_ytdlp(),
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
        vec![
            ToolStatus {
                name: "FFmpeg",
                path: Some(self.ffmpeg.display().to_string()),
                version: ffmpeg_version,
                required: true,
                hint: "brew install ffmpeg · winget install Gyan.FFmpeg · apt install ffmpeg",
            },
            ToolStatus {
                name: "ffprobe",
                path: Some(self.ffprobe.display().to_string()),
                version: ffprobe_version,
                required: true,
                hint: "Ships with FFmpeg.",
            },
            ToolStatus {
                name: "yt-dlp",
                path: self.yt_dlp.as_ref().map(|path| path.display().to_string()),
                version: self
                    .yt_dlp
                    .as_ref()
                    .and_then(|path| version_of(path, &["--version"])),
                required: false,
                hint: "pipx install yt-dlp · brew install yt-dlp",
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
                hint: "Build whisper.cpp and set CLIPFARM_WHISPER + CLIPFARM_WHISPER_MODEL. Without it, clipfarm falls back to VAD-only planning.",
            },
            ToolStatus {
                name: "whisper model",
                path: self
                    .whisper_model
                    .as_ref()
                    .map(|path| path.display().to_string()),
                version: None,
                required: false,
                hint: "Download ggml-base.en.bin or ggml-small.en.bin from the whisper.cpp repo.",
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
                hint: "Optional. `ollama pull llama3.2`, then enable it in Tuning. Clipfarm never calls a paid API.",
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
