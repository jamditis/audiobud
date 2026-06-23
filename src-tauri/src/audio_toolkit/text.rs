use crate::settings::WordReplacement;
use once_cell::sync::Lazy;
use regex::{Captures, Regex};
use rphonetic::{DoubleMetaphone, Encoder, Metaphone};
use std::collections::HashSet;
use strsim::{damerau_levenshtein, jaro_winkler};

/// Common English words used as a "do not overwrite a common word" veto in the matcher.
/// Loaded once from the bundled list; lines starting with `#` are provenance/comments.
static COMMON_WORDS: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    include_str!("common_words_en.txt")
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect()
});

static METAPHONE: Lazy<Metaphone> = Lazy::new(Metaphone::default);
static DOUBLE_METAPHONE: Lazy<DoubleMetaphone> = Lazy::new(DoubleMetaphone::default);

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

/// Applies deterministic literal replacements, in order, to the text.
///
/// Each [`WordReplacement`] maps an exact `from` phrase to an exact `to` output. This is the
/// safe path for large mishears the fuzzy matcher cannot recover without guessing (the
/// canonical case being "clawed" -> "Claude": 50% edit distance, phonetically distinct). It
/// runs for every engine, after fuzzy custom-word correction and before filler removal.
///
/// - `whole_word` (default true): matches only on word boundaries, so "cat" -> "dog" does
///   not corrupt "category".
/// - `case_sensitive` (default false): matching ignores case and the output adapts to the
///   matched text's case pattern (ALL CAPS / Capitalized / lower) via [`preserve_case_pattern`].
/// - An empty `to` deletes the match; spaces left dangling by a deletion (doubled, leading/trailing,
///   or stranded before punctuation) are cleaned up afterwards.
pub fn apply_replacements(text: &str, replacements: &[WordReplacement]) -> String {
    let mut result = text.to_string();
    let mut deleted_any = false;

    for replacement in replacements {
        if replacement.from.is_empty() {
            continue;
        }

        let escaped = regex::escape(&replacement.from);
        let pattern = if replacement.case_sensitive {
            escaped
        } else {
            format!("(?i){}", escaped)
        };

        let Ok(re) = Regex::new(&pattern) else {
            continue;
        };

        let to = replacement.to.clone();
        let case_sensitive = replacement.case_sensitive;
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
                if case_sensitive {
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

#[cfg(test)]
mod tests {
    use super::*;

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
        };
        assert_eq!(apply_replacements("C#12 ships", &[r.clone()]), "C#12 ships");
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
        };
        assert_eq!(
            apply_replacements("edit foo.env now", &[r.clone()]),
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
        };
        assert_eq!(
            apply_replacements("this is basically, fine", &[r.clone()]),
            "this is, fine"
        );
        assert_eq!(
            apply_replacements("stop basically. go", &[r.clone()]),
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
