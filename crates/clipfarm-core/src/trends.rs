use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::model::{CaptionStyle, Niche, Platform};
use crate::project::data_dir;

/// A trend profile is the editing brief clipfarm follows: how fast to cut, how
/// long a clip should be, which hooks are native to the feed, and what hashtags
/// travel with it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrendProfile {
    pub id: String,
    pub niche: Niche,
    pub platform: Platform,
    pub min_seconds: f64,
    pub max_seconds: f64,
    pub cut_every: f64,
    pub caption_style: CaptionStyle,
    pub hook_patterns: Vec<String>,
    pub hashtags: Vec<String>,
    pub notes: String,
    /// Where this profile came from ("bundled" or a local feed).
    pub source: String,
}

/// Bundled profiles are opinionated starting points, not scraped trend data.
/// They encode pacing that works for a niche without pretending to know today's
/// meme. A local feed can override them (see `load_override`).
pub fn profile(niche: Niche, platform: Platform) -> TrendProfile {
    let (cut_every, min_seconds, max_seconds, style, niche_tags, notes): (
        f64,
        f64,
        f64,
        CaptionStyle,
        Vec<&str>,
        &str,
    ) = match niche {
        Niche::Podcast => (
            6.0,
            22.0,
            55.0,
            CaptionStyle::Clean,
            vec!["podcast", "interview", "storytime"],
            "Let the answer land. Cut on breaths, not beats. Hook with the claim, not the intro.",
        ),
        Niche::Gaming => (
            2.2,
            18.0,
            42.0,
            CaptionStyle::Bold,
            vec!["gaming", "gameplay", "clutch"],
            "Open on the action frame. Punch-ins every two seconds; captions shout the callout.",
        ),
        Niche::Fitness => (
            3.4,
            18.0,
            45.0,
            CaptionStyle::Bold,
            vec!["fitness", "gymtok", "formcheck"],
            "Lead with the mistake or the result. One cue per cut, no circuit-style montage.",
        ),
        Niche::Tech => (
            4.8,
            22.0,
            58.0,
            CaptionStyle::Clean,
            vec!["tech", "coding", "buildinpublic"],
            "Show the artifact, then the tradeoff. Keep screen text readable at phone size.",
        ),
        Niche::Comedy => (
            3.6,
            12.0,
            40.0,
            CaptionStyle::Karaoke,
            vec!["comedy", "funny", "standup"],
            "Do not step on the laugh. Cut after the reaction, never before the punchline.",
        ),
        Niche::Education => (
            5.5,
            20.0,
            55.0,
            CaptionStyle::Clean,
            vec!["learnontiktok", "explained", "howto"],
            "One idea per clip. Put the payoff in the first three seconds, then earn it.",
        ),
        Niche::Generic => (
            4.0,
            18.0,
            50.0,
            CaptionStyle::Bold,
            vec!["clips", "highlights", "creator"],
            "Start on the most specific sentence in the clip.",
        ),
    };

    let (min_seconds, max_seconds, platform_tags) = match platform {
        Platform::YoutubeShorts => (
            min_seconds.max(20.0),
            max_seconds.min(58.0),
            vec!["shorts", "youtubeshorts"],
        ),
        Platform::TikTok => (
            min_seconds.max(15.0),
            max_seconds.min(45.0),
            vec!["tiktok", "fyp"],
        ),
        Platform::InstagramReels => (
            min_seconds.max(18.0),
            max_seconds.min(50.0),
            vec!["reels", "instareels"],
        ),
    };

    let mut hashtags: Vec<String> = platform_tags.into_iter().map(String::from).collect();
    hashtags.extend(niche_tags.into_iter().map(String::from));

    TrendProfile {
        id: format!("{}-{}", niche.slug(), platform.slug()),
        niche,
        platform,
        min_seconds,
        max_seconds,
        cut_every,
        caption_style: style,
        hook_patterns: hook_patterns(niche).into_iter().map(String::from).collect(),
        hashtags,
        notes: notes.to_string(),
        source: "bundled".into(),
    }
}

fn hook_patterns(niche: Niche) -> [&'static str; 3] {
    match niche {
        Niche::Podcast => [
            "the part where {topic}",
            "nobody warns you about {topic}",
            "{topic}, and it changed everything",
        ],
        Niche::Gaming => [
            "this {topic} play should not work",
            "the {topic} that broke the lobby",
            "{topic} but make it unhinged",
        ],
        Niche::Fitness => [
            "your {topic} is lying to you",
            "fix {topic} in one set",
            "the {topic} mistake everyone makes",
        ],
        Niche::Tech => [
            "{topic}, but measure it first",
            "the {topic} tradeoff nobody benchmarks",
            "i shipped {topic} so you don't have to",
        ],
        Niche::Comedy => [
            "{topic} but it escalates",
            "the {topic} bit that broke me",
            "explaining {topic} with a straight face",
        ],
        Niche::Education => [
            "{topic}, explained in one breath",
            "the {topic} trick you missed",
            "stop memorizing {topic}",
        ],
        Niche::Generic => [
            "the {topic} moment",
            "{topic}, no context",
            "this is your sign to watch {topic}",
        ],
    }
}

pub fn all() -> Vec<TrendProfile> {
    let mut profiles = Vec::with_capacity(Niche::ALL.len() * Platform::ALL.len());
    for niche in Niche::ALL {
        for platform in Platform::ALL {
            profiles.push(resolve(niche, platform));
        }
    }
    profiles
}

/// Users (or a scheduled trend script) can drop a JSON array of profiles at
/// `<data>/trends.json`. Any matching niche+platform wins over the bundled one.
pub fn load_override() -> Option<Vec<TrendProfile>> {
    let path = data_dir().join("trends.json");
    let text = std::fs::read_to_string(path).ok()?;
    let profiles: Vec<TrendProfile> = serde_json::from_str(&text).ok()?;
    if profiles.is_empty() {
        None
    } else {
        Some(profiles)
    }
}

pub fn resolve(niche: Niche, platform: Platform) -> TrendProfile {
    if let Some(profiles) = load_override() {
        if let Some(found) = profiles
            .into_iter()
            .find(|candidate| candidate.niche == niche && candidate.platform == platform)
        {
            return found;
        }
    }
    profile(niche, platform)
}

pub fn save_override(profiles: &[TrendProfile]) -> Result<()> {
    let path = data_dir().join("trends.json");
    let json = serde_json::to_string_pretty(profiles)?;
    crate::util::write_atomic(&path, &json)
        .with_context(|| format!("Could not write {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_combo_fits_the_ceiling() {
        for profile in all() {
            assert!(profile.max_seconds <= 60.0);
            assert!(profile.min_seconds < profile.max_seconds);
            assert!(!profile.hook_patterns.is_empty());
            assert!(profile.hashtags.len() >= 3);
        }
    }

    #[test]
    fn tiktok_is_shorter_than_shorts() {
        let shorts = profile(Niche::Podcast, Platform::YoutubeShorts);
        let tiktok = profile(Niche::Podcast, Platform::TikTok);
        assert!(tiktok.max_seconds <= shorts.max_seconds);
    }
}
