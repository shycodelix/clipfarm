use crate::model::{Caption, Word};

/// Group word timestamps into readable, caption-safe lines.
pub fn build_captions(
    words: &[Word],
    max_chars: usize,
    max_seconds: f64,
    max_gap: f64,
) -> Vec<Caption> {
    let mut captions = Vec::new();
    let mut current: Vec<Word> = Vec::new();
    let mut char_count = 0usize;

    let flush = |captions: &mut Vec<Caption>, current: &mut Vec<Word>| {
        if current.is_empty() {
            return;
        }
        let start = current.first().map(|word| word.start).unwrap_or(0.0);
        let end = current.last().map(|word| word.end).unwrap_or(start);
        let text = current
            .iter()
            .map(|word| word.text.as_str())
            .collect::<Vec<_>>()
            .join(" ")
            .replace("  ", " ");
        captions.push(Caption {
            start,
            end,
            text,
            words: current.clone(),
        });
        current.clear();
    };

    for word in words {
        let cleaned = word.text.trim();
        if cleaned.is_empty() {
            continue;
        }
        let extra = cleaned.len() + usize::from(!current.is_empty());
        let too_long = char_count + extra > max_chars;
        let too_slow = current
            .last()
            .map(|last| word.start - last.end > max_gap)
            .unwrap_or(false);
        let too_tall = current
            .first()
            .map(|first| word.end - first.start > max_seconds)
            .unwrap_or(false);
        if !current.is_empty() && (too_long || too_slow || too_tall) {
            flush(&mut captions, &mut current);
            char_count = 0;
        }
        let mut word = word.clone();
        word.text = cleaned.to_string();
        char_count += word.text.len() + usize::from(!current.is_empty());
        // Sentence-ending punctuation always ends a caption.
        let ends_sentence =
            word.text.ends_with('.') || word.text.ends_with('?') || word.text.ends_with('!');
        current.push(word);
        if ends_sentence {
            flush(&mut captions, &mut current);
            char_count = 0;
        }
    }
    flush(&mut captions, &mut current);
    captions
}

/// Rebuild captions after words changed (for example filler removal).
pub fn rebuild(words: &[Word]) -> Vec<Caption> {
    build_captions(words, 42, 3.6, 0.5)
}

/// Remove disfluencies from the transcript used for captions. The video itself
/// keeps its original audio; this only cleans the words we display and the text
/// we score.
pub fn remove_fillers(words: &[Word]) -> Vec<Word> {
    const FILLERS: [&str; 8] = ["um", "uh", "erm", "uhh", "umm", "hmm", "mm", "er"];
    words
        .iter()
        .filter(|word| {
            let cleaned = word
                .text
                .trim()
                .trim_matches(|c: char| !c.is_alphanumeric())
                .to_ascii_lowercase();
            !FILLERS.contains(&cleaned.as_str())
        })
        .cloned()
        .collect()
}

/// Render an SRT for a clip. Captions are stored in source time; `offset` is the
/// clip start, so exported subtitles begin at zero.
pub fn to_srt(captions: &[Caption], offset: f64) -> String {
    let mut out = String::new();
    for (index, caption) in captions.iter().enumerate() {
        let start = (caption.start - offset).max(0.0);
        let end = (caption.end - offset).max(start + 0.2);
        out.push_str(&format!(
            "{}\n{} --> {}\n{}\n\n",
            index + 1,
            timestamp(start),
            timestamp(end),
            caption.text
        ));
    }
    out
}

pub fn timestamp(seconds: f64) -> String {
    let total_ms = (seconds.max(0.0) * 1000.0).round() as u64;
    let ms = total_ms % 1000;
    let total_s = total_ms / 1000;
    let s = total_s % 60;
    let m = (total_s / 60) % 60;
    let h = total_s / 3600;
    format!("{h:02}:{m:02}:{s:02},{ms:03}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(items: &[(&str, f64, f64)]) -> Vec<Word> {
        items
            .iter()
            .map(|(text, start, end)| Word {
                text: text.to_string(),
                start: *start,
                end: *end,
                probability: 1.0,
            })
            .collect()
    }

    #[test]
    fn groups_words_into_short_lines() {
        let input = words(&[
            ("This", 0.0, 0.3),
            ("is", 0.3, 0.5),
            ("a", 0.5, 0.6),
            ("sentence.", 0.6, 1.1),
            ("And", 1.2, 1.4),
            ("another", 1.4, 1.8),
            ("one.", 1.8, 2.1),
        ]);
        let captions = build_captions(&input, 42, 3.6, 0.5);
        assert_eq!(captions.len(), 2);
        assert_eq!(captions[0].text, "This is a sentence.");
        assert_eq!(captions[1].text, "And another one.");
    }

    #[test]
    fn filler_removal_leaves_meaning_alone() {
        let input = words(&[
            ("Um", 0.0, 0.2),
            ("this", 0.2, 0.4),
            ("like", 0.4, 0.6),
            ("uh", 0.6, 0.8),
            ("works", 0.8, 1.0),
        ]);
        let cleaned = remove_fillers(&input);
        let text = cleaned
            .iter()
            .map(|word| word.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        assert_eq!(text, "this like works");
    }

    #[test]
    fn srt_is_offset_and_monotonic() {
        let captions = vec![Caption {
            start: 10.0,
            end: 11.25,
            text: "hello".into(),
            words: vec![],
        }];
        let srt = to_srt(&captions, 10.0);
        assert!(srt.contains("00:00:00,000 --> 00:00:01,250"));
        assert!(srt.contains("hello"));
    }
}
