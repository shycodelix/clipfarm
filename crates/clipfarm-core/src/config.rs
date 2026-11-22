use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[cfg(target_os = "linux")]
const IS_LINUX_TARGET: bool = true;
#[cfg(not(target_os = "linux"))]
const IS_LINUX_TARGET: bool = false;

#[cfg(target_os = "windows")]
const IS_WINDOWS_TARGET: bool = true;
#[cfg(not(target_os = "windows"))]
const IS_WINDOWS_TARGET: bool = false;

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

    /// If true, the app may record coarse, local-only usage events described in the
    /// observation design doc. Off by default. No network, no content, no identity.
    #[serde(default)]
    pub observe_enabled: bool,
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
            observe_enabled: false,
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

    /// Resolve FFmpeg, preferring the app's own bundled copy when present,
    /// then an explicit config path, then PATH. The bundled layout differs per
    /// platform because cargo-packager places `externalBinaries` in different
    /// directories:
    ///   - macOS:   `<app>.app/Contents/MacOS/ffmpeg-<triple>`
    ///   - Linux:   `usr/bin/ffmpeg-<triple>`  (in the .deb; after install it is on PATH as `ffmpeg`)
    ///   - Windows: `$INSTDIR/ffmpeg-<triple>.exe`  (next to the app exe)
    ///
    /// On Linux the packager actually renames externals to their plain name when
    /// placing them into `usr/bin`, so the installed binary is `ffmpeg` on PATH.
    /// The resolver therefore checks the app-IPC/layout guess only as a fallback
    /// hint; the dominant behavior on every platform is "bundled binary found? use
    /// it; else PATH".
    pub fn resolve_ffmpeg(&self) -> Option<PathBuf> {
        // 1. Explicit user override.
        if let Some(path) = self.ffmpeg.clone().filter(|path| path.is_file()) {
            return Some(path);
        }

        // 2. Bundled copy shipped inside the app package.
        bundled_ffmpeg().filter(|path| path.is_file()).or_else(|| {
            // 3. The usual "ffprobe next to ffmpeg" portable layout, in case the
            //    user dropped a portable build next to our guess.
            self.ffmpeg
                .as_ref()
                .and_then(|ffmpeg| ffmpeg.parent())
                .map(|dir| {
                    dir.join(if cfg!(windows) {
                        "ffmpeg.exe"
                    } else {
                        "ffmpeg"
                    })
                })
                .filter(|path| path.is_file())
        })
    }

    /// Resolve ffprobe. Prefer an explicit override, then the bundled copy (same
    /// per-platform layout as ffmpeg), then PATH, then a sibling next to the
    /// resolved FFmpeg (the common portable-build layout).
    pub fn resolve_ffprobe(&self) -> Option<PathBuf> {
        if let Some(path) = self.ffprobe.clone().filter(|path| path.is_file()) {
            return Some(path);
        }

                    bundled_ffprobe().filter(|path| path.is_file()).or_else(|| {
            // Sibling ffprobe next to the ffmpeg we actually resolved is the most
            // reliable portable-build guess.
            let ffmpeg = self.resolve_ffmpeg()?;
            let dir = ffmpeg.parent()?.to_path_buf();
            let probe = dir.join(if cfg!(windows) {
                "ffprobe.exe"
            } else {
                "ffprobe"
            });
            if probe.is_file() { Some(probe) } else { None }
        })
    }

    pub fn resolve_ytdlp(&self) -> Option<PathBuf> {
        if let Some(path) = self.yt_dlp.clone().filter(|path| path.is_file()) {
            return Some(path);
        }

        // yt-dlp standalone executables use a platform-distinct basename, so the
        // bundled name is the real executable name on each platform:
        //   - Windows: yt-dlp.exe
        //   - macOS:   yt-dlp_macos
        //   - Linux:   yt-dlp_linux
        bundled_yt_dlp().filter(|path| path.is_file()).or_else(|| {
            which("yt-dlp").or_else(|| which("yt-dlp.exe"))
        })
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

/// Absolute path to ffmpeg when clipfarm is running from a packaged installer
/// that bundled FFmpeg as an external binary. Returns `None` when the process
/// image is not in a recognized app bundle / install layout (for example a
/// `cargo run` dev build or a source checkout). In that case the resolver falls
/// back to PATH / explicit config.
fn bundled_ffmpeg() -> Option<PathBuf> {
    // macOS: the app binary lives in `<app>.app/Contents/MacOS/`. Externals ship
    // in the same directory with the target-triple suffix.
    if let Some(exec) = std::env::current_exe().ok().as_ref() {
        if let Some(app) = app_bundle_dir(exec) {
            let macos_dir = app.join("Contents/MacOS");
            // The packager names externals `<name>-<triple>` (plus .exe on
            // Windows). We try a few likely names rather than depending on one
            // exact triple string at runtime.
            for name in bundled_names("ffmpeg") {
                let candidate = macos_dir.join(&name);
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }

        // Windows NSIS: externals are placed next to the app exe in the install
        // root ($INSTDIR), with the `<name>-<triple>.exe` naming.
        // Linux .deb: cargo-packager places externals into usr/bin/ alongside the
        // app binary, with the same `<name>-<triple>` naming, so probe the exe
        // parent dir on Linux as well.
        if IS_WINDOWS_TARGET || IS_LINUX_TARGET {
            for name in bundled_names(if IS_WINDOWS_TARGET { "ffmpeg.exe" } else { "ffmpeg" }) {
                let candidate = exec.parent()?.join(&name);
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
    }
    None
}

/// Bundled ffprobe path, using the same per-platform layout as ffmpeg.
fn bundled_ffprobe() -> Option<PathBuf> {
    if let Some(exec) = std::env::current_exe().ok().as_ref() {
        if let Some(app) = app_bundle_dir(exec) {
            let macos_dir = app.join("Contents/MacOS");
            for name in bundled_names("ffprobe") {
                let candidate = macos_dir.join(&name);
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
        if IS_WINDOWS_TARGET || IS_LINUX_TARGET {
            for name in bundled_names(if IS_WINDOWS_TARGET { "ffprobe.exe" } else { "ffprobe" }) {
                let candidate = exec.parent()?.join(&name);
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
    }
    None
}

/// Bundled yt-dlp path. cargo-packager names external binaries
/// `<name>-<target-triple>` (plus `.exe` on Windows), so the bundled copy is
/// checked under those triple-suffixed names first, then under the standalone
/// release names (yt-dlp.exe / yt-dlp_macos / yt-dlp_linux) as a fallback for
/// manual installs.
///
/// On Linux the packager places externals into `usr/bin/` next to the app binary,
/// so we probe the running exe's parent directory on Linux in the same way we do on
/// Windows. The earlier Linux-only fallback that scanned both the app-bundle dir and
/// the exe parent dir for every triple was both redundant (Linux is not an app bundle)
/// and incomplete (it never meaningfully covered the installed .deb layout).
fn bundled_yt_dlp() -> Option<PathBuf> {
    if let Some(exec) = std::env::current_exe().ok().as_ref() {
        // macOS: externals live inside the .app bundle's Contents/MacOS/.
        if let Some(app) = app_bundle_dir(exec) {
            let macos_dir = app.join("Contents/MacOS");
            for name in bundled_names("yt-dlp") {
                let candidate = macos_dir.join(&name);
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
            // Fallback: standalone release names (manual installs).
            for name in &["yt-dlp_macos", "yt-dlp.exe", "yt-dlp"] {
                let candidate = macos_dir.join(name);
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }

        // Windows NSIS + Linux .deb: externals live next to the app binary.
        if IS_WINDOWS_TARGET || IS_LINUX_TARGET {
            let dir = exec.parent()?;
            for name in bundled_names(if IS_WINDOWS_TARGET { "yt-dlp.exe" } else { "yt-dlp" }) {
                let candidate = dir.join(&name);
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
            // Fallback: standalone release names (manual installs, non-packaged
            // layouts).
            for name in &[
                if IS_WINDOWS_TARGET { "yt-dlp.exe" } else { "yt-dlp_linux" },
                "yt-dlp",
            ] {
                let candidate = dir.join(name);
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
    }
    None
}

/// Candidate names the packager might use for an external binary named `base`.
/// The packager names externals `<base>-<target-triple>` (plus `.exe` on
/// Windows). We probe a small set of likely triples instead of requiring one exact
/// string at runtime.
fn bundled_names(base: &str) -> Vec<String> {
    let is_win = IS_WINDOWS_TARGET;
    let suffixes: Vec<String> = if is_win {
        vec!["-x86_64-pc-windows-msvc.exe".into()]
    } else {
        vec![
            "-x86_64-unknown-linux-gnu".into(),
            "-x86_64-apple-darwin".into(),
            "-aarch64-apple-darwin".into(),
            "-aarch64-unknown-linux-gnu".into(),
            "-universal-apple-darwin".into(),
        ]
    };
    suffixes
        .into_iter()
        .map(|suffix| format!("{base}{suffix}"))
        .collect()
}

/// For a macOS app bundle, return the `<name>.app` directory given the running
/// executable path (which lives in `<name>.app/Contents/MacOS/`).
fn app_bundle_dir(exec: &std::path::Path) -> Option<std::path::PathBuf> {
    // Walk up to find a `Contents/MacOS` boundary; the parent of that parent is
    // the .app bundle.
    let mut dir = exec.parent()?.to_path_buf();
    while let Some(parent) = dir.parent() {
        if parent.file_name()? == "MacOS" {
            return Some(dir.parent()?.to_path_buf());
        }
        dir = parent.to_path_buf();
    }
    None
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

    #[test]
    fn bundled_names_include_expected_triples() {
        let names = bundled_names("ffmpeg");
        assert!(names.contains(&"ffmpeg-x86_64-unknown-linux-gnu".to_string()));
        assert!(names.contains(&"ffmpeg-x86_64-apple-darwin".to_string()));
    }
}
