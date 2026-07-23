//! Deterministic English number formatting (inverse text normalization).
//!
//! Speech engines (Whisper, Parakeet, …) spell numbers out in full — "twenty five
//! dollars", "ten percent", "three point five". [`format_numbers`] rewrites those spoken
//! numbers into the digits and symbols a reader expects ("$25", "10%", "3.5"). It is a pure,
//! offline, engine-agnostic transform — no model and no network — so it works the same for
//! every backend and for users who have not configured an LLM post-processing provider.
//!
//! # Design: precision first
//!
//! Like the rest of the text pipeline, a wrong rewrite is worse than a missed one. The
//! transform therefore:
//!   - only combines adjacent number words when they form a *valid* cardinal (so "twenty
//!     twenty" becomes "20 20", never the arithmetically-merged "40"),
//!   - only attaches a currency/percent symbol when the unit word directly follows the
//!     number ("five dollars" → "$5", but "five" and a later "dollars" are left apart),
//!   - only reads a decimal when a number sits on both sides of "point" and the fractional
//!     part is spoken as single digits ("three point one four" → "3.14"), and
//!   - leaves a bare, standalone "one" as a word, because it is so often non-numeric
//!     ("no one", "one of them", "which one") — while still converting it inside a larger
//!     number ("twenty one") or before a unit ("one dollar" → "$1").
//!
//! # Known limitations
//!
//! Years spoken as two groups ("nineteen eighty four") are not stitched into a single
//! token; each valid cardinal group is converted independently ("19 84"). A clock time is
//! stitched only when a meridiem follows it ("three thirty pm" -> "3:30 pm"); without one
//! the same two-group shape is ordinary prose more often than it is a time, so "at three
//! thirty" stays "at 3 30". The one o'clock hour is the exception to even that: a bare
//! "one" is deliberately kept as a word (see below), so no digit is ever produced for the
//! hour and "one thirty pm" stays "one 30 pm". Thousands separators are not inserted
//! ("$1200", not "$1,200").

/// A classified English number word (or word fragment from a hyphenated compound).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Cat {
    /// 0..=9
    Unit(u64),
    /// 10..=19
    Teen(u64),
    /// 20, 30, … 90
    Ten(u64),
    /// "hundred"
    Hundred,
    /// "thousand" / "million" / … (the multiplier value)
    Scale(u64),
}

/// Boundary punctuation stripped from the ends of a token before it is examined. Hyphen is
/// deliberately excluded so hyphenated compounds ("twenty-five") stay intact for the
/// compound splitter; interior punctuation is never touched.
fn is_boundary(c: char) -> bool {
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
            | '\u{2018}' // ‘
            | '\u{2019}' // ’
            | '\u{201C}' // “
            | '\u{201D}' // ”
            | '\u{00AB}' // «
            | '\u{00BB}' // »
            | '\u{2014}' // em dash
            | '\u{2013}' // en dash
    )
}

/// Maps a single lowercased word to its number category, if it is one.
fn word_to_cat(word: &str) -> Option<Cat> {
    let cat = match word {
        "zero" => Cat::Unit(0),
        "one" => Cat::Unit(1),
        "two" => Cat::Unit(2),
        "three" => Cat::Unit(3),
        "four" => Cat::Unit(4),
        "five" => Cat::Unit(5),
        "six" => Cat::Unit(6),
        "seven" => Cat::Unit(7),
        "eight" => Cat::Unit(8),
        "nine" => Cat::Unit(9),
        "ten" => Cat::Teen(10),
        "eleven" => Cat::Teen(11),
        "twelve" => Cat::Teen(12),
        "thirteen" => Cat::Teen(13),
        "fourteen" => Cat::Teen(14),
        "fifteen" => Cat::Teen(15),
        "sixteen" => Cat::Teen(16),
        "seventeen" => Cat::Teen(17),
        "eighteen" => Cat::Teen(18),
        "nineteen" => Cat::Teen(19),
        "twenty" => Cat::Ten(20),
        "thirty" => Cat::Ten(30),
        "forty" => Cat::Ten(40),
        "fifty" => Cat::Ten(50),
        "sixty" => Cat::Ten(60),
        "seventy" => Cat::Ten(70),
        "eighty" => Cat::Ten(80),
        "ninety" => Cat::Ten(90),
        "hundred" => Cat::Hundred,
        "thousand" => Cat::Scale(1_000),
        "million" => Cat::Scale(1_000_000),
        "billion" => Cat::Scale(1_000_000_000),
        "trillion" => Cat::Scale(1_000_000_000_000),
        _ => return None,
    };
    Some(cat)
}

/// Whether `next` may legitimately continue a cardinal number after `prev` — the ruleset that
/// keeps "twenty five" together (Ten→Unit) while splitting ill-formed runs like "twenty twenty"
/// (Ten→Ten) or "five six" (Unit→Unit).
fn can_follow(prev: Cat, next: Cat) -> bool {
    use Cat::*;
    match (prev, next) {
        (Unit(u), Hundred) | (Unit(u), Scale(_)) => u >= 1, // "five hundred", not "zero hundred"
        (Teen(_), Hundred) | (Teen(_), Scale(_)) => true, // "nineteen hundred", "fifteen thousand"
        (Ten(_), Unit(u)) => u >= 1,                      // "twenty five", not "twenty zero"
        (Ten(_), Hundred) | (Ten(_), Scale(_)) => true,   // "twenty thousand"
        (Hundred, Scale(_)) => true,                      // "hundred thousand"
        (Hundred, Unit(_)) | (Hundred, Teen(_)) | (Hundred, Ten(_)) => true, // "hundred twenty five"
        (Scale(s), Scale(s2)) => s2 < s, // "two million three thousand"
        (Scale(_), Unit(_)) | (Scale(_), Teen(_)) | (Scale(_), Ten(_)) | (Scale(_), Hundred) => {
            true
        }
        _ => false,
    }
}

/// Classifies a token's core into number categories, splitting hyphenated compounds
/// ("twenty-five" → [Ten(20), Unit(5)]). Returns `None` unless every fragment is a number word
/// *and* the fragments form a valid cardinal — so "twenty-something" and "five-six" are left
/// untouched.
fn token_number_cats(core: &str) -> Option<Vec<Cat>> {
    if core.is_empty() {
        return None;
    }
    let lower = core.to_lowercase();
    let mut cats = Vec::new();
    for part in lower.split('-') {
        cats.push(word_to_cat(part)?);
    }
    // Reject internally-invalid compounds (e.g. "five-six") so they are not mangled.
    for pair in cats.windows(2) {
        if !can_follow(pair[0], pair[1]) {
            return None;
        }
    }
    Some(cats)
}

/// Folds a validated sequence of categories into its integer value. Saturating arithmetic keeps
/// pathological input (endless "trillion"s) from overflowing.
fn cats_to_value(cats: &[Cat]) -> u64 {
    let mut result: u64 = 0;
    let mut current: u64 = 0;
    for c in cats {
        match *c {
            Cat::Unit(n) | Cat::Teen(n) | Cat::Ten(n) => current = current.saturating_add(n),
            Cat::Hundred => {
                current = if current == 0 {
                    100
                } else {
                    current.saturating_mul(100)
                }
            }
            Cat::Scale(s) => {
                let group = if current == 0 { 1 } else { current };
                result = result.saturating_add(group.saturating_mul(s));
                current = 0;
            }
        }
    }
    result.saturating_add(current)
}

/// A whitespace-delimited token split into leading punctuation, inner text, and trailing
/// punctuation, with the untouched original kept for verbatim re-emission of non-numbers.
#[derive(Clone, Debug)]
struct Token {
    lead: String,
    core: String,
    trail: String,
    original: String,
}

/// Splits a raw token into (leading punctuation, core, trailing punctuation). A token that is
/// entirely punctuation yields an empty core (and is therefore never treated as a number).
fn split_affixes(word: &str) -> (String, String, String) {
    let chars: Vec<char> = word.chars().collect();
    let first = chars.iter().position(|c| !is_boundary(*c));
    let last = chars.iter().rposition(|c| !is_boundary(*c));
    match (first, last) {
        (Some(s), Some(e)) => (
            chars[..s].iter().collect(),
            chars[s..=e].iter().collect(),
            chars[e + 1..].iter().collect(),
        ),
        _ => (String::new(), String::new(), word.to_string()),
    }
}

fn tokenize(text: &str) -> Vec<Token> {
    text.split_whitespace()
        .map(|w| {
            let (lead, core, trail) = split_affixes(w);
            Token {
                lead,
                core,
                trail,
                original: w.to_string(),
            }
        })
        .collect()
}

/// The result of consuming one well-formed cardinal group from the token stream.
struct NumGroup {
    value: u64,
    /// Index of the first token *after* the group.
    end: usize,
    /// Leading punctuation of the group's first token (e.g. the "(" of "(twenty").
    lead: String,
    /// Trailing punctuation of the group's last token, if it ended on punctuation.
    trail: String,
    /// True when the group is exactly the standalone word "one" (used for the bare-"one" carve-out).
    is_bare_one: bool,
}

/// Attempts to read one cardinal number starting at `start`. Consumes as many consecutive number
/// tokens as stay a valid cardinal, stopping at the first invalid transition, at a token that
/// carries trailing punctuation, or at a token that starts new leading punctuation. Returns `None`
/// if `start` is not a number token.
fn parse_number_group(tokens: &[Token], start: usize) -> Option<NumGroup> {
    if start >= tokens.len() {
        return None;
    }
    let mut cats: Vec<Cat> = Vec::new();
    let lead = tokens[start].lead.clone();
    let mut trail = String::new();
    let mut first_core_lower = String::new();
    let mut prev: Option<Cat> = None;
    let mut i = start;

    while i < tokens.len() {
        let t = &tokens[i];
        // Leading punctuation on a later token opens a new group; stop before it.
        if i > start && !t.lead.is_empty() {
            break;
        }
        let lower = t.core.to_lowercase();

        // "and" is part of the number only as a connector after a hundred/thousand and before a
        // further number word ("one hundred and five"); otherwise it ends the group.
        if lower == "and" {
            if t.trail.is_empty() && matches!(prev, Some(Cat::Hundred) | Some(Cat::Scale(_))) {
                if let Some(next) = tokens.get(i + 1) {
                    if next.lead.is_empty() && token_number_cats(&next.core).is_some() {
                        i += 1;
                        continue;
                    }
                }
            }
            break;
        }

        // "a"/"an" counts as 1, but only as a multiplier before "hundred"/"thousand"/… ("a
        // hundred" → 100). Elsewhere it is an article and must be left alone.
        if (lower == "a" || lower == "an") && t.trail.is_empty() {
            let qualifies = (prev.is_none() || matches!(prev, Some(Cat::Scale(_))))
                && tokens.get(i + 1).is_some_and(|next| {
                    next.lead.is_empty()
                        && matches!(
                            token_number_cats(&next.core).as_deref(),
                            Some([Cat::Hundred, ..]) | Some([Cat::Scale(_), ..])
                        )
                });
            if qualifies {
                if cats.is_empty() {
                    first_core_lower = lower.clone();
                }
                cats.push(Cat::Unit(1));
                prev = Some(Cat::Unit(1));
                i += 1;
                continue;
            }
            break;
        }

        match token_number_cats(&t.core) {
            None => break,
            Some(tcats) => {
                if let Some(p) = prev {
                    if !can_follow(p, tcats[0]) {
                        break;
                    }
                }
                if cats.is_empty() {
                    first_core_lower = lower.clone();
                }
                prev = Some(*tcats.last().unwrap());
                cats.extend(tcats);
                if !t.trail.is_empty() {
                    trail = t.trail.clone();
                    i += 1;
                    break;
                }
                i += 1;
            }
        }
    }

    if cats.is_empty() {
        return None;
    }
    let value = cats_to_value(&cats);
    let is_bare_one = cats.as_slice() == [Cat::Unit(1)] && first_core_lower == "one";
    Some(NumGroup {
        value,
        end: i,
        lead,
        trail,
        is_bare_one,
    })
}

/// Lowercased core of the token at `idx`, but only when it carries no leading punctuation (so a
/// unit word like "dollars" is recognized only when it directly abuts the number).
fn abutting_core_lower(tokens: &[Token], idx: usize) -> Option<String> {
    tokens
        .get(idx)
        .filter(|t| t.lead.is_empty())
        .map(|t| t.core.to_lowercase())
}

/// Unit words that pull a preceding number into a symbol/decimal form. A bare "one" is kept as a
/// word unless it is followed by one of these.
fn is_combinator_word(word: &str) -> bool {
    matches!(
        word,
        "dollar" | "dollars" | "buck" | "bucks" | "percent" | "point" | "cent" | "cents"
    )
}

/// English ordinal words. A cardinal directly followed by one of these forms a compound ordinal
/// ("twenty first"), so the cardinal is left as a word rather than half-converted to "20 first".
/// The plural time unit "seconds" is intentionally absent so "twenty seconds" still becomes
/// "20 seconds"; the singular "second" is included so dates like "July twenty second" are not
/// mangled into "July 20 second".
fn is_ordinal_word(word: &str) -> bool {
    matches!(
        word,
        "first"
            | "second"
            | "third"
            | "fourth"
            | "fifth"
            | "sixth"
            | "seventh"
            | "eighth"
            | "ninth"
            | "tenth"
            | "eleventh"
            | "twelfth"
            | "thirteenth"
            | "fourteenth"
            | "fifteenth"
            | "sixteenth"
            | "seventeenth"
            | "eighteenth"
            | "nineteenth"
            | "twentieth"
            | "thirtieth"
            | "fortieth"
            | "fiftieth"
            | "sixtieth"
            | "seventieth"
            | "eightieth"
            | "ninetieth"
            | "hundredth"
            | "thousandth"
            | "millionth"
            | "billionth"
            | "trillionth"
    )
}

/// Rewrites spelled-out English numbers in `text` into digits and symbols. See the module docs
/// for the precision rules and known limitations.
pub fn format_numbers(text: &str) -> String {
    let tokens = tokenize(text);
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;

    while i < tokens.len() {
        let Some(group) = parse_number_group(&tokens, i) else {
            out.push(tokens[i].original.clone());
            i += 1;
            continue;
        };

        let NumGroup {
            value,
            end,
            lead,
            trail,
            is_bare_one,
        } = group;
        let next_word = abutting_core_lower(&tokens, end);

        // A cardinal immediately followed by an ordinal word reads as a compound ordinal
        // ("July twenty first", "the twenty first century"). Converting only the cardinal would
        // emit "July 20 first", which is worse than leaving it alone, so emit the group's words
        // verbatim and let the ordinal pass through. (Full ordinal formatting — "21st" — is
        // deliberately out of scope: it would require converting bare "first"/"second", which are
        // far more often prose than numbers.)
        if trail.is_empty() && next_word.as_deref().is_some_and(is_ordinal_word) {
            for token in &tokens[i..end] {
                out.push(token.original.clone());
            }
            i = end;
            continue;
        }

        // A decimal only forms when "point" is followed by spoken digits; resolve it up front so the
        // bare-"one" rule below can distinguish a real decimal ("one point five" → "1.5") from a
        // dangling "point" ("one point blank"), where the word "one" must be kept.
        let decimal = if trail.is_empty() && next_word.as_deref() == Some("point") {
            parse_decimal_digits(&tokens, end)
        } else {
            None
        };

        // "point" only counts as a unit word (for overriding the bare-"one" rule) when a decimal
        // actually formed; otherwise "one point blank" would wrongly emit "1 point blank".
        let next_is_combinator = next_word.as_deref().is_some_and(is_combinator_word)
            && (next_word.as_deref() != Some("point") || decimal.is_some());

        // Bare, standalone "one" is usually not a number ("no one", "one of them"); keep the word
        // unless a unit word makes the numeric reading unambiguous ("one dollar" → "$1").
        if is_bare_one && !next_is_combinator {
            out.push(tokens[i].original.clone());
            i = end;
            continue;
        }

        // Symbol/decimal combinations only fire when the number itself ended cleanly (no trailing
        // punctuation between it and the unit word).
        if trail.is_empty() {
            if let Some((frac, frac_trail, next_i)) = decimal {
                out.push(format!("{lead}{value}.{frac}{frac_trail}"));
                i = next_i;
                continue;
            }
            match next_word.as_deref() {
                Some("percent") => {
                    let unit = &tokens[end];
                    out.push(format!("{lead}{value}%{}", unit.trail));
                    i = end + 1;
                    continue;
                }
                Some("dollar") | Some("dollars") | Some("buck") | Some("bucks") => {
                    let dollars = &tokens[end];
                    if let Some((cents, cents_trail, next_i)) =
                        parse_trailing_cents(&tokens, end + 1, dollars.trail.is_empty())
                    {
                        out.push(format!("{lead}${value}.{cents:02}{cents_trail}"));
                        i = next_i;
                    } else {
                        out.push(format!("{lead}${value}{}", dollars.trail));
                        i = end + 1;
                    }
                    continue;
                }
                _ => {}
            }
        }

        out.push(format!("{lead}{value}{trail}"));
        i = end;
    }

    stitch_clock_times(&out.join(" "))
}

/// Strips trailing boundary punctuation so a word can be matched on its own.
fn core_of(word: &str) -> &str {
    word.trim_end_matches(is_boundary)
}

/// Whether `word` is a spoken meridiem marker, with or without its periods.
fn is_meridiem(word: &str) -> bool {
    matches!(
        // core_of has already stripped the trailing period, so "a.m." arrives as "a.m".
        core_of(word).to_ascii_lowercase().as_str(),
        "am" | "pm" | "a.m" | "p.m"
    )
}

/// Whether `word` is bare digits that parse to a value in `range`, returning it.
fn bare_number(word: &str, lo: u32, hi: u32) -> Option<u32> {
    if word.is_empty() || !word.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    word.parse().ok().filter(|v| (lo..=hi).contains(v))
}

/// Joins an hour and a minute that the group converter emitted as two separate numbers
/// ("three thirty pm" -> "3 30 pm") into a clock time ("3:30 pm").
///
/// Anchored on a following meridiem, and on nothing else. The bare two-group shape is far
/// more often not a time than a time -- "three thirty-inch monitors", "two fifty gram bags"
/// -- so a rule that fired on the shape alone would rewrite ordinary prose, and this
/// module's standing trade is that a wrong rewrite costs more than a missed one. The
/// meridiem is the one signal that settles it, which is why the quieter "at three thirty"
/// is deliberately left as "at 3 30".
///
/// The minute must be a bare two-digit token, which is what one spoken group yields
/// ("thirty" -> "30", "forty five" -> "45"). That skips "three oh five pm": "oh" is not a
/// number word, so it survives as itself and no two-digit minute is ever formed.
fn stitch_clock_times(text: &str) -> String {
    let words: Vec<&str> = text.split(' ').collect();
    let mut out: Vec<String> = Vec::with_capacity(words.len());
    let mut i = 0;

    while i < words.len() {
        // Guard the window first: the checks below index i+1 and i+2, and combinators
        // would evaluate those eagerly even when the window is short.
        if i + 2 < words.len() {
            // Opening boundary punctuation belongs outside the time and is preserved. Any
            // trailing punctuation remains attached to the numeric core, so bare_number still
            // rejects "3, 30 pm" as a list rather than stitching it.
            let hour_digits = words[i].trim_start_matches(is_boundary);
            let hour_lead = &words[i][..words[i].len() - hour_digits.len()];
            let hour = bare_number(hour_digits, 1, 12);
            // A single spoken group always yields two digits, so the width check is what
            // keeps "three five pm" from being read as 3:05.
            let minute = bare_number(words[i + 1], 0, 59).filter(|_| words[i + 1].len() == 2);
            if let (Some(hour), Some(minute)) = (hour, minute) {
                if is_meridiem(words[i + 2]) {
                    out.push(format!("{hour_lead}{hour}:{minute:02}"));
                    // Both the hour and the minute went into that one token; the meridiem
                    // is left for the next turn so its own punctuation passes through.
                    i += 2;
                    continue;
                }
            }
        }

        out.push(words[i].to_string());
        i += 1;
    }

    out.join(" ")
}

/// Parses an optional "[and] <number> cents" tail after a dollar amount. `allow` gates the whole
/// attempt on the dollar word having had no trailing punctuation. The cents value must be < 100.
/// Returns `(cents, trailing_punctuation, index_after_cents_word)`.
fn parse_trailing_cents(
    tokens: &[Token],
    start: usize,
    allow: bool,
) -> Option<(u64, String, usize)> {
    if !allow {
        return None;
    }
    // Skip an optional bare "and" ("five dollars and fifty cents").
    let num_start = tokens
        .get(start)
        .filter(|t| t.lead.is_empty() && t.trail.is_empty() && t.core.eq_ignore_ascii_case("and"))
        .map_or(start, |_| start + 1);

    let group = parse_number_group(tokens, num_start)?;
    // The cents number must abut the amount cleanly (no split-off punctuation) and be a real
    // sub-dollar value.
    if !group.lead.is_empty() || !group.trail.is_empty() || group.value >= 100 {
        return None;
    }
    let cents_word = tokens.get(group.end)?;
    if !cents_word.lead.is_empty() {
        return None;
    }
    match cents_word.core.to_lowercase().as_str() {
        "cent" | "cents" => Some((group.value, cents_word.trail.clone(), group.end + 1)),
        _ => None,
    }
}

/// Parses a decimal fraction spoken as single digits after a "point" token located at `point_idx`.
/// Requires "point" to carry no trailing punctuation and to be followed by at least one spoken
/// digit. Returns `(fraction_digits, trailing_punctuation, index_after_fraction)`.
fn parse_decimal_digits(tokens: &[Token], point_idx: usize) -> Option<(String, String, usize)> {
    if !tokens[point_idx].trail.is_empty() {
        return None;
    }
    let mut frac = String::new();
    let mut last_trail = String::new();
    let mut idx = point_idx + 1;

    loop {
        // Each fractional position is one spoken digit: a cardinal 0–9 ("one point four" → ".4",
        // "zero" → "0") or the spoken zero "oh"/"o" ("one point oh" → "1.0"). A multi-digit word
        // (e.g. "fourteen") or a non-digit word ends the fraction.
        let (digit, next_idx, trail) = match parse_number_group(tokens, idx) {
            Some(group) if group.lead.is_empty() && group.value <= 9 && group.end == idx + 1 => {
                (group.value, group.end, group.trail)
            }
            _ => match tokens.get(idx) {
                Some(token)
                    if token.lead.is_empty()
                        && matches!(token.core.to_lowercase().as_str(), "oh" | "o") =>
                {
                    (0, idx + 1, token.trail.clone())
                }
                _ => break,
            },
        };
        frac.push_str(&digit.to_string());
        last_trail = trail;
        idx = next_idx;
        if !last_trail.is_empty() {
            break;
        }
    }

    if frac.is_empty() {
        None
    } else {
        Some((frac, last_trail, idx))
    }
}

#[cfg(test)]
mod tests {
    use super::format_numbers;

    #[test]
    fn cardinals_basic() {
        assert_eq!(format_numbers("twenty five"), "25");
        assert_eq!(format_numbers("one hundred"), "100");
        assert_eq!(format_numbers("one hundred twenty five"), "125");
        assert_eq!(format_numbers("one hundred and five"), "105");
        assert_eq!(format_numbers("two thousand"), "2000");
        assert_eq!(format_numbers("one thousand two hundred fifty"), "1250");
        assert_eq!(format_numbers("two hundred thousand"), "200000");
        assert_eq!(
            format_numbers("two million five hundred thousand"),
            "2500000"
        );
        assert_eq!(format_numbers("fifteen hundred"), "1500");
    }

    #[test]
    fn cardinals_hyphenated() {
        assert_eq!(format_numbers("twenty-five"), "25");
        assert_eq!(format_numbers("one hundred twenty-five"), "125");
    }

    #[test]
    fn cardinals_in_a_sentence() {
        assert_eq!(
            format_numbers("I have twenty three unread messages"),
            "I have 23 unread messages"
        );
        assert_eq!(
            format_numbers("we ordered fifty pizzas today"),
            "we ordered 50 pizzas today"
        );
    }

    #[test]
    fn a_and_an_as_one_before_scale() {
        assert_eq!(format_numbers("a hundred dollars"), "$100");
        assert_eq!(format_numbers("a thousand times"), "1000 times");
        // "a"/"an" as an article is untouched.
        assert_eq!(format_numbers("a dog and a cat"), "a dog and a cat");
        assert_eq!(format_numbers("an apple"), "an apple");
    }

    #[test]
    fn currency_dollars() {
        assert_eq!(format_numbers("five dollars"), "$5");
        assert_eq!(format_numbers("twenty five dollars"), "$25");
        assert_eq!(format_numbers("one dollar"), "$1");
        assert_eq!(
            format_numbers("it costs twenty five dollars total"),
            "it costs $25 total"
        );
        assert_eq!(format_numbers("five bucks"), "$5");
    }

    #[test]
    fn currency_dollars_and_cents() {
        assert_eq!(format_numbers("ten dollars and fifty cents"), "$10.50");
        assert_eq!(
            format_numbers("twenty five dollars and five cents"),
            "$25.05"
        );
        assert_eq!(format_numbers("three dollars ninety nine cents"), "$3.99");
    }

    #[test]
    fn currency_preserves_trailing_punctuation() {
        assert_eq!(
            format_numbers("that will be five dollars, please"),
            "that will be $5, please"
        );
        assert_eq!(format_numbers("I paid ten dollars."), "I paid $10.");
    }

    #[test]
    fn standalone_cents_stay_words() {
        assert_eq!(format_numbers("fifty cents"), "50 cents");
    }

    #[test]
    fn percent() {
        assert_eq!(format_numbers("ten percent"), "10%");
        assert_eq!(
            format_numbers("up by twenty five percent this year"),
            "up by 25% this year"
        );
        assert_eq!(format_numbers("one percent"), "1%");
    }

    #[test]
    fn decimals() {
        assert_eq!(format_numbers("three point five"), "3.5");
        assert_eq!(format_numbers("three point one four"), "3.14");
        assert_eq!(format_numbers("twenty five point five"), "25.5");
        // "point" without a numeric fraction is left as a word.
        assert_eq!(
            format_numbers("that is the whole point here"),
            "that is the whole point here"
        );
    }

    #[test]
    fn bare_one_is_kept_as_a_word() {
        assert_eq!(format_numbers("no one is here"), "no one is here");
        assert_eq!(format_numbers("one of them"), "one of them");
        assert_eq!(
            format_numbers("which one do you want"),
            "which one do you want"
        );
        // But "one" as part of a larger number or before a unit converts.
        assert_eq!(format_numbers("twenty one"), "21");
        assert_eq!(format_numbers("one hundred"), "100");
        assert_eq!(format_numbers("one dollar"), "$1");
        assert_eq!(format_numbers("one percent"), "1%");
    }

    #[test]
    fn invalid_combinations_split_rather_than_mis_add() {
        // "twenty twenty" must never become "40".
        assert_eq!(format_numbers("twenty twenty"), "20 20");
        assert_eq!(format_numbers("five six seven"), "5 6 7");
    }

    #[test]
    fn non_numbers_are_untouched() {
        assert_eq!(format_numbers("the quick brown fox"), "the quick brown fox");
        assert_eq!(
            format_numbers("twenty-something people"),
            "twenty-something people"
        );
        assert_eq!(format_numbers(""), "");
    }

    #[test]
    fn punctuation_around_numbers() {
        assert_eq!(format_numbers("(twenty-five)"), "(25)");
        assert_eq!(format_numbers("wait, twenty five!"), "wait, 25!");
    }

    #[test]
    fn zero_converts() {
        assert_eq!(format_numbers("zero"), "0");
        assert_eq!(format_numbers("zero dollars"), "$0");
    }

    #[test]
    fn cardinal_before_ordinal_is_left_as_words() {
        // A cardinal directly before an ordinal reads as a compound ordinal; half-converting it to
        // "20 first" is worse than leaving it, so the cardinal stays a word.
        assert_eq!(format_numbers("July twenty first"), "July twenty first");
        assert_eq!(
            format_numbers("the twenty first century"),
            "the twenty first century"
        );
        assert_eq!(
            format_numbers("on the twenty second of June"),
            "on the twenty second of June"
        );
        // Hyphenated ordinals were already safe (never a cardinal token).
        assert_eq!(
            format_numbers("the twenty-first amendment"),
            "the twenty-first amendment"
        );
        // The plural time unit is a cardinal + noun, not an ordinal, and still converts.
        assert_eq!(format_numbers("wait twenty seconds"), "wait 20 seconds");
        // A one-second duration also stays coherent (bare "one" kept).
        assert_eq!(format_numbers("give me one second"), "give me one second");
    }

    #[test]
    fn decimal_spoken_zero() {
        assert_eq!(format_numbers("version one point oh"), "version 1.0");
        assert_eq!(format_numbers("one point oh five"), "1.05");
        assert_eq!(format_numbers("three point o"), "3.0");
    }

    #[test]
    fn point_without_a_fraction_keeps_bare_one() {
        // "point" is not a real decimal here, so the bare "one" must survive rather than becoming
        // "1 point blank".
        assert_eq!(
            format_numbers("one point blank range"),
            "one point blank range"
        );
    }

    // #65: a spoken time is one of the things the reporter listed, and it was the one
    // the group converter left as two numbers ("3 30 pm").
    #[test]
    fn clock_time_with_meridiem_is_stitched() {
        assert_eq!(
            format_numbers("meet me at three thirty pm"),
            "meet me at 3:30 pm"
        );
        assert_eq!(format_numbers("three thirty PM"), "3:30 PM");
        assert_eq!(format_numbers("eleven forty five a.m."), "11:45 a.m.");
    }

    // Without the two-digit width rule this reads as 3:05, inventing a leading zero the
    // speaker never said. Two single-digit groups are not a time.
    #[test]
    fn two_single_digit_groups_are_not_a_time() {
        assert_eq!(format_numbers("three five pm"), "3 5 pm");
    }

    // One o'clock cannot stitch, and it is worth pinning so the next reader finds a
    // recorded decision rather than what looks like a bug. Two correct rules meet here:
    // a bare "one" stays a word because it is so often non-numeric, which means the hour
    // never becomes a digit for the stitcher to use. Reaching 1:30 would mean punching
    // through that carve-out, a worse trade in a module that would rather miss a rewrite
    // than make a wrong one.
    #[test]
    fn one_oclock_does_not_stitch_because_bare_one_stays_a_word() {
        assert_eq!(format_numbers("one thirty pm"), "one 30 pm");
        // Every other hour is unaffected, including the neighbouring twelve.
        assert_eq!(format_numbers("twelve thirty am"), "12:30 am");
        assert_eq!(format_numbers("two thirty pm"), "2:30 pm");
    }

    #[test]
    fn stitched_minute_keeps_two_digits() {
        // "oh eight" is not a number group, so the minute never forms; the hour still
        // converts and the rest is left alone rather than guessed at.
        assert_eq!(format_numbers("nine oh eight pm"), "9 oh 8 pm");
    }

    // The meridiem is the whole license for the rewrite. Without it the same shape is
    // ordinary prose far more often than it is a time.
    #[test]
    fn two_numbers_without_a_meridiem_are_left_apart() {
        assert_eq!(format_numbers("three thirty"), "3 30");
        assert_eq!(format_numbers("at three thirty"), "at 3 30");
        assert_eq!(
            format_numbers("three thirty inch monitors"),
            "3 30 inch monitors"
        );
    }

    #[test]
    fn out_of_range_pairs_are_not_times() {
        // 13 is not an hour on a spoken clock, and 74 is not a minute.
        assert_eq!(format_numbers("thirteen thirty pm"), "13 30 pm");
        assert_eq!(format_numbers("three seventy four pm"), "3 74 pm");
    }

    #[test]
    fn punctuation_between_the_parts_blocks_the_stitch() {
        // A comma makes it a list. The hour is not a bare token any more.
        assert_eq!(format_numbers("three, thirty pm"), "3, 30 pm");
    }

    #[test]
    fn opening_punctuation_around_a_time_survives() {
        assert_eq!(format_numbers("(three thirty pm)"), "(3:30 pm)");
        assert_eq!(
            format_numbers("she said \"three thirty pm\" clearly"),
            "she said \"3:30 pm\" clearly"
        );
        assert_eq!(
            format_numbers("meet at [three thirty pm]"),
            "meet at [3:30 pm]"
        );
    }

    #[test]
    fn trailing_punctuation_after_the_meridiem_survives() {
        assert_eq!(
            format_numbers("call at four fifteen pm."),
            "call at 4:15 pm."
        );
    }

    #[test]
    fn a_single_hour_before_a_meridiem_is_untouched() {
        // Nothing to stitch: one group, and "3 pm" already reads correctly.
        assert_eq!(format_numbers("three pm"), "3 pm");
    }
}
