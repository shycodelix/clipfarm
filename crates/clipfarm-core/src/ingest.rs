use anyhow::{anyhow, Context, Result};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::media;
use crate::model::{MediaInfo, Source, SourceKind};
use crate::tools::Tools;
use crate::util::{ensure_dir, slugify};

#[derive(Debug, Clone)]
pub struct IngestResult {
    pub source: Source,
    pub media: MediaInfo,
}

pub fn is_url(value: &str) -> bool {
    let value = value.trim().to_ascii_lowercase();
    (value.starts_with("http://") || value.starts_with("https://"))
        && !value.contains(char::is_whitespace)
}

/// A local file is referenced in place: clipfarm never copies gigabytes unless
/// the user explicitly asks for it.
pub fn ingest_local(tools: &Tools, path: &Path) -> Result<IngestResult> {
    let canonical = path
        .canonicalize()
        .with_context(|| format!("Could not open {}", path.display()))?;
    let media = media::probe(tools, &canonical)?;
    let display_name = canonical
        .file_stem()
        .map(|stem| stem.to_string_lossy().to_string())
        .unwrap_or_else(|| "Untitled source".into());
    Ok(IngestResult {
        source: Source {
            kind: SourceKind::Local,
            path: canonical,
            url: None,
            display_name,
        },
        media,
    })
}

/// Download a VOD with yt-dlp. This is the only network access clipfarm performs
/// on ingest, and it is always a user-provided URL.
pub fn ingest_url(
    tools: &Tools,
    url: &str,
    destination: &Path,
    progress: &mut dyn FnMut(f32, &str),
) -> Result<IngestResult> {
    if !is_url(url) {
        return Err(anyhow!("`{url}` does not look like an http(s) URL."));
    }
    let yt_dlp = tools.yt_dlp.as_ref().map(|(path, _)| path).ok_or_else(|| {
        anyhow!(
            "yt-dlp is not installed. Install it (pipx install yt-dlp) or drop a local file instead."
        )
    })?;
    ensure_dir(destination)?;
    let title_file = destination.join("title.txt");
    let _ = std::fs::remove_file(&title_file);

    progress(0.01, "asking yt-dlp nicely");
    let mut child = Command::new(yt_dlp)
        .args([
            "--no-playlist",
            "--newline",
            "--no-warnings",
            "-f",
            "bv*[height<=1080]+ba/b[height<=1080]/b",
            "--merge-output-format",
            "mp4",
            "--print-to-file",
            "after_move:%(title)s",
        ])
        .arg(&title_file)
        .args(["-o"])
        .arg(destination.join("source.%(ext)s"))
        .arg(url)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("Could not run {}", yt_dlp.display()))?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow!("No yt-dlp output"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| anyhow!("No yt-dlp errors"))?;
    let stderr_handle = std::thread::spawn(move || {
        let mut collected = String::new();
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            collected.push_str(&line);
            collected.push('\n');
        }
        collected
    });

    let percent = regex::Regex::new(r"(\d{1,3}(?:\.\d+)?)%").expect("static regex");
    for line in BufReader::new(stdout).lines().map_while(Result::ok) {
        if let Some(capture) = percent.captures(&line) {
            if let Some(value) = capture
                .get(1)
                .and_then(|value| value.as_str().parse::<f32>().ok())
            {
                let fraction = (value / 100.0).clamp(0.02, 0.92);
                let speed = line
                    .split("at ")
                    .nth(1)
                    .map(|tail| tail.split_whitespace().next().unwrap_or(""))
                    .unwrap_or("");
                let message = if speed.is_empty() {
                    format!("downloading {:.0}%", value)
                } else {
                    format!("downloading {:.0}% at {}", value, speed)
                };
                progress(fraction, &message);
            }
        }
    }
    let status = child.wait()?;
    let errors = stderr_handle.join().unwrap_or_default();
    if !status.success() {
        let tail = errors
            .lines()
            .rev()
            .take(5)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join("\n");
        return Err(anyhow!("yt-dlp could not fetch that URL:\n{tail}"));
    }

    progress(0.94, "probing the download");
    let downloaded = find_download(destination)
        .ok_or_else(|| anyhow!("yt-dlp finished but no media file appeared."))?;
    let media = media::probe(tools, &downloaded)?;
    let display_name = std::fs::read_to_string(&title_file)
        .ok()
        .map(|title| title.trim().to_string())
        .filter(|title| !title.is_empty())
        .unwrap_or_else(|| {
            downloaded
                .file_stem()
                .map(|stem| stem.to_string_lossy().to_string())
                .unwrap_or_else(|| slugify(url, 40))
        });
    progress(1.0, "downloaded");
    Ok(IngestResult {
        source: Source {
            kind: SourceKind::Remote,
            path: downloaded,
            url: Some(url.to_string()),
            display_name,
        },
        media,
    })
}

fn find_download(dir: &Path) -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_stem()
                .map(|stem| stem.to_string_lossy().starts_with("source"))
                .unwrap_or(false)
                && matches!(
                    path.extension().and_then(|ext| ext.to_str()),
                    Some("mp4" | "mkv" | "webm" | "mov" | "m4v")
                )
        })
        .collect();
    candidates.sort_by_key(|path| std::fs::metadata(path).map(|meta| meta.len()).unwrap_or(0));
    candidates.pop()
}

/// Import either a local path or a URL, into a project-specific folder.
pub fn ingest_any(
    tools: &Tools,
    input: &str,
    destination: &Path,
    progress: &mut dyn FnMut(f32, &str),
) -> Result<IngestResult> {
    if is_url(input) {
        ingest_url(tools, input, destination, progress)
    } else {
        progress(0.1, "opening the file");
        let result = ingest_local(tools, Path::new(input))?;
        progress(1.0, "ready");
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_are_detected_without_being_clever() {
        assert!(is_url("https://youtube.com/watch?v=abc"));
        assert!(is_url("http://twitch.tv/videos/1"));
        assert!(!is_url("/home/me/vod.mp4"));
        assert!(!is_url("not a url"));
        assert!(!is_url("ftp://example.com/file"));
    }
}
