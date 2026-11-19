use anyhow::{anyhow, Context, Result};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::model::MediaInfo;
use crate::tools::Tools;
use crate::util::parse_progress_seconds;

/// Probe a media file with ffprobe and normalize the parts clipfarm cares about.
pub fn probe(tools: &Tools, input: &Path) -> Result<MediaInfo> {
    if !input.is_file() {
        return Err(anyhow!("Media file not found: {}", input.display()));
    }
    let output = Command::new(&tools.ffprobe)
        .args([
            "-v",
            "error",
            "-print_format",
            "json",
            "-show_format",
            "-show_streams",
        ])
        .arg(input)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .with_context(|| format!("Could not run {}", tools.ffprobe.display()))?;
    if !output.status.success() {
        return Err(anyhow!(
            "ffprobe could not read {}: {}",
            input.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let json: Value = serde_json::from_slice(&output.stdout)?;
    parse_probe(&json, input)
}

fn parse_probe(json: &Value, input: &Path) -> Result<MediaInfo> {
    let streams = json
        .get("streams")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("{} has no media streams", input.display()))?;

    let video = streams
        .iter()
        .find(|stream| stream.get("codec_type").and_then(Value::as_str) == Some("video"));
    let audio = streams
        .iter()
        .find(|stream| stream.get("codec_type").and_then(Value::as_str) == Some("audio"));

    let video = video.ok_or_else(|| {
        anyhow!(
            "{} has no video stream — clipfarm needs moving pictures, not just vibes.",
            input.display()
        )
    })?;

    let fps = video
        .get("avg_frame_rate")
        .and_then(Value::as_str)
        .and_then(parse_rate)
        .or_else(|| {
            video
                .get("r_frame_rate")
                .and_then(Value::as_str)
                .and_then(parse_rate)
        })
        .unwrap_or(30.0);

    let duration = json
        .get("format")
        .and_then(|format| format.get("duration"))
        .and_then(Value::as_str)
        .and_then(|value| value.parse::<f64>().ok())
        .or_else(|| {
            video
                .get("duration")
                .and_then(Value::as_str)
                .and_then(|value| value.parse::<f64>().ok())
        })
        .unwrap_or(0.0);

    let size_bytes = json
        .get("format")
        .and_then(|format| format.get("size"))
        .and_then(Value::as_str)
        .and_then(|value| value.parse::<u64>().ok())
        .or_else(|| std::fs::metadata(input).ok().map(|meta| meta.len()))
        .unwrap_or(0);

    Ok(MediaInfo {
        duration,
        width: video.get("width").and_then(Value::as_u64).unwrap_or(0) as u32,
        height: video.get("height").and_then(Value::as_u64).unwrap_or(0) as u32,
        fps,
        has_audio: audio.is_some(),
        video_codec: video
            .get("codec_name")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string(),
        audio_codec: audio
            .and_then(|stream| stream.get("codec_name"))
            .and_then(Value::as_str)
            .unwrap_or("none")
            .to_string(),
        size_bytes,
    })
}

fn parse_rate(value: &str) -> Option<f64> {
    let (num, den) = value.split_once('/')?;
    let num = num.parse::<f64>().ok()?;
    let den = den.parse::<f64>().ok()?;
    if den <= 0.0 {
        return None;
    }
    Some((num / den * 1000.0).round() / 1000.0)
}

/// Extract a single JPEG frame, used for the review card. Returns the path.
pub fn thumbnail(tools: &Tools, input: &Path, at: f64, out: &Path) -> Result<PathBuf> {
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let status = Command::new(&tools.ffmpeg)
        .args(["-hide_banner", "-loglevel", "error", "-y", "-ss"])
        .arg(format!("{:.3}", at.max(0.0)))
        .arg("-i")
        .arg(input)
        .args(["-frames:v", "1", "-vf", "scale=640:-2", "-q:v", "4"])
        .arg(out)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .status()
        .with_context(|| format!("Could not run {}", tools.ffmpeg.display()))?;
    if !status.success() {
        return Err(anyhow!("FFmpeg could not extract a thumbnail at {at:.2}s"));
    }
    Ok(out.to_path_buf())
}

/// Decode a tiny grayscale frame and estimate where the interesting stuff is.
/// This is a deliberately cheap "smart framing" pass: it finds the horizontal
/// window with the most edge energy (faces, text, motion) and returns the crop
/// center as a 0..1 fraction. No model download required.
pub fn focus_center(tools: &Tools, input: &Path, at: f64) -> Result<f64> {
    const WIDTH: usize = 96;
    const HEIGHT: usize = 54;
    let output = Command::new(&tools.ffmpeg)
        .args(["-hide_banner", "-loglevel", "error", "-ss"])
        .arg(format!("{:.3}", at.max(0.0)))
        .arg("-i")
        .arg(input)
        .args([
            "-frames:v",
            "1",
            "-vf",
            &format!("scale={WIDTH}:{HEIGHT},format=gray"),
            "-f",
            "rawvideo",
            "-",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()?;
    if !output.status.success() || output.stdout.len() < WIDTH * HEIGHT {
        return Ok(0.5);
    }
    let pixels = &output.stdout[..WIDTH * HEIGHT];
    let mut column_energy = vec![0f64; WIDTH];
    let mut prev_row: &[u8] = &[];
    let (rows, _rem) = pixels.as_chunks::<WIDTH>();
    for row in rows {
        if !prev_row.is_empty() {
            for x in 1..WIDTH {
                let dx = (row[x] as f64 - row[x - 1] as f64).abs();
                let dy = (row[x] as f64 - prev_row[x] as f64).abs();
                column_energy[x] += dx + dy;
            }
        }
        prev_row = row;
    }
    // Window width for a 9:16 crop of 16:9 source: about 31 of 96 columns.
    let window = ((WIDTH as f64) * 9.0 / 16.0).round() as usize;
    let window = window.clamp(1, WIDTH);
    let mut best_x = 0usize;
    let mut best = f64::MIN;
    let mut running: f64 = column_energy.iter().take(window).sum();
    for x in 0..=(WIDTH - window) {
        if running > best {
            best = running;
            best_x = x;
        }
        if x + window < WIDTH {
            running -= column_energy[x];
            running += column_energy[x + window];
        }
    }
    let center = (best_x as f64 + window as f64 / 2.0) / WIDTH as f64;
    Ok(center.clamp(0.08, 0.92))
}

/// Run FFmpeg with `-progress pipe:1` and report 0..1 completion.
pub fn run_ffmpeg_with_progress(
    tools: &Tools,
    args: &[String],
    total_seconds: f64,
    progress: &mut dyn FnMut(f32, &str),
) -> Result<()> {
    let mut command = Command::new(&tools.ffmpeg);
    command
        .args([
            "-hide_banner",
            "-nostats",
            "-loglevel",
            "error",
            "-progress",
            "pipe:1",
        ])
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn()?;
    use std::io::{BufRead, BufReader};
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow!("No FFmpeg stdout"))?;
    let reader = BufReader::new(stdout);
    let total = total_seconds.max(0.1);
    let mut last_message = String::new();
    for line in reader.lines().map_while(Result::ok) {
        if let Some(seconds) = parse_progress_seconds(&line) {
            let fraction = (seconds / total).clamp(0.0, 0.999) as f32;
            progress(fraction, &last_message);
        } else if let Some(speed) = line.strip_prefix("speed=") {
            let speed = speed.trim();
            if !speed.is_empty() && speed != "N/A" {
                last_message = format!("encoding at {speed}");
            }
        }
    }
    let output = child.wait_with_output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let tail = stderr
            .lines()
            .rev()
            .take(8)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join("\n");
        return Err(anyhow!("FFmpeg failed:\n{tail}"));
    }
    progress(1.0, "done");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn probe_json_is_normalized() {
        let json = json!({
            "streams": [
                {"codec_type":"video","codec_name":"h264","width":1920,"height":1080,"avg_frame_rate":"30000/1001"},
                {"codec_type":"audio","codec_name":"aac"}
            ],
            "format": {"duration":"12.5","size":"1024"}
        });
        let info = parse_probe(&json, Path::new("demo.mp4")).expect("probe");
        assert_eq!(info.width, 1920);
        assert_eq!(info.height, 1080);
        assert!(info.has_audio);
        assert!((info.fps - 29.97).abs() < 0.02);
        assert_eq!(info.duration, 12.5);
    }

    #[test]
    fn audio_only_is_rejected_with_charm() {
        let json = json!({"streams": [{"codec_type":"audio","codec_name":"aac"}], "format": {}});
        let err = parse_probe(&json, Path::new("podcast.mp3"))
            .unwrap_err()
            .to_string();
        assert!(err.contains("no video stream"));
    }
}
