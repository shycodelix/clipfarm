use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::project::data_dir;
use crate::util::{which, write_atomic};

/// Persistent, user-editable configuration. Every path is optional: unset means
/// "look on PATH". Nothing here ever needs a cloud account.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub ffmpeg: Option<PathBuf>,
    #[serde(default)]
    pub ffprobe: Option<PathBuf>,
    #[serde(default)]
    pub yt_dlp: Option<PathBuf>,
    #[serde(default)]
    pub whisper_cli: Option<PathBuf>,
    #[serde(default)]
    pub whisper_model: Option<PathBuf>,
    /// Local-only assistant. Any non-loopback endpoint is rejected at call time.
    #[serde(default = "default_llm_endpoint")]
    pub llm_endpoint: String,
    #[serde(default = "default_llm_model")]
    pub llm_model: String,
    #[serde(default)]
    pub llm_enabled: bool,
}

fn default_llm_endpoint() -> String {
    "http://127.0.0.1:11434".to_string()
}

fn default_llm_model() -> String {
    "llama3.2".to_string()
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            ffmpeg: None,
            ffprobe: None,
            yt_dlp: None,
            whisper_cli: None,
            whisper_model: None,
            llm_endpoint: default_llm_endpoint(),
            llm_model: default_llm_model(),
            llm_enabled: false,
        }
    }
}

impl AppConfig {
    pub fn path() -> PathBuf {
        data_dir().join("config.json")
    }

    /// Load config, apply environment overrides, and never fail: a corrupt config
    /// should not stop someone from editing video.
    pub fn load() -> Self {
        let mut config: AppConfig = std::fs::read_to_string(Self::path())
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();

        if let Some(value) = std::env::var_os("CLIPFARM_FFMPEG") {
            config.ffmpeg = Some(PathBuf::from(value));
        }
        if let Some(value) = std::env::var_os("CLIPFARM_FFPROBE") {
            config.ffprobe = Some(PathBuf::from(value));
        }
        if let Some(value) = std::env::var_os("CLIPFARM_YTDLP") {
            config.yt_dlp = Some(PathBuf::from(value));
        }
        if let Some(value) = std::env::var_os("CLIPFARM_WHISPER") {
            config.whisper_cli = Some(PathBuf::from(value));
        }
        if let Some(value) = std::env::var_os("CLIPFARM_WHISPER_MODEL") {
            config.whisper_model = Some(PathBuf::from(value));
        }
        if let Ok(value) = std::env::var("OLLAMA_HOST") {
            if !value.trim().is_empty() {
                config.llm_endpoint = if value.starts_with("http") {
                    value
                } else {
                    format!("http://{value}")
                };
            }
        }
        if let Ok(value) = std::env::var("CLIPFARM_LLM_MODEL") {
            config.llm_model = value;
        }
        config
    }

    pub fn save(&self) -> anyhow::Result<()> {
        write_atomic(&Self::path(), &serde_json::to_string_pretty(self)?)
    }

    pub fn resolve_ffmpeg(&self) -> Option<PathBuf> {
        self.ffmpeg
            .clone()
            .filter(|path| path.is_file())
            .or_else(|| which("ffmpeg"))
    }

    pub fn resolve_ffprobe(&self) -> Option<PathBuf> {
        self.ffprobe
            .clone()
            .filter(|path| path.is_file())
            .or_else(|| which("ffprobe"))
            .or_else(|| {
                // A sibling ffprobe next to the configured ffmpeg is the common
                // "portable build" layout.
                self.ffmpeg
                    .as_ref()
                    .and_then(|ffmpeg| ffmpeg.parent())
                    .map(|dir| {
                        dir.join(if cfg!(windows) {
                            "ffprobe.exe"
                        } else {
                            "ffprobe"
                        })
                    })
                    .filter(|path| path.is_file())
            })
    }

    pub fn resolve_ytdlp(&self) -> Option<PathBuf> {
        self.yt_dlp
            .clone()
            .filter(|path| path.is_file())
            .or_else(|| which("yt-dlp"))
            .or_else(|| which("yt-dlp.exe"))
    }

    pub fn resolve_whisper(&self) -> Option<PathBuf> {
        self.whisper_cli
            .clone()
            .filter(|path| path.is_file())
            .or_else(|| which("whisper-cli"))
            .or_else(|| which("whisper.cpp"))
            .or_else(|| which("main"))
    }

    /// Only loopback endpoints are allowed. This is the zero-cost guarantee: no
    /// inference request can be pointed at a paid API by accident.
    pub fn llm_allowed(&self) -> bool {
        self.llm_enabled && is_loopback(&self.llm_endpoint)
    }
}

pub fn is_loopback(endpoint: &str) -> bool {
    let endpoint = endpoint.trim().to_ascii_lowercase();
    let Some(rest) = endpoint
        .strip_prefix("http://")
        .or_else(|| endpoint.strip_prefix("https://"))
    else {
        return false;
    };
    let authority = rest.split('/').next().unwrap_or("");
    let host = if let Some(stripped) = authority.strip_prefix('[') {
        match stripped.split(']').next() {
            Some(inner) => format!("[{inner}]"),
            None => return false,
        }
    } else {
        authority
            .rsplit_once(':')
            .map(|(host, _port)| host)
            .unwrap_or(authority)
            .to_string()
    };
    matches!(
        host.as_str(),
        "127.0.0.1" | "localhost" | "[::1]" | "0.0.0.0"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loopback_guard_rejects_the_cloud() {
        assert!(is_loopback("http://127.0.0.1:11434"));
        assert!(is_loopback("http://localhost:1234"));
        assert!(!is_loopback("https://api.openai.com"));
        assert!(!is_loopback("http://127.0.0.1.evil.example"));
        assert!(!is_loopback("https://localhost.evil.example"));
        assert!(!is_loopback("ftp://127.0.0.1"));
    }

    #[test]
    fn llm_disabled_by_default() {
        let config = AppConfig::default();
        assert!(!config.llm_allowed());
    }
}
