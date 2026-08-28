pub mod audio;
pub mod history;
pub mod models;
pub mod personalization;
pub mod transcription;
pub mod window_picker;

use crate::settings::{get_settings, write_settings, AppSettings, LogLevel};
use crate::utils::cancel_current_operation;
use std::sync::Mutex;
use tauri::{AppHandle, Manager};
use tauri_plugin_opener::OpenerExt;

#[tauri::command]
#[specta::specta]
pub fn cancel_operation(app: AppHandle) {
    cancel_current_operation(&app);
}

#[tauri::command]
#[specta::specta]
pub fn is_portable() -> bool {
    crate::portable::is_portable()
}

#[tauri::command]
#[specta::specta]
pub fn is_update_channel_available() -> bool {
    crate::update_channel_available()
}

#[tauri::command]
#[specta::specta]
pub fn get_app_dir_path(app: AppHandle) -> Result<String, String> {
    let app_data_dir = crate::portable::app_data_dir(&app)
        .map_err(|e| format!("Failed to get app data directory: {}", e))?;

    Ok(app_data_dir.to_string_lossy().to_string())
}

#[tauri::command]
#[specta::specta]
pub fn get_app_settings(app: AppHandle) -> Result<AppSettings, String> {
    Ok(get_settings(&app))
}

/// The app's chosen interface language, and nothing else.
///
/// Every window syncs i18next against this on load, including surfaces that
/// have no business seeing the rest of the settings: the whole `AppSettings`
/// carries the post-processing API keys, and a one-shot picker asking "what
/// language am I in" should not be handed those to answer it.
#[tauri::command]
#[specta::specta]
pub fn get_app_language(app: AppHandle) -> String {
    get_settings(&app).app_language
}

#[tauri::command]
#[specta::specta]
pub fn get_default_settings() -> Result<AppSettings, String> {
    Ok(crate::settings::get_default_settings())
}

#[tauri::command]
#[specta::specta]
pub fn get_log_dir_path(app: AppHandle) -> Result<String, String> {
    let log_dir = crate::portable::app_log_dir(&app)
        .map_err(|e| format!("Failed to get log directory: {}", e))?;

    Ok(log_dir.to_string_lossy().to_string())
}

#[specta::specta]
#[tauri::command]
pub fn set_log_level(app: AppHandle, level: LogLevel) -> Result<(), String> {
    let tauri_log_level: tauri_plugin_log::LogLevel = level.into();
    let log_level: log::Level = tauri_log_level.into();

    let mut settings = get_settings(&app);
    settings.log_level = level;
    // Persist before touching the runtime filter: on a store failure the
    // frontend rolls back its optimistic value, so the live log level must
    // not have changed either, or the session would keep filtering at the
    // new level with nothing to show for it on the next read.
    write_settings(&app, settings)?;

    // Update the file log level atomic so the filter picks up the new level
    crate::FILE_LOG_LEVEL.store(
        log_level.to_level_filter() as u8,
        std::sync::atomic::Ordering::Relaxed,
    );

    Ok(())
}

#[specta::specta]
#[tauri::command]
pub fn open_recordings_folder(app: AppHandle) -> Result<(), String> {
    let app_data_dir = crate::portable::app_data_dir(&app)
        .map_err(|e| format!("Failed to get app data directory: {}", e))?;

    let recordings_dir = app_data_dir.join("recordings");

    let path = recordings_dir.to_string_lossy().as_ref().to_string();
    app.opener()
        .open_path(path, None::<String>)
        .map_err(|e| format!("Failed to open recordings folder: {}", e))?;

    Ok(())
}

#[specta::specta]
#[tauri::command]
pub fn open_log_dir(app: AppHandle) -> Result<(), String> {
    let log_dir = crate::portable::app_log_dir(&app)
        .map_err(|e| format!("Failed to get log directory: {}", e))?;

    let path = log_dir.to_string_lossy().as_ref().to_string();
    app.opener()
        .open_path(path, None::<String>)
        .map_err(|e| format!("Failed to open log directory: {}", e))?;

    Ok(())
}

#[specta::specta]
#[tauri::command]
pub fn open_app_data_dir(app: AppHandle) -> Result<(), String> {
    let app_data_dir = crate::portable::app_data_dir(&app)
        .map_err(|e| format!("Failed to get app data directory: {}", e))?;

    let path = app_data_dir.to_string_lossy().as_ref().to_string();
    app.opener()
        .open_path(path, None::<String>)
        .map_err(|e| format!("Failed to open app data directory: {}", e))?;

    Ok(())
}

/// Check if Apple Intelligence is available on this device.
/// Called by the frontend when the user selects Apple Intelligence provider.
#[specta::specta]
#[tauri::command]
pub fn check_apple_intelligence_available() -> bool {
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        crate::apple_intelligence::check_apple_intelligence_availability()
    }
    #[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
    {
        false
    }
}

/// Try to initialize Enigo (keyboard/mouse simulation).
/// On macOS, this will return an error if accessibility permissions are not granted.
#[specta::specta]
#[tauri::command]
pub fn initialize_enigo(app: AppHandle) -> Result<(), String> {
    use crate::input::EnigoState;

    // Check if already initialized
    if app.try_state::<EnigoState>().is_some() {
        log::debug!("Enigo already initialized");
        return Ok(());
    }

    // Try to initialize
    match EnigoState::new() {
        Ok(enigo_state) => {
            app.manage(enigo_state);
            log::info!("Enigo initialized successfully after permission grant");
            Ok(())
        }
        Err(e) => {
            if cfg!(target_os = "macos") {
                log::warn!(
                    "Failed to initialize Enigo: {} (accessibility permissions may not be granted)",
                    e
                );
            } else {
                log::warn!("Failed to initialize Enigo: {}", e);
            }
            Err(format!("Failed to initialize input system: {}", e))
        }
    }
}

/// Marker state to track if shortcuts have been initialized.
pub struct ShortcutsInitialized;

#[derive(Default)]
enum ShortcutInitializationStatus {
    #[default]
    Idle,
    Initialized,
    CleanupFailed(String),
}

static SHORTCUT_INITIALIZATION: Mutex<ShortcutInitializationStatus> =
    Mutex::new(ShortcutInitializationStatus::Idle);

fn initialize_shortcuts_once<IsInitialized, Initialize, MarkInitialized>(
    state: &Mutex<ShortcutInitializationStatus>,
    is_initialized: IsInitialized,
    initialize: Initialize,
    mark_initialized: MarkInitialized,
) -> Result<(), String>
where
    IsInitialized: FnOnce() -> bool,
    Initialize: FnOnce() -> Result<(), String>,
    MarkInitialized: FnOnce(),
{
    let mut status = state
        .lock()
        .map_err(|_| "Shortcut initialization lock is poisoned".to_string())?;
    match &*status {
        ShortcutInitializationStatus::Initialized => return Ok(()),
        ShortcutInitializationStatus::CleanupFailed(error) => return Err(error.clone()),
        ShortcutInitializationStatus::Idle => {}
    }

    if is_initialized() {
        *status = ShortcutInitializationStatus::Initialized;
        return Ok(());
    }

    match initialize() {
        Ok(()) => {
            mark_initialized();
            *status = ShortcutInitializationStatus::Initialized;
            Ok(())
        }
        Err(error) => {
            if !crate::shortcut::initialization_error_allows_retry(&error) {
                *status = ShortcutInitializationStatus::CleanupFailed(error.clone());
            }
            Err(error)
        }
    }
}

/// Initialize keyboard shortcuts.
/// On macOS, this should be called after accessibility permissions are granted.
/// This is idempotent - calling it multiple times is safe.
#[specta::specta]
#[tauri::command]
pub fn initialize_shortcuts(app: AppHandle) -> Result<(), String> {
    initialize_shortcuts_once(
        &SHORTCUT_INITIALIZATION,
        || app.try_state::<ShortcutsInitialized>().is_some(),
        || crate::shortcut::init_shortcuts(&app),
        || {
            app.manage(ShortcutsInitialized);
        },
    )?;

    log::info!("Shortcuts initialized successfully");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::{Arc, Barrier, Mutex};
    use std::thread;
    use std::time::Duration;

    #[test]
    fn concurrent_shortcut_initialization_registers_once() {
        let state = Arc::new(Mutex::new(ShortcutInitializationStatus::Idle));
        let initialized = Arc::new(AtomicBool::new(false));
        let initialization_calls = Arc::new(AtomicUsize::new(0));
        let start = Arc::new(Barrier::new(3));
        let mut threads = Vec::new();

        for _ in 0..2 {
            let state = Arc::clone(&state);
            let initialized = Arc::clone(&initialized);
            let initialization_calls = Arc::clone(&initialization_calls);
            let start = Arc::clone(&start);
            threads.push(thread::spawn(move || {
                start.wait();
                initialize_shortcuts_once(
                    &state,
                    || initialized.load(Ordering::SeqCst),
                    || {
                        initialization_calls.fetch_add(1, Ordering::SeqCst);
                        thread::sleep(Duration::from_millis(20));
                        Ok(())
                    },
                    || initialized.store(true, Ordering::SeqCst),
                )
            }));
        }

        start.wait();
        for thread in threads {
            assert_eq!(thread.join().unwrap(), Ok(()));
        }
        assert!(initialized.load(Ordering::SeqCst));
        assert_eq!(initialization_calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn cleanup_failure_blocks_a_same_process_retry() {
        let state = Mutex::new(ShortcutInitializationStatus::Idle);
        let initialization_calls = AtomicUsize::new(0);
        let cleanup_error = "shortcut state may remain active".to_string();

        let first = initialize_shortcuts_once(
            &state,
            || false,
            || {
                initialization_calls.fetch_add(1, Ordering::SeqCst);
                Err(cleanup_error.clone())
            },
            || {},
        );
        let second = initialize_shortcuts_once(
            &state,
            || false,
            || {
                initialization_calls.fetch_add(1, Ordering::SeqCst);
                Ok(())
            },
            || {},
        );

        assert_eq!(first, Err(cleanup_error.clone()));
        assert_eq!(second, Err(cleanup_error));
        assert_eq!(initialization_calls.load(Ordering::SeqCst), 1);
    }
}
