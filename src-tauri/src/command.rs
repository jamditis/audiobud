//! Voice-command grammar (#7).
//!
//! AudioBud dictates: everything it hears becomes literal typed text. This
//! module is the first slice of the command layer that lets a spoken phrase
//! mean an *action* ("delete that", "new line", "go to end of line") instead of
//! text. It is deliberately the pure, decidable part of #7: a phrase in, a
//! structured [`EditCommand`] out. It holds no OS state and sends no key
//! events.
//!
//! Two things this module intentionally does NOT do, because #7 wants them
//! decided first and they are separate concerns:
//!
//! * It does not choose when a phrase is a command versus dictation. The design
//!   spec forbids a spoken command from ever leaking into the document as
//!   literal text, so the caller gates this parser behind an explicit command
//!   mode (a dedicated hotkey or push-to-talk is the recommended trigger).
//!   Inside that mode the whole utterance is a command, which is what resolves
//!   the "new line" command versus the words "new line" homophone collision.
//! * It does not press keys. Mapping an [`EditCommand`] to OS key events is the
//!   job of the injection layer (`input.rs`, via `enigo`), where the existing
//!   platform-specific key handling already lives.

/// A cursor or edit action parsed from a spoken command phrase.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditCommand {
    /// Move the cursor.
    Move(Motion),
    /// Extend the selection in a direction (Shift + the same motion).
    Select(Motion),
    /// Delete text.
    Delete(DeleteTarget),
    /// Insert a line break.
    InsertNewline,
    /// Insert a tab / indent.
    InsertTab,
    /// Undo the last edit.
    Undo,
    /// Redo the last undone edit.
    Redo,
}

/// A cursor motion, shared by [`EditCommand::Move`] and [`EditCommand::Select`]
/// so a "select" phrase reuses the exact same direction vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Motion {
    CharLeft,
    CharRight,
    WordLeft,
    WordRight,
    LineUp,
    LineDown,
    LineStart,
    LineEnd,
    DocumentStart,
    DocumentEnd,
}

/// What a delete command removes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeleteTarget {
    /// The word before the cursor.
    WordBack,
    /// The word after the cursor.
    WordForward,
    /// The current line.
    Line,
    /// The active selection.
    Selection,
}

/// Normalize a raw spoken phrase to a lowercase, single-spaced, punctuation-free
/// form so the vocabulary tables can match it. Punctuation is stripped from the
/// edges of each token, so "Delete that." and "delete  that" both become
/// "delete that".
fn normalize(phrase: &str) -> String {
    phrase
        .to_lowercase()
        .split_whitespace()
        .map(|token| token.trim_matches(|c: char| !c.is_alphanumeric()))
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Parse a motion phrase with no leading verb ("left", "word right", "end of
/// line"). Used on its own for a bare move and after a "select" prefix.
fn parse_motion(phrase: &str) -> Option<Motion> {
    // "go to" is a natural, meaning-free lead-in ("go to end of line", "go to
    // top"). Strip it once here so it works for every motion, rather than each
    // motion having to list its own "go to ..." alias (which is how the line and
    // document motions drifted out of sync).
    let phrase = phrase.strip_prefix("go to ").unwrap_or(phrase);
    let motion = match phrase {
        "left" | "char left" | "character left" => Motion::CharLeft,
        "right" | "char right" | "character right" => Motion::CharRight,
        "word left" | "left word" | "previous word" | "back word" => Motion::WordLeft,
        "word right" | "right word" | "next word" | "forward word" => Motion::WordRight,
        "up" | "line up" | "up line" => Motion::LineUp,
        "down" | "line down" | "down line" => Motion::LineDown,
        "home" | "line start" | "start of line" | "beginning of line" | "to start of line" => {
            Motion::LineStart
        }
        "end" | "line end" | "end of line" | "to end of line" => Motion::LineEnd,
        "document start" | "start of document" | "top of document" | "top" => Motion::DocumentStart,
        "document end" | "end of document" | "bottom of document" | "bottom" => Motion::DocumentEnd,
        _ => return None,
    };
    Some(motion)
}

/// Parse a delete phrase. Bare "delete" removes the active selection, matching
/// how a delete key behaves when text is selected.
fn parse_delete(phrase: &str) -> Option<EditCommand> {
    let target = match phrase {
        "delete" | "delete that" | "delete this" | "delete selection" => DeleteTarget::Selection,
        "delete word"
        | "delete previous word"
        | "delete last word"
        | "delete word back"
        | "backspace word" => DeleteTarget::WordBack,
        "delete next word" | "delete word forward" | "delete forward word" => {
            DeleteTarget::WordForward
        }
        "delete line" | "delete this line" | "delete the line" => DeleteTarget::Line,
        _ => return None,
    };
    Some(EditCommand::Delete(target))
}

/// Parse a spoken phrase into an [`EditCommand`], or `None` if it is not a
/// known command (the caller then treats it as dictation).
///
/// The caller is responsible for only invoking this in command mode; a phrase
/// like "word left" parses as a motion here regardless, and would be a false
/// positive if this ran on ordinary dictation.
pub fn parse_command(phrase: &str) -> Option<EditCommand> {
    let normalized = normalize(phrase);
    if normalized.is_empty() {
        return None;
    }

    // "select <motion>" extends the selection; it reuses the motion vocabulary.
    if let Some(rest) = normalized.strip_prefix("select ") {
        return parse_motion(rest).map(EditCommand::Select);
    }

    if let Some(delete) = parse_delete(&normalized) {
        return Some(delete);
    }

    match normalized.as_str() {
        "new line" | "newline" | "insert line" | "line break" => {
            return Some(EditCommand::InsertNewline)
        }
        "tab" | "insert tab" | "indent" => return Some(EditCommand::InsertTab),
        "undo" | "undo that" => return Some(EditCommand::Undo),
        "redo" | "redo that" => return Some(EditCommand::Redo),
        _ => {}
    }

    // A bare motion, or an explicit "move <motion>", moves the cursor.
    let motion_phrase = normalized.strip_prefix("move ").unwrap_or(&normalized);
    parse_motion(motion_phrase).map(EditCommand::Move)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bare_and_explicit_moves() {
        assert_eq!(
            parse_command("left"),
            Some(EditCommand::Move(Motion::CharLeft))
        );
        assert_eq!(
            parse_command("move left"),
            Some(EditCommand::Move(Motion::CharLeft))
        );
        assert_eq!(
            parse_command("word right"),
            Some(EditCommand::Move(Motion::WordRight))
        );
        assert_eq!(
            parse_command("end of line"),
            Some(EditCommand::Move(Motion::LineEnd))
        );
        assert_eq!(
            parse_command("go to top"),
            Some(EditCommand::Move(Motion::DocumentStart))
        );
    }

    #[test]
    fn go_to_prefix_resolves_for_every_motion() {
        // The module doc advertises "go to end of line", so the "go to" lead-in
        // must resolve for line motions, not only the document ones it once
        // listed explicitly.
        assert_eq!(
            parse_command("go to end of line"),
            Some(EditCommand::Move(Motion::LineEnd))
        );
        assert_eq!(
            parse_command("go to start of line"),
            Some(EditCommand::Move(Motion::LineStart))
        );
        assert_eq!(
            parse_command("go to bottom"),
            Some(EditCommand::Move(Motion::DocumentEnd))
        );
        // It also composes with the select prefix.
        assert_eq!(
            parse_command("select go to end of line"),
            Some(EditCommand::Select(Motion::LineEnd))
        );
    }

    #[test]
    fn select_reuses_the_motion_vocabulary() {
        assert_eq!(
            parse_command("select left"),
            Some(EditCommand::Select(Motion::CharLeft))
        );
        assert_eq!(
            parse_command("select word right"),
            Some(EditCommand::Select(Motion::WordRight))
        );
        assert_eq!(
            parse_command("select to end of line"),
            Some(EditCommand::Select(Motion::LineEnd))
        );
        // "select" with no motion is not a command on its own.
        assert_eq!(parse_command("select"), None);
        assert_eq!(parse_command("select the whole thing"), None);
    }

    #[test]
    fn parses_delete_targets() {
        assert_eq!(
            parse_command("delete"),
            Some(EditCommand::Delete(DeleteTarget::Selection))
        );
        assert_eq!(
            parse_command("delete that"),
            Some(EditCommand::Delete(DeleteTarget::Selection))
        );
        assert_eq!(
            parse_command("delete word"),
            Some(EditCommand::Delete(DeleteTarget::WordBack))
        );
        assert_eq!(
            parse_command("delete next word"),
            Some(EditCommand::Delete(DeleteTarget::WordForward))
        );
        assert_eq!(
            parse_command("delete line"),
            Some(EditCommand::Delete(DeleteTarget::Line))
        );
    }

    #[test]
    fn parses_insert_and_history_commands() {
        assert_eq!(parse_command("new line"), Some(EditCommand::InsertNewline));
        assert_eq!(parse_command("newline"), Some(EditCommand::InsertNewline));
        assert_eq!(parse_command("indent"), Some(EditCommand::InsertTab));
        assert_eq!(parse_command("undo"), Some(EditCommand::Undo));
        assert_eq!(parse_command("redo that"), Some(EditCommand::Redo));
    }

    #[test]
    fn normalizes_case_whitespace_and_trailing_punctuation() {
        assert_eq!(
            parse_command("Delete that."),
            Some(EditCommand::Delete(DeleteTarget::Selection))
        );
        assert_eq!(
            parse_command("  NEW   line  "),
            Some(EditCommand::InsertNewline)
        );
        assert_eq!(
            parse_command("Word, left"),
            Some(EditCommand::Move(Motion::WordLeft))
        );
    }

    #[test]
    fn rejects_non_commands_and_empty_input() {
        assert_eq!(parse_command(""), None);
        assert_eq!(parse_command("   "), None);
        assert_eq!(parse_command("the quick brown fox"), None);
        assert_eq!(parse_command("please write a paragraph about cats"), None);
    }

    #[test]
    fn rejects_dictation_that_merely_starts_with_a_command_word() {
        // The #7 safety contract is that a command never fires from ordinary
        // dictation. The dangerous class is prose that opens with command
        // vocabulary, which the exact-match grammar must reject. These pin that
        // contract so a later move to fuzzy or prefix matching cannot loosen it
        // with green tests.
        assert_eq!(parse_command("delete the seventh paragraph"), None);
        assert_eq!(parse_command("up in the mountains"), None);
        assert_eq!(parse_command("left of the door"), None);
        assert_eq!(parse_command("select a good time to meet"), None);
        // "move" alone is not a command, same as bare "select".
        assert_eq!(parse_command("move"), None);
    }
}
