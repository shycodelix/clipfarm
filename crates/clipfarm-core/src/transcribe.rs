use anyhow::{anyhow, Context, Result};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::model::{Transcript, Word};
use crate::tools::Tools;

/// Transcribe with whisper.cpp's CLI. Word-level timestamps come from running
/// whisper with one word per segment (`--max-len 1 --split-on-word`).
///
/// Returns `Ok(None)` when whisper.cpp or its model is not configured. Callers
/// treat that as "VAD-only mode", never as an error: clipfarm still works.
pub fn transcribe(
    tools: &Tools,
    media: &Path,
    work_dir: &Path,
    progress: &mut dyn FnMut(f32, &str),
) -> Result<Option<Transcript>> {
    let (Some(whisper), Some(model)) = (tools.whisper_cli.as_ref(), tools.whisper_model.as_ref())
    else {
        return Ok(None);
    };
    std::fs::create_dir_all(work_dir)?;
    let wav = work_dir.join("whisper-input.wav");
    progress(0.02, "extracting audio");
    extract_audio(tools, media, &wav)?;

    progress(0.12, "transcribing with whisper.cpp");
    let prefix = work_dir.join("whisper-output");
    let output = Command::new(whisper)
        .arg("--model")
        .arg(model)
        .arg("--file")
        .arg(&wav)
        .args([
            "--output-json",
            "--max-len",
            "1",
            "--split-on-word",
            "--output-file",
        ])
        .arg(&prefix)
        .args(["--print-progress"])
        .output()
        .with_context(|| format!("Could not run {}", whisper.display()))?;

    if !output.status.success() {
        return Err(anyhow!(
            "whisper.cpp failed: {}",
            String::from_utf8_lossy(&output.stderr)
                .lines()
                .last()
                .unwrap_or("unknown error")
        ));
    }
    let json_path = find_json(&prefix).ok_or_else(|| {
        anyhow!(
            "whisper.cpp finished but produced no JSON at {}.json — check its --output-json support.",
            prefix.display()
        )
    })?;
    let text = std::fs::read_to_string(&json_path)?;
    let json: Value = serde_json::from_str(&text)
        .with_context(|| format!("{} was not valid whisper JSON", json_path.display()))?;
    progress(0.96, "wrangling words");
    let transcript = parse_whisper_json(&json)?;
    Ok(Some(transcript))
}

fn extract_audio(tools: &Tools, media: &Path, out: &Path) -> Result<()> {
    let status = Command::new(&tools.ffmpeg)
        .args(["-hide_banner", "-loglevel", "error", "-y", "-i"])
        .arg(media)
        .args(["-vn", "-ac", "1", "-ar", "16000", "-c:a", "pcm_s16le"])
        .arg(out)
        .status()?;
    if !status.success() {
        return Err(anyhow!("FFmpeg could not extract audio for transcription."));
    }
    Ok(())
}

fn find_json(prefix: &Path) -> Option<PathBuf> {
    for suffix in [".json", ".json.json"] {
        let candidate = PathBuf::from(format!("{}{suffix}", prefix.display()));
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// Parse whisper.cpp's `transcription` array. Timestamps are milliseconds with
/// `offsets.from` / `offsets.to`; both are tolerated as absent.
pub fn parse_whisper_json(json: &Value) -> Result<Transcript> {
    let items = json
        .get("transcription")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("whisper JSON has no `transcription` array"))?;
    let mut words = Vec::new();
    let mut max_end = 0.0f64;
    for item in items {
        let text = item
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim()
            .to_string();
        if text.is_empty() || text == "[BLANK_AUDIO]" {
            continue;
        }
        let from = item
            .pointer("/offsets/from")
            .and_then(Value::as_f64)
            .unwrap_or(0.0)
            / 1000.0;
        let to = item
            .pointer("/offsets/to")
            .and_then(Value::as_f64)
            .unwrap_or(from * 1000.0)
            / 1000.0;
        if to <= from {
            continue;
        }
        max_end = max_end.max(to);
        words.push(Word {
            text,
            start: from,
            end: to,
            probability: 0.9,
        });
    }
    let language = json
        .pointer("/result/language")
        .and_then(Value::as_str)
        .unwrap_or("en")
        .to_string();
    Ok(Transcript {
        language,
        words,
        duration: max_end,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_word_timestamps() {
        let json: Value = serde_json::from_str(
            r#"{
              "result": {"language": "en"},
              "transcription": [
                {"offsets": {"from": 0, "to": 420}, "text": "The"},
                {"offsets": {"from": 420, "to": 900}, "text": "hook"},
                {"offsets": {"from": 900, "to": 1200}, "text": " lands."}
              ]
            }"#,
        )
        .unwrap();
        let transcript = parse_whisper_json(&json).expect("parse");
        assert_eq!(transcript.words.len(), 3);
        assert!((transcript.words[1].start - 0.42).abs() < 1e-6);
        assert_eq!(transcript.language, "en");
        assert!((transcript.duration - 1.2).abs() < 1e-6);
    }

    #[test]
    fn missing_transcription_is_an_error() {
        let json = serde_json::json!({"result": {}});
        assert!(parse_whisper_json(&json).is_err());
    }
}
