use anyhow::Result;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::captions;
use crate::llm;
use crate::model::{Clip, Project, MAX_CLIP_SECONDS};

/// Edits clipfarm understands. Kept small and explicit so a chat request can
/// never silently mangle a timeline.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum RefineOp {
    SetDuration { seconds: f64 },
    ShiftStart { seconds: f64 },
    SetStart { seconds: f64 },
    Tighten { factor: f64 },
    Shrink { seconds: f64 },
    ToggleCaptions { enabled: bool },
    SetVertical { enabled: bool },
    RemoveFillers,
    SetTitle { text: String },
    SetCaption { text: String },
}

/// Parse a human instruction into operations. Regex-first, because it is fast,
/// predicts nothing, and works offline.
pub fn parse_instruction(instruction: &str) -> Vec<RefineOp> {
    let text = instruction.trim();
    let lower = text.to_ascii_lowercase();
    let mut ops = Vec::new();

    let number = Regex::new(r"(\d+(?:\.\d+)?)").expect("static regex");
    let seconds = number
        .captures(&lower)
        .and_then(|capture| capture.get(1))
        .and_then(|value| value.as_str().parse::<f64>().ok());

    if let Some(rest) = text
        .strip_prefix("title:")
        .or_else(|| text.strip_prefix("Title:"))
        .or_else(|| text.strip_prefix("rename to "))
    {
        let title = rest.trim();
        if !title.is_empty() {
            ops.push(RefineOp::SetTitle {
                text: title.to_string(),
            });
        }
    }
    if let Some(rest) = text
        .strip_prefix("caption:")
        .or_else(|| text.strip_prefix("Caption:"))
    {
        let caption = rest.trim();
        if !caption.is_empty() {
            ops.push(RefineOp::SetCaption {
                text: caption.to_string(),
            });
        }
    }

    if lower.contains("no caption")
        || lower.contains("captions off")
        || lower.contains("hide caption")
        || lower.contains("remove caption")
    {
        ops.push(RefineOp::ToggleCaptions { enabled: false });
    } else if lower.contains("captions on")
        || lower.contains("add caption")
        || lower.contains("burn caption")
        || lower.contains("show caption")
    {
        ops.push(RefineOp::ToggleCaptions { enabled: true });
    }

    if lower.contains("horizontal")
        || lower.contains("16:9")
        || lower.contains("wide")
        || lower.contains("landscape")
    {
        ops.push(RefineOp::SetVertical { enabled: false });
    } else if lower.contains("vertical") || lower.contains("9:16") || lower.contains("portrait") {
        ops.push(RefineOp::SetVertical { enabled: true });
    }

    if lower.contains("filler") || lower.contains("um") || lower.contains("uh") {
        ops.push(RefineOp::RemoveFillers);
    }

    if lower.contains("shorter")
        || lower.contains("tighter")
        || lower.contains("snappier")
        || lower.contains("punchier")
        || lower.contains("trim")
        || lower.contains("cut it")
    {
        match seconds {
            Some(value)
                if lower.contains("second")
                    || lower.contains("sec")
                    || lower.contains('s') && value > 2.0 =>
            {
                ops.push(RefineOp::SetDuration { seconds: value })
            }
            _ => ops.push(RefineOp::Tighten { factor: 0.8 }),
        }
    } else if let Some(value) = seconds {
        // Only treat a bare duration as a length request. "start 3 seconds later"
        // is a trim-in request, not a three-second clip.
        if (lower.contains("second") || lower.contains("sec"))
            && !lower.contains("start")
            && !lower.contains("end")
        {
            ops.push(RefineOp::SetDuration { seconds: value });
        }
    }

    let mentions_start = lower.contains("start");
    if mentions_start {
        match seconds {
            Some(value) if lower.contains("later") || lower.contains("after") => {
                ops.push(RefineOp::ShiftStart { seconds: value });
            }
            Some(value) if lower.contains("at") || lower.contains("from") => {
                ops.push(RefineOp::SetStart { seconds: value });
            }
            Some(value) if lower.contains("earlier") => {
                ops.push(RefineOp::ShiftStart { seconds: -value });
            }
            None if lower.contains("later") || lower.contains("after") => {
                ops.push(RefineOp::ShiftStart { seconds: 2.0 });
            }
            None if lower.contains("earlier") => {
                ops.push(RefineOp::ShiftStart { seconds: -2.0 });
            }
            _ => {}
        }
    }
    if lower.contains("end") && lower.contains("earlier") {
        if let Some(value) = seconds {
            ops.push(RefineOp::Shrink { seconds: value });
        }
    }

    ops
}

/// Apply operations in order and return human-readable change descriptions.
pub fn apply_ops(project: &Project, clip: &mut Clip, ops: &[RefineOp]) -> Vec<String> {
    let mut changes = Vec::new();
    let source_duration = project
        .media
        .as_ref()
        .map(|media| media.duration)
        .unwrap_or(f64::MAX);
    for op in ops {
        match op {
            RefineOp::SetDuration { seconds } => {
                let requested = seconds.clamp(2.0, MAX_CLIP_SECONDS);
                clip.end = (clip.start + requested).min(source_duration);
                changes.push(if *seconds > MAX_CLIP_SECONDS {
                    format!("trimmed to {}s (ceiling is 60s)", requested as i64)
                } else {
                    format!("trimmed to {}s", requested as i64)
                });
            }
            RefineOp::ShiftStart { seconds } => {
                let original = clip.start;
                clip.start = (clip.start + seconds).max(0.0).min(clip.end - 2.0);
                clip.end += clip.start - original;
                clip.end = clip.end.min(source_duration);
                changes.push(format!("moved the in-point {:.1}s", clip.start - original));
            }
            RefineOp::SetStart { seconds } => {
                let original = clip.start;
                let start = seconds.max(0.0).min(clip.end - 2.0);
                let duration = clip.end - clip.start;
                clip.start = start;
                clip.end = (start + duration).min(source_duration);
                changes.push(format!(
                    "in-point set to {:.0}s (was {:.0}s)",
                    start, original
                ));
            }
            RefineOp::Shrink { seconds } => {
                clip.end = (clip.end - seconds.max(0.1)).max(clip.start + 2.0);
                changes.push(format!("ended {:.1}s earlier", seconds));
            }
            RefineOp::Tighten { factor } => {
                let factor = factor.clamp(0.4, 0.98);
                let target = (clip.duration() * factor).clamp(2.0, MAX_CLIP_SECONDS);
                clip.end = clip.start + target;
                changes.push(format!("tightened by {:.0}%", (1.0 - factor) * 100.0));
            }
            RefineOp::ToggleCaptions { enabled } => {
                clip.settings.captions_enabled = *enabled;
                changes.push(if *enabled {
                    "captions on".into()
                } else {
                    "captions off".into()
                });
            }
            RefineOp::SetVertical { enabled } => {
                clip.settings.vertical = *enabled;
                changes.push(if *enabled {
                    "9:16 vertical".into()
                } else {
                    "source aspect ratio".into()
                });
            }
            RefineOp::RemoveFillers => {
                let cleaned = captions::remove_fillers(&clip.captions_words());
                clip.captions = captions::rebuild(&cleaned);
                changes.push("removed filler words from captions".into());
            }
            RefineOp::SetTitle { text } => {
                clip.title = text.trim().to_string();
                changes.push("new title".into());
            }
            RefineOp::SetCaption { text } => {
                clip.caption = text.trim().to_string();
                changes.push("new caption".into());
            }
        }
        clip.start = clip.start.max(0.0);
        clip.end = clip.end.min(source_duration).max(clip.start + 2.0);
        clip.enforce_cap();
    }
    changes
}

/// Deterministic refine first. If nothing matched and a local model is enabled,
/// ask it for operations. Returns `None` when there is nothing sensible to do.
pub fn refine(
    project: &Project,
    clip: &mut Clip,
    instruction: &str,
    llm: Option<(&str, &str)>,
) -> Option<Vec<String>> {
    let ops = parse_instruction(instruction);
    if !ops.is_empty() {
        return Some(apply_ops(project, clip, &ops));
    }
    let (endpoint, model) = llm?;
    let ops = ask_llm_for_ops(endpoint, model, clip, instruction).ok()?;
    if ops.is_empty() {
        return None;
    }
    Some(apply_ops(project, clip, &ops))
}

fn ask_llm_for_ops(
    endpoint: &str,
    model: &str,
    clip: &Clip,
    instruction: &str,
) -> Result<Vec<RefineOp>> {
    let prompt = format!(
        "You edit short video clips. Allowed operations as JSON: \
         {{\"operations\":[{{\"op\":\"set_duration\",\"seconds\":30}},{{\"op\":\"shift_start\",\"seconds\":2}}, \
         {{\"op\":\"set_start\",\"seconds\":12}},{{\"op\":\"tighten\",\"factor\":0.8}}, \
         {{\"op\":\"toggle_captions\",\"enabled\":false}},{{\"op\":\"set_vertical\",\"enabled\":true}}, \
         {{\"op\":\"remove_fillers\"}},{{\"op\":\"set_title\",\"text\":\"...\"}}, \
         {{\"op\":\"set_caption\",\"text\":\"...\"}}]}}. \
         Clip: {:.1}s long, starts at {:.1}s, ends at {:.1}s. \
         Clips cannot exceed 60 seconds. User request: {}. \
         Return ONLY the JSON object.",
        clip.duration(),
        clip.start,
        clip.end,
        instruction
    );
    let value = llm::complete_json(endpoint, model, &prompt, std::time::Duration::from_secs(12))?;
    parse_llm_ops(&value)
}

fn parse_llm_ops(value: &Value) -> Result<Vec<RefineOp>> {
    let operations = value
        .get("operations")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut ops = Vec::new();
    for operation in operations {
        if let Ok(op) = serde_json::from_value::<RefineOp>(operation) {
            ops.push(op);
        }
    }
    Ok(ops)
}

impl Clip {
    /// Convenience for caption editing: flatten caption words.
    pub fn captions_words(&self) -> Vec<crate::model::Word> {
        self.captions
            .iter()
            .flat_map(|caption| caption.words.iter().cloned())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ClipSettings, Platform, Transcript, Word};
    use chrono::Utc;
    use uuid::Uuid;

    fn clip() -> (Project, Clip) {
        let project = Project {
            schema_version: 1,
            id: Uuid::new_v4(),
            name: "test".into(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            niche: crate::model::Niche::Podcast,
            platform: Platform::TikTok,
            source: None,
            media: Some(crate::model::MediaInfo {
                duration: 500.0,
                ..Default::default()
            }),
            transcript: Transcript::default(),
            clips: vec![],
            settings: Default::default(),
        };
        let clip = Clip {
            id: Uuid::new_v4(),
            start: 10.0,
            end: 60.0,
            score: 80.0,
            hook: "hook".into(),
            reasons: vec![],
            title: "title".into(),
            caption: "caption".into(),
            hashtags: vec![],
            captions: vec![],
            settings: ClipSettings::default(),
            status: crate::model::ClipStatus::Kept,
            thumbnail: None,
            exports: vec![],
        };
        (project, clip)
    }

    #[test]
    fn duration_request_is_applied_and_capped() {
        let (project, mut clip) = clip();
        let ops = parse_instruction("make it 30 seconds, tighter");
        assert!(ops.contains(&RefineOp::SetDuration { seconds: 30.0 }));
        apply_ops(&project, &mut clip, &ops);
        assert!((clip.duration() - 30.0).abs() < 0.01);
    }

    #[test]
    fn asking_for_90_seconds_respects_the_ceiling() {
        let (project, mut clip) = clip();
        let ops = parse_instruction("make it 90 seconds");
        apply_ops(&project, &mut clip, &ops);
        assert!(clip.duration() <= MAX_CLIP_SECONDS + 1e-6);
    }

    #[test]
    fn caption_and_orientation_toggles_work() {
        let (project, mut clip) = clip();
        let ops = parse_instruction("captions off, make it horizontal");
        apply_ops(&project, &mut clip, &ops);
        assert!(!clip.settings.captions_enabled);
        assert!(!clip.settings.vertical);
    }

    #[test]
    fn explicit_title_wins() {
        let (project, mut clip) = clip();
        let ops = parse_instruction("title: the bit about cold coffee");
        apply_ops(&project, &mut clip, &ops);
        assert_eq!(clip.title, "the bit about cold coffee");
    }

    #[test]
    fn later_start_moves_the_window() {
        let (project, mut clip) = clip();
        let ops = parse_instruction("start 3 seconds later");
        let changes = apply_ops(&project, &mut clip, &ops);
        assert!((clip.start - 13.0).abs() < 0.01);
        assert_eq!(changes.len(), 1);
    }

    #[test]
    fn llm_ops_parse_from_json() {
        let value = serde_json::json!({
            "operations": [
                {"op": "set_duration", "seconds": 25},
                {"op": "toggle_captions", "enabled": false}
            ]
        });
        let ops = parse_llm_ops(&value).expect("ops");
        assert_eq!(ops.len(), 2);
        assert_eq!(ops[0], RefineOp::SetDuration { seconds: 25.0 });
    }

    #[test]
    fn filler_removal_rebuilds_captions() {
        let (project, mut clip) = clip();
        let words: Vec<Word> = ["um", "this", "uh", "works"]
            .iter()
            .enumerate()
            .map(|(index, text)| Word {
                text: text.to_string(),
                start: index as f64 * 0.5,
                end: index as f64 * 0.5 + 0.4,
                probability: 1.0,
            })
            .collect();
        clip.captions = crate::captions::rebuild(&words);
        let ops = vec![RefineOp::RemoveFillers];
        apply_ops(&project, &mut clip, &ops);
        let text = clip
            .captions
            .iter()
            .map(|c| c.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        assert_eq!(text, "this works");
    }
}
