use crate::settings::WordReplacement;
use once_cell::sync::Lazy;
use regex::{Captures, Regex};
use rphonetic::{DoubleMetaphone, Encoder, Metaphone};
use std::collections::{HashMap, HashSet};
use strsim::{damerau_levenshtein, jaro_winkler};
use unicode_normalization::char::is_combining_mark;
use unicode_normalization::UnicodeNormalization;

/// Common English words used as a "do not overwrite a common word" veto in the matcher.
/// Loaded once from the bundled list; lines starting with `#` are provenance/comments.
static COMMON_WORDS: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    include_str!("common_words_en.txt")
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect()
});

/// Whether `word` is an everyday English word in the bundled common-word list. Backs both the
/// fuzzy matcher's common-word veto and history mining (issue #16), which uses it to avoid
/// suggesting ordinary words. Comparison is case-insensitive.
pub fn is_common_word(word: &str) -> bool {
    COMMON_WORDS.contains(word.to_lowercase().as_str())
}

/// Lowercase dictionary words and contractions used only by correction capture. Unlike
/// [`COMMON_WORDS`], this is intentionally broad: two valid English forms are too ambiguous to
/// turn into a permanent global replacement. Capitalized names and brands are excluded when the
/// bundled list is generated, so a correction such as `clawed` -> `Claude` remains learnable.
static ENGLISH_WORDS: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    include_str!("english_words_en.txt")
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect()
});

fn is_known_english_word(word: &str) -> bool {
    let normalized = word.to_lowercase().replace('\u{2019}', "'");
    ENGLISH_WORDS.contains(normalized.as_str())
}

/// Lowercased forms of capitalized entries from the same bundled SCOWL source. Kept separate from
/// [`ENGLISH_WORDS`] so a common-word mishear can still learn a name (`clawed` -> `Claude`), while
/// semantic name/calendar swaps are rejected even when raw mode lowercases them.
static ENGLISH_NAMED_ENTITIES: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    include_str!("english_named_entities_en.txt")
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect()
});

fn is_known_english_named_entity(word: &str) -> bool {
    let normalized = word.to_lowercase().replace('\u{2019}', "'");
    ENGLISH_NAMED_ENTITIES.contains(normalized.as_str())
}

static METAPHONE: Lazy<Metaphone> = Lazy::new(Metaphone::default);
static DOUBLE_METAPHONE: Lazy<DoubleMetaphone> = Lazy::new(DoubleMetaphone::default);

fn has_phonetic_match(from: &str, to: &str) -> bool {
    // The encoders are English/ASCII algorithms. Discarding non-ASCII letters would make a
    // mixed-script change such as `αfoo` -> `βfoo` look like the identical pair `foo` -> `foo`.
    if from
        .chars()
        .chain(to.chars())
        .any(|c| c.is_alphabetic() && !c.is_ascii_alphabetic())
    {
        return false;
    }
    let from_alpha = alpha_only(from);
    let to_alpha = alpha_only(to);
    !from_alpha.is_empty()
        && !to_alpha.is_empty()
        && (METAPHONE.is_encoded_equals(&from_alpha, &to_alpha)
            || DOUBLE_METAPHONE.is_double_metaphone_equal(&from_alpha, &to_alpha, false)
            || DOUBLE_METAPHONE.is_double_metaphone_equal(&from_alpha, &to_alpha, true))
}

fn has_close_spelling_evidence(from: &str, to: &str) -> bool {
    // This extractor's dictionaries and phonetic evidence are English/ASCII. Treating a changed
    // non-ASCII letter as a one-edit typo can turn a mixed-script semantic change into a global
    // rule, especially when a long shared ASCII prefix inflates Jaro-Winkler.
    if from
        .chars()
        .chain(to.chars())
        .any(|c| c.is_alphabetic() && !c.is_ascii_alphabetic())
    {
        return false;
    }
    let alphabetic_key = |value: &str| -> String {
        value
            .chars()
            .filter(|c| c.is_alphabetic())
            .flat_map(char::to_lowercase)
            .collect()
    };
    let from_alpha = alphabetic_key(from);
    let to_alpha = alphabetic_key(to);
    !from_alpha.is_empty()
        && !to_alpha.is_empty()
        && damerau_levenshtein(&from_alpha, &to_alpha) <= 1
        && jaro_winkler(&from_alpha, &to_alpha) >= 0.90
}

fn has_strong_mishear_evidence(from: &str, to: &str) -> bool {
    has_close_spelling_evidence(from, to) || has_phonetic_match(from, to)
}

/// First alphanumeric character of a string, if any (used as a cheap match anchor).
fn first_alnum(s: &str) -> Option<char> {
    s.chars().find(|c| c.is_alphanumeric())
}

/// ASCII-alphabetic characters only, for phonetic encoding (digits/punctuation dropped).
fn alpha_only(s: &str) -> String {
    s.chars().filter(|c| c.is_ascii_alphabetic()).collect()
}

/// Length (in chars) of the common leading prefix of two strings.
fn common_prefix_len(a: &str, b: &str) -> usize {
    a.chars().zip(b.chars()).take_while(|(x, y)| x == y).count()
}

/// Builds an n-gram string by cleaning and concatenating words
///
/// Strips punctuation from each word, lowercases, and joins without spaces.
/// This allows matching "Charge B" against "ChargeBee".
fn build_ngram(words: &[&str]) -> String {
    words
        .iter()
        .map(|w| {
            w.trim_matches(|c: char| !c.is_alphanumeric())
                .to_lowercase()
        })
        .collect::<Vec<_>>()
        .concat()
}

/// Finds the best matching custom word for a candidate string.
///
/// Precision-first: a wrong correction makes dictation feel unsafe, so a fuzzy match is
/// only accepted when several independent signals agree. The candidate must clear, in order:
///   1. a length gate (reject wildly different lengths),
///   2. a first-alphanumeric anchor (same starting char),
///   3. a length-bucketed edit-distance floor (Damerau, so transpositions like
///      "wrold"/"world" still score well), tightened further by `threshold`,
///   4. a common-word veto (never overwrite an everyday English word with a rare
///      dictionary entry unless the match is near-exact),
///   5. two-of-N phonetic/similarity agreement (Metaphone, Double Metaphone, Jaro-Winkler).
///
/// Exact matches (after lowercasing/space-removal) bypass the fuzzy gate and win outright;
/// this is what recases brands the user dictated correctly (e.g. "codex" -> "Codex").
///
/// `threshold` is the legacy sensitivity dial: lowering it raises the edit-distance floor
/// (stricter); it can no longer loosen matching below the per-length floors.
///
/// # Returns
/// The best matching custom word and its score (lower is better), if any match was found.
fn find_best_match<'a>(
    candidate: &str,
    custom_words: &'a [String],
    custom_words_nospace: &[String],
    threshold: f64,
    multiword: bool,
) -> Option<(&'a String, f64)> {
    if candidate.is_empty() || candidate.chars().count() > 50 {
        return None;
    }

    let cand_len = candidate.chars().count();
    let cand_first = first_alnum(candidate);
    let cand_alpha = alpha_only(candidate);
    let cand_is_common = COMMON_WORDS.contains(candidate);

    let mut best_match: Option<&String> = None;
    let mut best_score = f64::MAX;

    for (i, target) in custom_words_nospace.iter().enumerate() {
        if target.is_empty() {
            continue;
        }

        // Exact match (after lowercasing/space-removal): accept immediately, best score.
        // This is the recasing path (e.g. "codex" -> "Codex") and bypasses every veto.
        if candidate == target {
            return Some((&custom_words[i], 0.0));
        }

        let target_len = target.chars().count();
        let max_len = cand_len.max(target_len) as f64;
        let min_len = cand_len.min(target_len);

        // 1. Length gate: reject wildly different lengths (with a 1-char floor so short
        //    words are not over-matched, e.g. "cat"/"chat").
        let len_diff = (cand_len as i32 - target_len as i32).abs() as f64;
        if len_diff > (max_len * 0.25).max(1.0) {
            continue;
        }

        // 2. First-alphanumeric anchor (salvaged from PR #20): kills "region"/"Legion".
        if cand_first != first_alnum(target) {
            continue;
        }

        // 2b. For multi-word n-grams, require a shared prefix so the first spoken word actually
        //     participates in the match. Without this "my mac book" fuzzily matches "MacBook"
        //     by suffix and swallows the leading "my". Exact matches already returned above, so
        //     this only constrains fuzzy multi-word matches; single-word transposition typos
        //     ("wrold"/"world") are unaffected because they are not multi-word.
        if multiword && common_prefix_len(candidate, target) < 2 {
            continue;
        }

        // 3. Length-bucketed edit-distance floor. Damerau-Levenshtein so a single adjacent
        //    transposition ("wrold"/"world") stays cheap. Lowering `threshold` tightens it.
        let dl = damerau_levenshtein(candidate, target);
        let lev_sim = 1.0 - (dl as f64 / max_len);
        let bucket_floor: f64 = if max_len <= 6.0 { 0.70 } else { 0.60 };
        let effective_floor = bucket_floor.max(1.0 - threshold * 3.0);
        if lev_sim < effective_floor {
            continue;
        }

        // 4. Common-word veto: never overwrite an everyday English word with a rare
        //    dictionary entry unless the match is near-exact ("really"/"rally", "cloud"/"Claude").
        if cand_is_common && lev_sim < 0.92 {
            continue;
        }

        // 5. Multi-signal agreement. Phonetic codes are require-agreement gates, not recall
        //    boosters, and only safe for candidates of length >= 4 (short words collide
        //    phonetically far too easily). Accept when two independent signals agree, or when
        //    string similarity alone is very high (jw >= 0.93): the latter recovers genuine
        //    typos like "wrold"/"world", where a transposition changes the phonetic skeleton
        //    (Metaphone makes the leading "wr" silent) and leaves edit distance as the only
        //    honest signal. The common-word veto above already protects real words.
        let jw = jaro_winkler(candidate, target);
        let mut signals = 0;
        if jw >= 0.90 {
            signals += 1;
        }
        if min_len >= 4 && !cand_alpha.is_empty() {
            let target_alpha = alpha_only(target);
            if !target_alpha.is_empty() {
                if METAPHONE.is_encoded_equals(&cand_alpha, &target_alpha) {
                    signals += 1;
                }
                if DOUBLE_METAPHONE.is_double_metaphone_equal(&cand_alpha, &target_alpha, false)
                    || DOUBLE_METAPHONE.is_double_metaphone_equal(&cand_alpha, &target_alpha, true)
                {
                    signals += 1;
                }
            }
        }
        if signals < 2 && jw < 0.93 {
            continue;
        }

        // Score is the absolute edit distance (lower is better). Using the absolute distance
        // rather than a length-normalized ratio lets the caller compare matches across n-gram
        // lengths fairly -- a longer n-gram cannot look "closer" merely by being longer.
        let score = dl as f64;
        if score < best_score {
            best_match = Some(&custom_words[i]);
            best_score = score;
        }
    }

    best_match.map(|m| (m, best_score))
}

/// Applies custom word corrections to transcribed text using fuzzy matching
///
/// This function corrects words in the input text by finding the best matches from a list
/// of custom words using the precision-first multi-signal gate in [`find_best_match`]
/// (Damerau-Levenshtein + Metaphone/Double Metaphone + Jaro-Winkler, with a common-word
/// veto). It also matches multi-word speech artifacts via n-grams (e.g. "Charge B" ->
/// "ChargeBee"), choosing the n-gram with the smallest absolute edit distance.
///
/// # Arguments
/// * `text` - The input text to correct
/// * `custom_words` - List of custom words to match against
/// * `threshold` - Maximum similarity score to accept (0.0 = exact match, 1.0 = any match)
///
/// # Returns
/// The corrected text with custom words applied
pub fn apply_custom_words(text: &str, custom_words: &[String], threshold: f64) -> String {
    if custom_words.is_empty() {
        return text.to_string();
    }

    // Pre-compute lowercase versions to avoid repeated allocations
    let custom_words_lower: Vec<String> = custom_words.iter().map(|w| w.to_lowercase()).collect();

    // Pre-compute versions with spaces removed for n-gram comparison
    let custom_words_nospace: Vec<String> = custom_words_lower
        .iter()
        .map(|w| w.replace(' ', ""))
        .collect();

    let words: Vec<&str> = text.split_whitespace().collect();
    let mut result = Vec::new();
    let mut i = 0;

    while i < words.len() {
        // Evaluate every n-gram (1..=3) starting at this position and choose the match with the
        // smallest absolute edit distance. On a tie the direction depends on match quality: an
        // exact tie (distance 0) prefers the longer phrase, so a more specific dictionary entry
        // wins over a prefix of it ("New York Times" over "New York"); a fuzzy tie prefers fewer
        // words, so a longer n-gram cannot swallow an unrelated trailing word just because the
        // extra characters happened to match the same distance ("Charge B, che" stays as-is).
        let mut best: Option<(usize, &String, f64)> = None;

        for n in 1..=3 {
            if i + n > words.len() {
                break;
            }

            let ngram_words = &words[i..i + n];
            let ngram = build_ngram(ngram_words);

            if let Some((replacement, score)) = find_best_match(
                &ngram,
                custom_words,
                &custom_words_nospace,
                threshold,
                n >= 2,
            ) {
                let is_better = match best {
                    None => true,
                    Some((best_n, _, best_score)) => {
                        if score < best_score - f64::EPSILON {
                            true
                        } else if (score - best_score).abs() <= f64::EPSILON {
                            // Tie on distance: prefer the longer phrase only when both matched
                            // exactly (distance 0); otherwise prefer fewer words.
                            if score.abs() <= f64::EPSILON {
                                n > best_n
                            } else {
                                n < best_n
                            }
                        } else {
                            false
                        }
                    }
                };
                if is_better {
                    best = Some((n, replacement, score));
                }
            }
        }

        if let Some((n, replacement, _)) = best {
            let ngram_words = &words[i..i + n];

            // Extract punctuation from first and last words of the n-gram
            let (prefix, _) = extract_punctuation(ngram_words[0]);
            let (_, suffix) = extract_punctuation(ngram_words[n - 1]);

            // Preserve case from first word
            let corrected = preserve_case_pattern(ngram_words[0], replacement);

            result.push(format!("{}{}{}", prefix, corrected, suffix));
            i += n;
        } else {
            result.push(words[i].to_string());
            i += 1;
        }
    }

    result.join(" ")
}

/// A "word" character for whole-word boundary checks: alphanumeric or underscore, matching the
/// usual `\w` definition (but Unicode-aware via [`char::is_alphanumeric`]).
fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn word_boundary_shape(value: &str) -> Vec<bool> {
    let mut shape = Vec::new();
    for is_word in value.chars().map(is_word_char) {
        if shape.last() != Some(&is_word) {
            shape.push(is_word);
        }
    }
    shape
}

/// True when the byte range `[start, end)` in `haystack` is a whole-word match -- flanked by a
/// non-word character or a string boundary on each side. Unlike `\b`, this works for phrases that
/// begin or end with punctuation (e.g. "C#", ".env").
fn is_word_boundary_match(haystack: &str, start: usize, end: usize) -> bool {
    let before_ok = haystack[..start]
        .chars()
        .next_back()
        .is_none_or(|c| !is_word_char(c));
    let after_ok = haystack[end..]
        .chars()
        .next()
        .is_none_or(|c| !is_word_char(c));
    before_ok && after_ok
}

fn replacement_regex(value: &str, case_sensitive: bool) -> Option<Regex> {
    let escaped = regex::escape(value);
    let pattern = if case_sensitive {
        escaped
    } else {
        format!("(?i){escaped}")
    };
    Regex::new(&pattern).ok()
}

fn regex_has_replacement_match(re: &Regex, text: &str, whole_word: bool) -> bool {
    re.find_iter(text)
        .any(|matched| !whole_word || is_word_boundary_match(text, matched.start(), matched.end()))
}

fn regex_matches_entire(re: &Regex, text: &str) -> bool {
    re.find(text)
        .is_some_and(|matched| matched.start() == 0 && matched.end() == text.len())
}

/// Every target a rule can emit in production. Case-insensitive rules may keep their literal
/// target, capitalize its first character, or uppercase it completely depending on the matched
/// source. Reusing [`preserve_case_pattern`] keeps cascade analysis aligned with runtime behavior,
/// including Unicode mappings such as `ß` -> `SS` and `ı` -> `I`.
fn runtime_replacement_variants(rule: &WordReplacement) -> Vec<String> {
    let mut variants = vec![rule.to.clone()];
    if !rule.case_sensitive && !rule.preserve_replacement_case {
        for source_case in ["Xx", "XX"] {
            let variant = preserve_case_pattern(source_case, &rule.to);
            if !variants.contains(&variant) {
                variants.push(variant);
            }
        }
    }
    variants
}

/// Whether context on either side of `earlier_target` can complete `later_source` across their
/// join. Keep one extra character from the target on each side so whole-word checks see the real
/// adjacent boundary instead of a boundary introduced by truncation.
fn partial_overlap_can_complete(
    earlier_target: &str,
    later_source: &str,
    later_pattern: &Regex,
    later_whole_word: bool,
) -> bool {
    let context_chars = later_source.chars().count().saturating_add(1);
    let mut earlier_tail: Vec<char> = earlier_target.chars().rev().take(context_chars).collect();
    earlier_tail.reverse();
    let earlier_tail: String = earlier_tail.into_iter().collect();
    let earlier_head: String = earlier_target.chars().take(context_chars).collect();

    later_source
        .char_indices()
        .map(|(index, _)| index)
        .filter(|index| *index > 0)
        .any(|index| {
            let suffix_completed = format!("{earlier_tail}{}", &later_source[index..]);
            if regex_has_replacement_match(later_pattern, &suffix_completed, later_whole_word) {
                return true;
            }

            let prefix_completed = format!("{}{earlier_head}", &later_source[..index]);
            regex_has_replacement_match(later_pattern, &prefix_completed, later_whole_word)
        })
}

/// Applies deterministic literal replacements, in order, to the text.
///
/// Each [`WordReplacement`] maps an exact `from` phrase to an exact `to` output. This is the
/// safe path for large mishears the fuzzy matcher cannot recover without guessing (the
/// canonical case being "clawed" -> "Claude": 50% edit distance, phonetically distinct). It
/// runs for every engine, after fuzzy custom-word correction and before filler removal.
///
/// - `whole_word` (default true): matches only on word boundaries, so "cat" -> "dog" does
///   not corrupt "category".
/// - `case_sensitive` (default false): matching ignores case and the output mirrors the matched
///   text when it is ALL CAPS or Capitalized; otherwise the replacement's own casing is kept (so a
///   lowercase match of `clawed` still yields `Claude`, not `claude`). See [`preserve_case_pattern`].
/// - `preserve_replacement_case` (default false): keeps the exact replacement casing for learned
///   names, brands, and acronyms such as `io` -> `iOS`, regardless of the matched source casing.
/// - An empty `to` deletes the match; spaces left dangling by a deletion (doubled, leading/trailing,
///   or stranded before punctuation) are cleaned up afterwards.
pub fn apply_replacements(text: &str, replacements: &[WordReplacement]) -> String {
    let mut result = text.to_string();
    let mut deleted_any = false;

    for replacement in replacements {
        if replacement.from.is_empty() {
            continue;
        }

        let Some(re) = replacement_regex(&replacement.from, replacement.case_sensitive) else {
            continue;
        };

        let to = replacement.to.clone();
        let case_sensitive = replacement.case_sensitive;
        let preserve_replacement_case = replacement.preserve_replacement_case;
        let whole_word = replacement.whole_word;
        if to.is_empty() {
            deleted_any = true;
        }

        // The regex crate has no lookaround, and `\b` cannot anchor a phrase that begins or ends
        // with punctuation (e.g. "C#" or ".env"), so whole-word matching is verified directly
        // against the surrounding characters rather than baked into the pattern.
        let source = std::mem::take(&mut result);
        result = re
            .replace_all(&source, |caps: &Captures| {
                let m = caps.get(0).expect("capture group 0 always exists");
                if whole_word && !is_word_boundary_match(&source, m.start(), m.end()) {
                    return m.as_str().to_string();
                }
                if case_sensitive || preserve_replacement_case {
                    to.clone()
                } else {
                    // Adapt the replacement to how the user dictated the source phrase.
                    preserve_case_pattern(m.as_str(), &to)
                }
            })
            .to_string();
    }

    if deleted_any {
        // A deletion can leave "a  b", a leading/trailing space, or a space stranded before
        // punctuation ("this is basically, fine" -> "this is , fine"); tidy all three without
        // touching intentional single spaces.
        result = SPACE_BEFORE_PUNCT_PATTERN
            .replace_all(&result, "$1")
            .to_string();
        result = MULTI_SPACE_PATTERN.replace_all(&result, " ").to_string();
        result = result.trim().to_string();
    }

    result
}

/// Preserves the case pattern of the original word when applying a replacement.
///
/// All-caps detection ignores non-alphabetic characters so multi-word phrases are handled
/// ("CLOUD CODE" -> "CLAUDE CODE", not "Claude Code"); it requires at least one letter so a
/// digit- or punctuation-only original is not mistaken for all-caps.
fn preserve_case_pattern(original: &str, replacement: &str) -> String {
    let mut letters = original.chars().filter(|c| c.is_alphabetic()).peekable();
    let all_caps = letters.peek().is_some() && letters.all(|c| c.is_uppercase());

    if all_caps {
        replacement.to_uppercase()
    } else if original.chars().next().is_some_and(|c| c.is_uppercase()) {
        let mut chars: Vec<char> = replacement.chars().collect();
        if let Some(first_char) = chars.get_mut(0) {
            *first_char = first_char.to_uppercase().next().unwrap_or(*first_char);
        }
        chars.into_iter().collect()
    } else {
        replacement.to_string()
    }
}

/// Extracts punctuation prefix and suffix from a word
fn extract_punctuation(word: &str) -> (&str, &str) {
    let prefix_end = word.chars().take_while(|c| !c.is_alphanumeric()).count();
    let suffix_start = word
        .char_indices()
        .rev()
        .take_while(|(_, c)| !c.is_alphanumeric())
        .count();

    let prefix = if prefix_end > 0 {
        &word[..prefix_end]
    } else {
        ""
    };

    let suffix = if suffix_start > 0 {
        &word[word.len() - suffix_start..]
    } else {
        ""
    };

    (prefix, suffix)
}

/// Returns filler words appropriate for the given language code.
///
/// Some words like "um" and "ha" are real words in certain languages
/// (e.g., Portuguese "um" = "a/an", Spanish "ha" = "has"), so we only
/// include them as fillers for languages where they are truly fillers.
fn get_filler_words_for_language(lang: &str) -> &'static [&'static str] {
    let base_lang = lang.split(&['-', '_'][..]).next().unwrap_or(lang);

    match base_lang {
        "en" => &[
            "uh", "um", "uhm", "umm", "uhh", "uhhh", "ah", "hmm", "hm", "mmm", "mm", "mh", "eh",
            "ehh", "ha",
        ],
        "es" => &["ehm", "mmm", "hmm", "hm"],
        "pt" => &["ahm", "hmm", "mmm", "hm"],
        "fr" => &["euh", "hmm", "hm", "mmm"],
        "de" => &["äh", "ähm", "hmm", "hm", "mmm"],
        "it" => &["ehm", "hmm", "mmm", "hm"],
        "cs" => &["ehm", "hmm", "mmm", "hm"],
        "pl" => &["hmm", "mmm", "hm"],
        "tr" => &["hmm", "mmm", "hm"],
        "ru" => &["хм", "ммм", "hmm", "mmm"],
        "uk" => &["хм", "ммм", "hmm", "mmm"],
        "ar" => &["hmm", "mmm"],
        "ja" => &["hmm", "mmm"],
        "ko" => &["hmm", "mmm"],
        "vi" => &["hmm", "mmm", "hm"],
        "zh" => &["hmm", "mmm"],
        // Conservative universal fallback (no "um", "eh", "ha")
        _ => &[
            "uh", "uhm", "umm", "uhh", "uhhh", "ah", "hmm", "hm", "mmm", "mm", "mh", "ehh",
        ],
    }
}

static MULTI_SPACE_PATTERN: Lazy<Regex> = Lazy::new(|| Regex::new(r"\s{2,}").unwrap());

// Space(s) stranded before closing punctuation after a deletion replacement (e.g. deleting
// "basically" from "this is basically, fine" leaves "this is , fine"). Only closing/clause marks
// are listed; opening brackets keep their leading space ("foo (bar)").
static SPACE_BEFORE_PUNCT_PATTERN: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"[ \t]+([,.;:!?)\]}])").unwrap());

/// Collapses repeated words (3+ repetitions) to a single instance.
/// E.g., "wh wh wh wh" -> "wh", "I I I I" -> "I"
fn collapse_stutters(text: &str) -> String {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.is_empty() {
        return text.to_string();
    }

    let mut result: Vec<&str> = Vec::new();
    let mut i = 0;

    while i < words.len() {
        let word = words[i];
        let word_lower = word.to_lowercase();

        if word_lower.chars().all(|c| c.is_alphabetic()) {
            // Count consecutive repetitions (case-insensitive)
            let mut count = 1;
            while i + count < words.len() && words[i + count].to_lowercase() == word_lower {
                count += 1;
            }

            // If 3+ repetitions, collapse to single instance
            if count >= 3 {
                result.push(word);
                i += count;
            } else {
                result.push(word);
                i += 1;
            }
        } else {
            result.push(word);
            i += 1;
        }
    }

    result.join(" ")
}

/// Filters transcription output by removing filler words and stutter artifacts.
///
/// This function cleans up raw transcription text by:
/// 1. Removing filler words based on the app language (or custom list)
/// 2. Collapsing repeated word stutters (e.g., "wh wh wh" -> "wh")
/// 3. Cleaning up excess whitespace
///
/// # Arguments
/// * `text` - The raw transcription text to filter
/// * `lang` - The app language code (e.g., "en", "pt-BR") used to select filler words
/// * `custom_filler_words` - Optional user-provided filler word list. `Some(vec)` overrides
///   language defaults; `Some(empty vec)` disables filtering; `None` uses language defaults.
///
/// # Returns
/// The filtered text with filler words and stutters removed
pub fn filter_transcription_output(
    text: &str,
    lang: &str,
    custom_filler_words: &Option<Vec<String>>,
) -> String {
    let mut filtered = text.to_string();

    // Build filler patterns from custom list or language defaults
    let patterns: Vec<Regex> = match custom_filler_words {
        Some(words) => words
            .iter()
            .filter_map(|word| Regex::new(&format!(r"(?i)\b{}\b[,.]?", regex::escape(word))).ok())
            .collect(),
        None => get_filler_words_for_language(lang)
            .iter()
            .map(|word| Regex::new(&format!(r"(?i)\b{}\b[,.]?", regex::escape(word))).unwrap())
            .collect(),
    };

    // Remove filler words
    for pattern in &patterns {
        filtered = pattern.replace_all(&filtered, "").to_string();
    }

    // Collapse repeated 1-2 letter words (stutter artifacts like "wh wh wh wh")
    filtered = collapse_stutters(&filtered);

    // Clean up multiple spaces to single space
    filtered = MULTI_SPACE_PATTERN.replace_all(&filtered, " ").to_string();

    // Trim leading/trailing whitespace
    filtered.trim().to_string()
}

/// True if the token is an all-caps acronym worth preserving (e.g. "API", "GPU", "CJS2026").
/// Requires at least two letters, all uppercase; digits are allowed alongside.
fn is_acronym(token: &str) -> bool {
    let letters: Vec<char> = token.chars().filter(|c| c.is_alphabetic()).collect();
    letters.len() >= 2 && letters.iter().all(|c| c.is_uppercase())
}

/// If the token is the English pronoun "I" or one of its contractions (ignoring surrounding
/// punctuation), returns its canonical capitalized form together with whether the engine already
/// wrote it capitalized. Raw mode uses both: when the output is known to be English it forces the
/// capitalized form (lowercasing "I" reads as broken), but for auto-detected or non-English
/// dictation it only keeps the casing the engine produced -- otherwise a language that uses a
/// lowercase standalone "i" (e.g. Polish/Croatian "i" = "and") would be wrongly capitalized.
fn english_i_canonical(token: &str) -> Option<(String, bool)> {
    let core = token.trim_matches(|c: char| !c.is_alphanumeric() && c != '\'' && c != '\u{2019}');
    let was_uppercase = core.chars().next().is_some_and(char::is_uppercase);
    let normalized = core.to_lowercase().replace('\u{2019}', "'");
    let canonical = match normalized.as_str() {
        "i" => "I",
        "i'm" => "I'm",
        "i'll" => "I'll",
        "i've" => "I've",
        "i'd" => "I'd",
        _ => return None,
    };
    Some((canonical.to_string(), was_uppercase))
}

/// Punctuation a raw transcript drops when it sits at a token boundary: sentence and clause marks,
/// quotes, and brackets. Technical symbols that are part of a token's meaning (`#`, `+`, `*`, ...)
/// are deliberately excluded so terms like `C#`, `C++`, and `F#` survive raw mode instead of being
/// trimmed down to a bare letter.
fn is_boundary_punctuation(c: char) -> bool {
    matches!(
        c,
        '.' | ','
            | ';'
            | ':'
            | '!'
            | '?'
            | '"'
            | '\''
            | '`'
            | '('
            | ')'
            | '['
            | ']'
            | '{'
            | '}'
            | '\u{2026}' // ellipsis
            | '\u{2018}' // left single quote
            | '\u{2019}' // right single quote
            | '\u{201C}' // left double quote
            | '\u{201D}' // right double quote
            | '\u{00AB}' // left guillemet
            | '\u{00BB}' // right guillemet
            | '\u{2014}' // em dash
            | '\u{2013}' // en dash
    )
}

/// Strips leading and trailing boundary punctuation from a token while preserving everything
/// interior to it -- that is, every character between the first and last non-boundary character is
/// kept. This keeps decimals, versions, hyphenated words, dotted filenames, emails, contraction
/// apostrophes, path separators (including Windows `C:\...` drive separators), and trailing
/// technical symbols (`C#`, `C++`) intact, while removing only the sentence/clause punctuation a raw
/// transcript should drop. Optionally lowercases the alphanumerics.
fn strip_token_punctuation(token: &str, lowercase: bool) -> String {
    let chars: Vec<char> = token.chars().collect();
    let (Some(first), Some(last)) = (
        chars.iter().position(|c| !is_boundary_punctuation(*c)),
        chars.iter().rposition(|c| !is_boundary_punctuation(*c)),
    ) else {
        // The whole token is boundary punctuation, so it is dropped.
        return String::new();
    };

    let mut out = String::with_capacity(last - first + 1);
    for &c in &chars[first..=last] {
        if c.is_alphanumeric() && lowercase {
            out.extend(c.to_lowercase());
        } else {
            out.push(c);
        }
    }

    out
}

/// Converts text to a raw transcript (issue #19): lowercased and stripped of terminal/clause
/// punctuation, with two deliberate exceptions that would otherwise read as broken -- all-caps
/// acronyms keep their case, and the English standalone "I"/"I'm"/"I'll"/"I've"/"I'd" stay
/// capitalized. Punctuation interior to a token is kept by one script-neutral rule (only
/// leading/trailing punctuation is stripped), which covers decimals, version strings, `GPT-4`,
/// `claude.md`, `user@example.com`, `well-known`, and path separators.
///
/// `force_english_i` controls the "I" exception. When the output is known to be English (an
/// explicit English dictation language, or translate-to-English), it is `true` and a standalone
/// "i" is always capitalized. When the language is unknown (auto-detect; `transcribe-rs` does not
/// report the detected language) or explicitly non-English, it is `false` and the token keeps the
/// casing the engine produced: English engines already emit "I" capitalized, so English stays
/// correct, while a language that uses a lowercase standalone "i" (Polish/Croatian "i" = "and") is
/// not wrongly capitalized.
///
/// This is a pure, deterministic, engine-agnostic transform -- no model, no proper-noun
/// detection. Worked example:
/// `Open Claude.md and read GPT-4 notes.` -> `open claude.md and read GPT-4 notes`
pub fn strip_to_raw_text(text: &str, force_english_i: bool) -> String {
    let mut out: Vec<String> = Vec::new();

    for token in text.split_whitespace() {
        if let Some((canonical, was_uppercase)) = english_i_canonical(token) {
            // Force capitalization for known-English output; otherwise only preserve the casing the
            // engine already produced so non-English standalone "i" is left lowercase.
            if force_english_i || was_uppercase {
                out.push(canonical);
                continue;
            }
        }

        let keep_case = is_acronym(token);
        let stripped = strip_token_punctuation(token, !keep_case);
        if !stripped.is_empty() {
            out.push(stripped);
        }
    }

    out.join(" ")
}

/// The word core of a token: its outer prose punctuation trimmed off, so
/// `"clawed,"` -> `"clawed"` and `"(Claude)"` -> `"Claude"`. Only prose
/// delimiters are stripped -- closing marks off the right, opening marks off the
/// left -- while symbols that belong to a term are kept: `"C#"` keeps its `#`,
/// `".env"` keeps its leading dot. Trimming those would narrow a learned pair to
/// its letters (`C# -> F#` becoming `C -> F`), and a whole-word replacement of a
/// bare `C` would then corrupt every later transcript -- the opposite of the
/// precision this module promises (#67).
fn word_core(token: &str) -> &str {
    const OPENERS: &[char] = &[
        '(', '[', '{', '"', '\'', '\u{2018}', '\u{201C}', '\u{00AB}', '\u{2013}', '\u{2014}',
    ];
    const CLOSERS: &[char] = &[
        '.', ',', ';', ':', '!', '?', ')', ']', '}', '"', '\'', '\u{2019}', '\u{201D}', '\u{00BB}',
        '\u{2026}', '\u{2013}', '\u{2014}',
    ];
    token
        .trim_start_matches(|c: char| OPENERS.contains(&c))
        .trim_end_matches(|c: char| CLOSERS.contains(&c))
}

/// The punctuation trimmed from either side of a token by [`word_core`]. A core is always a
/// contiguous slice of its token, so its first occurrence marks the boundary after the opener.
fn word_outer_affixes<'a>(token: &'a str, core: &str) -> (&'a str, &'a str) {
    let start = token
        .find(core)
        .expect("word_core always returns a substring of its token");
    (&token[..start], &token[start + core.len()..])
}

/// Positions `(i, j)` of the longest common subsequence of two token lists,
/// compared on lowercased cores so `"Claude"` and `"claude,"` count as the same
/// anchor. The backtrack prefers the diagonal, so ties resolve deterministically.
///
/// The `(n + 1) * (m + 1)` DP table lives in one flat `Vec<u32>` indexed
/// `i * stride + j`, so the search is a single allocation rather than `n + 1`
/// nested vectors. The caller caps `n` and `m`, so the table cannot grow without
/// bound on a large paste.
fn lcs_anchors(a: &[String], b: &[String]) -> Vec<(usize, usize)> {
    let n = a.len();
    let m = b.len();
    let stride = m + 1;
    let mut dp = vec![0u32; (n + 1) * stride];
    let at = |i: usize, j: usize| i * stride + j;
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            dp[at(i, j)] = if a[i] == b[j] {
                dp[at(i + 1, j + 1)] + 1
            } else {
                dp[at(i + 1, j)].max(dp[at(i, j + 1)])
            };
        }
    }
    let mut anchors = Vec::new();
    let (mut i, mut j) = (0, 0);
    while i < n && j < m {
        if a[i] == b[j] {
            anchors.push((i, j));
            i += 1;
            j += 1;
        } else if dp[at(i + 1, j)] > dp[at(i, j + 1)] {
            i += 1;
        } else {
            // Prefer advancing the corrected side on a tie. This keeps a later unchanged target
            // anchored to itself (`jon John` -> `John John`) so the adjacent typo remains a
            // one-for-one substitution instead of becoming a deletion plus insertion.
            j += 1;
        }
    }
    anchors
}

/// Whether casing inside a replacement is semantic rather than an ordinary lowercase,
/// ALL-CAPS, or Capitalized presentation. Brand forms such as `iOS`, `eBay`, and `OpenAI` carry
/// casing that must survive regardless of how the misheard source was capitalized.
fn has_intentional_mixed_case(value: &str) -> bool {
    let letters: Vec<char> = value.chars().filter(|c| c.is_alphabetic()).collect();
    let has_upper = letters.iter().any(|c| c.is_uppercase());
    let has_lower = letters.iter().any(|c| c.is_lowercase());
    if !has_upper || !has_lower {
        return false;
    }

    // A conventional Capitalized word adapts to sentence/all-caps context. Any other mixture is
    // treated as intentional internal casing.
    !letters[0].is_uppercase() || letters[1..].iter().any(|c| !c.is_lowercase())
}

fn is_all_caps(value: &str) -> bool {
    let mut letters = value.chars().filter(|c| c.is_alphabetic()).peekable();
    letters.peek().is_some() && letters.all(|c| c.is_uppercase())
}

fn is_capitalized(value: &str) -> bool {
    let mut letters = value.chars().filter(|c| c.is_alphabetic());
    letters.next().is_some_and(|first| first.is_uppercase()) && letters.all(|c| c.is_lowercase())
}

fn alphanumeric_components(value: &str) -> Vec<&str> {
    value
        .split(|c: char| !c.is_alphanumeric())
        .filter(|component| !component.is_empty())
        .collect()
}

/// Keep target casing when the user explicitly changed presentation or entered an internally
/// cased brand. Ambiguous matching presentation casing is rejected before this helper is called.
fn learned_target_presentation(from: &str, to: &str) -> (String, bool) {
    if has_intentional_mixed_case(to) {
        return (to.to_string(), true);
    }

    // Normal runtime replacement mirrors title/all-caps presentation from the matched source.
    // Preserve the literal target whenever that adaptation would undo an explicit cross-case edit
    // such as `Sonet` -> `sonnet`. Lowercase `clawed` -> `Claude` needs no override because the
    // default path already retains the replacement's own casing for a lowercase match.
    let from_letters: String = from.chars().filter(|c| c.is_alphabetic()).collect();
    let to_letters: String = to.chars().filter(|c| c.is_alphabetic()).collect();
    let preserve = preserve_case_pattern(&from_letters, &to_letters) != to_letters
        || (is_known_english_word(to) && (is_capitalized(to) || is_all_caps(to)));
    (to.to_string(), preserve)
}

/// Whether two token cores differ only by a regular English inflection. The broad English
/// dictionary catches ordinary words; this guard covers acronyms, brands, and invented terms that
/// are intentionally absent from it (`API`->`APIs`, `Zorp`->`Zorped`). Precision wins over recall:
/// a contextual grammar edit must never become a permanent global replacement.
fn is_regular_inflection_edit(from: &str, to: &str) -> bool {
    fn is_inflected_form(
        base: &str,
        inflected: &str,
        surface: &str,
        allow_stylized_s: bool,
    ) -> bool {
        for suffix in ["s", "es", "ed", "ing", "er", "est"] {
            let Some(stem) = inflected.strip_suffix(suffix) else {
                continue;
            };

            if stem == base {
                // `io` -> `iOS` is a canonical brand recasing, not a plural. Preserve this narrow
                // addition case while still rejecting `API` -> `APIs` and `Claude` -> `Claudes`.
                if suffix == "s"
                    && allow_stylized_s
                    && surface.chars().last().is_some_and(|c| c.is_uppercase())
                    && has_intentional_mixed_case(surface)
                {
                    return false;
                }
                return true;
            }

            if base
                .strip_suffix('e')
                .is_some_and(|base_stem| stem == base_stem)
            {
                return true;
            }

            let mut stem_chars = stem.chars();
            if let Some(last) = stem_chars.next_back() {
                let shortened: String = stem_chars.collect();
                if shortened.ends_with(last) && shortened == base {
                    return true;
                }
            }
        }

        if let Some(base_stem) = base.strip_suffix('y') {
            return inflected == format!("{base_stem}ies")
                || inflected == format!("{base_stem}ied");
        }

        false
    }

    let is_inflection_pair = |from: &str, to: &str| {
        let from_lower = from.to_lowercase();
        let to_lower = to.to_lowercase();
        is_inflected_form(&from_lower, &to_lower, to, true)
            || is_inflected_form(&to_lower, &from_lower, from, false)
    };

    if is_inflection_pair(from, to) {
        return true;
    }

    // A grammatical suffix can occur before a shared symbol-delimited tail rather than at the
    // end of the full token (`mother-in-law` -> `mothers-in-law`, `API/client` -> `APIs/client`).
    // Compare aligned components so compound syntax cannot hide the same unsafe global edit.
    let from_components = alphanumeric_components(from);
    let to_components = alphanumeric_components(to);
    from_components.len() == to_components.len()
        && from_components.len() > 1
        && from_components
            .iter()
            .zip(to_components)
            .any(|(from_component, to_component)| is_inflection_pair(from_component, to_component))
}

/// Whether one token is just a grammatical contraction of the other. The broad dictionary veto
/// catches ordinary pairs, but unknown names and brands need the same protection (`Claude` ->
/// `Claude'll`). Curly apostrophes are normalized because platform text input commonly produces
/// them. A few irregular negative contractions need explicit stems after their spelling change.
fn is_contraction_edit(from: &str, to: &str) -> bool {
    fn is_contracted_form(base: &str, contracted: &str) -> bool {
        for suffix in ["'s", "'ll", "'re", "'ve", "'d", "'m", "n't"] {
            if contracted.strip_suffix(suffix) == Some(base) {
                return true;
            }
        }

        matches!(
            (base, contracted),
            ("can", "can't") | ("will", "won't") | ("shall", "shan't")
        )
    }

    let normalize = |value: &str| value.to_lowercase().replace('\u{2019}', "'");
    let from = normalize(from);
    let to = normalize(to);
    is_contracted_form(&from, &to) || is_contracted_form(&to, &from)
}

fn has_safe_lexical_evidence(from: &str, to: &str) -> bool {
    let from_is_dictionary_word = is_known_english_word(from);
    let to_is_dictionary_word = is_known_english_word(to);
    let from_is_named_entity = is_known_english_named_entity(from);
    let to_is_named_entity = is_known_english_named_entity(to);

    // A case-insensitive global rule can never safely start from a token that is both an ordinary
    // word and a name (`angel`, `mark`, `rose`). Apply the veto independently of the target list:
    // corrected nicknames and uncommon names may not appear in the pinned named-entity subset.
    if from_is_dictionary_word && from_is_named_entity {
        return false;
    }
    if from_is_named_entity && to_is_named_entity {
        // Raw-mode transcripts can lowercase proper names and calendar terms. Treat two known
        // entities as a semantic swap unless they are an exceptionally close spelling/phonetic
        // correction such as `jon` -> `John`. The latter remains useful while distant pairs such
        // as `monday` -> `tuesday` and `paris` -> `london` fail closed.
        if !has_close_spelling_evidence(from, to) || !has_phonetic_match(from, to) {
            return false;
        }
    } else if (from_is_dictionary_word ^ to_is_dictionary_word)
        && (from_is_named_entity ^ to_is_named_entity)
        && !has_phonetic_match(from, to)
    {
        // A common-word/name swap is usually semantic (`browser` -> `Firefox`, `monday` ->
        // `today`). Require the phonetic evidence that distinguishes a genuine dictation mishear
        // such as `clawed` -> `Claude`.
        return false;
    }
    if has_intentional_mixed_case(from)
        && has_intentional_mixed_case(to)
        && !has_strong_mishear_evidence(from, to)
    {
        // Unknown mixed-case terms are commonly brands or products. Do not turn a semantic edit
        // such as `OpenAI` -> `ChatGPT` into a global rule without mishear evidence.
        return false;
    }
    let shares_known_category = (from_is_dictionary_word && to_is_dictionary_word)
        || (from_is_named_entity && to_is_named_entity);
    if !shares_known_category && !has_strong_mishear_evidence(from, to) {
        // Raw mode can remove the presentation cues that distinguish unknown brands and proper
        // nouns. An unclassified pair such as `figma` -> `gitlab` needs spelling or phonetic
        // evidence before it can become a global rule.
        return false;
    }

    !from_is_dictionary_word || !to_is_dictionary_word
}

/// Whether a one-for-one `from`->`to` token core is worth learning as a literal
/// replacement. See `extract_learned_replacements` for the rationale of each guard.
fn is_learnable_substitution(from: &str, to: &str) -> bool {
    let ok_shape = |w: &str| w.chars().count() >= 2 && w.chars().any(|c| c.is_alphabetic());
    if !ok_shape(from) || !ok_shape(to) {
        return false;
    }
    if from.to_lowercase() == to.to_lowercase() {
        return false;
    }
    // Runtime matching uses Unicode simple case folding, which is broader than `to_lowercase()`.
    // Reject visually distinct edits such as long-s `ſample` -> `Sample` when the persisted source
    // regex would also match the target (and therefore ordinary `sample`) in production.
    if replacement_regex(from, false).is_some_and(|re| regex_matches_entire(&re, to)) {
        return false;
    }
    // Symbols inside the core are part of a term's identity. Matching symbol structure can safely
    // learn `C#` -> `F#`, but adding syntax (`clawed` -> `Claude.md`) would append it to every
    // future occurrence and must not become a global rule.
    let symbol_skeleton =
        |w: &str| -> String { w.chars().filter(|c| !c.is_alphanumeric()).collect() };
    if symbol_skeleton(from) != symbol_skeleton(to) {
        return false;
    }
    // Typography, punctuation, and diacritic-only edits (`US`->`U.S.`, `expose`->`exposé`) are not
    // mishears. Learning one would be especially dangerous because the case-insensitive rule could
    // rewrite an otherwise valid spelling everywhere. Canonical decomposition handles both
    // precomposed and combining-mark forms of the same accent.
    let orthographic_key = |w: &str| -> String {
        w.nfd()
            .filter(|c| !is_combining_mark(*c))
            .filter(|c| c.is_alphanumeric())
            .flat_map(char::to_lowercase)
            .collect()
    };
    let from_orthographic = orthographic_key(from);
    if !from_orthographic.is_empty() && from_orthographic == orthographic_key(to) {
        return false;
    }
    // A contextual contraction (`Claude` -> `Claude'll`) is a grammar edit, not a mishear.
    // Learning it would rewrite every future occurrence even when the suffix does not belong.
    if is_contraction_edit(from, to) {
        return false;
    }
    // A change between two digit-bearing identifiers is version semantics, including suffix
    // variants such as `GPT-4` -> `GPT-4o`. A one-sided digit addition is also semantic unless it
    // is the narrow three-character acronym form used by `B2B` and `MP3`. (The pure-number case
    // `204`->`205` is already dropped by the shape guard.)
    let has_digit = |w: &str| w.chars().any(char::is_numeric);
    let from_has_digit = has_digit(from);
    let to_has_digit = has_digit(to);
    if from_has_digit ^ to_has_digit {
        let from_alpha = alpha_only(from);
        let to_alpha = alpha_only(to);
        let is_short_digit_acronym = !from_has_digit
            && to_has_digit
            && !is_known_english_word(from)
            && from.chars().all(|c| c.is_ascii_alphabetic())
            && from_alpha.chars().count() == 2
            && to.chars().count() == 3
            && to.chars().filter(|c| c.is_ascii_digit()).count() == 1
            && to.chars().all(|c| c.is_ascii_alphanumeric())
            && to
                .chars()
                .filter(|c| c.is_ascii_alphabetic())
                .all(|c| c.is_ascii_uppercase())
            && matches!(to, "MP3" | "B2B")
            && from_alpha.eq_ignore_ascii_case(&to_alpha);
        if !is_short_digit_acronym {
            return false;
        }
    } else if from_has_digit && to_has_digit {
        return false;
    }
    // Dictionary/name/category safeguards must hold for the complete token and for each changed
    // component. The generated lexical lists intentionally exclude symbols, so checking only the
    // full string would let `angel-like` -> `Angela-like` hide the unsafe `angel` homograph.
    if !has_safe_lexical_evidence(from, to) {
        return false;
    }
    let from_components = alphanumeric_components(from);
    let to_components = alphanumeric_components(to);
    if from_components.len() != to_components.len()
        || from_components
            .iter()
            .zip(to_components)
            .filter(|(from_component, to_component)| {
                from_component.to_lowercase() != to_component.to_lowercase()
            })
            .any(|(from_component, to_component)| {
                !has_safe_lexical_evidence(from_component, to_component)
            })
    {
        return false;
    }
    // Matching title/all-caps presentation is inherently ambiguous even outside the dictionary:
    // it may be sentence formatting (`Helo` -> `Hello`) or a semantic proper-name/calendar swap
    // (`Monday` -> `Tuesday`). There is no safe global choice, so reject it. An explicit
    // lowercase-to-titlecase edit remains learnable and preserves the user's casing.
    if symbol_skeleton(from).is_empty()
        && symbol_skeleton(to).is_empty()
        && ((is_capitalized(from) && is_capitalized(to)) || (is_all_caps(from) && is_all_caps(to)))
    {
        return false;
    }
    if is_regular_inflection_edit(from, to) {
        return false;
    }
    true
}

/// Extract high-confidence heard->meant replacement pairs from a transcript the
/// user edited in place (issue #67). This is the correction-capture signal:
/// `original` is what the engine produced, `corrected` is what the user fixed it
/// to, and the diff between them records mishears worth learning. The pairs feed
/// `PersonalizationData.learned_replacements`, which is applied through
/// `apply_replacements` at transcription time.
///
/// Precision over recall, by design -- a deterministic replacement is applied to
/// every future transcript, so a wrong pair is worse than a missed one. Only
/// clean one-for-one token substitutions are considered, and the whole edit is
/// rejected if it contains an insertion, deletion, multi-token rewrite, or
/// reorder. A candidate must clear every guard in `is_learnable_substitution`;
/// if the same heard word was corrected to two different words, none of its
/// pairs are learned. Duplicates collapse to a single pair.
/// `preceding_replacements` must contain the user-authored and already-learned rules in their
/// production order. It lets the extractor reject a new rule that would add a cascade to an
/// earlier target when the caller appends the returned rules.
///
/// The result is deterministic and does no I/O, so it is unit-tested in
/// `audio_toolkit` without a running app. Capturing the correction and appending
/// accepted pairs to the store is the caller's job (issue #67 parts 1 and 3).
///
/// Known limitation: the dictionary-word veto in `is_learnable_substitution` is English-only, so
/// a non-English grammar edit (French `la`->`le`) still passes every guard.
/// The fix needs the dictation language, which this pure function does not receive; it is
/// tracked for the parts-1/3 wiring in issue #126.
pub fn extract_learned_replacements(
    original: &str,
    corrected: &str,
    preceding_replacements: &[WordReplacement],
) -> Vec<WordReplacement> {
    // Normal UI-authored rules are capped at 200 entries and 100 characters per field. Imported
    // settings can bypass that UI, so keep their effect on extraction bounded as well.
    const MAX_PRECEDING_RULES: usize = 512;
    const MAX_PRECEDING_FIELD_BYTES: usize = 1024;
    if preceding_replacements.len() > MAX_PRECEDING_RULES
        || preceding_replacements.iter().any(|rule| {
            rule.from.len() > MAX_PRECEDING_FIELD_BYTES || rule.to.len() > MAX_PRECEDING_FIELD_BYTES
        })
    {
        return Vec::new();
    }

    // A deletion can join formerly separated fragments into an arbitrary new source. Any rule can
    // also create a boundary without contributing source characters (`foo` -> `bar.` can activate
    // a following punctuation-led source). Without the original input context there is no bounded
    // pairwise proof for these cases, so fail closed.
    if preceding_replacements.iter().any(|rule| {
        rule.to.is_empty() || word_boundary_shape(&rule.from) != word_boundary_shape(&rule.to)
    }) {
        return Vec::new();
    }

    // Bound raw input before tokenization and normalization. The LCS cell budget controls the
    // quadratic table, while these limits prevent one giant token or an asymmetric paste above
    // 64 KiB from driving proportional allocations and an oversized replacement regex.
    const MAX_INPUT_BYTES: usize = 64 * 1024;
    const MAX_TOKEN_BYTES: usize = 256;
    if original.len() > MAX_INPUT_BYTES || corrected.len() > MAX_INPUT_BYTES {
        return Vec::new();
    }

    let orig: Vec<&str> = original.split_whitespace().collect();
    let corr: Vec<&str> = corrected.split_whitespace().collect();
    if orig
        .iter()
        .chain(&corr)
        .any(|token| token.len() > MAX_TOKEN_BYTES)
    {
        return Vec::new();
    }

    // The LCS table below is O(n*m). Cap both the individual dimensions and their product so a
    // long paste cannot allocate or traverse a disproportionate table on a constrained device.
    // Learning nothing past either bound is precision-safe degradation: a missed pair beats a
    // wrong one.
    const MAX_TOKENS: usize = 4096;
    const MAX_LCS_CELLS: usize = 1_000_000;
    let lcs_cells = (orig.len() + 1).checked_mul(corr.len() + 1);
    if orig.len() > MAX_TOKENS
        || corr.len() > MAX_TOKENS
        || lcs_cells.is_none_or(|cells| cells > MAX_LCS_CELLS)
    {
        return Vec::new();
    }

    let orig_keys: Vec<String> = orig.iter().map(|t| word_core(t).to_lowercase()).collect();
    let corr_keys: Vec<String> = corr.iter().map(|t| word_core(t).to_lowercase()).collect();

    let anchors = lcs_anchors(&orig_keys, &corr_keys);

    // Every changed gap must be exactly one token on each side. If an insertion, deletion, or
    // multi-token rewrite appears anywhere in the edit, learn nothing from the whole transcript:
    // that more complex edit can contradict an otherwise plausible one-token pair.
    // Cascade analysis is pairwise. A transcript with hundreds of changed tokens can fit the LCS
    // budget yet still make that later scan disproportionately expensive, so bound the number of
    // raw substitutions before deduplication. Learning nothing from an unusually large edit is the
    // precision-safe outcome.
    const MAX_CANDIDATES: usize = 64;
    let mut candidates: Vec<(String, String, bool)> = Vec::new();
    let (mut pi, mut pj) = (0usize, 0usize);
    for (ai, aj) in anchors
        .iter()
        .copied()
        .chain(std::iter::once((orig.len(), corr.len())))
    {
        match (ai - pi, aj - pj) {
            (0, 0) => {}
            (1, 1) => {
                let from_token = orig[pi];
                let to_token = corr[pj];
                let from = word_core(from_token);
                let to = word_core(to_token);
                if !is_learnable_substitution(from, to) {
                    return Vec::new();
                }
                // The extractor trims matching prose delimiters (`clawed.` -> `Claude.`), but a
                // changed affix may be meaningful target syntax (`prent` -> `print()`, `dokter` ->
                // `Dr.`). Reject the ambiguous pair instead of persisting a truncated target.
                if word_outer_affixes(from_token, from) != word_outer_affixes(to_token, to) {
                    return Vec::new();
                }
                if candidates.len() >= MAX_CANDIDATES {
                    return Vec::new();
                }
                let (to, preserve_replacement_case) = learned_target_presentation(from, to);
                candidates.push((from.to_string(), to, preserve_replacement_case));
            }
            _ => return Vec::new(),
        }
        pi = ai + 1;
        pj = aj + 1;
    }

    // Group candidate sources by the same literal matching each rule will use in production.
    // Most learned rules are case-insensitive, while the narrow digit-bearing MP3/B2B rules are
    // exact-case so they cannot rewrite existing uppercase acronyms. The candidate cap keeps this
    // pairwise check bounded while avoiding a second, subtly different case-fold implementation.
    let candidate_source_patterns: Vec<Option<Regex>> = candidates
        .iter()
        .map(|(from, to, _)| replacement_regex(from, to.chars().any(char::is_numeric)))
        .collect();
    let runtime_sources_equivalent = |left: usize, right: usize| {
        candidate_source_patterns[left]
            .as_ref()
            .is_some_and(|pattern| regex_matches_entire(pattern, &candidates[right].0))
            && candidate_source_patterns[right]
                .as_ref()
                .is_some_and(|pattern| regex_matches_entire(pattern, &candidates[left].0))
    };
    let mut conflicting_sources = vec![false; candidates.len()];
    let mut candidate_preserve_flags: Vec<bool> = candidates
        .iter()
        .map(|(_, _, preserve_replacement_case)| *preserve_replacement_case)
        .collect();
    for left in 0..candidates.len() {
        for right in left + 1..candidates.len() {
            if runtime_sources_equivalent(left, right) {
                if candidates[left].1 != candidates[right].1 {
                    conflicting_sources[left] = true;
                    conflicting_sources[right] = true;
                } else {
                    // The same case-insensitive source and literal target can derive different
                    // presentation metadata from differently cased observations (`Sonet` and
                    // `sonet` -> `sonnet`). Preserve the literal target if either observation
                    // requires it, then let the normal deduplication path emit one shared rule.
                    let preserve_replacement_case =
                        candidate_preserve_flags[left] || candidate_preserve_flags[right];
                    candidate_preserve_flags[left] = preserve_replacement_case;
                    candidate_preserve_flags[right] = preserve_replacement_case;
                }
            }
        }
    }

    // A token present on both sides must be fully accounted for by LCS anchors. Otherwise it moved
    // during the edit, and the surrounding one-token gaps are not reliable substitutions. Counts
    // preserve the valid `John ... jon` -> `John ... John` case: the one original `John` is still
    // anchored even though the correction adds a second.
    let mut orig_counts: HashMap<String, usize> = HashMap::new();
    for key in &orig_keys {
        *orig_counts.entry(key.clone()).or_insert(0) += 1;
    }
    let mut corr_counts: HashMap<String, usize> = HashMap::new();
    for key in &corr_keys {
        *corr_counts.entry(key.clone()).or_insert(0) += 1;
    }
    let mut anchored_counts: HashMap<String, usize> = HashMap::new();
    for &(i, _) in &anchors {
        *anchored_counts.entry(orig_keys[i].clone()).or_insert(0) += 1;
    }
    if orig_counts.iter().any(|(key, orig_count)| {
        corr_counts.get(key).is_some_and(|corr_count| {
            anchored_counts.get(key).copied().unwrap_or(0) < (*orig_count).min(*corr_count)
        })
    }) {
        return Vec::new();
    }

    // The heard word must not survive anywhere in the final correction, including inside another
    // changed token. If any candidate source still has an effective runtime match, the user did
    // not reject it everywhere, so none of this multi-edit transcript is reliable enough to turn
    // into global rules (`cloud` -> `Claude` beside `foo's` -> `cloud's` is ambiguous as a set).
    if candidates
        .iter()
        .any(|(from, to, preserve_replacement_case)| {
            let rule = WordReplacement {
                from: from.clone(),
                to: to.clone(),
                whole_word: true,
                // The only safe digit-bearing targets are the explicit `MP3` and `B2B` cases.
                // Match their lowercase dictated sources exactly so legitimate uppercase
                // acronyms such as `MP` and `BB` remain untouched.
                case_sensitive: to.chars().any(char::is_numeric),
                preserve_replacement_case: *preserve_replacement_case,
            };
            apply_replacements(corrected, std::slice::from_ref(&rule)) != corrected
        })
    {
        return Vec::new();
    }

    let mut out = Vec::new();
    let mut seen_candidate_indexes = Vec::new();
    for (index, (from, to, _)) in candidates.iter().enumerate() {
        if conflicting_sources[index] {
            continue;
        }
        let rule = WordReplacement {
            from: from.clone(),
            preserve_replacement_case: candidate_preserve_flags[index],
            to: to.clone(),
            whole_word: true,
            case_sensitive: to.chars().any(char::is_numeric),
        };
        if apply_replacements(corrected, std::slice::from_ref(&rule)) != corrected {
            continue;
        }
        let already_seen = seen_candidate_indexes.iter().any(|&seen_index| {
            runtime_sources_equivalent(index, seen_index)
                && candidates[seen_index].1 == rule.to
                && candidate_preserve_flags[seen_index] == rule.preserve_replacement_case
        });
        if already_seen {
            continue;
        }
        seen_candidate_indexes.push(index);
        out.push(rule);
    }
    if out.is_empty() {
        return out;
    }
    // Replacements run sequentially. Only interactions whose later rule is new can introduce a
    // cascade, so check each new source against every earlier target. Test both directions: a new
    // rule can directly rewrite an earlier target, or surrounding input can complete the new
    // source around that target (`foo` -> `bar`, then `bar-baz` -> `qux`). Compile each literal
    // once and reuse the production boundary matcher instead of replaying full replacement
    // suffixes and recompiling their regexes for every target.
    // Also bound interactions with an existing settings file before allocating regex/variant
    // tables. This covers settings imported outside the UI's normal list caps.
    const MAX_CASCADE_INTERACTIONS: usize = 4096;
    let new_to_existing = out.len().checked_mul(preceding_replacements.len());
    let new_to_new = out
        .len()
        .checked_mul(out.len().saturating_sub(1))
        .map(|pairs| pairs / 2);
    let cascade_interactions = new_to_existing.and_then(|existing| {
        new_to_new.and_then(|within_batch| existing.checked_add(within_batch))
    });
    if cascade_interactions.is_none_or(|count| count > MAX_CASCADE_INTERACTIONS) {
        return Vec::new();
    }

    let mut combined = Vec::with_capacity(preceding_replacements.len() + out.len());
    combined.extend_from_slice(preceding_replacements);
    combined.extend(out.iter().cloned());
    let emitted_targets: Vec<Vec<String>> =
        combined.iter().map(runtime_replacement_variants).collect();

    // `partial_overlap_can_complete` performs up to two regex searches per character split. Count
    // both source directions and every emitted target casing variant before compiling the regex
    // tables. This keeps a valid 64 KiB edit from creating millions of temporary strings.
    const MAX_CASCADE_MATCH_STEPS: usize = 100_000;
    let mut cascade_match_steps = 0usize;
    let source_splits: Vec<usize> = combined
        .iter()
        .map(|rule| rule.from.chars().count().saturating_sub(1))
        .collect();
    for later_index in preceding_replacements.len()..combined.len() {
        let later_splits = source_splits[later_index];
        let Some(steps_per_variant) = later_splits
            .checked_mul(2)
            .and_then(|steps| steps.checked_add(2))
        else {
            return Vec::new();
        };
        for (earlier_index, earlier_targets) in emitted_targets[..later_index].iter().enumerate() {
            // Two containment searches plus two searches at every split in both source directions.
            let Some(source_pair_steps) = later_splits
                .checked_add(source_splits[earlier_index])
                .and_then(|splits| splits.checked_mul(2))
                .and_then(|steps| steps.checked_add(2))
            else {
                return Vec::new();
            };
            let Some(pair_steps) = earlier_targets.len().checked_mul(steps_per_variant) else {
                return Vec::new();
            };
            let Some(total_steps) = cascade_match_steps
                .checked_add(source_pair_steps)
                .and_then(|steps| steps.checked_add(pair_steps))
            else {
                return Vec::new();
            };
            if total_steps > MAX_CASCADE_MATCH_STEPS {
                return Vec::new();
            }
            cascade_match_steps = total_steps;
        }
    }

    let source_patterns: Vec<Option<Regex>> = combined
        .iter()
        .map(|rule| replacement_regex(&rule.from, rule.case_sensitive))
        .collect();
    // Detect an earlier target inside a later source using the later rule's actual case behavior.
    // Keep both compiled forms because the target belongs to the earlier rule but the matching
    // semantics belong to the later one.
    let case_sensitive_target_patterns: Vec<Vec<Option<Regex>>> = emitted_targets
        .iter()
        .map(|variants| {
            variants
                .iter()
                .map(|target| replacement_regex(target, true))
                .collect()
        })
        .collect();
    let case_insensitive_target_patterns: Vec<Vec<Option<Regex>>> = emitted_targets
        .iter()
        .map(|variants| {
            variants
                .iter()
                .map(|target| replacement_regex(target, false))
                .collect()
        })
        .collect();
    for later_index in preceding_replacements.len()..combined.len() {
        let later = &combined[later_index];
        for earlier_index in 0..later_index {
            let earlier = &combined[earlier_index];
            let source_preemption =
                source_patterns[earlier_index]
                    .as_ref()
                    .is_some_and(|pattern| {
                        regex_has_replacement_match(pattern, &later.from, earlier.whole_word)
                    });
            // An earlier, more contextual source also preempts the new rule when the new source
            // occurs inside it (`x-sonet` before `sonet`). The new rule's production regex still
            // covers a case-sensitive earlier `SONET` when the new source is case-insensitive.
            let contextual_source_preemption =
                source_patterns[later_index]
                    .as_ref()
                    .is_some_and(|pattern| {
                        regex_has_replacement_match(pattern, &earlier.from, later.whole_word)
                    });
            let partial_source_preemption =
                source_patterns[later_index]
                    .as_ref()
                    .is_some_and(|pattern| {
                        partial_overlap_can_complete(
                            &earlier.from,
                            &later.from,
                            pattern,
                            later.whole_word,
                        )
                    })
                    || source_patterns[earlier_index]
                        .as_ref()
                        .is_some_and(|pattern| {
                            partial_overlap_can_complete(
                                &later.from,
                                &earlier.from,
                                pattern,
                                earlier.whole_word,
                            )
                        });
            let direct = source_patterns[later_index]
                .as_ref()
                .is_some_and(|pattern| {
                    emitted_targets[earlier_index].iter().any(|target| {
                        regex_has_replacement_match(pattern, target, later.whole_word)
                    })
                });
            let target_patterns = if later.case_sensitive {
                &case_sensitive_target_patterns
            } else {
                &case_insensitive_target_patterns
            };
            let completed_by_context =
                target_patterns[earlier_index]
                    .iter()
                    .flatten()
                    .any(|pattern| {
                        regex_has_replacement_match(pattern, &later.from, earlier.whole_word)
                    });
            let partial_overlap = source_patterns[later_index]
                .as_ref()
                .is_some_and(|pattern| {
                    emitted_targets[earlier_index].iter().any(|target| {
                        partial_overlap_can_complete(target, &later.from, pattern, later.whole_word)
                    })
                });
            if source_preemption
                || contextual_source_preemption
                || partial_source_preemption
                || direct
                || completed_by_context
                || partial_overlap
            {
                return Vec::new();
            }
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn extract_learned_replacements(original: &str, corrected: &str) -> Vec<WordReplacement> {
        super::extract_learned_replacements(original, corrected, &[])
    }

    fn learned_pairs(v: &[WordReplacement]) -> Vec<(String, String)> {
        v.iter().map(|r| (r.from.clone(), r.to.clone())).collect()
    }

    #[test]
    fn extractor_learns_a_clean_mishear() {
        let out = extract_learned_replacements("I asked clawed to help", "I asked Claude to help");
        assert_eq!(
            learned_pairs(&out),
            vec![("clawed".to_string(), "Claude".to_string())]
        );
        assert!(out[0].whole_word && !out[0].case_sensitive);
        assert!(!out[0].preserve_replacement_case);
    }

    #[test]
    fn extractor_ignores_unchanged_text() {
        assert!(extract_learned_replacements("no edits here", "no edits here").is_empty());
    }

    #[test]
    fn extractor_ignores_pure_case_change() {
        assert!(extract_learned_replacements("hello world", "Hello world").is_empty());
    }

    #[test]
    fn extractor_ignores_common_to_common_swap() {
        // "their" -> "there": a homophone/grammar edit between two common words,
        // not a mishear. Learning it would rewrite every future "their".
        assert!(extract_learned_replacements("their cat sat", "there cat sat").is_empty());
        assert!(
            extract_learned_replacements("please accept this", "please except this").is_empty()
        );
        assert!(extract_learned_replacements("good advice today", "good advise today").is_empty());
    }

    #[test]
    fn extractor_rejects_cross_category_semantic_swaps() {
        assert!(extract_learned_replacements("open browser now", "open Firefox now").is_empty());
        assert!(extract_learned_replacements("meet monday", "meet today").is_empty());
        assert!(extract_learned_replacements("ask OpenAI now", "ask ChatGPT now").is_empty());
        assert!(extract_learned_replacements("open figma now", "open gitlab now").is_empty());
    }

    #[test]
    fn extractor_drops_contradicted_heard_word() {
        // "clawed" corrected two different ways in one transcript -> learn neither.
        let out = extract_learned_replacements(
            "clawed here and clawed there",
            "Claude here and Cloud there",
        );
        assert!(
            out.is_empty(),
            "contradicted pairs must be dropped, got {out:?}"
        );
        assert!(extract_learned_replacements(
            "clawed here and clawed there",
            "Claude here and Cloud Code there"
        )
        .is_empty());
    }

    #[test]
    fn extractor_ignores_insertion_and_deletion() {
        assert!(extract_learned_replacements("hello world", "hello big world").is_empty());
        assert!(extract_learned_replacements("hello big world", "hello world").is_empty());
    }

    #[test]
    fn extractor_trims_surrounding_punctuation() {
        let out = extract_learned_replacements("I saw clawed.", "I saw Claude.");
        assert_eq!(
            learned_pairs(&out),
            vec![("clawed".to_string(), "Claude".to_string())]
        );
    }

    #[test]
    fn extractor_dedupes_repeated_pair() {
        let out = extract_learned_replacements("clawed and clawed", "Claude and Claude");
        assert_eq!(
            learned_pairs(&out),
            vec![("clawed".to_string(), "Claude".to_string())]
        );
    }

    #[test]
    fn extractor_merges_compatible_casing_variants() {
        let out = extract_learned_replacements("Sonet and sonet", "sonnet and sonnet");
        assert_eq!(
            learned_pairs(&out),
            vec![("Sonet".to_string(), "sonnet".to_string())]
        );
        assert!(out[0].preserve_replacement_case);
        assert_eq!(
            apply_replacements("Sonet and sonet", &out),
            "sonnet and sonnet"
        );
    }

    #[test]
    fn extractor_ignores_numeric_only_change() {
        assert!(extract_learned_replacements("section 204", "section 205").is_empty());
    }

    #[test]
    fn extractor_ignores_single_character_change() {
        assert!(extract_learned_replacements("grade a work", "grade b work").is_empty());
    }

    #[test]
    fn extractor_learns_multiple_distinct_pairs_in_reading_order() {
        let out = extract_learned_replacements("clawed wrote a sonet", "Claude wrote a sonnet");
        assert_eq!(
            learned_pairs(&out),
            vec![
                ("clawed".to_string(), "Claude".to_string()),
                ("sonet".to_string(), "sonnet".to_string()),
            ]
        );
    }

    #[test]
    fn extractor_keeps_symbol_bearing_term_intact() {
        // A symbol-bearing term is learned with its symbol, not stripped to its
        // letters. Narrowing this to `stagign -> staging` would miss that the hyphen
        // is part of the technical token rather than prose punctuation.
        let out = extract_learned_replacements("use stagign-api today", "use staging-api today");
        assert_eq!(
            learned_pairs(&out),
            vec![("stagign-api".to_string(), "staging-api".to_string())]
        );
        assert!(extract_learned_replacements(
            "deploy staging-api today",
            "deploy production-api today"
        )
        .is_empty());
    }

    #[test]
    fn extractor_keeps_leading_dot_so_a_case_edit_is_not_a_new_word() {
        // `.env -> .ENV` keeps its leading dot, so both sides share one anchor key
        // (cores match case-insensitively) and it is never a substitution
        // candidate -- it is never narrowed to a bare `env -> ENV`.
        assert!(
            extract_learned_replacements("edit the .env file", "edit the .ENV file").is_empty()
        );
    }

    #[test]
    fn extractor_never_learns_from_a_punctuation_only_token() {
        // An empty or all-punctuation `from` has a sub-two-char core, so the shape
        // guard drops it. Locks the invariant that a learned `from` is always a
        // real word `apply_replacements` can match on.
        assert!(extract_learned_replacements("I saw !!!", "I saw Claude").is_empty());
        assert!(extract_learned_replacements("", "").is_empty());
        assert!(extract_learned_replacements("...", "Claude").is_empty());
    }

    #[test]
    fn extractor_rejects_a_possessive_inflection_edit() {
        // `Claude` -> `Claude's` is a grammar fix, not a mishear; learning it would rewrite
        // every future `Claude` into `Claude's`.
        assert!(extract_learned_replacements("ask Claude team", "ask Claude's team").is_empty());
    }

    #[test]
    fn extractor_rejects_a_possessive_edit_with_a_curly_apostrophe() {
        // In-place edits typed on macOS/iOS use a curly apostrophe (U+2019). Contraction matching
        // normalizes that form, so it cannot slip through as a new word.
        assert!(
            extract_learned_replacements("ask Claude team", "ask Claude\u{2019}s team").is_empty()
        );
    }

    #[test]
    fn extractor_rejects_a_version_bump_on_an_identifier() {
        // `GPT-4` -> `GPT-5` differs only in its version digit; learning it would rewrite every
        // later mention of GPT-4. (The bare-number case `204`->`205` is already dropped by the
        // shape guard; this is the alphanumeric sibling.)
        assert!(extract_learned_replacements("try GPT-4 first", "try GPT-5 first").is_empty());
        assert!(extract_learned_replacements("try GPT4 first", "try GPT-5 first").is_empty());
        assert!(extract_learned_replacements("try GPT-٤ first", "try GPT-٥ first").is_empty());
        assert!(extract_learned_replacements("try GPT-４ first", "try GPT-５ first").is_empty());
        assert!(extract_learned_replacements("try GPT-4 first", "try GPT-4o first").is_empty());
    }

    #[test]
    fn extractor_rejects_a_word_kept_unchanged_elsewhere() {
        // The user corrected the first `cloud` but kept the second, so `cloud` is not a
        // universal mishear -- learning `cloud`->`Claude` would corrupt the kept occurrence.
        assert!(extract_learned_replacements(
            "ask cloud about the cloud",
            "ask Claude about the cloud"
        )
        .is_empty());
    }

    #[test]
    fn extractor_does_not_learn_from_a_word_reorder() {
        // A pure reorder leaves every token present on both sides, so no shuffled gap is a real
        // correction even though the LCS gaps look one-for-one.
        assert!(
            extract_learned_replacements("Alice Bob Charlie Delta", "Bob Alice Delta Charlie")
                .is_empty()
        );
        assert!(extract_learned_replacements("Alice Bob Charlie", "Delta Bob Alice").is_empty());
    }

    #[test]
    fn extractor_learns_correcting_one_name_to_match_another() {
        // The heard word `jon` is gone from the output, so this is a real one-way mishear even
        // though the meant word `John` already appeared elsewhere in the original -- the
        // kept-unchanged guard keys on the heard word surviving, not on the target pre-existing.
        let out =
            extract_learned_replacements("send it to John from jon", "send it to John from John");
        assert_eq!(
            learned_pairs(&out),
            vec![("jon".to_string(), "John".to_string())]
        );
    }

    #[test]
    fn extractor_rejects_a_common_contraction_edit() {
        // `were` -> `we're` is a grammar correction between two common forms. Learning it would
        // corrupt every future valid use, such as `we were going` -> `we we're going`.
        assert!(
            extract_learned_replacements("i think were going", "i think we're going").is_empty()
        );
        assert!(
            extract_learned_replacements("i think were going", "i think we’re going").is_empty()
        );
    }

    #[test]
    fn extractor_rejects_contractions_of_unknown_names() {
        assert!(extract_learned_replacements("ask Claude today", "ask Claude'll today").is_empty());
        assert!(extract_learned_replacements("ask Claude today", "ask Claude’ll today").is_empty());
    }

    #[test]
    fn extractor_rejects_diacritic_only_edits() {
        assert!(
            extract_learned_replacements("read the expose today", "read the exposé today")
                .is_empty()
        );
    }

    #[test]
    fn extractor_rejects_runtime_equivalent_unicode_forms() {
        // Rust regex's case-insensitive matching treats long-s (U+017F) as an `s`. Persisting this
        // visually different edit would therefore rewrite every ordinary `sample` at runtime.
        assert!(extract_learned_replacements("use ſample today", "use Sample today").is_empty());
    }

    #[test]
    fn extractor_rejects_conflicting_runtime_equivalent_sources() {
        // These source spellings are distinct under `to_lowercase()` but identical to the
        // case-insensitive regex used by production. Keeping both would let the first rule shadow
        // the second and silently apply the wrong target.
        assert!(
            extract_learned_replacements("ſaaaaaa keep saaaaaa", "ſaaaaab keep saaaaac",)
                .is_empty()
        );
    }

    #[test]
    fn extractor_rejects_mixed_script_changes_as_spelling_evidence() {
        // ASCII-only phonetic projection must not discard the Greek character that actually
        // changed and then treat the identical `foo` suffix as perfect mishear evidence.
        assert!(extract_learned_replacements("use αfoo today", "use βfoo today").is_empty());
        // A long shared prefix can otherwise push Jaro-Winkler above the spelling threshold.
        assert!(
            extract_learned_replacements("use fooooooα today", "use fooooooβ today").is_empty()
        );
    }

    #[test]
    fn extractor_rejects_ambiguous_presentation_case() {
        assert!(extract_learned_replacements("Helo there", "Hello there").is_empty());
        assert!(
            extract_learned_replacements("Goodbye. Helo there", "Goodbye. Hello there").is_empty()
        );
        assert!(extract_learned_replacements("FIX SONET TODAY", "FIX SONNET TODAY").is_empty());
        assert!(
            extract_learned_replacements("Aple released updates", "Apple released updates")
                .is_empty()
        );
        assert!(
            extract_learned_replacements("meet Monday morning", "meet Tuesday morning").is_empty()
        );
        assert!(
            extract_learned_replacements("meet monday morning", "meet tuesday morning").is_empty()
        );
        assert!(extract_learned_replacements("visit Paris", "visit London").is_empty());
        assert!(extract_learned_replacements("visit paris", "visit london").is_empty());
        assert!(extract_learned_replacements("ask USA", "ask UK").is_empty());

        let lowercase = extract_learned_replacements("fix sonet today", "fix sonnet today");
        assert_eq!(
            learned_pairs(&lowercase),
            vec![("sonet".to_string(), "sonnet".to_string())]
        );

        let explicit_lowercase =
            extract_learned_replacements("fix Sonet today", "fix sonnet today");
        assert_eq!(
            learned_pairs(&explicit_lowercase),
            vec![("Sonet".to_string(), "sonnet".to_string())]
        );
        assert!(explicit_lowercase[0].preserve_replacement_case);
        assert_eq!(apply_replacements("Sonet", &explicit_lowercase), "sonnet");

        let symbol_led = extract_learned_replacements("fix .Sonet today", "fix .sonnet today");
        assert_eq!(
            learned_pairs(&symbol_led),
            vec![(".Sonet".to_string(), ".sonnet".to_string())]
        );
        assert!(symbol_led[0].preserve_replacement_case);
        assert_eq!(apply_replacements(".SONET", &symbol_led), ".sonnet");
    }

    #[test]
    fn extractor_rejects_targets_that_add_outer_syntax() {
        assert!(extract_learned_replacements("run prent today", "run print() today").is_empty());
        assert!(extract_learned_replacements("ask dokter tomorrow", "ask Dr. tomorrow").is_empty());
        assert!(extract_learned_replacements("run prent.", "run print().").is_empty());
        assert!(extract_learned_replacements("open clawed", "open Claude.md").is_empty());
        assert!(extract_learned_replacements("ask clawed", "ask Claude-Code").is_empty());
    }

    #[test]
    fn extractor_rejects_targets_with_conflicting_semantic_case() {
        assert!(extract_learned_replacements("opnai and opnai", "OpenAI and Openai").is_empty());
    }

    #[test]
    fn extractor_preserves_a_dictionary_homograph_used_as_a_name() {
        assert!(extract_learned_replacements("Use Aple Music", "Use Apple Music").is_empty());
        assert!(extract_learned_replacements("Aple Music", "Apple Music").is_empty());

        // Lowercase-to-titlecase is an explicit semantic case edit rather than presentation copied
        // from the source token, so it can safely retain the exact name.
        let learned = extract_learned_replacements("Use aple Music", "Use Apple Music");
        assert_eq!(
            learned_pairs(&learned),
            vec![("aple".to_string(), "Apple".to_string())]
        );
        assert!(learned[0].preserve_replacement_case);
        assert_eq!(apply_replacements("try aple", &learned), "try Apple");
    }

    #[test]
    fn extractor_rejects_a_dictionary_homograph_corrected_to_a_name() {
        // `angel` is both an ordinary dictionary word and a known name. Even a close
        // phonetic/name match cannot safely become a global rule because it would rewrite
        // later ordinary uses of the word.
        assert!(extract_learned_replacements("ask angel", "ask Angela").is_empty());
        assert!(extract_learned_replacements("ask rose", "ask Roz").is_empty());
    }

    #[test]
    fn extractor_rejects_a_source_retained_inside_a_runtime_word_boundary() {
        assert!(extract_learned_replacements(
            "ask cloud about cloud-based storage",
            "ask Claude about cloud-based storage"
        )
        .is_empty());
        assert!(extract_learned_replacements(
            "ask cloud about cloud's storage",
            "ask Claude about cloud's storage"
        )
        .is_empty());
        let learned = extract_learned_replacements(
            "ask cloud about foo's storage",
            "ask Claude about cloud's storage",
        );
        assert!(learned.is_empty(), "unexpected learned rules: {learned:?}");
    }

    #[test]
    fn extractor_learns_an_adjacent_typo_when_its_target_follows() {
        let learned = extract_learned_replacements("klohd Claude", "Claude Claude");
        assert_eq!(
            learned_pairs(&learned),
            vec![("klohd".to_string(), "Claude".to_string())]
        );
    }

    #[test]
    fn extractor_rejects_a_rule_that_would_cascade_from_a_preceding_target() {
        let preceding = vec![repl("foo", "bar-baz")];
        assert!(
            super::extract_learned_replacements("use bar today", "use qux today", &preceding)
                .is_empty()
        );

        let contextual_preceding = vec![repl("foo", "bar")];
        assert!(super::extract_learned_replacements(
            "use bar-baz today",
            "use qux-quux today",
            &contextual_preceding,
        )
        .is_empty());

        assert!(super::extract_learned_replacements(
            "use bar-baz today",
            "use qux-baz today",
            &[repl("foo-bar", "x-bar")],
        )
        .is_empty());
        assert!(super::extract_learned_replacements(
            "use y-bar today",
            "use y-qux today",
            &[repl("foo", "bar-x")],
        )
        .is_empty());

        assert!(super::extract_learned_replacements(
            "fix sonet today",
            "fix sonnet today",
            &[repl("x-sonet", "x-poem")],
        )
        .is_empty());

        let mut case_sensitive_preemption = repl("SONET", "poem");
        case_sensitive_preemption.case_sensitive = true;
        assert!(super::extract_learned_replacements(
            "fix sonet today",
            "fix sonnet today",
            &[case_sensitive_preemption],
        )
        .is_empty());

        let mut shorter_case_sensitive_preemption = repl("SONET", "poem");
        shorter_case_sensitive_preemption.case_sensitive = true;
        assert!(super::extract_learned_replacements(
            "fix sonet-api today",
            "fix sonnet-api today",
            &[shorter_case_sensitive_preemption],
        )
        .is_empty());

        assert!(super::extract_learned_replacements(
            "fix bar-baz today",
            "fix bar-bazz today",
            &[repl("foo-bar", "foo-qux")],
        )
        .is_empty());

        let mut substring_deletion = repl("-", "");
        substring_deletion.whole_word = false;
        assert!(super::extract_learned_replacements(
            "use clawd today",
            "use Claude today",
            &[substring_deletion],
        )
        .is_empty());

        let whole_word_deletion = repl("foo", "");
        assert!(super::extract_learned_replacements(
            "use bar- today",
            "use baz- today",
            &[whole_word_deletion],
        )
        .is_empty());

        let mut boundary_changer = repl("_", "-");
        boundary_changer.whole_word = false;
        assert!(super::extract_learned_replacements(
            "use clawd today",
            "use Claude today",
            &[boundary_changer],
        )
        .is_empty());

        assert!(super::extract_learned_replacements(
            "use .clawd today",
            "use .Claude today",
            &[repl("foo", "bar.")],
        )
        .is_empty());

        assert!(super::extract_learned_replacements(
            "use SSAMPLE today",
            "use Stample today",
            &[repl("foo", "ßample")],
        )
        .is_empty());

        // Runtime case adaptation can emit a form that neither literal target contains. The first
        // pair turns title-cased `Xample` into `Sample`, which would then activate the second pair.
        assert!(super::extract_learned_replacements(
            "xample here and sample",
            "ßample here and stample",
            &[],
        )
        .is_empty());

        // Dotless-i uppercases to `I` even though regex simple folding does not equate `ı` and `i`.
        assert!(super::extract_learned_replacements(
            "xample here and iample",
            "ıample here and izample",
            &[],
        )
        .is_empty());

        // An old user-authored cascade does not block an unrelated learned correction: only pairs
        // whose later rule is new are considered instead of requiring the old list to be pristine.
        let already_cascading = vec![repl("foo", "bar"), repl("bar", "baz")];
        let unrelated = super::extract_learned_replacements(
            "ask clawd today",
            "ask Claude today",
            &already_cascading,
        );
        assert_eq!(
            learned_pairs(&unrelated),
            vec![("clawd".to_string(), "Claude".to_string())]
        );

        // Earlier rules do not revisit a target emitted by a later rule, so this ordering is safe.
        let earlier_zorp_rule = vec![repl("zorp", "qux")];
        let later_target = super::extract_learned_replacements(
            "use zopr today",
            "use zorp today",
            &earlier_zorp_rule,
        );
        assert_eq!(
            learned_pairs(&later_target),
            vec![("zopr".to_string(), "zorp".to_string())]
        );
    }

    #[test]
    fn extractor_rejects_an_lcs_table_above_the_cell_budget() {
        let original = std::iter::repeat_n("same", 1_000)
            .chain(std::iter::once("clawd"))
            .collect::<Vec<_>>()
            .join(" ");
        let corrected = std::iter::repeat_n("same", 1_000)
            .chain(std::iter::once("Claude"))
            .collect::<Vec<_>>()
            .join(" ");

        assert!(extract_learned_replacements(&original, &corrected).is_empty());
    }

    #[test]
    fn extractor_rejects_too_many_candidates_for_the_cascade_scan() {
        let original = std::iter::repeat_n("clawd steady", 65)
            .collect::<Vec<_>>()
            .join(" ");
        let corrected = std::iter::repeat_n("Claude steady", 65)
            .collect::<Vec<_>>()
            .join(" ");

        assert!(extract_learned_replacements(&original, &corrected).is_empty());
    }

    #[test]
    fn extractor_rejects_oversized_input_and_tokens() {
        let oversized_source = "a".repeat(257);
        let oversized_target = "b".repeat(257);
        assert!(extract_learned_replacements(&oversized_source, &oversized_target).is_empty());

        let padding = "x".repeat(140);
        let original = std::iter::once("clawed".to_string())
            .chain(std::iter::repeat_n(padding.clone(), 499))
            .collect::<Vec<_>>()
            .join(" ");
        let corrected = std::iter::once("Claude".to_string())
            .chain(std::iter::repeat_n(padding, 499))
            .collect::<Vec<_>>()
            .join(" ");
        assert!(extract_learned_replacements(&original, &corrected).is_empty());
    }

    #[test]
    fn extractor_rejects_pairs_that_would_cascade_when_applied() {
        // These pairs would be applied sequentially as crowd->cloud->Claude, so learning both
        // would silently turn a future standalone "crowd" into "Claude".
        assert!(
            extract_learned_replacements("crowd here and cloud", "cloud here and Claude")
                .is_empty()
        );
        assert!(extract_learned_replacements("use foo and bar", "use bar-baz and qux").is_empty());

        // The shorter rule runs first and would rewrite the source of the compound rule before the
        // latter gets a chance to match.
        assert!(
            extract_learned_replacements("use aple and aple-pay", "use Apple and ample-pay",)
                .is_empty()
        );
    }

    #[test]
    fn extractor_bounds_contextual_cascade_match_work() {
        let preceding = (0..80)
            .map(|index| repl(&format!("q{index}q"), &format!("z{index}z")))
            .collect::<Vec<_>>();
        let from = format!("{}b", "a".repeat(239));
        let to = format!("{}c", "a".repeat(239));

        assert!(super::extract_learned_replacements(&from, &to, &preceding).is_empty());
    }

    #[test]
    fn extractor_ignores_unicode_dash_punctuation_edits() {
        assert!(extract_learned_replacements("hello there", "—hello there").is_empty());
        assert!(extract_learned_replacements("hello there", "hello— there").is_empty());
    }

    #[test]
    fn extractor_rejects_punctuation_only_edits() {
        assert!(extract_learned_replacements("the US market", "the U.S. market").is_empty());
        assert!(extract_learned_replacements("tell us tomorrow", "tell U.S. tomorrow").is_empty());
    }

    #[test]
    fn extractor_rejects_inflection_only_edits_for_unknown_terms() {
        assert!(extract_learned_replacements("these API work", "these APIs work").is_empty());
        assert!(extract_learned_replacements("ask Claude now", "ask Claudes now").is_empty());
        assert!(extract_learned_replacements("we Zorp daily", "we Zorped daily").is_empty());
        assert!(extract_learned_replacements("start Zorp now", "start Zorping now").is_empty());
    }

    #[test]
    fn extractor_rejects_inflections_inside_compound_terms() {
        assert!(
            extract_learned_replacements("visit my mother-in-law", "visit my mothers-in-law",)
                .is_empty()
        );
        assert!(extract_learned_replacements("use Zorp-api", "use Zorps-api").is_empty());
        assert!(extract_learned_replacements("open API/client", "open APIs/client").is_empty());
    }

    #[test]
    fn extractor_rejects_homographs_inside_compound_terms() {
        assert!(extract_learned_replacements("ask angel-like", "ask Angela-like").is_empty());
        assert!(extract_learned_replacements("ask rose-api", "ask Roz-api").is_empty());
    }

    #[test]
    fn extractor_learns_a_target_word_that_naturally_ends_in_s() {
        // Only apostrophe-bearing contraction suffixes are handled here, so a meant word whose
        // canonical spelling ends in `s` (`io` -> `iOS`) is not mistaken for one.
        let out = extract_learned_replacements("open io settings", "open iOS settings");
        assert_eq!(
            learned_pairs(&out),
            vec![("io".to_string(), "iOS".to_string())]
        );
        assert!(out[0].preserve_replacement_case);
    }

    #[test]
    fn learned_replacement_preserves_intentional_mixed_case() {
        let learned = extract_learned_replacements("open io settings", "open iOS settings");
        assert_eq!(apply_replacements("io Io IO", &learned), "iOS iOS iOS");
    }

    #[test]
    fn extractor_learns_a_correction_that_adds_digits() {
        // A short dictated acronym can gain its single canonical digit, but a product version
        // addition is a semantic edit and must not become a global rule.
        let out = extract_learned_replacements("the mp format", "the MP3 format");
        assert_eq!(
            learned_pairs(&out),
            vec![("mp".to_string(), "MP3".to_string())]
        );
        assert!(out[0].case_sensitive);
        assert_eq!(apply_replacements("mp MP", &out), "MP3 MP");

        let b2b = extract_learned_replacements("a bb vendor", "a B2B vendor");
        assert_eq!(
            learned_pairs(&b2b),
            vec![("bb".to_string(), "B2B".to_string())]
        );
        assert!(b2b[0].case_sensitive);
        assert_eq!(apply_replacements("bb BB", &b2b), "B2B BB");
        assert!(extract_learned_replacements("say no today", "say NO2 today").is_empty());
        assert!(extract_learned_replacements("ask ai today", "ask AI2 today").is_empty());
        assert!(extract_learned_replacements("ask uk today", "ask UK2 today").is_empty());
        assert!(extract_learned_replacements("buy iphone now", "buy iphone16 now").is_empty());
    }

    #[test]
    fn extractor_keeps_distinct_case_sensitive_digit_rules() {
        let out = extract_learned_replacements("Mp and mp", "MP3 and MP3");
        assert_eq!(
            learned_pairs(&out),
            vec![
                ("Mp".to_string(), "MP3".to_string()),
                ("mp".to_string(), "MP3".to_string()),
            ]
        );
        assert!(out.iter().all(|rule| rule.case_sensitive));
        assert_eq!(apply_replacements("Mp mp MP", &out), "MP3 MP3 MP");
    }

    #[test]
    fn extractor_allows_case_distinct_preceding_targets_for_digit_rules() {
        let preceding = vec![repl("foo", "MP")];
        let out = super::extract_learned_replacements("use mp today", "use MP3 today", &preceding);
        assert_eq!(
            learned_pairs(&out),
            vec![("mp".to_string(), "MP3".to_string())]
        );
        assert!(out[0].case_sensitive);
    }

    #[test]
    fn test_apply_custom_words_exact_match() {
        let text = "hello world";
        let custom_words = vec!["Hello".to_string(), "World".to_string()];
        let result = apply_custom_words(text, &custom_words, 0.5);
        assert_eq!(result, "Hello World");
    }

    #[test]
    fn test_apply_custom_words_fuzzy_match() {
        let text = "helo wrold";
        let custom_words = vec!["hello".to_string(), "world".to_string()];
        let result = apply_custom_words(text, &custom_words, 0.5);
        assert_eq!(result, "hello world");
    }

    #[test]
    fn test_apply_custom_words_prefers_longest_exact_phrase() {
        // Both "New York" and "New York Times" match exactly (score 0); the longer, more specific
        // dictionary phrase must win rather than the shorter prefix.
        let custom_words = vec!["New York".to_string(), "New York Times".to_string()];
        let result = apply_custom_words("new york times", &custom_words, 0.18);
        assert_eq!(result, "New York Times");
    }

    #[test]
    fn test_preserve_case_pattern() {
        assert_eq!(preserve_case_pattern("HELLO", "world"), "WORLD");
        assert_eq!(preserve_case_pattern("Hello", "world"), "World");
        assert_eq!(preserve_case_pattern("hello", "WORLD"), "WORLD");
    }

    #[test]
    fn test_extract_punctuation() {
        assert_eq!(extract_punctuation("hello"), ("", ""));
        assert_eq!(extract_punctuation("!hello?"), ("!", "?"));
        assert_eq!(extract_punctuation("...hello..."), ("...", "..."));
    }

    #[test]
    fn test_empty_custom_words() {
        let text = "hello world";
        let custom_words = vec![];
        let result = apply_custom_words(text, &custom_words, 0.5);
        assert_eq!(result, "hello world");
    }

    #[test]
    fn test_filter_filler_words() {
        let text = "So uhm I was thinking uh about this";
        let result = filter_transcription_output(text, "en", &None);
        assert_eq!(result, "So I was thinking about this");
    }

    #[test]
    fn test_filter_filler_words_case_insensitive() {
        let text = "UHM this is UH a test";
        let result = filter_transcription_output(text, "en", &None);
        assert_eq!(result, "this is a test");
    }

    #[test]
    fn test_filter_filler_words_with_punctuation() {
        let text = "Well, uhm, I think, uh. that's right";
        let result = filter_transcription_output(text, "en", &None);
        assert_eq!(result, "Well, I think, that's right");
    }

    #[test]
    fn test_filter_cleans_whitespace() {
        let text = "Hello    world   test";
        let result = filter_transcription_output(text, "en", &None);
        assert_eq!(result, "Hello world test");
    }

    #[test]
    fn test_filter_trims() {
        let text = "  Hello world  ";
        let result = filter_transcription_output(text, "en", &None);
        assert_eq!(result, "Hello world");
    }

    #[test]
    fn test_filter_combined() {
        let text = "  Uhm, so I was, uh, thinking about this  ";
        let result = filter_transcription_output(text, "en", &None);
        assert_eq!(result, "so I was, thinking about this");
    }

    #[test]
    fn test_filter_preserves_valid_text() {
        let text = "This is a completely normal sentence.";
        let result = filter_transcription_output(text, "en", &None);
        assert_eq!(result, "This is a completely normal sentence.");
    }

    #[test]
    fn test_filter_stutter_collapse() {
        let text = "w wh wh wh wh wh wh wh wh wh why";
        let result = filter_transcription_output(text, "en", &None);
        assert_eq!(result, "w wh why");
    }

    #[test]
    fn test_filter_stutter_short_words() {
        let text = "I I I I think so so so so";
        let result = filter_transcription_output(text, "en", &None);
        assert_eq!(result, "I think so");
    }

    #[test]
    fn test_filter_stutter_longer_words() {
        let text = "Check data doc doc doc doc documentation.";
        let result = filter_transcription_output(text, "en", &None);
        assert_eq!(result, "Check data doc documentation.");
    }

    #[test]
    fn test_filter_stutter_mixed_case() {
        let text = "No NO no NO no";
        let result = filter_transcription_output(text, "en", &None);
        assert_eq!(result, "No");
    }

    #[test]
    fn test_filter_stutter_preserves_two_repetitions() {
        let text = "no no is fine";
        let result = filter_transcription_output(text, "en", &None);
        assert_eq!(result, "no no is fine");
    }

    #[test]
    fn test_filter_english_removes_um() {
        let text = "um I think um this is good";
        let result = filter_transcription_output(text, "en", &None);
        assert_eq!(result, "I think this is good");
    }

    #[test]
    fn test_filter_portuguese_preserves_um() {
        // "um" means "a/an" in Portuguese
        let text = "um gato bonito";
        let result = filter_transcription_output(text, "pt", &None);
        assert_eq!(result, "um gato bonito");
    }

    #[test]
    fn test_filter_spanish_preserves_ha() {
        // "ha" means "has" in Spanish
        let text = "ha sido un buen día";
        let result = filter_transcription_output(text, "es", &None);
        assert_eq!(result, "ha sido un buen día");
    }

    #[test]
    fn test_filter_language_code_with_region() {
        // "pt-BR" should normalize to "pt"
        let text = "um gato bonito";
        let result = filter_transcription_output(text, "pt-BR", &None);
        assert_eq!(result, "um gato bonito");
    }

    #[test]
    fn test_filter_custom_filler_words_override() {
        let custom = Some(vec!["okay".to_string(), "right".to_string()]);
        let text = "okay so I think right this works";
        let result = filter_transcription_output(text, "en", &custom);
        assert_eq!(result, "so I think this works");
    }

    #[test]
    fn test_filter_custom_filler_words_empty_disables() {
        let custom = Some(vec![]);
        let text = "So uhm I was thinking uh about this";
        let result = filter_transcription_output(text, "en", &custom);
        // No filler words removed since custom list is empty
        assert_eq!(result, "So uhm I was thinking uh about this");
    }

    #[test]
    fn test_filter_unknown_language_uses_fallback() {
        let text = "uh I think uhm this works";
        let result = filter_transcription_output(text, "xx", &None);
        assert_eq!(result, "I think this works");
    }

    #[test]
    fn test_filter_fallback_does_not_remove_um() {
        // Fallback (unknown language) should not remove "um" since it's a real word in some languages
        let text = "um I think this works";
        let result = filter_transcription_output(text, "xx", &None);
        assert_eq!(result, "um I think this works");
    }

    #[test]
    fn test_apply_custom_words_ngram_two_words() {
        let text = "il cui nome è Charge B, che permette";
        let custom_words = vec!["ChargeBee".to_string()];
        let result = apply_custom_words(text, &custom_words, 0.5);
        assert!(result.contains("ChargeBee,"));
        assert!(!result.contains("Charge B"));
    }

    #[test]
    fn test_apply_custom_words_ngram_three_words() {
        let text = "use Chat G P T for this";
        let custom_words = vec!["ChatGPT".to_string()];
        let result = apply_custom_words(text, &custom_words, 0.5);
        assert!(result.contains("ChatGPT"));
    }

    #[test]
    fn test_apply_custom_words_prefers_longer_ngram() {
        let text = "Open AI GPT model";
        let custom_words = vec!["OpenAI".to_string(), "GPT".to_string()];
        let result = apply_custom_words(text, &custom_words, 0.5);
        assert_eq!(result, "OpenAI GPT model");
    }

    #[test]
    fn test_apply_custom_words_ngram_preserves_case() {
        let text = "CHARGE B is great";
        let custom_words = vec!["ChargeBee".to_string()];
        let result = apply_custom_words(text, &custom_words, 0.5);
        assert!(result.contains("CHARGEBEE"));
    }

    #[test]
    fn test_apply_custom_words_ngram_with_spaces_in_custom() {
        // Custom word with space should also match against split words
        let text = "using Mac Book Pro";
        let custom_words = vec!["MacBook Pro".to_string()];
        let result = apply_custom_words(text, &custom_words, 0.5);
        assert!(result.contains("MacBook"));
    }

    #[test]
    fn test_apply_custom_words_trailing_number_not_doubled() {
        // Verify that trailing non-alpha chars (like numbers) aren't double-counted
        // between build_ngram stripping them and extract_punctuation capturing them
        let text = "use GPT4 for this";
        let custom_words = vec!["GPT-4".to_string()];
        let result = apply_custom_words(text, &custom_words, 0.5);
        // Should NOT produce "GPT-44" (double-counting the trailing 4)
        assert!(
            !result.contains("GPT-44"),
            "got double-counted result: {}",
            result
        );
    }

    // --- Precision-first matcher: regressions the old Soundex matcher introduced (#18) ---

    #[test]
    fn test_matcher_does_not_overwrite_common_words() {
        // None of these everyday words may be replaced by the phonetically-near brand.
        let cases = [
            ("I deployed to the cloud today", "Claude"),
            ("we use the region us-east", "Legion"),
            ("I was working all day", "Workday"),
            ("that was really good", "rally"),
        ];
        for (text, brand) in cases {
            let custom_words = vec![brand.to_string()];
            let result = apply_custom_words(text, &custom_words, 0.18);
            assert_eq!(result, text, "common word was overwritten by {}", brand);
        }
    }

    #[test]
    fn test_matcher_recases_exact_dictation() {
        // Exact (modulo case) matches still apply -- this is the safe recasing path.
        let result = apply_custom_words("I opened codex", &["Codex".to_string()], 0.18);
        assert_eq!(result, "I opened Codex");
    }

    // --- Deterministic replacement map (#18 / the real "clawed" -> "Claude" fix) ---

    fn repl(from: &str, to: &str) -> WordReplacement {
        WordReplacement {
            from: from.to_string(),
            to: to.to_string(),
            whole_word: true,
            case_sensitive: false,
            preserve_replacement_case: false,
        }
    }

    #[test]
    fn test_apply_replacements_basic() {
        let result = apply_replacements("Open clawed.md please", &[repl("clawed", "Claude")]);
        assert_eq!(result, "Open Claude.md please");
    }

    #[test]
    fn test_apply_replacements_preserves_case() {
        assert_eq!(
            apply_replacements("CLAWED is here", &[repl("clawed", "Claude")]),
            "CLAUDE is here"
        );
        assert_eq!(
            apply_replacements("Clawed is here", &[repl("clawed", "Claude")]),
            "Claude is here"
        );
    }

    #[test]
    fn test_apply_replacements_preserves_caps_multiword() {
        // All-caps detection must ignore the inter-word space so shouted phrases stay shouted.
        assert_eq!(
            apply_replacements("CLOUD CODE rocks", &[repl("cloud code", "Claude Code")]),
            "CLAUDE CODE rocks"
        );
    }

    #[test]
    fn test_apply_replacements_multi_word() {
        let result =
            apply_replacements("use cloud code daily", &[repl("cloud code", "Claude Code")]);
        assert_eq!(result, "use Claude Code daily");
    }

    #[test]
    fn test_apply_replacements_whole_word_only() {
        // "cat" must not corrupt "category".
        let result = apply_replacements("a category of cat", &[repl("cat", "dog")]);
        assert_eq!(result, "a category of dog");
    }

    #[test]
    fn test_apply_replacements_deletion_cleans_spaces() {
        let result = apply_replacements("this is basically fine", &[repl("basically", "")]);
        assert_eq!(result, "this is fine");
    }

    #[test]
    fn test_apply_replacements_case_sensitive() {
        let r = WordReplacement {
            from: "WIP".to_string(),
            to: "work in progress".to_string(),
            whole_word: true,
            case_sensitive: true,
            preserve_replacement_case: false,
        };
        assert_eq!(
            apply_replacements("WIP and wip", &[r]),
            "work in progress and wip"
        );
    }

    #[test]
    fn test_apply_replacements_whole_word_trailing_punctuation() {
        // A whole-word phrase ending in punctuation still respects the right boundary: "C#" must
        // not rewrite the prefix inside "C#12", but must fire when the token stands alone.
        let r = WordReplacement {
            from: "C#".to_string(),
            to: "CSharp".to_string(),
            whole_word: true,
            case_sensitive: true,
            preserve_replacement_case: false,
        };
        assert_eq!(
            apply_replacements("C#12 ships", std::slice::from_ref(&r)),
            "C#12 ships"
        );
        assert_eq!(
            apply_replacements("I love C# code", &[r]),
            "I love CSharp code"
        );
    }

    #[test]
    fn test_apply_replacements_whole_word_leading_punctuation() {
        // A whole-word phrase starting with punctuation respects the left boundary: ".env" must not
        // match inside "foo.env", but must fire when the token stands alone.
        let r = WordReplacement {
            from: ".env".to_string(),
            to: "dotenv".to_string(),
            whole_word: true,
            case_sensitive: true,
            preserve_replacement_case: false,
        };
        assert_eq!(
            apply_replacements("edit foo.env now", std::slice::from_ref(&r)),
            "edit foo.env now"
        );
        assert_eq!(
            apply_replacements("edit the .env now", &[r]),
            "edit the dotenv now"
        );
    }

    #[test]
    fn test_apply_replacements_deletion_before_punctuation() {
        // Deleting a word adjacent to punctuation must not leave the punctuation detached.
        let r = WordReplacement {
            from: "basically".to_string(),
            to: String::new(),
            whole_word: true,
            case_sensitive: false,
            preserve_replacement_case: false,
        };
        assert_eq!(
            apply_replacements("this is basically, fine", std::slice::from_ref(&r)),
            "this is, fine"
        );
        assert_eq!(
            apply_replacements("stop basically. go", std::slice::from_ref(&r)),
            "stop. go"
        );
        // A deletion not adjacent to punctuation still collapses the doubled space cleanly.
        assert_eq!(
            apply_replacements("this is basically fine", &[r]),
            "this is fine"
        );
    }

    // --- Raw mode (#19) ---

    #[test]
    fn test_strip_to_raw_text_worked_example() {
        assert_eq!(
            strip_to_raw_text("Open Claude.md and read GPT-4 notes.", true),
            "open claude.md and read GPT-4 notes"
        );
    }

    #[test]
    fn test_strip_to_raw_text_preserves_acronyms_and_i() {
        assert_eq!(
            strip_to_raw_text("I'm using the API on a GPU now.", true),
            "I'm using the API on a GPU now"
        );
    }

    #[test]
    fn test_strip_to_raw_text_preserves_structural_punctuation() {
        assert_eq!(
            strip_to_raw_text("Email Me@Example.com about v0.1.0, well-known stuff!", true),
            "email me@example.com about v0.1.0 well-known stuff"
        );
    }

    #[test]
    fn test_strip_to_raw_text_forces_english_i_when_known_english() {
        // When the output is known to be English, a standalone "i" is always capitalized.
        assert_eq!(strip_to_raw_text("i am", true), "I am");
    }

    #[test]
    fn test_strip_to_raw_text_preserves_engine_casing_when_language_unknown() {
        // Auto-detect / non-English: keep the engine's casing instead of forcing English rules.
        // English engines emit a capital "I", so English stays correct...
        assert_eq!(strip_to_raw_text("I am", false), "I am");
        // ...but a language that uses a lowercase standalone "i" (Polish/Croatian "i" = "and") is
        // left lowercase rather than wrongly capitalized.
        assert_eq!(strip_to_raw_text("kot i pies", false), "kot i pies");
    }

    #[test]
    fn test_strip_to_raw_text_preserves_technical_symbols() {
        // Trailing technical symbols are part of the token, not sentence punctuation, so raw mode
        // keeps them (lowercased) rather than collapsing "C#"/"C++"/"F#" to "c"/"c"/"f".
        assert_eq!(
            strip_to_raw_text("I write C# and C++ and F#.", true),
            "I write c# and c++ and f#"
        );
    }

    #[test]
    fn test_strip_to_raw_text_preserves_windows_path() {
        // Interior punctuation is kept, including Windows drive/path separators -- only the
        // trailing sentence punctuation is stripped.
        assert_eq!(
            strip_to_raw_text("Open C:\\Users\\Joe please.", true),
            "open c:\\users\\joe please"
        );
    }
}
