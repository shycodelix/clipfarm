use anyhow::{anyhow, Result};
use std::path::PathBuf;
use uuid::Uuid;

use crate::analyze::{plan_clips, AnalyzeOptions};
use crate::ingest::ingest_any;
use crate::media;
use crate::model::{Clip, Niche, Platform, Project};
use crate::project::{new_project, project_media_dir, save_project};
use crate::tools::Tools;
use crate::transcribe::transcribe;
use crate::vad::{self, SpeechSegment, VadOptions};

#[derive(Debug, Clone)]
pub struct AnalysisReport {
    pub clips: usize,
    pub speech_segments: usize,
    pub transcribed: bool,
    pub warnings: Vec<String>,
}

/// Ingest a local file or URL and create a saved project for it.
pub fn create_project(
    tools: &Tools,
    input: &str,
    name: Option<String>,
    niche: Niche,
    platform: Platform,
    progress: &mut dyn FnMut(f32, &str),
) -> Result<Project> {
    let mut project = new_project("Untitled harvest", niche, platform, None, None);
    let destination = project_media_dir(project.id);
    let ingested = ingest_any(tools, input, &destination, progress)?;
    project.name = name
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| ingested.source.display_name.clone());
    project.source = Some(ingested.source);
    project.media = Some(ingested.media);
    progress(1.0, "project ready");
    save_project(&mut project)?;
    Ok(project)
}

/// Run the whole local analysis pass, mutating and saving the project.
///
/// Missing optional tools never fail the run: no whisper means VAD-only
/// planning, no FFmpeg means nothing works and that error is explicit.
pub fn analyze(
    tools: &Tools,
    project: &mut Project,
    options: &AnalyzeOptions,
    progress: &mut dyn FnMut(f32, &str),
) -> Result<AnalysisReport> {
    let source = project
        .source
        .as_ref()
        .ok_or_else(|| anyhow!("This project has no source media."))?
        .path
        .clone();
    if !source.is_file() {
        return Err(anyhow!("Source media is missing: {}", source.display()));
    }
    let media_info = match project.media.clone() {
        Some(info) => info,
        None => {
            let info = media::probe(tools, &source)?;
            project.media = Some(info.clone());
            info
        }
    };

    let mut warnings = Vec::new();
    progress(0.04, "listening for the good parts");
    let speech: Vec<SpeechSegment> = if media_info.has_audio {
        match vad::detect(tools, &source, &VadOptions::default()) {
            Ok(segments) => segments,
            Err(error) => {
                warnings.push(format!("VAD failed, using fixed windows: {error}"));
                Vec::new()
            }
        }
    } else {
        warnings.push("Source has no audio — captions and speech scoring are skipped.".into());
        Vec::new()
    };

    if speech.is_empty() {
        warnings.push("No speech detected; clips use even windows.".into());
    } else {
        progress(0.22, &format!("found {} stretches of speech", speech.len()));
    }

    let mut transcribed = false;
    if media_info.has_audio && tools.whisper_ready() {
        let work_dir = project_media_dir(project.id).join("whisper");
        let mut mapped = |fraction: f32, message: &str| {
            progress(0.28 + fraction * 0.42, message);
        };
        match transcribe(tools, &source, &work_dir, &mut mapped) {
            Ok(Some(transcript)) => {
                transcribed = true;
                progress(0.72, "placing words on the timeline");
                project.transcript = transcript;
            }
            Ok(None) => {}
            Err(error) => warnings.push(format!("Transcription skipped: {error}")),
        }
    } else if media_info.has_audio {
        warnings.push(
            "whisper.cpp is not configured, so this pass is VAD-only (no word captions).".into(),
        );
    }

    progress(0.74, "picking hooks");
    let mut plan_progress = |fraction: f32, message: &str| {
        progress(0.74 + fraction * 0.18, message);
    };
    let outcome = plan_clips(project, &speech, options, &mut plan_progress);
    let clip_count = outcome.clips.len();
    project.clips = outcome.clips;
    project.settings.target_clips = options.target_clips;
    project.settings.max_clip_seconds = options.max_seconds.min(crate::model::MAX_CLIP_SECONDS);
    project.updated_at = chrono::Utc::now();

    progress(0.92, "stealing thumbnails");
    let thumbnail_dir = project_media_dir(project.id).join("thumbs");
    for clip in &mut project.clips {
        let at = clip.start + (clip.duration() * 0.25).min(2.5);
        let path = thumbnail_dir.join(format!("{}.jpg", clip.id));
        if media::thumbnail(tools, &source, at, &path).is_ok() {
            clip.thumbnail = Some(path);
        }
    }

    save_project(project)?;
    progress(1.0, "clips are ready");
    Ok(AnalysisReport {
        clips: clip_count,
        speech_segments: speech.len(),
        transcribed,
        warnings,
    })
}

/// Resolve a clip by 1-based index, full id, id prefix, or title fragment.
pub fn find_clip<'a>(project: &'a Project, selector: &str) -> Result<&'a Clip> {
    find_clip_index(project, selector).map(|index| &project.clips[index])
}

pub fn find_clip_index(project: &Project, selector: &str) -> Result<usize> {
    let selector = selector.trim();
    if let Ok(index) = selector.parse::<usize>() {
        if index >= 1 && index <= project.clips.len() {
            return Ok(index - 1);
        }
    }
    if let Some(index) = project
        .clips
        .iter()
        .position(|clip| clip.id.to_string() == selector)
    {
        return Ok(index);
    }
    if let Some(index) = project
        .clips
        .iter()
        .position(|clip| clip.id.to_string().starts_with(selector))
    {
        return Ok(index);
    }
    if let Some(index) = project.clips.iter().position(|clip| {
        clip.title
            .to_ascii_lowercase()
            .contains(&selector.to_ascii_lowercase())
    }) {
        return Ok(index);
    }
    Err(anyhow!(
        "No clip matches `{selector}`. Run `clipfarm clips <project>`."
    ))
}

pub fn export_path(
    project: &Project,
    clip: &Clip,
    index: usize,
    kind: crate::model::ExportKind,
) -> PathBuf {
    let dir = crate::project::export_dir(project);
    let suffix = match kind {
        crate::model::ExportKind::Raw => "raw",
        crate::model::ExportKind::Rendered => "short",
    };
    dir.join(format!(
        "{:02}-{}-{}.mp4",
        index + 1,
        crate::util::slugify(&clip.title, 48),
        suffix
    ))
}

/// A project id is enough to identify a folder; this is handy for `doctor`.
pub fn project_folder(id: Uuid) -> PathBuf {
    project_media_dir(id)
}
