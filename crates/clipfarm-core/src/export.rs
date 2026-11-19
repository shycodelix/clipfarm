use anyhow::{anyhow, Result};
use std::path::{Path, PathBuf};

use crate::captions;
use crate::media;
use crate::model::{Clip, ExportKind, ExportRecord, MediaInfo, Project, MAX_CLIP_SECONDS};
use crate::tools::Tools;
use crate::util::human_bytes;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Quality {
    High,
    Balanced,
    Small,
}

impl Quality {
    pub fn label(self) -> &'static str {
        match self {
            Quality::High => "high",
            Quality::Balanced => "balanced",
            Quality::Small => "small",
        }
    }

    fn crf(self) -> u32 {
        match self {
            Quality::High => 18,
            Quality::Balanced => 21,
            Quality::Small => 26,
        }
    }

    fn bitrate(self) -> &'static str {
        match self {
            Quality::High => "10M",
            Quality::Balanced => "6M",
            Quality::Small => "3M",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ExportOptions {
    pub kind: ExportKind,
    pub output: PathBuf,
    pub quality: Quality,
}

impl ExportOptions {
    pub fn raw(output: impl Into<PathBuf>) -> Self {
        Self {
            kind: ExportKind::Raw,
            output: output.into(),
            quality: Quality::High,
        }
    }

    pub fn rendered(output: impl Into<PathBuf>) -> Self {
        Self {
            kind: ExportKind::Rendered,
            output: output.into(),
            quality: Quality::Balanced,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ExportOutcome {
    pub clip_id: uuid::Uuid,
    pub path: PathBuf,
    pub kind: ExportKind,
    pub preset: String,
    pub bytes: u64,
    pub duration: f64,
    pub burned_captions: bool,
    pub encoder: String,
    pub note: String,
}

/// The one and only export entry point. Raw is a stream copy (fast, lossless
/// container-level trim); rendered burns captions, reframes to 9:16 and encodes
/// H.264/AAC with the best available encoder.
pub fn export_clip(
    tools: &Tools,
    project: &Project,
    clip: &Clip,
    options: &ExportOptions,
    progress: &mut dyn FnMut(f32, &str),
) -> Result<ExportOutcome> {
    clip.validate_duration()?;
    if clip.duration() > MAX_CLIP_SECONDS + 1e-6 {
        return Err(anyhow!("Refusing to export more than 60 seconds."));
    }
    let source = project
        .source
        .as_ref()
        .ok_or_else(|| anyhow!("This project has no source media."))?;
    if !source.path.is_file() {
        return Err(anyhow!(
            "Source media moved or disappeared: {}",
            source.path.display()
        ));
    }
    if let Some(parent) = options.output.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let media = match project.media.clone() {
        Some(media) => media,
        None => media::probe(tools, &source.path)?,
    };

    match options.kind {
        ExportKind::Raw => export_raw(tools, &source.path, clip, options, progress),
        ExportKind::Rendered => export_rendered(
            tools,
            &source.path,
            clip,
            &media,
            project,
            options,
            progress,
        ),
    }
}

fn export_raw(
    tools: &Tools,
    source: &Path,
    clip: &Clip,
    options: &ExportOptions,
    progress: &mut dyn FnMut(f32, &str),
) -> Result<ExportOutcome> {
    progress(0.05, "stream copying");
    let args = vec![
        "-y".to_string(),
        "-ss".to_string(),
        format!("{:.3}", clip.start),
        "-to".to_string(),
        format!("{:.3}", clip.end),
        "-i".to_string(),
        source.display().to_string(),
        "-map".to_string(),
        "0:v:0".to_string(),
        "-map".to_string(),
        "0:a?".to_string(),
        "-c".to_string(),
        "copy".to_string(),
        "-avoid_negative_ts".to_string(),
        "make_zero".to_string(),
        "-movflags".to_string(),
        "+faststart".to_string(),
        options.output.display().to_string(),
    ];
    media::run_ffmpeg_with_progress(tools, &args, clip.duration(), progress)?;

    // Keyframe-aligned cuts can overshoot the requested out-point. The 60s
    // ceiling is non-negotiable, so re-encode only the rare overshooting file.
    let actual = media::probe(tools, &options.output)
        .map(|info| info.duration)
        .unwrap_or(0.0);
    let mut note = format!(
        "Lossless stream copy. Cut points land on the nearest keyframe (actual {:.1}s).",
        actual
    );
    let mut encoder = "copy".to_string();
    let mut preset = "stream-copy".to_string();
    if actual > MAX_CLIP_SECONDS + 0.01 {
        progress(0.5, "re-encoding to respect the 60s ceiling");
        let fallback = vec![
            "-y".to_string(),
            "-ss".to_string(),
            format!("{:.3}", clip.start),
            "-i".to_string(),
            source.display().to_string(),
            "-t".to_string(),
            format!("{:.3}", clip.duration().min(MAX_CLIP_SECONDS)),
            "-map".to_string(),
            "0:v:0".to_string(),
            "-map".to_string(),
            "0:a?".to_string(),
            "-c:v".to_string(),
            "libx264".to_string(),
            "-preset".to_string(),
            "veryfast".to_string(),
            "-crf".to_string(),
            "16".to_string(),
            "-c:a".to_string(),
            "aac".to_string(),
            "-b:a".to_string(),
            "192k".to_string(),
            "-movflags".to_string(),
            "+faststart".to_string(),
            options.output.display().to_string(),
        ];
        media::run_ffmpeg_with_progress(tools, &fallback, clip.duration(), progress)?;
        encoder = "libx264".into();
        preset = "stream-copy-overshoot".into();
        note = "The keyframe cut overshot 60s, so this raw file was re-encoded at CRF 16 to keep the ceiling.".into();
    }
    let bytes = std::fs::metadata(&options.output)?.len();
    Ok(ExportOutcome {
        clip_id: clip.id,
        path: options.output.clone(),
        kind: ExportKind::Raw,
        preset,
        bytes,
        duration: actual,
        burned_captions: false,
        encoder,
        note: format!("{} ({})", note, human_bytes(bytes)),
    })
}

fn export_rendered(
    tools: &Tools,
    source: &Path,
    clip: &Clip,
    media: &MediaInfo,
    project: &Project,
    options: &ExportOptions,
    progress: &mut dyn FnMut(f32, &str),
) -> Result<ExportOutcome> {
    let (encoder, encoder_note) = choose_encoder(tools, options.quality);
    let (width, height) = if clip.settings.vertical {
        (1080u32, 1920u32)
    } else {
        (media.width.max(2) & !1, media.height.max(2) & !1)
    };

    let mut filters: Vec<String> = Vec::new();
    if clip.settings.vertical {
        if media.width >= media.height {
            let crop_width = ((media.height as f64 * 9.0 / 16.0).round() as u32).max(2) & !1;
            let slack = media.width.saturating_sub(crop_width);
            let focus = clip.settings.focus_x.clamp(0.0, 1.0);
            let x = ((focus * media.width as f64) - crop_width as f64 / 2.0)
                .clamp(0.0, slack as f64) as u32
                & !1;
            filters.push(format!(
                "crop={crop_width}:{}:{x}:0",
                media.height.max(2) & !1
            ));
        }
        filters.push(format!(
            "scale={width}:{height}:force_original_aspect_ratio=decrease:flags=lanczos"
        ));
        filters.push(format!(
            "pad={width}:{height}:(ow-iw)/2:(oh-ih)/2:color=black"
        ));
    } else {
        filters.push(format!("scale={width}:{height}:flags=lanczos"));
    }

    let mut subtitle_file: Option<PathBuf> = None;
    let mut burned_captions = false;
    if clip.settings.captions_enabled && !clip.captions.is_empty() && tools.supports_subtitles() {
        let directory = std::env::temp_dir().join(format!("clipfarm-{}", clip.id));
        std::fs::create_dir_all(&directory)?;
        let srt = directory.join("captions.srt");
        std::fs::write(&srt, captions::to_srt(&clip.captions, clip.start))?;
        filters.push(format!(
            "subtitles=filename='{}':force_style='{}'",
            escape_filter_path(&srt),
            clip.settings.caption_style.force_style()
        ));
        subtitle_file = Some(directory);
        burned_captions = true;
    }

    let filter = filters.join(",");
    let args = vec![
        "-y".to_string(),
        "-ss".to_string(),
        format!("{:.3}", clip.start),
        "-i".to_string(),
        source.display().to_string(),
        "-t".to_string(),
        format!("{:.3}", clip.duration()),
        "-map".to_string(),
        "0:v:0".to_string(),
        "-map".to_string(),
        "0:a?".to_string(),
        "-vf".to_string(),
        filter,
        "-c:v".to_string(),
        encoder.to_string(),
        "-pix_fmt".to_string(),
        "yuv420p".to_string(),
    ];

    let mut args = args;
    match encoder.as_str() {
        "h264_videotoolbox" | "h264_nvenc" | "h264_qsv" | "h264_amf" => {
            args.push("-b:v".into());
            args.push(options.quality.bitrate().into());
        }
        _ => {
            args.push("-preset".into());
            args.push("medium".into());
            args.push("-crf".into());
            args.push(options.quality.crf().to_string());
        }
    }
    args.extend(
        [
            "-c:a",
            "aac",
            "-b:a",
            "192k",
            "-movflags",
            "+faststart",
            "-r",
            "30",
        ]
        .into_iter()
        .map(String::from),
    );
    args.push(options.output.display().to_string());

    let result = media::run_ffmpeg_with_progress(tools, &args, clip.duration(), progress);
    if let Some(directory) = subtitle_file {
        let _ = std::fs::remove_dir_all(directory);
    }
    result?;

    let bytes = std::fs::metadata(&options.output)?.len();
    let preset = format!(
        "{}-{}p-{}",
        project.platform.slug(),
        height,
        options.quality.label()
    );
    let note = if clip.settings.captions_enabled && !clip.captions.is_empty() && !burned_captions {
        "FFmpeg was built without libass, so captions were not burned in. Use the SRT sidecar or install a full FFmpeg build.".to_string()
    } else if burned_captions {
        format!(
            "{} burned in. {}",
            clip.settings.caption_style.label(),
            human_bytes(bytes)
        )
    } else {
        human_bytes(bytes)
    };
    Ok(ExportOutcome {
        clip_id: clip.id,
        path: options.output.clone(),
        kind: ExportKind::Rendered,
        preset,
        bytes,
        duration: clip.duration(),
        burned_captions,
        encoder: format!("{encoder} ({encoder_note})"),
        note,
    })
}

fn choose_encoder(tools: &Tools, quality: Quality) -> (String, String) {
    // Hardware encoders are faster but their rate control is less predictable.
    // Keep libx264 for the "small" preset so file size stays honest.
    if quality != Quality::Small {
        if let Some((encoder, label)) = tools.hardware_encoder() {
            return (encoder.to_string(), label.to_string());
        }
    }
    ("libx264".into(), "software, most compatible".into())
}

fn escape_filter_path(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "/")
        .replace(':', "\\:")
        .replace('\'', "\\'")
}

/// Saves a `.srt` next to an export so editors can toggle captions later.
pub fn sidecar_subtitles(clip: &Clip, output: &Path) -> Result<Option<PathBuf>> {
    if clip.captions.is_empty() {
        return Ok(None);
    }
    let srt = output.with_extension("srt");
    std::fs::write(&srt, captions::to_srt(&clip.captions, clip.start))?;
    Ok(Some(srt))
}

pub fn record_export(clip: &mut Clip, outcome: &ExportOutcome) {
    clip.exports.push(ExportRecord {
        path: outcome.path.clone(),
        kind: outcome.kind,
        preset: outcome.preset.clone(),
        bytes: outcome.bytes,
        created_at: chrono::Utc::now(),
    });
    clip.status = crate::model::ClipStatus::Exported;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_paths_are_escaped_for_windows_drives() {
        let escaped = escape_filter_path(Path::new(r"C:\Users\me\clip farm\a.srt"));
        assert!(escaped.contains("C\\:/Users/me/clip farm/a.srt"));
        assert!(!escaped.contains("C:/"));
    }

    #[test]
    fn presets_are_sane() {
        assert!(Quality::High.crf() < Quality::Small.crf());
    }
}
