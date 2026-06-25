//! Tauri commands for opt-in, on-device personalization (issue #16, Tier 1).
//!
//! All data lives in `AppSettings::personalization` (see [`crate::settings::PersonalizationData`]),
//! a store kept separate from the user-authored `custom_words`/`word_replacements` so it can be
//! inspected, exported, and reset on its own. Nothing here reaches the network.

use crate::managers::history::HistoryManager;
use crate::managers::personalization::{mine_word_suggestions, WordSuggestion};
use std::collections::HashSet;
use std::sync::Arc;
use tauri::{AppHandle, State};

/// Maximum number of accepted learned words (mirrors the custom-words cap).
const LEARNED_WORDS_CAP: usize = 500;

/// Toggle the opt-in personalization master switch.
#[tauri::command]
#[specta::specta]
pub fn update_personalization_enabled(app: AppHandle, enabled: bool) -> Result<(), String> {
    let mut settings = crate::settings::get_settings(&app);
    settings.personalization.enabled = enabled;
    crate::settings::write_settings(&app, settings);
    Ok(())
}

/// Mine custom-vocabulary suggestions from the user's transcript history.
///
/// Returns an empty list when personalization is disabled. Words already in the dictionary,
/// already learned, or previously dismissed are excluded.
#[tauri::command]
#[specta::specta]
pub async fn get_word_suggestions(
    app: AppHandle,
    history_manager: State<'_, Arc<HistoryManager>>,
    limit: u32,
) -> Result<Vec<WordSuggestion>, String> {
    let settings = crate::settings::get_settings(&app);
    if !settings.personalization.enabled {
        return Ok(Vec::new());
    }

    let mut exclude_lower: HashSet<String> = HashSet::new();
    for word in settings
        .custom_words
        .iter()
        .chain(settings.personalization.learned_words.iter())
        .chain(settings.personalization.dismissed_suggestions.iter())
    {
        exclude_lower.insert(word.to_lowercase());
    }

    let history_manager = history_manager.inner().clone();
    let limit = limit as usize;

    // Mining is pure CPU work over (potentially) the whole history; keep it off the async runtime.
    tauri::async_runtime::spawn_blocking(move || {
        let texts = history_manager
            .get_all_transcription_texts()
            .map_err(|e| e.to_string())?;
        Ok::<_, String>(mine_word_suggestions(&texts, &exclude_lower, limit))
    })
    .await
    .map_err(|e| format!("Suggestion mining task panicked: {}", e))?
}

/// Accept a mined suggestion into the learned-words list (deduped, case-insensitive). Also clears
/// the word from the dismissed list if it was there.
#[tauri::command]
#[specta::specta]
pub fn accept_word_suggestion(app: AppHandle, word: String) -> Result<(), String> {
    let trimmed = word.trim();
    if trimmed.is_empty() {
        return Err("Word is empty".to_string());
    }
    let lower = trimmed.to_lowercase();

    let mut settings = crate::settings::get_settings(&app);
    let personalization = &mut settings.personalization;
    personalization
        .dismissed_suggestions
        .retain(|w| w.to_lowercase() != lower);

    let already_learned = personalization
        .learned_words
        .iter()
        .any(|w| w.to_lowercase() == lower);
    if !already_learned {
        if personalization.learned_words.len() >= LEARNED_WORDS_CAP {
            return Err(format!(
                "Learned words list is full ({} max)",
                LEARNED_WORDS_CAP
            ));
        }
        personalization.learned_words.push(trimmed.to_string());
    }

    crate::settings::write_settings(&app, settings);
    Ok(())
}

/// Dismiss a mined suggestion so it is never surfaced again (deduped, case-insensitive).
#[tauri::command]
#[specta::specta]
pub fn dismiss_word_suggestion(app: AppHandle, word: String) -> Result<(), String> {
    let trimmed = word.trim();
    if trimmed.is_empty() {
        return Err("Word is empty".to_string());
    }
    let lower = trimmed.to_lowercase();

    let mut settings = crate::settings::get_settings(&app);
    let personalization = &mut settings.personalization;
    if !personalization
        .dismissed_suggestions
        .iter()
        .any(|w| w.to_lowercase() == lower)
    {
        personalization
            .dismissed_suggestions
            .push(trimmed.to_string());
    }

    crate::settings::write_settings(&app, settings);
    Ok(())
}

/// Replace the learned-words list (used to edit/remove entries from the UI).
#[tauri::command]
#[specta::specta]
pub fn update_learned_words(app: AppHandle, words: Vec<String>) -> Result<(), String> {
    let mut settings = crate::settings::get_settings(&app);
    settings.personalization.learned_words = words;
    crate::settings::write_settings(&app, settings);
    Ok(())
}

/// Clear learned personalization data (the "reset personalization" control): learned words,
/// learned replacements, and dismissed suggestions. The opt-in `enabled` toggle is preserved -- a
/// deliberate opt-in shouldn't be silently revoked by clearing stale learned data -- and
/// user-authored `custom_words`/`word_replacements` are untouched.
#[tauri::command]
#[specta::specta]
pub fn reset_personalization(app: AppHandle) -> Result<(), String> {
    let mut settings = crate::settings::get_settings(&app);
    let was_enabled = settings.personalization.enabled;
    settings.personalization = crate::settings::PersonalizationData {
        enabled: was_enabled,
        ..Default::default()
    };
    crate::settings::write_settings(&app, settings);
    Ok(())
}

/// Serialize personalization data to pretty JSON. Pure (no I/O) so it is unit-testable
/// and reused by the export command.
pub(crate) fn serialize_personalization(
    data: &crate::settings::PersonalizationData,
) -> Result<String, String> {
    serde_json::to_string_pretty(data).map_err(|e| e.to_string())
}

/// Export the personalization data as pretty-printed JSON to a user-chosen `path` (the frontend
/// picks it via the native save dialog). The file write happens here in Rust so no JS filesystem
/// capability is needed. Nothing leaves the device.
#[tauri::command]
#[specta::specta]
pub fn export_personalization(app: AppHandle, path: String) -> Result<(), String> {
    let settings = crate::settings::get_settings(&app);
    let json = serialize_personalization(&settings.personalization)?;
    std::fs::write(&path, json).map_err(|e| format!("Failed to write {}: {}", path, e))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::PersonalizationData;

    #[test]
    fn serialize_personalization_includes_fields_and_round_trips() {
        let data = PersonalizationData {
            enabled: true,
            learned_words: vec!["frobnicate".to_string()],
            learned_replacements: vec![],
            dismissed_suggestions: vec!["bar".to_string()],
        };
        let json = serialize_personalization(&data).expect("serialize");
        assert!(json.contains("\"frobnicate\""));
        assert!(json.contains("\"enabled\": true"));

        let back: PersonalizationData = serde_json::from_str(&json).expect("round-trip parse");
        assert_eq!(back.learned_words, data.learned_words);
        assert_eq!(back.dismissed_suggestions, data.dismissed_suggestions);
    }
}
