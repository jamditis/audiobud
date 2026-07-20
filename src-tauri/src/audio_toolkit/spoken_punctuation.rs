//! Spoken punctuation for raw transcript mode: "is this thing on question mark" prints
//! "is this thing on?".
//!
//! Deterministic and offline, like [`crate::audio_toolkit::numbers`], and deliberately
//! separate from it: this pass reads words the speaker meant as *commands*, while the
//! numbers pass rewrites words they meant as *content*. Keeping them apart is what lets
//! raw mode take one without the other.
//!
//! # Spacing
//!
//! A symbol is worth little if it lands with a space in front of it, so each command
//! records which side it attaches to. "on question mark" closes up to "on?", and "open
//! paren hello" opens up to "(hello". This is the whole reason the pass rebuilds the
//! string instead of running a plain find-and-replace.
//!
//! # Literal words
//!
//! There is no escape word. With the setting on, "question mark" is always the symbol;
//! to type the words, turn raw formatting off. An escape word ("literal question mark")
//! is the obvious next step, but it needs its own thinking about what happens when the
//! escape word itself is the thing you want to type, so it is not guessed at here.
//!
//! # Known limitations
//!
//! - en-US only. The command table is English and the spacing rules assume a
//!   space-separated language.
//! - Commands are matched on word boundaries but not on meaning, so a sentence that
//!   genuinely discusses "a question mark" becomes "a?". Dictation systems that solve
//!   this at all solve it with an escape word, which is the note above.
//! - "quote" opens and "unquote" closes. A speaker who says "quote" for both gets two
//!   opening quotes, because guessing from context is how you get it wrong silently.

/// Which neighbour a symbol closes up against.
#[derive(Clone, Copy, PartialEq)]
enum Attach {
    /// Closes up to the word before it: `on` + `?` -> `on?`.
    Left,
    /// Closes up to the word after it: `(` + `hello` -> `(hello`.
    Right,
    /// Closes up to both: `well` + `-` + `known` -> `well-known`. A hyphen that left a
    /// space on either side would not be doing a hyphen's job.
    Both,
    /// Closes up to neither. Line breaks supply their own separation.
    Neither,
}

/// Spoken forms and the symbol each prints. Longer phrases are matched first, so entries
/// here are written in whatever order reads clearly rather than in match order.
const COMMANDS: &[(&[&str], &str, Attach)] = &[
    (&["question", "mark"], "?", Attach::Left),
    (&["exclamation", "mark"], "!", Attach::Left),
    (&["exclamation", "point"], "!", Attach::Left),
    (&["full", "stop"], ".", Attach::Left),
    (&["period"], ".", Attach::Left),
    (&["comma"], ",", Attach::Left),
    (&["colon"], ":", Attach::Left),
    (&["semicolon"], ";", Attach::Left),
    (&["semi", "colon"], ";", Attach::Left),
    (&["hyphen"], "-", Attach::Both),
    (&["dash"], "-", Attach::Both),
    (&["open", "paren"], "(", Attach::Right),
    (&["open", "parenthesis"], "(", Attach::Right),
    (&["close", "paren"], ")", Attach::Left),
    (&["close", "parenthesis"], ")", Attach::Left),
    (&["open", "quote"], "\"", Attach::Right),
    (&["quote"], "\"", Attach::Right),
    (&["close", "quote"], "\"", Attach::Left),
    (&["unquote"], "\"", Attach::Left),
    (&["new", "paragraph"], "\n\n", Attach::Neither),
    (&["new", "line"], "\n", Attach::Neither),
];

/// The comparable core of a token: lowercased, without surrounding punctuation. The engine
/// often ends a token with its own comma or period, and "question mark." is still the
/// command. Inner punctuation is left alone so a hyphenated word stays one word.
fn core_of(token: &str) -> String {
    token
        .trim_matches(|c: char| !c.is_alphanumeric())
        .to_lowercase()
}

/// Matches a command starting at `words[i]`, returning the symbol, how it attaches, and how
/// many words it consumed.
///
/// When two phrases both match here the longer one wins. No pair in [`COMMANDS`] can do that
/// today -- "close quote" and "quote" start on different words, so only one is ever a
/// candidate at a given `i` -- so this is guarding the table's future rather than its
/// present. It is what makes a one-word command safe to add when it is also the first word of
/// a two-word one ("new" alongside "new line"), which is otherwise a silent truncation.
fn match_command(words: &[&str], i: usize) -> Option<(&'static str, Attach, usize)> {
    let mut best: Option<(&'static str, Attach, usize)> = None;
    for (phrase, symbol, attach) in COMMANDS {
        let n = phrase.len();
        if i + n > words.len() {
            continue;
        }
        if phrase
            .iter()
            .enumerate()
            .all(|(k, w)| core_of(words[i + k]) == *w)
            && best.map_or(true, |(_, _, best_n)| n > best_n)
        {
            best = Some((symbol, *attach, n));
        }
    }
    best
}

/// Rewrites spoken punctuation commands in `text` as their symbols.
///
/// Runs of whitespace collapse to single spaces, including line breaks, because the pass
/// rebuilds the string from its words. Nothing upstream of the raw path is relying on that --
/// `strip_to_raw_text` has already flattened the text by the time this runs -- but it does mean
/// this is not a transform to reach for on text whose existing layout matters.
pub fn apply_spoken_punctuation(text: &str) -> String {
    let words: Vec<&str> = text.split_whitespace().collect();
    let mut out = String::with_capacity(text.len());
    // Whether the next thing written needs a space in front of it. Starts false so the
    // output never opens with one, and is cleared by anything that closes up to its right.
    let mut needs_space = false;
    let mut i = 0;

    while i < words.len() {
        match match_command(&words, i) {
            Some((symbol, attach, consumed)) => {
                if attach == Attach::Right && needs_space {
                    out.push(' ');
                }
                out.push_str(symbol);
                // Only a Left-attached symbol leaves a space behind it. Right and Both
                // close up to what follows, and a line break supplies its own separation.
                needs_space = attach == Attach::Left;
                i += consumed;
            }
            None => {
                if needs_space {
                    out.push(' ');
                }
                out.push_str(words[i]);
                needs_space = true;
                i += 1;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closes_a_question_up_to_the_word_before_it() {
        assert_eq!(
            apply_spoken_punctuation("is this thing on question mark"),
            "is this thing on?"
        );
    }

    #[test]
    fn opens_a_paren_up_to_the_word_after_it() {
        assert_eq!(
            apply_spoken_punctuation("call me open paren later close paren today"),
            "call me (later) today"
        );
    }

    #[test]
    fn handles_each_single_word_command() {
        assert_eq!(
            apply_spoken_punctuation("wait comma then go"),
            "wait, then go"
        );
        assert_eq!(apply_spoken_punctuation("done period"), "done.");
        assert_eq!(apply_spoken_punctuation("stop semicolon go"), "stop; go");
        assert_eq!(apply_spoken_punctuation("note colon this"), "note: this");
        assert_eq!(apply_spoken_punctuation("well hyphen known"), "well-known");
    }

    #[test]
    fn handles_multi_word_commands() {
        assert_eq!(apply_spoken_punctuation("stop full stop"), "stop.");
        assert_eq!(apply_spoken_punctuation("wow exclamation mark"), "wow!");
        assert_eq!(apply_spoken_punctuation("wow exclamation point"), "wow!");
    }

    #[test]
    fn quote_opens_and_unquote_closes() {
        assert_eq!(
            apply_spoken_punctuation("she said quote hello unquote loudly"),
            "she said \"hello\" loudly"
        );
    }

    // "close quote" must not be read as the "quote" that opens one. The two phrases start on
    // different words so the longest-match rule is not what saves it, but the outcome is the
    // one that matters and it is worth pinning either way.
    #[test]
    fn a_closing_quote_is_not_read_as_an_opening_one() {
        assert_eq!(
            apply_spoken_punctuation("open quote hi close quote there"),
            "\"hi\" there"
        );
    }

    // A left-attached symbol has to leave a space behind it, or the next word runs into it.
    // Every other test here ends on its command, where that is invisible.
    #[test]
    fn a_left_attached_symbol_keeps_the_space_after_it() {
        assert_eq!(
            apply_spoken_punctuation("is it on question mark yes it is"),
            "is it on? yes it is"
        );
        assert_eq!(
            apply_spoken_punctuation("stop full stop then go"),
            "stop. then go"
        );
    }

    #[test]
    fn new_line_and_new_paragraph_break_the_text() {
        assert_eq!(apply_spoken_punctuation("one new line two"), "one\ntwo");
        assert_eq!(
            apply_spoken_punctuation("one new paragraph two"),
            "one\n\ntwo"
        );
    }

    // The engine punctuates its own output, so the command word often arrives with a comma
    // or period stuck to it. It is still the command.
    #[test]
    fn matches_a_command_the_engine_already_punctuated() {
        assert_eq!(
            apply_spoken_punctuation("is it on question mark."),
            "is it on?"
        );
    }

    #[test]
    fn is_case_insensitive() {
        assert_eq!(apply_spoken_punctuation("Really Question Mark"), "Really?");
    }

    // Text with no commands must come back as it went in. Raw mode's promise is that it does
    // not touch what you said, so a pass that reflowed ordinary prose would break it.
    #[test]
    fn leaves_text_without_commands_alone() {
        let plain = "the quick brown fox jumps over the lazy dog";
        assert_eq!(apply_spoken_punctuation(plain), plain);
    }

    #[test]
    fn handles_empty_and_whitespace_only_input() {
        assert_eq!(apply_spoken_punctuation(""), "");
        assert_eq!(apply_spoken_punctuation("   "), "");
    }

    // A command with nothing before it must not emit a leading space.
    #[test]
    fn does_not_open_the_output_with_a_space() {
        assert_eq!(apply_spoken_punctuation("open paren hi"), "(hi");
        assert_eq!(apply_spoken_punctuation("comma hi"), ", hi");
    }

    // Back-to-back commands are how a real sentence ends: "...done period new paragraph".
    #[test]
    fn handles_consecutive_commands() {
        assert_eq!(
            apply_spoken_punctuation("done period new paragraph next"),
            "done.\n\nnext"
        );
    }
}
