use anyhow::{anyhow, Context, Result};
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};

use crate::tools::Tools;

/// One stretch of speech, in source-media seconds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpeechSegment {
    pub start: f64,
    pub end: f64,
    /// Mean level in dBFS, used by the hook scorer.
    pub mean_db: f32,
}

impl SpeechSegment {
    pub fn duration(&self) -> f64 {
        (self.end - self.start).max(0.0)
    }
}

#[derive(Debug, Clone)]
pub struct VadOptions {
    /// Below this level a frame is silence. -38 dBFS is forgiving of room tone.
    pub threshold_db: f32,
    pub min_speech: f64,
    pub min_silence: f64,
    pub pad: f64,
}

impl Default for VadOptions {
    fn default() -> Self {
        Self {
            threshold_db: -38.0,
            min_speech: 0.22,
            min_silence: 0.34,
            pad: 0.10,
        }
    }
}

/// Streaming energy VAD. Audio is decoded by FFmpeg to mono f32 and read in
/// 30 ms frames, so a three-hour VOD never has to fit in memory.
pub fn detect(tools: &Tools, input: &Path, options: &VadOptions) -> Result<Vec<SpeechSegment>> {
    if !input.is_file() {
        return Err(anyhow!("Media file not found: {}", input.display()));
    }
    let mut child = Command::new(&tools.ffmpeg)
        .args(["-hide_banner", "-loglevel", "error", "-i"])
        .arg(input)
        .args([
            "-vn",
            "-ac",
            "1",
            "-ar",
            "16000",
            "-f",
            "f32le",
            "-acodec",
            "pcm_f32le",
            "pipe:1",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| format!("Could not run {}", tools.ffmpeg.display()))?;

    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow!("No FFmpeg audio pipe"))?;
    let sample_rate = 16_000usize;
    let frame_samples = 480usize; // 30 ms
    let frame_seconds = frame_samples as f64 / sample_rate as f64;

    let mut segments = Vec::new();
    let mut speech_start: Option<f64> = None;
    let mut last_speech = 0.0;
    let mut energy_sum = 0.0f64;
    let mut energy_frames = 0usize;
    let mut pending: Vec<u8> = Vec::with_capacity(frame_samples * 4 * 8);
    let mut buffer = vec![0u8; 64 * 1024];
    let mut frame_index = 0usize;

    loop {
        let read = stdout.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        pending.extend_from_slice(&buffer[..read]);
        while pending.len() >= frame_samples * 4 {
            let frame_bytes: Vec<u8> = pending.drain(..frame_samples * 4).collect();
            let mut sum_sq = 0f64;
            for chunk in frame_bytes.chunks_exact(4) {
                let sample = f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
                sum_sq += (sample as f64) * (sample as f64);
            }
            let rms = (sum_sq / frame_samples as f64).sqrt();
            let db = 20.0 * (rms.max(1e-7)).log10();
            let time = frame_index as f64 * frame_seconds;
            frame_index += 1;

            if db > options.threshold_db as f64 {
                if speech_start.is_none() {
                    speech_start = Some(time);
                    energy_sum = 0.0;
                    energy_frames = 0;
                }
                last_speech = time + frame_seconds;
                energy_sum += db.max(options.threshold_db as f64);
                energy_frames += 1;
            } else if let Some(start) = speech_start {
                if time - last_speech >= options.min_silence {
                    push_segment(
                        &mut segments,
                        start,
                        last_speech,
                        energy_sum,
                        energy_frames,
                        options,
                    );
                    speech_start = None;
                }
            }
        }
    }
    if let Some(start) = speech_start {
        let end = frame_index as f64 * frame_seconds;
        push_segment(
            &mut segments,
            start,
            end,
            energy_sum,
            energy_frames,
            options,
        );
    }
    let status = child.wait()?;
    if segments.is_empty() && !status.success() {
        return Err(anyhow!(
            "FFmpeg could not decode audio from {}.",
            input.display()
        ));
    }
    Ok(merge_segments(segments, 0.05))
}

fn push_segment(
    segments: &mut Vec<SpeechSegment>,
    start: f64,
    end: f64,
    energy_sum: f64,
    energy_frames: usize,
    options: &VadOptions,
) {
    if end - start < options.min_speech {
        return;
    }
    let start = (start - options.pad).max(0.0);
    let end = end + options.pad;
    segments.push(SpeechSegment {
        start,
        end,
        mean_db: if energy_frames == 0 {
            options.threshold_db
        } else {
            (energy_sum / energy_frames as f64) as f32
        },
    });
}

fn merge_segments(mut segments: Vec<SpeechSegment>, gap: f64) -> Vec<SpeechSegment> {
    segments.sort_by(|a, b| {
        a.start
            .partial_cmp(&b.start)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut merged: Vec<SpeechSegment> = Vec::with_capacity(segments.len());
    for segment in segments {
        if let Some(last) = merged.last_mut() {
            if segment.start <= last.end + gap {
                let last_duration = last.duration();
                let segment_duration = segment.duration();
                let total = last_duration + segment_duration;
                if total > 0.0 {
                    last.mean_db = ((last.mean_db as f64 * last_duration
                        + segment.mean_db as f64 * segment_duration)
                        / total) as f32;
                }
                last.end = last.end.max(segment.end);
                continue;
            }
        }
        merged.push(segment);
    }
    merged
}

/// Pure detection over PCM, exposed for tests and other audio sources.
pub fn detect_pcm(pcm: &[f32], sample_rate: usize, options: &VadOptions) -> Vec<SpeechSegment> {
    let frame_samples = (sample_rate as f64 * 0.03).round().max(1.0) as usize;
    let frame_seconds = frame_samples as f64 / sample_rate as f64;
    let mut segments = Vec::new();
    let mut speech_start: Option<f64> = None;
    let mut last_speech = 0.0;
    let mut energy_sum = 0.0;
    let mut energy_frames = 0usize;
    for (index, frame) in pcm.chunks(frame_samples).enumerate() {
        if frame.len() < frame_samples / 2 {
            break;
        }
        let sum_sq: f64 = frame.iter().map(|s| (*s as f64) * (*s as f64)).sum();
        let rms = (sum_sq / frame.len() as f64).sqrt();
        let db = 20.0 * (rms.max(1e-7)).log10();
        let time = index as f64 * frame_seconds;
        if db > options.threshold_db as f64 {
            if speech_start.is_none() {
                speech_start = Some(time);
                energy_sum = 0.0;
                energy_frames = 0;
            }
            last_speech = time + frame_seconds;
            energy_sum += db.max(options.threshold_db as f64);
            energy_frames += 1;
        } else if let Some(start) = speech_start {
            if time - last_speech >= options.min_silence {
                push_segment(
                    &mut segments,
                    start,
                    last_speech,
                    energy_sum,
                    energy_frames,
                    options,
                );
                speech_start = None;
            }
        }
    }
    if let Some(start) = speech_start {
        let end = pcm.len() as f64 / sample_rate as f64;
        push_segment(
            &mut segments,
            start,
            end,
            energy_sum,
            energy_frames,
            options,
        );
    }
    merge_segments(segments, 0.05)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(seconds: f64, amplitude: f32) -> Vec<f32> {
        let sample_rate = 16_000;
        let count = (seconds * sample_rate as f64) as usize;
        (0..count)
            .map(|index| {
                let t = index as f32 / sample_rate as f32;
                amplitude * (2.0 * std::f32::consts::PI * 220.0 * t).sin()
            })
            .collect()
    }

    fn silence(seconds: f64) -> Vec<f32> {
        vec![0.0; (seconds * 16_000.0) as usize]
    }

    #[test]
    fn finds_two_turns_of_speech() {
        let mut pcm = silence(0.5);
        pcm.extend(tone(1.0, 0.25));
        pcm.extend(silence(0.8));
        pcm.extend(tone(0.9, 0.25));
        pcm.extend(silence(0.5));
        let options = VadOptions::default();
        let segments = detect_pcm(&pcm, 16_000, &options);
        assert_eq!(segments.len(), 2, "segments: {segments:?}");
        assert!((segments[0].start - 0.4).abs() < 0.12);
        assert!((segments[1].start - 2.2).abs() < 0.2);
        assert!(segments[0].mean_db > -30.0);
    }

    #[test]
    fn pure_silence_has_no_speech() {
        let segments = detect_pcm(&silence(2.0), 16_000, &VadOptions::default());
        assert!(segments.is_empty());
    }
}
