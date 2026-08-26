//! Tauri commands for the one-shot window picker (#124, #259).
//!
//! Two commands are the whole surface: the overlay asks what to render, then
//! reports the gesture that ended the pick. Both handles cross the boundary as
//! STRINGS (see [`crate::window_picker::handle_id`]) because an `HWND` on Win64
//! can exceed a JS number's safe range.

use tauri::AppHandle;

use crate::window_picker::backend;
use crate::window_picker::{PickArmed, PickerGesture, PickerWindow};

/// The rows the picker should render, remembered pick first. An empty list is a
/// normal answer, not an error: the overlay shows its empty state.
#[tauri::command]
#[specta::specta]
pub fn list_picker_windows(app: AppHandle) -> Vec<PickerWindow> {
    backend::offered_rows(&app)
}

/// Report the gesture that ended the pick and arm what it asked for. The picker
/// window closes either way, so the overlay does not have to close itself.
#[tauri::command]
#[specta::specta]
pub fn resolve_window_pick(app: AppHandle, gesture: PickerGesture) -> PickArmed {
    backend::resolve_pick(&app, gesture)
}
