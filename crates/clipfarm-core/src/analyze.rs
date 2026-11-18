use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::captions;
use crate::metadata;
use crate::model::{Clip, ClipSettings, ClipStatus, Niche, Project, Transcript, MAX_CLIP_SECONDS};
use crate::trends;
use crate::util::clamp;
use crate::vad::SpeechSegment;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalyzeOptions {
    pub target_clips: usize,
    pub max_seconds: f64,
    pub min_seconds: f64,
}

impl Default for AnalyzeOptions {
    fn default() -> Self {
        Self {
            target_clips: 6,
            max_seconds: MAX_CLIP_SECONDS,
            min_seconds: 12.0,
        }
    }
}

impl AnalyzeOptions {
    pub fn from_project(project: &Project) -> Self {
        Self {
            target_clips: project.settings.target_clips.clamp(1, 40),
            max_seconds: project.settings.max_clip_seconds.min(MAX_CLIP_SECONDS),
            min_seconds: 10.0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PlanOutcome {
    pub clips: Vec<Clip>,
    pub used_transcript: bool,
    pub candidates: usize,
}

/// Turn speech segments (and, when available, a transcript) into review-ready
/// clips. Every returned clip is guaranteed to satisfy the 60s ceiling.
pub fn plan_clips(
    project: &Project,
    speech: &[SpeechSegment],
    options: &AnalyzeOptions,
    progress: &mut dyn FnMut(f32, &str),
) -> PlanOutcome {
    let profile = trends::resolve(project.niche, project.platform);
    let media_duration = project
        .media
        .as_ref()
        .map(|media| media.duration)
        .unwrap_or_else(|| speech.iter().map(|segment| segment.end).fold(0.0, f64::max));

    let max_seconds = options
        .max_seconds
        .min(MAX_CLIP_SECONDS)
        .min(profile.max_seconds.max(10.0))
        .clamp(4.0, MAX_CLIP_SECONDS);
    let min_seconds = options.min_seconds.min(max_seconds * 0.6).max(4.0);
    let target_seconds =
        ((profile.min_seconds + profile.max_seconds) / 2.0).clamp(min_seconds, max_seconds);

    progress(0.40, "finding the moments");
    // Join across pauses the way an editor would: faster feeds tolerate less
    // dead air than a podcast does, but even a podcast gets ~3s of patience.
    let join_gap = (profile.cut_every * 0.55).clamp(1.2, 3.2);
    let windows = build_windows(
        speech,
        media_duration,
        max_seconds,
        min_seconds,
        target_seconds,
        join_gap,
    );

    let rollup = speech
        .iter()
        .map(|segment| segment.mean_db)
        .fold(-60.0f32, |a, b| a.max(b));
    let mut candidates: Vec<(f64, f64, f32, Vec<String>, String)> =
        Vec::with_capacity(windows.len());
    for (index, (start, end)) in windows.iter().copied().enumerate() {
        let (score, reasons, hook) = score_window(
            start,
            end,
            speech,
            &project.transcript,
            project.niche,
            &profile,
            rollup,
        );
        candidates.push((start, end, score, reasons, hook));
        let fraction = 0.42 + (index as f32 / windows.len().max(1) as f32) * 0.4;
        progress(fraction, "scoring hooks");
    }

    let used_transcript = !project.transcript.words.is_empty();
    let mut ranked = candidates;
    ranked.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));
    ranked.truncate(options.target_clips.clamp(1, 40));

    progress(0.86, "writing titles");
    let mut clips: Vec<Clip> = ranked
        .into_iter()
        .enumerate()
        .map(|(index, (start, end, score, reasons, hook))| {
            let words = project.transcript.words_between(start, end);
            let meta = metadata::generate(&words, project.niche, project.platform, &profile);
            let captions = if words.is_empty() {
                Vec::new()
            } else {
                captions::build_captions(&words, 42, 3.6, 0.5)
            };
            // Without a transcript, timestamped hooks beat three copies of the
            // same generic title.
            let title = if words.is_empty() {
                hook.clone()
            } else {
                meta.titles
                    .get(index % meta.titles.len().max(1))
                    .cloned()
                    .unwrap_or_else(|| format!("clip {}", index + 1))
            };
            let caption = if words.is_empty() {
                format!(
                    "No transcript, no filler: a {:.0}s stretch from “{}” at {}.",
                    end - start,
                    project.name,
                    crate::util::human_duration(start)
                )
            } else {
                meta.caption.clone()
            };
            let mut clip = Clip {
                id: Uuid::new_v4(),
                start,
                end,
                score,
                hook,
                reasons,
                title,
                caption,
                hashtags: meta.hashtags,
                captions,
                settings: ClipSettings {
                    vertical: project.settings.vertical,
                    captions_enabled: project.settings.captions_enabled && !words.is_empty(),
                    caption_style: profile.caption_style,
                    ..Default::default()
                },
                status: ClipStatus::Kept,
                thumbnail: None,
                exports: Vec::new(),
            };
            clip.end = clip.end.min(media_duration.max(0.0)).max(clip.start + 2.0);
            clip.enforce_cap();
            clip
        })
        .collect();

    clips.sort_by(|a, b| {
        a.start
            .partial_cmp(&b.start)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    progress(0.95, "ready to review");

    PlanOutcome {
        clips,
        used_transcript,
        candidates: windows.len(),
    }
}

/// Split long monologues and group short utterances into platform-sized windows.
fn build_windows(
    speech: &[SpeechSegment],
    media_duration: f64,
    max_seconds: f64,
    min_seconds: f64,
    target_seconds: f64,
    join_gap: f64,
) -> Vec<(f64, f64)> {
    let mut pieces: Vec<(f64, f64)> = Vec::new();
    for segment in speech {
        let start = segment.start.max(0.0);
        let end = if media_duration > 0.0 {
            segment.end.min(media_duration)
        } else {
            segment.end
        };
        if end - start < 0.3 {
            continue;
        }
        let chunk_limit = (max_seconds - 0.4).max(3.0);
        let divisions = ((end - start) / chunk_limit).ceil().max(1.0) as usize;
        let chunk = (end - start) / divisions as f64;
        for index in 0..divisions {
            let chunk_start = start + chunk * index as f64;
            let chunk_end = (start + chunk * (index + 1) as f64).min(end);
            if chunk_end - chunk_start >= 0.25 {
                pieces.push((chunk_start, chunk_end));
            }
        }
    }

    if pieces.is_empty() {
        // No audio analysis (silent footage or VAD unavailable): still produce
        // evenly sized review windows instead of an empty screen.
        if media_duration <= 1.0 {
            return Vec::new();
        }
        let count = ((media_duration / target_seconds.max(5.0)).ceil() as usize).max(1);
        let step = media_duration / count as f64;
        let step = step.min(max_seconds).max(2.0);
        return (0..count)
            .map(|index| {
                let start = index as f64 * step;
                (start, (start + step).min(media_duration))
            })
            .filter(|(start, end)| end - start >= 2.0)
            .collect();
    }

    let mut windows: Vec<(f64, f64)> = Vec::new();
    let mut index = 0usize;
    while index < pieces.len() {
        let (start, mut end) = pieces[index];
        let mut next = index + 1;
        while next < pieces.len()
            && end - start < target_seconds
            && pieces[next].0 - end <= join_gap
            && pieces[next].1 - start <= max_seconds - 0.2
        {
            end = pieces[next].1;
            next += 1;
        }
        windows.push((start, end.min(start + MAX_CLIP_SECONDS)));
        index = next;
    }

    // Fold a runt tail into the previous window when it fits; otherwise drop it
    // rather than shipping a three-second "clip" nobody will post.
    if windows.len() >= 2 {
        let last = *windows.last().unwrap();
        if last.1 - last.0 < min_seconds {
            let previous = windows.len() - 2;
            if last.1 - windows[previous].0 <= max_seconds - 0.2 {
                windows[previous].1 = last.1;
                windows.pop();
            } else {
                windows.pop();
            }
        }
    }
    windows
}

fn score_window(
    start: f64,
    end: f64,
    speech: &[SpeechSegment],
    transcript: &Transcript,
    niche: Niche,
    profile: &crate::trends::TrendProfile,
    peak_db: f32,
) -> (f32, Vec<String>, String) {
    let duration = (end - start).max(0.1);
    let speech_overlap: f64 = speech
        .iter()
        .map(|segment| (segment.end.min(end) - segment.start.max(start)).max(0.0))
        .sum();
    let density = clamp(speech_overlap / duration, 0.0, 1.0);
    let text = transcript.text_between(start, end);
    let opening = transcript.text_between(start, start + 8.0);
    let lower = text.to_ascii_lowercase();
    let opening_lower = opening.to_ascii_lowercase();
    let word_count = transcript.words_between(start, end).len();
    let pace = word_count as f64 / duration;

    let mut score = 28.0f32;
    let mut reasons: Vec<String> = Vec::new();

    if (18.0..=52.0).contains(&duration) {
        score += 9.0;
        reasons.push("platform-length".into());
    }
    if density >= 0.72 {
        score += 8.0;
        reasons.push("almost no dead air".into());
    } else if density < 0.4 {
        score -= 6.0;
        reasons.push("loose pacing".into());
    }
    if peak_db > -24.0 {
        score += 5.0;
        reasons.push("high energy".into());
    }
    if opening_lower.contains('?') {
        score += 6.0;
        reasons.push("opens with a question".into());
    }
    let keyword_hits = niche
        .keywords()
        .iter()
        .filter(|keyword| lower.contains(**keyword))
        .count();
    if keyword_hits > 0 {
        score += (keyword_hits.min(3) as f32) * 3.0;
        reasons.push(format!("{keyword_hits} niche hook words"));
    }
    if ["haha", "laugh", "applause", "crowd"]
        .iter()
        .any(|marker| lower.contains(marker))
    {
        score += 5.0;
        reasons.push("reaction in the clip".into());
    }
    if ["never", "nobody", "the truth", "how ", "why ", "actually"]
        .iter()
        .any(|opener| opening_lower.starts_with(opener))
    {
        score += 4.0;
        reasons.push("strong opening line".into());
    }
    if (2.2..=5.2).contains(&pace) {
        score += 5.0;
    }
    if duration > profile.max_seconds {
        score -= 8.0;
    }

    // A clip should not open or close on half a word.
    let padded_start = (start - 0.06).max(0.0);
    let hook = if opening.trim().is_empty() {
        format!(
            "the {} moment at {}",
            niche.label().to_ascii_lowercase(),
            crate::util::human_duration(padded_start)
        )
    } else {
        let mut hook: String = opening
            .split_whitespace()
            .take(11)
            .collect::<Vec<_>>()
            .join(" ");
        hook = hook.trim_end_matches([',', ';', ':']).to_string();
        if !hook.ends_with(['.', '!', '?']) {
            hook.push('…');
        }
        hook
    };

    if reasons.is_empty() {
        reasons.push("solid, unremarkable pacing".into());
    }
    (score.clamp(0.0, 100.0), reasons, hook)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{MediaInfo, Platform};
    use chrono::Utc;

    fn project(duration: f64) -> Project {
        let mut project = Project {
            schema_version: 1,
            id: Uuid::new_v4(),
            name: "test".into(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            niche: Niche::Podcast,
            platform: Platform::TikTok,
            source: None,
            media: Some(MediaInfo {
                duration,
                width: 1920,
                height: 1080,
                fps: 30.0,
                has_audio: true,
                ..Default::default()
            }),
            transcript: Transcript::default(),
            clips: Vec::new(),
            settings: Default::default(),
        };
        project.settings.target_clips = 3;
        project
    }

    fn segments(lengths: &[(f64, f64)]) -> Vec<SpeechSegment> {
        lengths
            .iter()
            .map(|(start, end)| SpeechSegment {
                start: *start,
                end: *end,
                mean_db: -20.0,
            })
            .collect()
    }

    #[test]
    fn clips_never_exceed_the_ceiling() {
        let project = project(600.0);
        let speech = segments(&[
            (0.0, 19.0),
            (20.0, 39.0),
            (40.0, 59.0),
            (60.0, 79.0),
            (80.0, 99.0),
            (100.0, 119.0),
        ]);
        let mut options = AnalyzeOptions::default();
        options.max_seconds = 90.0; // request more than allowed on purpose
        let outcome = plan_clips(&project, &speech, &options, &mut |_, _| {});
        assert!(!outcome.clips.is_empty());
        for clip in &outcome.clips {
            assert!(clip.duration() <= MAX_CLIP_SECONDS);
            assert!(clip.duration() >= 2.0);
            assert!(clip.score >= 0.0 && clip.score <= 100.0);
        }
        let mut sorted = outcome.clips.clone();
        sorted.sort_by(|a, b| a.start.partial_cmp(&b.start).unwrap());
        assert_eq!(
            sorted.first().unwrap().start,
            outcome.clips.first().unwrap().start
        );
    }

    #[test]
    fn a_five_minute_monologue_is_split_into_short_clips() {
        let project = project(180.0);
        let speech = segments(&[(0.0, 180.0)]);
        let mut options = AnalyzeOptions::default();
        options.target_clips = 20;
        let outcome = plan_clips(&project, &speech, &options, &mut |_, _| {});
        assert!(
            outcome.clips.len() >= 3,
            "got {} clips",
            outcome.clips.len()
        );
        for clip in &outcome.clips {
            assert!(clip.duration() <= MAX_CLIP_SECONDS + 1e-6);
        }
    }

    #[test]
    fn silence_still_yields_review_windows() {
        let project = project(120.0);
        let outcome = plan_clips(&project, &[], &AnalyzeOptions::default(), &mut |_, _| {});
        assert!(!outcome.clips.is_empty());
        assert!(outcome.clips.iter().all(|clip| clip.end <= 120.0));
    }

    #[test]
    fn transcript_hooks_and_captions_are_attached() {
        let mut project = project(60.0);
        project.transcript = Transcript {
            language: "en".into(),
            words: "nobody tells you the truth about money and it costs you years"
                .split_whitespace()
                .enumerate()
                .map(|(index, word)| crate::model::Word {
                    text: word.into(),
                    start: index as f64 * 0.5,
                    end: index as f64 * 0.5 + 0.45,
                    probability: 0.95,
                })
                .collect(),
            duration: 7.0,
        };
        let speech = segments(&[(0.0, 30.0)]);
        let outcome = plan_clips(
            &project,
            &speech,
            &AnalyzeOptions::default(),
            &mut |_, _| {},
        );
        let clip = outcome.clips.first().expect("clip");
        assert!(!clip.captions.is_empty());
        assert!(!clip.title.is_empty());
        assert!(clip.hook.to_lowercase().contains("nobody"));
    }
}
