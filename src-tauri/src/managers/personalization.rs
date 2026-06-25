//! On-device personalization mining (issue #16, Tier 1).
//!
//! Deterministic, fully local extraction of likely custom-vocabulary words from the user's own
//! transcript history. The goal is *high precision* one-tap suggestions: it is better to suggest a
//! handful of obviously-correct proper nouns than to flood the user with noise. So the gate is
//! intentionally conservative -- a candidate must be a recurring proper-noun-like token (seen
//! capitalized in a non-sentence-initial position), not an everyday English word, and frequent
//! enough to matter. Nothing here touches the network or any model; it is pure string analysis.

use crate::audio_toolkit::is_common_word;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::{HashMap, HashSet};

/// Minimum number of occurrences across history before a word is suggested.
const MIN_FREQUENCY: u32 = 3;
/// Minimum length (in chars) for a candidate word.
const MIN_WORD_LEN: usize = 3;
/// Maximum length (in chars) for a candidate word. Matches the manual custom-word cap so learned
/// words never bypass the length validation users get when adding a word by hand.
const MAX_WORD_LEN: usize = 50;

/// A mined vocabulary suggestion: a word the user frequently dictates, with its occurrence count.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Type)]
pub struct WordSuggestion {
    /// The suggested surface form (the most frequent capitalized spelling seen in history).
    pub word: String,
    /// How many times the word appears across the mined transcripts.
    pub count: u32,
}

/// Per-word accumulator built while scanning transcripts.
#[derive(Default)]
struct Candidate {
    /// Surface form -> occurrence count, used to pick the spelling to suggest.
    forms: HashMap<String, u32>,
    /// Total occurrences (case-insensitive).
    total: u32,
    /// Seen capitalized in a non-sentence-initial position -- the proper-noun signal.
    proper_noun: bool,
}

/// Split `text` into words, tagging each with whether it begins a sentence.
///
/// A word is sentence-initial if it is the first word of the text or the first word after a
/// sentence terminator (`.`, `?`, `!`). Words are maximal runs of alphanumerics with internal
/// apostrophes/hyphens (e.g. "don't", "well-known"); leading/trailing `'`/`-` are trimmed.
fn tokenize_with_sentence_flags(text: &str) -> Vec<(String, bool)> {
    let mut out = Vec::new();
    let mut current = String::new();
    // True at text start and after any sentence terminator; consumed when a word ends.
    let mut at_sentence_start = true;

    let flush = |current: &mut String, out: &mut Vec<(String, bool)>, start: bool| {
        if current.is_empty() {
            return;
        }
        let trimmed = current.trim_matches(|c| c == '\'' || c == '-');
        if !trimmed.is_empty() {
            out.push((trimmed.to_string(), start));
        }
        current.clear();
    };

    for ch in text.chars() {
        if ch.is_alphanumeric() || ((ch == '\'' || ch == '-') && !current.is_empty()) {
            current.push(ch);
        } else {
            // Only a finished word clears the sentence-start flag. Whitespace and other separators
            // between a terminator and the next word must NOT reset it, otherwise a word that
            // genuinely starts a sentence (after ". ") would be misread as mid-sentence and could be
            // mistaken for a proper noun.
            if !current.is_empty() {
                flush(&mut current, &mut out, at_sentence_start);
                at_sentence_start = false;
            }
            if ch == '.' || ch == '?' || ch == '!' {
                at_sentence_start = true;
            }
        }
    }
    flush(&mut current, &mut out, at_sentence_start);
    out
}

/// Whether the token is made up only of digits (with optional internal `-`/`'`), e.g. "2024".
fn is_numeric_token(word: &str) -> bool {
    word.chars()
        .all(|c| c.is_numeric() || c == '-' || c == '\'')
        && word.chars().any(|c| c.is_numeric())
}

fn first_is_uppercase(word: &str) -> bool {
    word.chars().next().is_some_and(|c| c.is_uppercase())
}

/// Pick the surface form to suggest: prefer a capitalized spelling, then the most frequent, then
/// the lexicographically smallest (for determinism).
fn pick_surface_form(forms: &HashMap<String, u32>) -> String {
    let mut entries: Vec<(&String, &u32)> = forms.iter().collect();
    entries.sort_by(|a, b| {
        first_is_uppercase(b.0)
            .cmp(&first_is_uppercase(a.0))
            .then(b.1.cmp(a.1))
            .then(a.0.cmp(b.0))
    });
    entries
        .first()
        .map(|(form, _)| (*form).clone())
        .unwrap_or_default()
}

/// Mine likely custom-vocabulary suggestions from `texts`.
///
/// - `exclude_lower`: lowercased words to skip (already in the dictionary, learned, or dismissed).
/// - `limit`: maximum number of suggestions to return.
///
/// Returns suggestions ranked by frequency (desc), then alphabetically, capped at `limit`.
pub fn mine_word_suggestions(
    texts: &[String],
    exclude_lower: &HashSet<String>,
    limit: usize,
) -> Vec<WordSuggestion> {
    if limit == 0 {
        return Vec::new();
    }

    let mut candidates: HashMap<String, Candidate> = HashMap::new();

    for text in texts {
        for (word, sentence_initial) in tokenize_with_sentence_flags(text) {
            let lower = word.to_lowercase();
            let len = lower.chars().count();
            if len < MIN_WORD_LEN
                || len > MAX_WORD_LEN
                || is_numeric_token(&word)
                || is_common_word(&lower)
                || exclude_lower.contains(&lower)
            {
                continue;
            }

            let entry = candidates.entry(lower).or_default();
            entry.total += 1;
            *entry.forms.entry(word.clone()).or_insert(0) += 1;
            if first_is_uppercase(&word) && !sentence_initial {
                entry.proper_noun = true;
            }
        }
    }

    let mut result: Vec<WordSuggestion> = candidates
        .into_iter()
        .filter(|(_, c)| c.total >= MIN_FREQUENCY && c.proper_noun)
        .map(|(_, c)| WordSuggestion {
            word: pick_surface_form(&c.forms),
            count: c.total,
        })
        .collect();

    result.sort_by(|a, b| {
        b.count
            .cmp(&a.count)
            .then_with(|| a.word.to_lowercase().cmp(&b.word.to_lowercase()))
    });
    result.truncate(limit);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn no_exclude() -> HashSet<String> {
        HashSet::new()
    }

    #[test]
    fn suggests_frequent_proper_nouns() {
        // "Codex" appears 3x mid-sentence, capitalized -> a proper-noun suggestion.
        let texts = vec![
            "I asked Codex to review it.".to_string(),
            "Then Codex found a bug.".to_string(),
            "Finally Codex shipped it.".to_string(),
        ];
        let out = mine_word_suggestions(&texts, &no_exclude(), 10);
        assert_eq!(out.len(), 1, "expected exactly one suggestion: {out:?}");
        assert_eq!(out[0].word, "Codex");
        assert_eq!(out[0].count, 3);
    }

    #[test]
    fn honors_minimum_frequency() {
        // Only 2 occurrences -> below MIN_FREQUENCY (3).
        let texts = vec![
            "Ask Codex about it.".to_string(),
            "Codex knows.".to_string(),
        ];
        assert!(mine_word_suggestions(&texts, &no_exclude(), 10).is_empty());
    }

    #[test]
    fn vetoes_common_words() {
        // "There" is capitalized mid-sentence but is an everyday word -> never suggested.
        let texts = vec![
            "Look There it goes.".to_string(),
            "Stop There right now.".to_string(),
            "Wait There a moment.".to_string(),
        ];
        assert!(mine_word_suggestions(&texts, &no_exclude(), 10).is_empty());
    }

    #[test]
    fn excludes_sentence_initial_only_capitalization() {
        // "Tomorrow" is only ever capitalized as the first word of a sentence -> not a proper noun.
        let texts = vec![
            "Tomorrow we ship.".to_string(),
            "Tomorrow it rains.".to_string(),
            "Tomorrow we rest.".to_string(),
        ];
        assert!(mine_word_suggestions(&texts, &no_exclude(), 10).is_empty());
    }

    #[test]
    fn excludes_sentence_start_after_midtext_terminator() {
        // "Tomorrow" only ever appears right after a sentence terminator inside the text. It must be
        // treated as sentence-initial (not a proper noun) even mid-string -- the separator between
        // the terminator and the word must not reset the sentence-start flag.
        let texts =
            vec!["We ship today. Tomorrow we rest. Tomorrow we plan. Tomorrow we go.".to_string()];
        assert!(mine_word_suggestions(&texts, &no_exclude(), 10).is_empty());
    }

    #[test]
    fn rejects_overlong_tokens() {
        // A repeated capitalized token longer than the manual custom-word cap must not be suggested.
        let long = "Supercalifragilisticexpialidocious".repeat(2); // 68 chars > MAX_WORD_LEN
        let texts = vec![
            format!("We love {long} here."),
            format!("We love {long} there."),
            format!("We love {long} daily."),
        ];
        assert!(mine_word_suggestions(&texts, &no_exclude(), 10).is_empty());
    }

    #[test]
    fn respects_exclude_set() {
        let texts = vec![
            "I asked Codex to review it.".to_string(),
            "Then Codex found a bug.".to_string(),
            "Finally Codex shipped it.".to_string(),
        ];
        let mut exclude = HashSet::new();
        exclude.insert("codex".to_string());
        assert!(mine_word_suggestions(&texts, &exclude, 10).is_empty());
    }

    #[test]
    fn ranks_by_frequency_then_alphabetically() {
        let mut texts = Vec::new();
        // Kubernetes capitalized mid-sentence 5x.
        for _ in 0..5 {
            texts.push("We deployed Kubernetes today.".to_string());
        }
        // Grafana capitalized mid-sentence 3x.
        for _ in 0..3 {
            texts.push("We checked Grafana again.".to_string());
        }
        let out = mine_word_suggestions(&texts, &no_exclude(), 10);
        assert_eq!(out.len(), 2, "{out:?}");
        assert_eq!(out[0].word, "Kubernetes");
        assert_eq!(out[0].count, 5);
        assert_eq!(out[1].word, "Grafana");
        assert_eq!(out[1].count, 3);
    }

    #[test]
    fn truncates_to_limit() {
        let texts = vec![
            "Use Alpha then Bravo then Charlie now.".to_string(),
            "Use Alpha then Bravo then Charlie again.".to_string(),
            "Use Alpha then Bravo then Charlie today.".to_string(),
        ];
        let out = mine_word_suggestions(&texts, &no_exclude(), 2);
        assert_eq!(out.len(), 2, "limit should cap results: {out:?}");
    }

    #[test]
    fn zero_limit_returns_empty() {
        let texts = vec!["Codex Codex Codex are mid sentence here.".to_string()];
        assert!(mine_word_suggestions(&texts, &no_exclude(), 0).is_empty());
    }

    #[test]
    fn ignores_pure_numbers() {
        let texts = vec![
            "We shipped 2024 builds.".to_string(),
            "We shipped 2024 builds.".to_string(),
            "We shipped 2024 builds.".to_string(),
        ];
        assert!(mine_word_suggestions(&texts, &no_exclude(), 10).is_empty());
    }

    #[test]
    fn prefers_capitalized_surface_form() {
        // Mixed casing: the capitalized spelling should be the suggested surface form.
        let texts = vec![
            "We use Grafana here.".to_string(),
            "We use Grafana there.".to_string(),
            "We use grafana sometimes.".to_string(),
            "We use Grafana daily.".to_string(),
        ];
        let out = mine_word_suggestions(&texts, &no_exclude(), 10);
        assert_eq!(out.len(), 1, "{out:?}");
        assert_eq!(out[0].word, "Grafana");
        assert_eq!(out[0].count, 4, "count is case-insensitive");
    }

    #[test]
    fn keeps_alphanumeric_tokens_together() {
        // A versioned product name keeps its digits (tokenizer allows internal alphanumerics).
        let texts = vec![
            "Deploy Parakeet3 to prod.".to_string(),
            "Deploy Parakeet3 to stage.".to_string(),
            "Deploy Parakeet3 to dev.".to_string(),
        ];
        let out = mine_word_suggestions(&texts, &no_exclude(), 10);
        assert_eq!(out.len(), 1, "{out:?}");
        assert_eq!(out[0].word, "Parakeet3");
    }
}
