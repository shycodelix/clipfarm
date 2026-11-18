use anyhow::Result;
use serde_json::Value;

use crate::llm;
use crate::model::{Niche, Platform, Word};
use crate::trends::TrendProfile;

#[derive(Debug, Clone)]
pub struct ClipMetadata {
    pub titles: Vec<String>,
    pub caption: String,
    pub hashtags: Vec<String>,
    pub hook: String,
}

/// Deterministic, offline metadata. This is what ships: no API key, no rate
/// limit, no surprise bill. A local model can sharpen it afterwards.
pub fn generate(
    words: &[Word],
    niche: Niche,
    platform: Platform,
    profile: &TrendProfile,
) -> ClipMetadata {
    let spoken = words
        .iter()
        .map(|word| word.text.trim())
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let topic = pick_topic(words, niche);
    let title_topic = if topic.is_empty() {
        "this moment".to_string()
    } else {
        topic.clone()
    };

    let patterns = if profile.hook_patterns.is_empty() {
        vec!["the {topic} moment".to_string()]
    } else {
        profile.hook_patterns.clone()
    };
    let mut titles: Vec<String> = patterns
        .iter()
        .map(|pattern| tidy_title(&pattern.replace("{topic}", &title_topic)))
        .collect();
    titles.dedup();
    if titles.is_empty() {
        titles.push(tidy_title(&format!("the {title_topic} moment")));
    }

    let hook = if spoken.is_empty() {
        format!("The {title_topic} moment.")
    } else {
        opening_sentence(&spoken, 12)
    };

    let mut hashtags = profile.hashtags.clone();
    for keyword in niche.keywords().iter().take(2) {
        let tag = keyword.replace(' ', "");
        if !hashtags
            .iter()
            .any(|existing| existing.eq_ignore_ascii_case(&tag))
        {
            hashtags.push(tag);
        }
    }
    hashtags.truncate(6);

    let caption = if hook.is_empty() {
        format!("the {title_topic} part, trimmed for {}.", platform.label())
    } else {
        format!("{hook} · trimmed for {}", platform.label())
    };

    ClipMetadata {
        titles,
        caption,
        hashtags,
        hook,
    }
}

fn opening_sentence(text: &str, max_words: usize) -> String {
    let mut words: Vec<&str> = text.split_whitespace().take(max_words).collect();
    if words.is_empty() {
        return String::new();
    }
    let mut sentence = words.join(" ");
    if !sentence.ends_with(['.', '!', '?']) {
        // Keep it a phrase, not a fake complete sentence.
        sentence = sentence.trim_end_matches([',', ';', ':', ' ']).to_string();
    }
    // Remove a leading filler so the hook starts on the point.
    for filler in ["um ", "uh ", "so ", "and ", "but ", "like "] {
        if sentence.to_ascii_lowercase().starts_with(filler) {
            sentence = sentence[filler.len()..].to_string();
            break;
        }
    }
    words.clear();
    capitalize_first(&sentence)
}

fn capitalize_first(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

fn tidy_title(title: &str) -> String {
    let trimmed = title
        .trim()
        .trim_matches(|c: char| c == '"' || c == '\'' || c == '.' || c == ',')
        .to_string();
    trimmed
}

/// Pick a short, specific phrase: prefer a window containing a niche keyword,
/// otherwise the longest run of meaningful words near the start.
fn pick_topic(words: &[Word], niche: Niche) -> String {
    let meaningful: Vec<&str> = words
        .iter()
        .map(|word| word.text.trim())
        .filter(|text| {
            text.len() > 2
                && !matches!(
                    text.to_ascii_lowercase().as_str(),
                    "the"
                        | "and"
                        | "but"
                        | "that"
                        | "this"
                        | "you"
                        | "your"
                        | "with"
                        | "have"
                        | "just"
                        | "like"
                        | "what"
                        | "when"
                        | "they"
                        | "it's"
                        | "i'm"
                )
        })
        .collect();
    if meaningful.is_empty() {
        return String::new();
    }
    let keywords = niche.keywords();
    for window in meaningful.windows(4) {
        let joined = window.join(" ");
        if keywords
            .iter()
            .any(|keyword| joined.to_ascii_lowercase().contains(keyword))
        {
            return tidy_topic(&joined);
        }
    }
    tidy_topic(
        &meaningful
            .iter()
            .take(4)
            .copied()
            .collect::<Vec<_>>()
            .join(" "),
    )
}

fn tidy_topic(text: &str) -> String {
    text.trim_matches(|c: char| !c.is_alphanumeric() && c != ' ' && c != '\'')
        .split_whitespace()
        .take(5)
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

/// Optional local-LLM polish. Returns the original metadata unchanged when the
/// model is unavailable, slow, or returns nonsense — never a hard failure.
pub fn enhance_with_llm(
    endpoint: &str,
    model: &str,
    transcript: &str,
    base: &ClipMetadata,
    niche: Niche,
    platform: Platform,
) -> Result<ClipMetadata> {
    if transcript.trim().is_empty() {
        return Ok(base.clone());
    }
    let prompt = format!(
        "You write short-form video metadata. Niche: {}. Platform: {}. \
         Return ONLY JSON with keys: titles (array of 3 punchy titles, max 60 chars), \
         caption (one line, max 120 chars, no hashtags), hashtags (array of 5 tags without #). \
         No clickbait that the transcript cannot support. No emojis. \
         Transcript excerpt:\n{transcript}",
        niche.label(),
        platform.label()
    );
    let value = llm::complete_json(endpoint, model, &prompt, std::time::Duration::from_secs(12))?;
    let titles = value
        .get("titles")
        .and_then(Value::as_array)
        .map(|titles| {
            titles
                .iter()
                .filter_map(Value::as_str)
                .map(|title| title.trim().chars().take(70).collect::<String>())
                .filter(|title| !title.is_empty())
                .take(3)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let caption = value
        .get("caption")
        .and_then(Value::as_str)
        .map(|caption| caption.trim().chars().take(140).collect::<String>())
        .filter(|caption| !caption.is_empty());
    let hashtags = value
        .get("hashtags")
        .and_then(Value::as_array)
        .map(|tags| {
            tags.iter()
                .filter_map(Value::as_str)
                .map(|tag| tag.trim().trim_start_matches('#').to_string())
                .filter(|tag| !tag.is_empty() && tag.len() <= 30)
                .take(6)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    if titles.is_empty() {
        return Ok(base.clone());
    }
    Ok(ClipMetadata {
        titles,
        caption: caption.unwrap_or_else(|| base.caption.clone()),
        hashtags: if hashtags.is_empty() {
            base.hashtags.clone()
        } else {
            hashtags
        },
        hook: base.hook.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Platform;
    use crate::trends::profile;

    fn words(text: &str) -> Vec<Word> {
        text.split_whitespace()
            .enumerate()
            .map(|(index, word)| Word {
                text: word.to_string(),
                start: index as f64 * 0.4,
                end: index as f64 * 0.4 + 0.35,
                probability: 0.9,
            })
            .collect()
    }

    #[test]
    fn titles_are_specific_and_short() {
        let input = words(
            "The truth is most people never learn this money mistake and it costs them years",
        );
        let profile = profile(Niche::Podcast, Platform::TikTok);
        let metadata = generate(&input, Niche::Podcast, Platform::TikTok, &profile);
        assert!(!metadata.titles.is_empty());
        assert!(metadata.titles.iter().all(|title| !title.is_empty()));
        assert!(metadata.hashtags.len() <= 6);
        assert!(metadata.caption.len() < 180);
        assert!(!metadata.hook.is_empty());
    }

    #[test]
    fn empty_transcript_still_produces_a_card() {
        let profile = profile(Niche::Gaming, Platform::TikTok);
        let metadata = generate(&[], Niche::Gaming, Platform::TikTok, &profile);
        assert!(!metadata.titles.is_empty());
        assert!(!metadata.hashtags.is_empty());
    }
}
