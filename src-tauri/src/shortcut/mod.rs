//! Keyboard shortcut management module
//!
//! This module provides a unified interface for keyboard shortcuts with
//! multiple backend implementations:
//!
//! - `tauri`: Uses Tauri's built-in global-shortcut plugin
//! - `handy_keys`: Uses the handy-keys library for more control
//!
//! The active implementation is determined by the `keyboard_implementation`
//! setting and can be changed at runtime.

mod handler;
pub mod handy_keys;
mod tauri_impl;

use log::{error, info, warn};
use serde::Serialize;
use specta::Type;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};

use crate::output_target;
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
use crate::settings::APPLE_INTELLIGENCE_DEFAULT_MODEL_ID;
use crate::settings::{
    self, get_settings, KeyboardImplementation, LLMPrompt, OverlayAnchor, OverlayCustomPosition,
    OverlayPosition, PasteMethod, ShortcutBinding, APPLE_INTELLIGENCE_PROVIDER_ID,
};
use crate::tray;

// Note: Commands are accessed via shortcut::handy_keys:: in lib.rs

const UNCERTAIN_SHORTCUT_STATE: &str = "shortcut state may remain active";
const PRIMARY_SHORTCUT_ID: &str = "transcribe";

pub(crate) struct ShortcutRegistrationError {
    message: String,
    cleanup_required: bool,
}

impl ShortcutRegistrationError {
    fn before_activation(message: String) -> Self {
        Self {
            message,
            cleanup_required: false,
        }
    }

    fn state_may_have_changed(message: String) -> Self {
        Self {
            message,
            cleanup_required: true,
        }
    }

    fn into_message(self) -> String {
        self.message
    }
}

pub(crate) fn initialization_error_allows_retry(error: &str) -> bool {
    !error.contains(UNCERTAIN_SHORTCUT_STATE)
}

fn with_retry_safe_handy_keys_fallback<T, F>(handy_error: String, fallback: F) -> Result<T, String>
where
    F: FnOnce() -> Result<T, String>,
{
    if !initialization_error_allows_retry(&handy_error) {
        return Err(handy_error);
    }
    fallback()
}

/// Commits a fallback that is already live.
///
/// If persistence fails, remove the live fallback so runtime routing continues
/// to agree with the backend named in settings. A failed cleanup makes the
/// backend state uncertain and requires a restart.
fn commit_initialized_fallback<Persist, Cleanup>(
    persist: Persist,
    cleanup: Cleanup,
) -> Result<(), String>
where
    Persist: FnOnce() -> Result<(), String>,
    Cleanup: FnOnce() -> Result<(), String>,
{
    let Err(persist_error) = persist() else {
        return Ok(());
    };

    match cleanup() {
        Ok(()) => Err(format!(
            "Failed to persist the Tauri shortcut fallback: {persist_error}. The live Tauri fallback was removed; retry shortcut initialization."
        )),
        Err(cleanup_error) => Err(format!(
            "Failed to persist the Tauri shortcut fallback: {persist_error}. Failed to remove the live Tauri fallback: {cleanup_error}; {UNCERTAIN_SHORTCUT_STATE}; restart AudioBud before retrying shortcut initialization"
        )),
    }
}

pub(super) fn configured_initial_bindings(app: &AppHandle) -> Vec<(String, ShortcutBinding)> {
    let default_bindings = settings::get_default_settings().bindings;
    let user_settings = settings::load_or_create_app_settings(app);

    default_bindings
        .into_iter()
        .filter(|(id, _)| id != "cancel")
        .filter(|(id, _)| {
            id != "transcribe_with_post_process" || user_settings.post_process_enabled
        })
        .map(|(id, default_binding)| {
            let binding = user_settings
                .bindings
                .get(&id)
                .cloned()
                .unwrap_or(default_binding);
            (id, binding)
        })
        .collect()
}

pub(super) fn register_initial_bindings<Register, Unregister>(
    mut bindings: Vec<(String, ShortcutBinding)>,
    mut register: Register,
    mut unregister: Unregister,
) -> Result<(), String>
where
    Register: FnMut(&ShortcutBinding) -> Result<(), ShortcutRegistrationError>,
    Unregister: FnMut(&ShortcutBinding) -> Result<(), String>,
{
    let primary_index = bindings
        .iter()
        .position(|(id, _)| id == PRIMARY_SHORTCUT_ID)
        .ok_or_else(|| format!("Required shortcut '{PRIMARY_SHORTCUT_ID}' is not configured"))?;
    let (primary_id, primary_binding) = bindings.remove(primary_index);

    if let Err(error) = register(&primary_binding) {
        let registration_error = format!(
            "Failed to register shortcut '{primary_id}': {}",
            error.message
        );
        if !error.cleanup_required {
            return Err(registration_error);
        }
        if let Err(cleanup_error) = unregister(&primary_binding) {
            warn!(
                "Failed to clean up shortcut '{}': {}",
                primary_binding.id, cleanup_error
            );
            return Err(format!(
                "{registration_error}; cleanup failed for '{}': {cleanup_error}; {UNCERTAIN_SHORTCUT_STATE}; restart AudioBud before retrying shortcut initialization",
                primary_binding.id
            ));
        }
        return Err(registration_error);
    }

    for (id, binding) in bindings {
        if let Err(error) = register(&binding) {
            warn!(
                "Failed to register optional shortcut '{id}': {}",
                error.message
            );
            if error.cleanup_required {
                if let Err(cleanup_error) = unregister(&binding) {
                    return Err(format!(
                        "Failed to register optional shortcut '{id}': {}; cleanup failed for '{}': {cleanup_error}; {UNCERTAIN_SHORTCUT_STATE}; restart AudioBud before retrying shortcut initialization",
                        error.message, binding.id
                    ));
                }
            }
        }
    }

    Ok(())
}

/// Initialize shortcuts using the configured implementation
pub fn init_shortcuts(app: &AppHandle) -> Result<(), String> {
    let user_settings = settings::load_or_create_app_settings(app);

    // Check which implementation to use
    match user_settings.keyboard_implementation {
        KeyboardImplementation::Tauri => tauri_impl::init_shortcuts(app),
        KeyboardImplementation::HandyKeys => {
            match handy_keys::init_shortcuts(app) {
                Ok(()) => Ok(()),
                Err(handy_error) => {
                    error!("Failed to initialize handy-keys shortcuts: {handy_error}");
                    let error_context = handy_error.clone();
                    with_retry_safe_handy_keys_fallback(handy_error, || {
                        warn!("Falling back to Tauri global shortcut implementation");

                        tauri_impl::init_shortcuts(app).map_err(|tauri_error| {
                            format!(
                                "HandyKeys initialization failed: {error_context}. Tauri fallback failed: {tauri_error}"
                            )
                        })?;

                        // Persist only after the fallback has working shortcuts.
                        let mut current_settings = settings::get_settings(app);
                        current_settings.keyboard_implementation = KeyboardImplementation::Tauri;
                        commit_initialized_fallback(
                            || settings::write_settings(app, current_settings),
                            || unregister_all_shortcuts(app, KeyboardImplementation::Tauri),
                        )
                    })
                }
            }
        }
    }
}

/// Register the cancel shortcut (called when recording starts)
pub fn register_cancel_shortcut(app: &AppHandle) {
    let settings = get_settings(app);
    match settings.keyboard_implementation {
        KeyboardImplementation::Tauri => tauri_impl::register_cancel_shortcut(app),
        KeyboardImplementation::HandyKeys => handy_keys::register_cancel_shortcut(app),
    }
}

/// Unregister the cancel shortcut (called when recording stops)
pub fn unregister_cancel_shortcut(app: &AppHandle) {
    let settings = get_settings(app);
    match settings.keyboard_implementation {
        KeyboardImplementation::Tauri => tauri_impl::unregister_cancel_shortcut(app),
        KeyboardImplementation::HandyKeys => handy_keys::unregister_cancel_shortcut(app),
    }
}

/// Register a shortcut using the appropriate implementation
pub fn register_shortcut(app: &AppHandle, binding: ShortcutBinding) -> Result<(), String> {
    let settings = get_settings(app);
    match settings.keyboard_implementation {
        KeyboardImplementation::Tauri => tauri_impl::register_shortcut(app, binding),
        KeyboardImplementation::HandyKeys => handy_keys::register_shortcut(app, binding),
    }
}

/// Unregister a shortcut using the appropriate implementation
pub fn unregister_shortcut(app: &AppHandle, binding: ShortcutBinding) -> Result<(), String> {
    let settings = get_settings(app);
    match settings.keyboard_implementation {
        KeyboardImplementation::Tauri => tauri_impl::unregister_shortcut(app, binding),
        KeyboardImplementation::HandyKeys => handy_keys::unregister_shortcut(app, binding),
    }
}

// ============================================================================
// Binding Management Commands
// ============================================================================

#[derive(Serialize, Type)]
pub struct BindingResponse {
    success: bool,
    binding: Option<ShortcutBinding>,
    error: Option<String>,
}

#[tauri::command]
#[specta::specta]
pub fn change_binding(
    app: AppHandle,
    id: String,
    binding: String,
) -> Result<BindingResponse, String> {
    // Reject empty bindings — every shortcut should have a value
    if binding.trim().is_empty() {
        return Err("Binding cannot be empty".to_string());
    }

    let mut settings = settings::get_settings(&app);

    // Get the binding to modify, or create it from defaults if it doesn't exist
    let binding_to_modify = match settings.bindings.get(&id) {
        Some(binding) => binding.clone(),
        None => {
            // Try to get the default binding for this id
            let default_settings = settings::get_default_settings();
            match default_settings.bindings.get(&id) {
                Some(default_binding) => {
                    warn!(
                        "Binding '{}' not found in settings, creating from defaults",
                        id
                    );
                    default_binding.clone()
                }
                None => {
                    let error_msg = format!("Binding with id '{}' not found in defaults", id);
                    warn!("change_binding error: {}", error_msg);
                    return Ok(BindingResponse {
                        success: false,
                        binding: None,
                        error: Some(error_msg),
                    });
                }
            }
        }
    };

    // If this is the cancel binding, just update the settings and return
    // It's managed dynamically, so we don't register/unregister here
    if id == "cancel" {
        if let Some(mut b) = settings.bindings.get(&id).cloned() {
            b.current_binding = binding;
            settings.bindings.insert(id.clone(), b.clone());
            settings::write_settings(&app, settings)?;
            return Ok(BindingResponse {
                success: true,
                binding: Some(b.clone()),
                error: None,
            });
        }
    }

    // Validate the new shortcut for the current keyboard implementation
    // before touching anything else, so a bad shortcut string never
    // unregisters the existing (valid) binding for nothing.
    if let Err(e) = validate_shortcut_for_implementation(&binding, settings.keyboard_implementation)
    {
        warn!("change_binding validation error: {}", e);
        return Err(e);
    }

    // Create an updated binding
    let mut updated_binding = binding_to_modify.clone();
    updated_binding.current_binding = binding;

    // Update the binding in the settings and persist BEFORE touching live
    // registrations: on a store failure the frontend rolls back its
    // optimistic value, so the shortcut actually registered must still be
    // the old one too, or the UI would show the old binding while the app
    // keeps using the unsaved new one until restart.
    settings
        .bindings
        .insert(id.clone(), updated_binding.clone());
    settings::write_settings(&app, settings)?;

    // Unregister the existing binding
    if let Err(e) = unregister_shortcut(&app, binding_to_modify.clone()) {
        let error_msg = format!("Failed to unregister shortcut: {}", e);
        error!("change_binding error: {}", error_msg);
    }

    // Register the new binding
    if let Err(e) = register_shortcut(&app, updated_binding.clone()) {
        let error_msg = format!("Failed to register shortcut: {}", e);
        error!("change_binding error: {}", error_msg);

        // The new binding is already persisted+cached at this point, but the
        // OS refused to register it (e.g. owned by another app), so the live
        // shortcut is unregistered rather than the old one. Roll back both
        // the store and the live registration so the store, cache, live
        // registration, and this response all agree on the old binding —
        // otherwise the frontend would show the old binding as active (this
        // response's success:false leaves its optimistic update rolled back)
        // while the persisted/cached settings still hold the new one.
        let mut rollback_settings = settings::get_settings(&app);
        rollback_settings
            .bindings
            .insert(id, binding_to_modify.clone());
        if let Err(revert_err) = settings::write_settings(&app, rollback_settings) {
            warn!(
                "Failed to revert binding '{}' after a registration failure: {revert_err}",
                binding_to_modify.id
            );
        }
        if let Err(reregister_err) = register_shortcut(&app, binding_to_modify) {
            error!("Failed to re-register the old binding during rollback: {reregister_err}");
        }

        return Ok(BindingResponse {
            success: false,
            binding: None,
            error: Some(error_msg),
        });
    }

    // Return the updated binding
    Ok(BindingResponse {
        success: true,
        binding: Some(updated_binding),
        error: None,
    })
}

#[tauri::command]
#[specta::specta]
pub fn reset_binding(app: AppHandle, id: String) -> Result<BindingResponse, String> {
    let binding = settings::get_stored_binding(&app, &id);
    change_binding(app, id, binding.default_binding)
}

/// Temporarily unregister a binding while the user is editing it in the UI.
/// This avoids firing the action while keys are being recorded.
#[tauri::command]
#[specta::specta]
pub fn suspend_binding(app: AppHandle, id: String) -> Result<(), String> {
    if let Some(b) = settings::get_bindings(&app).get(&id).cloned() {
        if let Err(e) = unregister_shortcut(&app, b) {
            error!("suspend_binding error for id '{}': {}", id, e);
            return Err(e);
        }
    }
    Ok(())
}

/// Re-register the binding after the user has finished editing.
#[tauri::command]
#[specta::specta]
pub fn resume_binding(app: AppHandle, id: String) -> Result<(), String> {
    if let Some(b) = settings::get_bindings(&app).get(&id).cloned() {
        if let Err(e) = register_shortcut(&app, b) {
            error!("resume_binding error for id '{}': {}", id, e);
            return Err(e);
        }
    }
    Ok(())
}

// ============================================================================
// Keyboard Implementation Switching
// ============================================================================

/// Result of changing keyboard implementation
#[derive(Serialize, Type)]
pub struct ImplementationChangeResult {
    pub success: bool,
    /// List of binding IDs that were reset to defaults due to incompatibility
    pub reset_bindings: Vec<String>,
}

/// Change the keyboard implementation with runtime switching.
/// This will unregister all shortcuts from the old implementation,
/// validate shortcuts for the new implementation (resetting invalid ones to defaults),
/// and register them with the new implementation.
#[tauri::command]
#[specta::specta]
pub fn change_keyboard_implementation_setting(
    app: AppHandle,
    implementation: String,
) -> Result<ImplementationChangeResult, String> {
    let current_settings = settings::get_settings(&app);
    let current_impl = current_settings.keyboard_implementation;
    let new_impl = parse_keyboard_implementation(&implementation);

    // If same implementation, nothing to do
    if current_impl == new_impl {
        return Ok(ImplementationChangeResult {
            success: true,
            reset_bindings: vec![],
        });
    }

    info!(
        "Switching keyboard implementation from {:?} to {:?}",
        current_impl, new_impl
    );

    // Persist the new implementation before touching any registrations. If
    // this fails (e.g. the settings store is unavailable), returning here
    // leaves the current implementation's shortcuts registered instead of
    // unregistering them first and then failing with none registered until
    // restart.
    let mut settings = current_settings;
    settings.keyboard_implementation = new_impl;
    settings::write_settings(&app, settings)?;

    // Unregister all shortcuts from the current implementation
    unregister_all_shortcuts(&app, current_impl).map_err(|cleanup_error| {
        format!(
            "Failed to remove the current shortcut backend: {cleanup_error}; {UNCERTAIN_SHORTCUT_STATE}; restart AudioBud before retrying the backend switch"
        )
    })?;

    // Initialize new implementation if needed (HandyKeys needs state)
    if new_impl == KeyboardImplementation::HandyKeys && initialize_handy_keys_with_rollback(&app)? {
        // Shortcuts already registered during init
        return Ok(ImplementationChangeResult {
            success: true,
            reset_bindings: vec![],
        });
    }

    // Register all shortcuts with new implementation, resetting invalid ones
    let reset_bindings = register_all_shortcuts_for_implementation(&app, new_impl);

    // Emit event to notify frontend of the change
    let _ = app.emit(
        "settings-changed",
        serde_json::json!({
            "setting": "keyboard_implementation",
            "value": implementation,
            "reset_bindings": reset_bindings
        }),
    );

    info!("Keyboard implementation switched to {:?}", new_impl);

    Ok(ImplementationChangeResult {
        success: true,
        reset_bindings,
    })
}

/// Get the current keyboard implementation
#[tauri::command]
#[specta::specta]
pub fn get_keyboard_implementation(app: AppHandle) -> String {
    let settings = settings::get_settings(&app);
    match settings.keyboard_implementation {
        KeyboardImplementation::Tauri => "tauri".to_string(),
        KeyboardImplementation::HandyKeys => "handy_keys".to_string(),
    }
}

// ============================================================================
// Validation Helpers
// ============================================================================

/// Validate a shortcut for a specific implementation
fn validate_shortcut_for_implementation(
    raw: &str,
    implementation: KeyboardImplementation,
) -> Result<(), String> {
    match implementation {
        KeyboardImplementation::Tauri => tauri_impl::validate_shortcut(raw),
        KeyboardImplementation::HandyKeys => handy_keys::validate_shortcut(raw),
    }
}

/// Parse a keyboard implementation string into the enum
fn parse_keyboard_implementation(s: &str) -> KeyboardImplementation {
    match s {
        "tauri" => KeyboardImplementation::Tauri,
        "handy_keys" => KeyboardImplementation::HandyKeys,
        other => {
            warn!(
                "Invalid keyboard implementation '{}', defaulting to tauri",
                other
            );
            KeyboardImplementation::Tauri
        }
    }
}

/// Unregister all shortcuts for the current implementation
fn unregister_all_shortcuts(
    app: &AppHandle,
    implementation: KeyboardImplementation,
) -> Result<(), String> {
    let current_settings = settings::get_settings(app);
    let mut failures = Vec::new();

    for (id, binding) in current_settings.bindings {
        // Skip cancel shortcut as it's dynamically registered
        if id == "cancel"
            || (id == "transcribe_with_post_process" && !current_settings.post_process_enabled)
        {
            continue;
        }

        let result = match implementation {
            KeyboardImplementation::Tauri => tauri_impl::unregister_shortcut(app, binding),
            KeyboardImplementation::HandyKeys => handy_keys::unregister_shortcut(app, binding),
        };

        if let Err(e) = result {
            warn!(
                "Failed to unregister shortcut '{}' during switch: {}",
                id, e
            );
            failures.push(format!("{id}: {e}"));
        }
    }

    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("; "))
    }
}

/// Register all shortcuts for a specific implementation, validating and resetting invalid ones
fn register_all_shortcuts_for_implementation(
    app: &AppHandle,
    implementation: KeyboardImplementation,
) -> Vec<String> {
    let mut reset_bindings = Vec::new();
    let mut settings_dirty = false;
    let default_bindings = settings::get_default_settings().bindings;
    let mut current_settings = settings::get_settings(app);

    for (id, default_binding) in &default_bindings {
        // Skip cancel shortcut as it's dynamically registered
        if id == "cancel" {
            continue;
        }

        // Skip post-processing shortcut when the feature is disabled
        if id == "transcribe_with_post_process" && !current_settings.post_process_enabled {
            continue;
        }

        // Back-fill bindings introduced in a newer version so they are persisted and appear in the
        // settings UI, rather than only being registered at runtime for the current session. An
        // existing user upgrading from before this binding existed would otherwise not see it (and
        // could not edit it) until a later launch.
        let mut binding = match current_settings.bindings.get(id) {
            Some(existing) => existing.clone(),
            None => {
                current_settings
                    .bindings
                    .insert(id.clone(), default_binding.clone());
                settings_dirty = true;
                default_binding.clone()
            }
        };

        // Validate the shortcut for the target implementation
        if let Err(e) =
            validate_shortcut_for_implementation(&binding.current_binding, implementation)
        {
            info!(
                "Shortcut '{}' ({}) is invalid for {:?}: {}. Resetting to default.",
                id, binding.current_binding, implementation, e
            );

            // Reset to default
            binding.current_binding = default_binding.current_binding.clone();
            current_settings
                .bindings
                .insert(id.clone(), binding.clone());
            reset_bindings.push(id.clone());
            settings_dirty = true;
        }

        // Register with the appropriate implementation
        let result = match implementation {
            KeyboardImplementation::Tauri => tauri_impl::register_shortcut(app, binding),
            KeyboardImplementation::HandyKeys => handy_keys::register_shortcut(app, binding),
        };

        if let Err(e) = result {
            error!(
                "Failed to register shortcut '{}' for {:?}: {}",
                id, implementation, e
            );
        }
    }

    // Persist any newly back-filled or reset bindings.
    if settings_dirty {
        if let Err(e) = settings::write_settings(app, current_settings) {
            warn!("Failed to persist back-filled/reset bindings: {e}");
        }
    }

    reset_bindings
}

/// Initialize HandyKeys if not already initialized, with rollback on failure
fn initialize_handy_keys_with_rollback(app: &AppHandle) -> Result<bool, String> {
    if app.try_state::<handy_keys::HandyKeysState>().is_some() {
        return Ok(false); // Already initialized, caller should continue
    }

    if let Err(handy_error) = handy_keys::init_shortcuts(app) {
        error!("Failed to initialize HandyKeys: {handy_error}");
        let error_context = handy_error.clone();
        return with_retry_safe_handy_keys_fallback(handy_error, || {
            // Rollback to Tauri
            let mut settings = settings::get_settings(app);
            settings.keyboard_implementation = KeyboardImplementation::Tauri;
            settings::write_settings(app, settings).map_err(|revert_error| {
                format!(
                    "Failed to initialize HandyKeys: {error_context}. Tauri rollback was not started because its setting could not be saved: {revert_error}"
                )
            })?;
            if let Err(fallback_error) = tauri_impl::init_shortcuts(app) {
                return Err(format!(
                    "Failed to initialize HandyKeys: {error_context}. Tauri rollback also failed: {fallback_error}"
                ));
            }
            Err(format!(
                "Failed to initialize HandyKeys: {error_context}. Reverted to Tauri."
            ))
        });
    }

    // init_shortcuts already registered shortcuts
    Ok(true)
}

// ============================================================================
// General Settings Commands
// ============================================================================

/// Work a setting change implies beyond persisting the new value.
///
/// Split out from the code that runs it so the mapping is a plain, testable
/// table: before issue #166 each of these lived inside its own `change_*_setting`
/// command, where the only way to check that (say) changing the paste method
/// still drops a meaningless target-lock was to run the app.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SettingEffect {
    /// Tell the settings window and the tray about the new value.
    EmitChanged,
    /// Register or unregister the OS autostart entry.
    ApplyAutostart,
    /// Move the overlay window to match the new placement.
    UpdateOverlayPosition,
    /// Show or hide the tray icon.
    SetTrayVisibility,
    /// Rebuild the tray menu (its labels are localized).
    RefreshTrayMenu,
    /// Re-apply accelerator globals and unload the model so it reloads on the
    /// new backend.
    ReloadAccelerator,
    /// Register or unregister the post-processing shortcut.
    SyncPostProcessShortcut,
    /// Drop an active target-lock that delivery can no longer use (#162).
    ClearTargetLockIfFocusFree,
}

/// What each setting requires beyond the write itself. Settings with no entry
/// need nothing but persistence, which is the majority of them.
pub(crate) fn effects_for_setting(key: &str) -> &'static [SettingEffect] {
    use SettingEffect::*;
    match key {
        "autostart_enabled" => &[ApplyAutostart, EmitChanged],
        "overlay_position" => &[UpdateOverlayPosition, EmitChanged],
        // Both of these decide whether delivery still targets a focused window,
        // so either one changing can strand a target-lock (#162).
        "auto_submit" => &[ClearTargetLockIfFocusFree, EmitChanged],
        "paste_method" => &[ClearTargetLockIfFocusFree],
        // A profile can decide the same two things for the locked window's own
        // application, so editing the list can strand a lock exactly as
        // changing the global settings can (#123).
        "output_profiles" => &[ClearTargetLockIfFocusFree, EmitChanged],
        "post_process_enabled" => &[SyncPostProcessShortcut],
        "app_language" => &[RefreshTrayMenu],
        "show_tray_icon" => &[SetTrayVisibility],
        "whisper_accelerator" | "ort_accelerator" | "whisper_gpu_device" => &[ReloadAccelerator],
        "push_to_talk"
        | "start_hidden"
        | "update_checks_enabled"
        | "debug_mode"
        | "mute_while_recording"
        | "append_trailing_space"
        | "raw_output"
        | "format_numbers"
        | "format_raw_output" => &[EmitChanged],
        _ => &[],
    }
}

fn run_setting_effects(
    app: &AppHandle,
    key: &str,
    settings: &settings::AppSettings,
    value: &serde_json::Value,
) {
    for effect in effects_for_setting(key) {
        match effect {
            SettingEffect::EmitChanged => {
                let _ = app.emit(
                    "settings-changed",
                    serde_json::json!({ "setting": key, "value": value }),
                );
            }
            SettingEffect::ApplyAutostart => {
                let autostart_manager = app.autolaunch();
                let _ = if settings.autostart_enabled {
                    autostart_manager.enable()
                } else {
                    autostart_manager.disable()
                };
            }
            SettingEffect::UpdateOverlayPosition => {
                // Moves the existing window rather than recreating it.
                crate::utils::update_overlay_position(app);
            }
            SettingEffect::SetTrayVisibility => {
                tray::set_tray_visibility(app, settings.show_tray_icon);
            }
            SettingEffect::RefreshTrayMenu => {
                tray::update_tray_menu(
                    app,
                    &tray::TrayIconState::Idle,
                    Some(&settings.app_language),
                );
            }
            SettingEffect::ReloadAccelerator => reload_accelerator(app),
            SettingEffect::SyncPostProcessShortcut => {
                if let Some(binding) = settings
                    .bindings
                    .get("transcribe_with_post_process")
                    .cloned()
                {
                    let _ = if settings.post_process_enabled {
                        register_shortcut(app, binding)
                    } else {
                        unregister_shortcut(app, binding)
                    };
                }
            }
            SettingEffect::ClearTargetLockIfFocusFree => {
                clear_target_lock_if_focus_free(app, settings)
            }
        }
    }
}

/// Settings whose change is more than a value write, and whose own command does
/// work this path cannot: re-registering shortcuts, switching the active model,
/// reconfiguring a device, re-initializing the logger. Routing them through the
/// generic mutator would persist the value and skip that work, leaving the app
/// out of step with its own settings.
const SETTINGS_WITH_DEDICATED_COMMANDS: &[&str] = &[
    "bindings",
    "selected_model",
    "keyboard_implementation",
    "log_level",
    "selected_microphone",
    "clamshell_microphone",
    "selected_output_device",
    "always_on_microphone",
    "history_limit",
    "recording_retention_period",
    "model_unload_timeout",
    "post_process_provider_id",
    "post_process_providers",
    "post_process_api_keys",
    "post_process_models",
    "post_process_prompts",
    "post_process_selected_prompt_id",
    "personalization",
    "paste_method",
    "external_script_path",
    // `set_overlay_anchor`/`reset_overlay_position` keep this in sync with the
    // coarse `overlay_position` and actually move the overlay window
    // (`update_overlay_position`); a bare write through the generic mutator
    // would change the stored placement without moving anything on screen.
    "overlay_custom_position",
    // Bookkeeping for the tray hide/show toggle (`toggle_overlay_visibility`)
    // and `normalize_after_change`, which are the only writers that should
    // decide what a future "show" restores to. A generic write here would
    // change that restore target without touching the visible overlay,
    // exactly like `overlay_custom_position` above.
    "overlay_restore_position",
];

/// Persist one setting and run whatever its change implies.
///
/// The single mutation path for simple settings, shared by the generic command,
/// the tray quick-toggles, and the two commands that must prompt before writing.
/// Effects run after the write so anything they trigger (a tray rebuild, an
/// overlay move, a target-lock release) already observes the new value.
pub(crate) fn apply_setting_change(
    app: &AppHandle,
    key: &str,
    value: serde_json::Value,
) -> Result<(), String> {
    let mut settings = settings::get_settings(app);
    settings::apply_setting_value(&mut settings, key, value.clone())?;
    // Propagate a write failure before running any effect, so a save that
    // didn't persist is never followed by a change that behaves as if it did
    // (and the frontend's rollback+toast path sees the failure instead of a
    // false confirmation).
    settings::write_settings(app, settings.clone())?;
    run_setting_effects(app, key, &settings, &value);
    Ok(())
}

/// Persist a single setting, addressed by the field name in the stored settings
/// object, with `value` as its JSON encoding.
///
/// Replaces ~33 near-identical `change_*_setting` commands (issue #166). The
/// value arrives as a JSON string rather than a typed argument because one
/// command has to carry every setting's type; `apply_setting_value` then
/// type-checks it against the real `AppSettings` field, so a wrong type or an
/// unknown key is an error instead of a silent no-op. Settings that need to
/// prompt the user first (`paste_method`, `external_script_path`) or that live
/// outside `AppSettings` keep their own commands.
#[tauri::command]
#[specta::specta]
pub fn update_setting(app: AppHandle, key: String, value: String) -> Result<(), String> {
    if SETTINGS_WITH_DEDICATED_COMMANDS.contains(&key.as_str()) {
        return Err(format!(
            "Setting '{key}' must be changed through its own command"
        ));
    }
    let value: serde_json::Value = serde_json::from_str(&value)
        .map_err(|e| format!("Setting '{key}' was not given a valid JSON value: {e}"))?;
    apply_setting_change(&app, &key, value)
}

/// Toggle the recording overlay between hidden and visible for the tray
/// quick-toggle (issue #12). Hiding remembers the current placement in
/// `overlay_restore_position`; showing restores it (defaulting to Bottom) so the
/// user's Top/Bottom choice survives a hide/show cycle instead of being reset.
/// Not a Tauri command — only the tray menu handler calls it.
pub fn toggle_overlay_visibility(app: AppHandle) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    if settings.overlay_position == OverlayPosition::None {
        settings.overlay_position = settings
            .overlay_restore_position
            .unwrap_or(OverlayPosition::Bottom);
    } else {
        settings.overlay_restore_position = Some(settings.overlay_position);
        settings.overlay_position = OverlayPosition::None;
    }
    settings::write_settings(&app, settings)?;

    // Update overlay position without recreating window
    crate::utils::update_overlay_position(&app);

    // Rebuild the tray checkmark and refresh the settings window.
    let _ = app.emit(
        "settings-changed",
        serde_json::json!({ "setting": "overlay_position" }),
    );

    Ok(())
}

/// Set a precise overlay placement from the #9 reposition grid: an anchor on the
/// active monitor with a zero drag-nudge. Overrides the centered Top/Bottom
/// default until reset.
#[tauri::command]
#[specta::specta]
pub fn set_overlay_anchor(app: AppHandle, anchor: String) -> Result<(), String> {
    let parsed = match anchor.as_str() {
        "topleft" => OverlayAnchor::TopLeft,
        "topcenter" => OverlayAnchor::TopCenter,
        "topright" => OverlayAnchor::TopRight,
        "middleleft" => OverlayAnchor::MiddleLeft,
        "middlecenter" => OverlayAnchor::MiddleCenter,
        "middleright" => OverlayAnchor::MiddleRight,
        "bottomleft" => OverlayAnchor::BottomLeft,
        "bottomcenter" => OverlayAnchor::BottomCenter,
        "bottomright" => OverlayAnchor::BottomRight,
        other => return Err(format!("Invalid overlay anchor: {other}")),
    };

    let mut settings = get_settings(&app);
    settings.overlay_custom_position = Some(OverlayCustomPosition {
        anchor: parsed,
        dx: 0.0,
        dy: 0.0,
    });
    // Keep the coarse Top/Bottom in sync with the grid row so the Linux
    // layer-shell fallback (which can't free-position) and the settings dropdown
    // both reflect the chosen anchor. Don't un-hide a disabled overlay.
    if settings.overlay_position != OverlayPosition::None {
        settings.overlay_position = match parsed {
            OverlayAnchor::TopLeft | OverlayAnchor::TopCenter | OverlayAnchor::TopRight => {
                OverlayPosition::Top
            }
            _ => OverlayPosition::Bottom,
        };
    }
    settings::write_settings(&app, settings)?;

    crate::utils::update_overlay_position(&app);

    Ok(())
}

/// Clear any custom overlay placement, returning the bug to the centered
/// Top/Bottom default (#9 reset-to-default).
#[tauri::command]
#[specta::specta]
pub fn reset_overlay_position(app: AppHandle) -> Result<(), String> {
    let mut settings = get_settings(&app);
    settings.overlay_custom_position = None;
    // Resetting the fine placement also returns the coarse Top/Bottom to the
    // default bottom-centered placement. Choosing a top-row anchor pins
    // overlay_position to Top (see set_overlay_anchor), so without this "Reset to
    // default" would clear the nudge but leave the overlay stuck at the top
    // instead of the app default. Guard on visibility so reset never un-hides a
    // deliberately hidden overlay.
    if settings.overlay_position != OverlayPosition::None {
        settings.overlay_position = OverlayPosition::Bottom;
    }
    settings::write_settings(&app, settings)?;

    crate::utils::update_overlay_position(&app);

    Ok(())
}

/// The external-script paste method runs an arbitrary program with the
/// transcript as an argument (see `clipboard::paste_via_external_script`), so
/// arming it - selecting the method or setting a non-empty script path -
/// requires an out-of-band confirmation the webview cannot satisfy on its own.
/// Clearing the path (None/empty) is always safe and needs no prompt.
pub(crate) fn external_script_path_requires_confirmation(path: &Option<String>) -> bool {
    matches!(path, Some(p) if !p.trim().is_empty())
}

/// Show a native, modal OK/Cancel dialog and return whether the user confirmed.
///
/// `MessageDialog::blocking_show` parks the calling thread until the user
/// answers, so it must never run on the webview/main thread. We dispatch it onto
/// the blocking pool (same pattern as the transcription and accelerator commands)
/// and await the result; a join error fails closed (treated as "not confirmed").
async fn confirm_external_script(app: &AppHandle, message: String) -> bool {
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .message(message)
            .title("AudioBud security check")
            .kind(MessageDialogKind::Warning)
            .buttons(MessageDialogButtons::OkCancel)
            .blocking_show()
    })
    .await
    .unwrap_or(false)
}

/// Clear the active target-lock (#120) when delivery to the locked window no
/// longer touches a focused window.
///
/// The paste method and auto-submit setting judged here are the ones the locked
/// window would actually be delivered with: its own application's output profile
/// folded over the global settings (#123). Judging the globals alone would drop a
/// lock whose application has a profile that still needs focus, and keep one
/// whose profile made delivery focus-free.
///
/// This is `clipboard::requires_focus_for_delivery`, not
/// `PasteMethod::requires_focus()` alone: ExternalScript's method step is
/// window-independent, but with auto-submit on, delivery still injects a
/// return keystroke into the focused window afterward, so a lock is only
/// safe to drop once BOTH the method and the auto-submit setting agree
/// delivery is focus-free.
///
/// Both settings can change independently -- the paste-method dropdown and
/// the auto-submit toggle (settings window and tray, #12) -- so every place
/// that changes either one must re-run this check with the OTHER setting's
/// current value, not just its own; otherwise a stale lock survives a change
/// that made it meaningless and can silently reactivate later if the user
/// switches back to a focus-requiring combination (#162).
fn clear_target_lock_if_focus_free(app: &AppHandle, settings: &settings::AppSettings) {
    let locked = app
        .try_state::<output_target::PinnedTarget>()
        .and_then(|pinned| pinned.locked());
    let locked_app = locked.and_then(|identity| output_target::backend::window_label(identity).0);
    let effective =
        crate::output_profile::EffectiveOutput::resolve(settings, locked_app.as_deref());
    if crate::clipboard::requires_focus_for_delivery(effective.paste_method, effective.auto_submit)
    {
        return;
    }
    if locked.is_some() {
        info!(
            "Paste method {:?} (auto_submit={}) cannot target a focused window; clearing the active target-lock",
            effective.paste_method, effective.auto_submit
        );
    }
    // Released through the shared unlock rather than PinnedTarget::unlock: the
    // lock is shown in three places now (#255) -- the overlay indicator, the
    // tray checkmark, the settings card -- and clearing it silently would leave
    // every one of them claiming a lock the app no longer holds. This also
    // clears a stale "lock lost" latch, which is equally meaningless once
    // delivery cannot reach a window at all, and does nothing when there is
    // neither a lock nor a latch.
    output_target::backend::unlock_output_target(app);
}

#[tauri::command]
#[specta::specta]
pub async fn change_paste_method_setting(app: AppHandle, method: String) -> Result<(), String> {
    let parsed = match method.as_str() {
        "ctrl_v" => PasteMethod::CtrlV,
        "direct" => PasteMethod::Direct,
        "none" => PasteMethod::None,
        "shift_insert" => PasteMethod::ShiftInsert,
        "ctrl_shift_v" => PasteMethod::CtrlShiftV,
        "external_script" => PasteMethod::ExternalScript,
        other => {
            warn!("Invalid paste method '{}', defaulting to ctrl_v", other);
            PasteMethod::CtrlV
        }
    };
    if matches!(parsed, PasteMethod::ExternalScript) {
        let message = "AudioBud's external-script paste method runs your configured \
            external program every time it pastes a transcription. Only enable this \
            if you placed that script yourself and trust it. Enable it?"
            .to_string();
        if !confirm_external_script(&app, message).await {
            info!("External-script paste method not confirmed; leaving paste method unchanged");
            // Err (not Ok) so the frontend rolls back its optimistic selection
            // instead of showing the unpersisted method as saved.
            return Err(
                "External-script paste was not enabled (confirmation declined)".to_string(),
            );
        }
    }
    // Shared path with the generic mutator, so the target-lock check that
    // `effects_for_setting("paste_method")` declares runs here too.
    apply_setting_change(
        &app,
        "paste_method",
        serde_json::to_value(parsed).map_err(|e| e.to_string())?,
    )
}

#[tauri::command]
#[specta::specta]
pub fn get_available_typing_tools() -> Vec<String> {
    #[cfg(target_os = "linux")]
    {
        crate::clipboard::get_available_typing_tools()
    }
    #[cfg(not(target_os = "linux"))]
    {
        vec!["auto".to_string()]
    }
}

#[tauri::command]
#[specta::specta]
pub async fn change_external_script_path_setting(
    app: AppHandle,
    path: Option<String>,
) -> Result<(), String> {
    if external_script_path_requires_confirmation(&path) {
        let script = path.as_deref().unwrap_or_default();
        let message = format!(
            "AudioBud will run this program every time it pastes a transcription:\n\n\
            {script}\n\n\
            Only allow this if you placed this script yourself and trust it. Allow it to run?"
        );
        if !confirm_external_script(&app, message).await {
            info!("External-script path not confirmed; leaving script path unchanged");
            // Err (not Ok) so the frontend rolls back its optimistic value
            // instead of showing the unpersisted path as saved.
            return Err("External-script path was not set (confirmation declined)".to_string());
        }
    }
    apply_setting_change(
        &app,
        "external_script_path",
        serde_json::to_value(path).map_err(|e| e.to_string())?,
    )
}

#[tauri::command]
#[specta::specta]
pub fn change_post_process_base_url_setting(
    app: AppHandle,
    provider_id: String,
    base_url: String,
) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    let label = settings
        .post_process_provider(&provider_id)
        .map(|provider| provider.label.clone())
        .ok_or_else(|| format!("Provider '{}' not found", provider_id))?;

    let provider = settings
        .post_process_provider_mut(&provider_id)
        .expect("Provider looked up above must exist");

    if provider.id != "custom" {
        return Err(format!(
            "Provider '{}' does not allow editing the base URL",
            label
        ));
    }

    provider.base_url = base_url;
    settings::write_settings(&app, settings)?;
    Ok(())
}

/// Generic helper to validate provider exists
fn validate_provider_exists(
    settings: &settings::AppSettings,
    provider_id: &str,
) -> Result<(), String> {
    if !settings
        .post_process_providers
        .iter()
        .any(|provider| provider.id == provider_id)
    {
        return Err(format!("Provider '{}' not found", provider_id));
    }
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_post_process_api_key_setting(
    app: AppHandle,
    provider_id: String,
    api_key: String,
) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    validate_provider_exists(&settings, &provider_id)?;
    settings.post_process_api_keys.insert(provider_id, api_key);
    settings::write_settings(&app, settings)?;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_post_process_model_setting(
    app: AppHandle,
    provider_id: String,
    model: String,
) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    validate_provider_exists(&settings, &provider_id)?;
    settings.post_process_models.insert(provider_id, model);
    settings::write_settings(&app, settings)?;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn set_post_process_provider(app: AppHandle, provider_id: String) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    validate_provider_exists(&settings, &provider_id)?;
    settings.post_process_provider_id = provider_id;
    settings::write_settings(&app, settings)?;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn add_post_process_prompt(
    app: AppHandle,
    name: String,
    prompt: String,
) -> Result<LLMPrompt, String> {
    let mut settings = settings::get_settings(&app);

    // Generate unique ID using timestamp and random component
    let id = format!("prompt_{}", chrono::Utc::now().timestamp_millis());

    let new_prompt = LLMPrompt {
        id: id.clone(),
        name,
        prompt,
    };

    settings.post_process_prompts.push(new_prompt.clone());
    settings::write_settings(&app, settings)?;

    Ok(new_prompt)
}

#[tauri::command]
#[specta::specta]
pub fn update_post_process_prompt(
    app: AppHandle,
    id: String,
    name: String,
    prompt: String,
) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);

    if let Some(existing_prompt) = settings
        .post_process_prompts
        .iter_mut()
        .find(|p| p.id == id)
    {
        existing_prompt.name = name;
        existing_prompt.prompt = prompt;
        settings::write_settings(&app, settings)?;
        Ok(())
    } else {
        Err(format!("Prompt with id '{}' not found", id))
    }
}

#[tauri::command]
#[specta::specta]
pub fn delete_post_process_prompt(app: AppHandle, id: String) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);

    // Don't allow deleting the last prompt
    if settings.post_process_prompts.len() <= 1 {
        return Err("Cannot delete the last prompt".to_string());
    }

    // Find and remove the prompt
    let original_len = settings.post_process_prompts.len();
    settings.post_process_prompts.retain(|p| p.id != id);

    if settings.post_process_prompts.len() == original_len {
        return Err(format!("Prompt with id '{}' not found", id));
    }

    // If the deleted prompt was selected, select the first one or None
    if settings.post_process_selected_prompt_id.as_ref() == Some(&id) {
        settings.post_process_selected_prompt_id =
            settings.post_process_prompts.first().map(|p| p.id.clone());
    }

    settings::write_settings(&app, settings)?;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn fetch_post_process_models(
    app: AppHandle,
    provider_id: String,
) -> Result<Vec<String>, String> {
    let settings = settings::get_settings(&app);

    // Find the provider
    let provider = settings
        .post_process_providers
        .iter()
        .find(|p| p.id == provider_id)
        .ok_or_else(|| format!("Provider '{}' not found", provider_id))?;

    if provider.id == APPLE_INTELLIGENCE_PROVIDER_ID {
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        {
            return Ok(vec![APPLE_INTELLIGENCE_DEFAULT_MODEL_ID.to_string()]);
        }

        #[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
        {
            return Err("Apple Intelligence is only available on Apple silicon Macs running macOS 15 or later.".to_string());
        }
    }

    // Get API key
    let api_key = settings
        .post_process_api_keys
        .get(&provider_id)
        .cloned()
        .unwrap_or_default();

    // Skip fetching if no API key for providers that typically need one
    if api_key.trim().is_empty() && provider.id != "custom" {
        return Err(format!(
            "API key is required for {}. Please add an API key to list available models.",
            provider.label
        ));
    }

    crate::llm_client::fetch_models(provider, api_key).await
}

#[tauri::command]
#[specta::specta]
pub fn set_post_process_selected_prompt(app: AppHandle, id: String) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);

    // Verify the prompt exists
    if !settings.post_process_prompts.iter().any(|p| p.id == id) {
        return Err(format!("Prompt with id '{}' not found", id));
    }

    settings.post_process_selected_prompt_id = Some(id);
    settings::write_settings(&app, settings)?;
    Ok(())
}

/// Re-apply the accelerator globals and unload the model so it reloads with the
/// new backend on the next transcription. Runs after the setting is persisted.
fn reload_accelerator(app: &AppHandle) {
    crate::managers::transcription::apply_accelerator_settings(app);

    let tm = app.state::<std::sync::Arc<crate::managers::transcription::TranscriptionManager>>();
    if tm.is_model_loaded() {
        if let Err(e) = tm.unload_model() {
            log::warn!("Failed to unload model after accelerator change: {e}");
        }
    }
}

/// Return which accelerators and GPU devices are available for this build.
///
/// First-call cost is dominated by enumerating GPU devices through the
/// whisper.cpp Metal/Vulkan backend, which loads dynamic libraries and
/// probes hardware. Run it on the blocking pool so the webview thread
/// stays responsive — see also the startup pre-warm in `lib.rs`.
#[tauri::command]
#[specta::specta]
pub async fn get_available_accelerators() -> crate::managers::transcription::AvailableAccelerators {
    tauri::async_runtime::spawn_blocking(crate::managers::transcription::get_available_accelerators)
        .await
        .expect("get_available_accelerators panicked")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    #[derive(Default)]
    struct ShortcutBackendModel {
        active: Vec<String>,
        fail_next_registration: Option<String>,
        activate_before_failure: bool,
        fail_cleanup: bool,
    }

    impl ShortcutBackendModel {
        fn register(&mut self, binding: &ShortcutBinding) -> Result<(), ShortcutRegistrationError> {
            if self.fail_next_registration.as_deref() == Some(binding.id.as_str()) {
                self.fail_next_registration = None;
                if self.activate_before_failure {
                    self.active.push(binding.id.clone());
                    return Err(ShortcutRegistrationError::state_may_have_changed(
                        "simulated backend failure".to_string(),
                    ));
                }
                return Err(ShortcutRegistrationError::before_activation(
                    "simulated backend failure".to_string(),
                ));
            }
            self.active.push(binding.id.clone());
            Ok(())
        }

        fn unregister(&mut self, binding: &ShortcutBinding) -> Result<(), String> {
            if self.fail_cleanup {
                return Err("simulated cleanup failure".to_string());
            }
            self.active.retain(|id| id != &binding.id);
            Ok(())
        }
    }

    #[test]
    fn external_script_path_requires_confirmation_when_armed() {
        assert!(external_script_path_requires_confirmation(&Some(
            "paste.ps1".to_string()
        )));
        assert!(external_script_path_requires_confirmation(&Some(
            "C:\\tools\\paste.exe".to_string()
        )));
    }

    #[test]
    fn every_setting_reserved_for_its_own_command_still_exists() {
        // A renamed or removed field would otherwise leave a dead entry here and
        // silently reopen the generic path for something that needs more than a
        // write.
        let settings = serde_json::to_value(settings::get_default_settings()).unwrap();
        let settings = settings.as_object().expect("settings are a JSON object");
        for key in SETTINGS_WITH_DEDICATED_COMMANDS {
            assert!(
                settings.contains_key(*key),
                "'{key}' is reserved but is not a setting"
            );
        }
    }

    #[test]
    fn the_generic_command_refuses_settings_that_need_their_own() {
        // The guard is the only thing standing between the generic mutator and a
        // write that skips re-registering a shortcut or reconfiguring a device.
        assert!(SETTINGS_WITH_DEDICATED_COMMANDS.contains(&"bindings"));
        assert!(SETTINGS_WITH_DEDICATED_COMMANDS.contains(&"selected_model"));
        // ...but never for a setting the effect table already handles and that
        // has no confirmation prompt of its own.
        for key in ["auto_submit", "post_process_enabled"] {
            assert!(
                !SETTINGS_WITH_DEDICATED_COMMANDS.contains(&key),
                "'{key}' is handled by the effect table and must stay writable"
            );
        }
    }

    #[test]
    fn the_generic_command_refuses_confirmation_gated_settings() {
        // `paste_method` and `external_script_path` must go through
        // `change_paste_method_setting` / `change_external_script_path_setting`
        // so selecting the external-script paste method (and the program it
        // will run) always passes through the native confirmation dialog.
        // Reaching them through the generic `update_setting` would let a
        // renderer arm command execution without ever prompting the user.
        // `update_setting`'s guard check runs before it touches its `AppHandle`
        // argument, so exercise the same `.contains` check the command makes
        // rather than constructing a real `AppHandle` in a unit test.
        for key in ["paste_method", "external_script_path"] {
            assert!(
                SETTINGS_WITH_DEDICATED_COMMANDS.contains(&key),
                "'{key}' must stay confirmation-gated, not reachable via update_setting"
            );
        }
    }

    #[test]
    fn the_generic_command_refuses_overlay_custom_position() {
        // `set_overlay_anchor`/`reset_overlay_position` keep `overlay_position`
        // in sync with `overlay_custom_position` and call
        // `update_overlay_position` to actually move the window. A generic
        // write would persist a placement the overlay never moves to. Its
        // restore companion is bookkeeping for the same hide/show toggle and
        // must stay off the generic path for the same reason.
        for key in ["overlay_custom_position", "overlay_restore_position"] {
            assert!(
                SETTINGS_WITH_DEDICATED_COMMANDS.contains(&key),
                "'{key}' must stay reachable only through the commands that apply its runtime effect"
            );
        }
    }

    #[test]
    fn delivery_settings_still_clear_a_stranded_target_lock() {
        // Both settings decide whether delivery targets a focused window, so
        // both must keep dropping a lock that can no longer be used (#162).
        // Collapsing their commands into the generic mutator moved this from
        // two hand-written call sites into the effect table.
        // A per-application profile decides the same two things for one
        // application, so editing the list can strand the lock the same way
        // (#123).
        for key in ["paste_method", "auto_submit", "output_profiles"] {
            assert!(
                effects_for_setting(key).contains(&SettingEffect::ClearTargetLockIfFocusFree),
                "changing '{key}' must re-check the target-lock"
            );
        }
    }

    #[test]
    fn output_profiles_are_written_through_the_generic_mutator() {
        // The whole list is one plain `AppSettings` field, so add, edit, and
        // remove are all the same wholesale write. Keeping it off the
        // dedicated-command list is what lets the settings UI persist it with
        // `update_setting` like any other list setting (#123).
        assert!(!SETTINGS_WITH_DEDICATED_COMMANDS.contains(&"output_profiles"));
        assert!(effects_for_setting("output_profiles").contains(&SettingEffect::EmitChanged));
    }

    #[test]
    fn auto_submit_still_notifies_its_two_surfaces() {
        // The settings window and the tray quick-toggle (#12) share this
        // setting, so a change from either has to be broadcast.
        assert!(effects_for_setting("auto_submit").contains(&SettingEffect::EmitChanged));
    }

    #[test]
    fn settings_with_side_effects_keep_them() {
        let expectations: &[(&str, SettingEffect)] = &[
            ("autostart_enabled", SettingEffect::ApplyAutostart),
            ("overlay_position", SettingEffect::UpdateOverlayPosition),
            ("show_tray_icon", SettingEffect::SetTrayVisibility),
            ("app_language", SettingEffect::RefreshTrayMenu),
            ("whisper_accelerator", SettingEffect::ReloadAccelerator),
            ("ort_accelerator", SettingEffect::ReloadAccelerator),
            ("whisper_gpu_device", SettingEffect::ReloadAccelerator),
            (
                "post_process_enabled",
                SettingEffect::SyncPostProcessShortcut,
            ),
            ("raw_output", SettingEffect::EmitChanged),
        ];
        for (key, effect) in expectations {
            assert!(
                effects_for_setting(key).contains(effect),
                "changing '{key}' must still run {effect:?}"
            );
        }
    }

    #[test]
    fn plain_settings_need_nothing_but_the_write() {
        for key in [
            "audio_feedback_volume",
            "selected_language",
            "custom_words",
            "typing_tool",
        ] {
            assert!(
                effects_for_setting(key).is_empty(),
                "unexpected effect for '{key}'"
            );
        }
    }

    #[test]
    fn successful_rollback_leaves_a_later_initialization_retry_safe() {
        let bindings = vec![
            ("transcribe".to_string(), test_binding("transcribe")),
            ("second".to_string(), test_binding("second")),
        ];
        let backend = RefCell::new(ShortcutBackendModel {
            fail_next_registration: Some("transcribe".to_string()),
            activate_before_failure: true,
            ..ShortcutBackendModel::default()
        });

        let first_result = register_initial_bindings(
            bindings.clone(),
            |binding| backend.borrow_mut().register(binding),
            |binding| backend.borrow_mut().unregister(binding),
        );

        assert_eq!(
            first_result,
            Err("Failed to register shortcut 'transcribe': simulated backend failure".to_string())
        );
        assert!(backend.borrow().active.is_empty());

        let second_result = register_initial_bindings(
            bindings,
            |binding| backend.borrow_mut().register(binding),
            |binding| backend.borrow_mut().unregister(binding),
        );

        assert_eq!(second_result, Ok(()));
        assert_eq!(backend.borrow().active, vec!["transcribe", "second"]);
    }

    #[test]
    fn pre_registration_failure_does_not_attempt_cleanup() {
        let cleanup_called = std::cell::Cell::new(false);

        let result = register_initial_bindings(
            vec![("transcribe".to_string(), test_binding("transcribe"))],
            |_| {
                Err(ShortcutRegistrationError::before_activation(
                    "invalid shortcut".to_string(),
                ))
            },
            |_| {
                cleanup_called.set(true);
                Err("the invalid shortcut cannot be parsed for cleanup".to_string())
            },
        );

        assert_eq!(
            result,
            Err("Failed to register shortcut 'transcribe': invalid shortcut".to_string())
        );
        assert!(!cleanup_called.get());
        assert!(initialization_error_allows_retry(
            result.as_ref().unwrap_err()
        ));
    }

    #[test]
    fn optional_registration_failure_keeps_the_primary_shortcut_active() {
        let backend = RefCell::new(ShortcutBackendModel {
            fail_next_registration: Some("second".to_string()),
            ..ShortcutBackendModel::default()
        });

        let result = register_initial_bindings(
            vec![
                ("transcribe".to_string(), test_binding("transcribe")),
                ("second".to_string(), test_binding("second")),
            ],
            |binding| backend.borrow_mut().register(binding),
            |binding| backend.borrow_mut().unregister(binding),
        );

        assert_eq!(result, Ok(()));
        assert_eq!(backend.borrow().active, vec!["transcribe"]);
    }

    #[test]
    fn partial_optional_registration_is_cleaned_up() {
        let backend = RefCell::new(ShortcutBackendModel {
            fail_next_registration: Some("second".to_string()),
            activate_before_failure: true,
            ..ShortcutBackendModel::default()
        });

        let result = register_initial_bindings(
            vec![
                ("transcribe".to_string(), test_binding("transcribe")),
                ("second".to_string(), test_binding("second")),
            ],
            |binding| backend.borrow_mut().register(binding),
            |binding| backend.borrow_mut().unregister(binding),
        );

        assert_eq!(result, Ok(()));
        assert_eq!(backend.borrow().active, vec!["transcribe"]);
    }

    #[test]
    fn failed_optional_cleanup_marks_shortcut_state_uncertain() {
        let backend = RefCell::new(ShortcutBackendModel {
            fail_next_registration: Some("second".to_string()),
            activate_before_failure: true,
            fail_cleanup: true,
            ..ShortcutBackendModel::default()
        });

        let result = register_initial_bindings(
            vec![
                ("transcribe".to_string(), test_binding("transcribe")),
                ("second".to_string(), test_binding("second")),
            ],
            |binding| backend.borrow_mut().register(binding),
            |binding| backend.borrow_mut().unregister(binding),
        );

        let error = result.expect_err("failed optional cleanup must fail initialization");
        assert!(error.contains(UNCERTAIN_SHORTCUT_STATE));
        assert!(!initialization_error_allows_retry(&error));
    }

    #[test]
    fn rollback_failure_reports_that_shortcut_state_may_remain_active() {
        let backend = RefCell::new(ShortcutBackendModel {
            fail_next_registration: Some("transcribe".to_string()),
            activate_before_failure: true,
            fail_cleanup: true,
            ..ShortcutBackendModel::default()
        });

        let result = register_initial_bindings(
            vec![
                ("transcribe".to_string(), test_binding("transcribe")),
                ("second".to_string(), test_binding("second")),
            ],
            |binding| backend.borrow_mut().register(binding),
            |binding| backend.borrow_mut().unregister(binding),
        );

        assert_eq!(
            result,
            Err(
                "Failed to register shortcut 'transcribe': simulated backend failure; cleanup failed for 'transcribe': simulated cleanup failure; shortcut state may remain active; restart AudioBud before retrying shortcut initialization"
                    .to_string()
            )
        );
        assert_eq!(backend.borrow().active, vec!["transcribe"]);
        assert!(!initialization_error_allows_retry(
            result.as_ref().unwrap_err()
        ));
    }

    #[test]
    fn uncertain_shortcut_state_does_not_start_a_fallback_backend() {
        let fallback_started = std::cell::Cell::new(false);
        let cleanup_error =
            "shortcut state may remain active; restart AudioBud before retrying".to_string();

        let result = with_retry_safe_handy_keys_fallback(cleanup_error.clone(), || {
            fallback_started.set(true);
            Ok(())
        });

        assert_eq!(result, Err(cleanup_error));
        assert!(!fallback_started.get());
    }

    #[test]
    fn retry_safe_handy_keys_error_can_start_the_tauri_fallback() {
        let fallback_started = std::cell::Cell::new(false);

        let result = with_retry_safe_handy_keys_fallback(
            "HandyKeys manager startup timed out".to_string(),
            || {
                fallback_started.set(true);
                Ok(())
            },
        );

        assert_eq!(result, Ok(()));
        assert!(fallback_started.get());
    }

    #[test]
    fn failed_fallback_persistence_removes_the_live_fallback() {
        let cleanup_called = std::cell::Cell::new(false);

        let result = commit_initialized_fallback(
            || Err("simulated settings write failure".to_string()),
            || {
                cleanup_called.set(true);
                Ok(())
            },
        );

        assert!(cleanup_called.get());
        assert_eq!(
            result,
            Err(
                "Failed to persist the Tauri shortcut fallback: simulated settings write failure. The live Tauri fallback was removed; retry shortcut initialization."
                    .to_string()
            )
        );
    }

    #[test]
    fn failed_fallback_cleanup_marks_shortcut_state_uncertain() {
        let result = commit_initialized_fallback(
            || Err("simulated settings write failure".to_string()),
            || Err("simulated cleanup failure".to_string()),
        );

        let error = result.expect_err("failed persistence and cleanup must fail");
        assert!(error.contains(UNCERTAIN_SHORTCUT_STATE));
        assert!(!initialization_error_allows_retry(&error));
    }

    #[test]
    fn persisted_fallback_keeps_the_live_backend() {
        let cleanup_called = std::cell::Cell::new(false);

        let result = commit_initialized_fallback(
            || Ok(()),
            || {
                cleanup_called.set(true);
                Ok(())
            },
        );

        assert_eq!(result, Ok(()));
        assert!(!cleanup_called.get());
    }

    fn test_binding(id: &str) -> ShortcutBinding {
        ShortcutBinding {
            id: id.to_string(),
            name: id.to_string(),
            description: String::new(),
            default_binding: "ctrl+a".to_string(),
            current_binding: "ctrl+a".to_string(),
        }
    }

    #[test]
    fn external_script_path_requires_no_confirmation_when_cleared() {
        assert!(!external_script_path_requires_confirmation(&None));
        assert!(!external_script_path_requires_confirmation(&Some(
            String::new()
        )));
        assert!(!external_script_path_requires_confirmation(&Some(
            "   ".to_string()
        )));
    }
}
