use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use uuid::Uuid;

use crate::model::{
    Clip, ClipStatus, MediaInfo, Niche, Platform, Project, ProjectSettings, Source, Transcript,
    MAX_CLIP_SECONDS,
};
use crate::util::{ensure_dir, write_atomic};

/// Where clipfarm keeps projects and generated media.
pub fn data_dir() -> PathBuf {
    if let Some(value) = std::env::var_os("CLIPFARM_HOME") {
        return PathBuf::from(value);
    }
    dirs::data_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("clipfarm")
}

pub fn projects_dir() -> PathBuf {
    data_dir().join("projects")
}

pub fn project_media_dir(id: Uuid) -> PathBuf {
    data_dir().join("media").join(id.to_string())
}

pub fn new_project(
    name: impl Into<String>,
    niche: Niche,
    platform: Platform,
    source: Option<Source>,
    media: Option<MediaInfo>,
) -> Project {
    let now = Utc::now();
    Project {
        schema_version: 1,
        id: Uuid::new_v4(),
        name: name.into(),
        created_at: now,
        updated_at: now,
        niche,
        platform,
        source,
        media,
        transcript: Transcript::default(),
        clips: Vec::new(),
        settings: ProjectSettings::default(),
    }
}

/// Fill in clip defaults so a freshly planned clip is always review-ready.
pub fn attach_clip(mut project: Project, mut clip: Clip) -> Project {
    clip.enforce_cap();
    if clip.end
        > project
            .media
            .as_ref()
            .map(|m| m.duration)
            .unwrap_or(f64::MAX)
    {
        clip.end = project
            .media
            .as_ref()
            .map(|m| m.duration)
            .unwrap_or(clip.end);
    }
    if clip.status == ClipStatus::Planned {
        clip.status = ClipStatus::Kept;
    }
    project.clips.push(clip);
    project.updated_at = Utc::now();
    project
}

pub fn project_path(id: Uuid) -> PathBuf {
    projects_dir().join(format!("{id}.json"))
}

pub fn save_project(project: &mut Project) -> Result<()> {
    project.updated_at = Utc::now();
    for clip in &mut project.clips {
        // The 60s ceiling is enforced on save as well as on export; no code path
        // is allowed to persist an over-long clip.
        clip.enforce_cap();
    }
    let path = project_path(project.id);
    let json = serde_json::to_string_pretty(project).context("Could not serialize project")?;
    write_atomic(&path, &json)
}

pub fn load_project(selector: &str) -> Result<Project> {
    let selector = selector.trim();
    if selector.is_empty() {
        return Err(anyhow!("Give me a project id or a path to a project.json."));
    }
    let direct = Path::new(selector);
    if direct.is_file() {
        let text = std::fs::read_to_string(direct)
            .with_context(|| format!("Could not read {}", direct.display()))?;
        return serde_json::from_str(&text)
            .with_context(|| format!("{} is not a clipfarm project", direct.display()));
    }
    let projects = list_projects()?;
    if let Some(summary) = projects
        .iter()
        .find(|summary| summary.id.to_string() == selector)
        .or_else(|| {
            projects
                .iter()
                .find(|summary| summary.id.to_string().starts_with(selector))
        })
        .or_else(|| {
            projects
                .iter()
                .find(|summary| summary.name.eq_ignore_ascii_case(selector))
        })
    {
        let text = std::fs::read_to_string(project_path(summary.id))?;
        return Ok(serde_json::from_str(&text)?);
    }
    Err(anyhow!(
        "No project matches `{selector}`. Try `clipfarm projects`."
    ))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectSummary {
    pub id: Uuid,
    pub name: String,
    pub niche: Niche,
    pub platform: Platform,
    pub clips: usize,
    pub duration: f64,
    pub updated_at: DateTime<Utc>,
}

pub fn list_projects() -> Result<Vec<ProjectSummary>> {
    let dir = projects_dir();
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut summaries = Vec::new();
    for entry in std::fs::read_dir(&dir)?.flatten() {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(project) = serde_json::from_str::<Project>(&text) else {
            continue;
        };
        summaries.push(ProjectSummary {
            id: project.id,
            name: project.name,
            niche: project.niche,
            platform: project.platform,
            clips: project.clips.len(),
            duration: project
                .media
                .as_ref()
                .map(|media| media.duration)
                .unwrap_or(0.0),
            updated_at: project.updated_at,
        });
    }
    summaries.sort_by_key(|a| std::cmp::Reverse(a.updated_at));
    Ok(summaries)
}

pub fn delete_project(id: Uuid) -> Result<()> {
    let path = project_path(id);
    if path.exists() {
        std::fs::remove_file(&path)?;
    }
    let media = project_media_dir(id);
    if media.exists() {
        std::fs::remove_dir_all(media)?;
    }
    Ok(())
}

/// Where exports live. Kept inside the data dir by default so the app never
/// writes into the user's source folders unless they ask for it.
pub fn export_dir(project: &Project) -> PathBuf {
    data_dir()
        .join("exports")
        .join(crate::util::slugify(&project.name, 48))
}

pub fn prepare_export_dir(project: &Project) -> Result<PathBuf> {
    let dir = export_dir(project);
    ensure_dir(&dir)?;
    Ok(dir)
}

pub fn clip_budget(project: &Project) -> f64 {
    project.settings.max_clip_seconds.min(MAX_CLIP_SECONDS)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ClipSettings, Platform};

    /// CLIPFARM_HOME is process-global; keep env-mutating tests from racing.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn projects_round_trip() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        let unique = format!("/tmp/clipfarm-test-{}", Uuid::new_v4());
        std::env::set_var("CLIPFARM_HOME", &unique);
        let mut project = new_project(
            "Test harvest",
            Niche::Podcast,
            Platform::TikTok,
            None,
            Some(MediaInfo {
                duration: 100.0,
                ..Default::default()
            }),
        );
        save_project(&mut project).expect("save");
        let loaded = load_project(&project.id.to_string()).expect("load");
        assert_eq!(loaded.name, "Test harvest");
        let summaries = list_projects().expect("list");
        assert_eq!(summaries.len(), 1);
        assert!(clone_clip_settings_defaults());
        std::env::remove_var("CLIPFARM_HOME");
        std::fs::remove_dir_all(unique).ok();
    }

    fn clone_clip_settings_defaults() -> bool {
        ClipSettings::default().vertical
    }

    #[test]
    fn project_ids_resolve_by_prefix() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        let unique = format!("/tmp/clipfarm-test-{}", Uuid::new_v4());
        std::env::set_var("CLIPFARM_HOME", &unique);
        let mut project = new_project("Prefix test", Niche::Gaming, Platform::TikTok, None, None);
        save_project(&mut project).expect("save");
        let prefix = &project.id.to_string()[..8];
        assert_eq!(load_project(prefix).expect("prefix load").id, project.id);
        std::env::remove_var("CLIPFARM_HOME");
        std::fs::remove_dir_all(unique).ok();
    }
}
