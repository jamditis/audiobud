use crate::managers::history::{HistoryEntry, HistoryManager};
use crate::managers::model::ModelManager;
use crate::managers::transcription::TranscriptionManager;
use crate::settings;
use crate::settings::OverlayPosition;
use crate::tray_i18n::get_tray_translations;
use log::{error, info, warn};
use std::sync::Arc;
use tauri::image::Image;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::TrayIcon;
use tauri::{AppHandle, Manager, Theme};
use tauri_plugin_clipboard_manager::ClipboardExt;

#[derive(Clone, Debug, PartialEq)]
pub enum TrayIconState {
    Idle,
    Recording,
    Transcribing,
}

/// Tracks the tray's current logical state so menu rebuilds triggered by events
/// (e.g. settings changes) can preserve the recording/transcribing menu instead of
/// forcing Idle. Updated by `change_tray_icon`, the single point all
/// recording/transcription transitions flow through.
pub struct CurrentTrayState(pub std::sync::Mutex<TrayIconState>);

impl CurrentTrayState {
    pub fn new() -> Self {
        Self(std::sync::Mutex::new(TrayIconState::Idle))
    }
}

impl Default for CurrentTrayState {
    fn default() -> Self {
        Self::new()
    }
}

/// Reads the current tray state, defaulting to Idle if it has not been managed yet.
pub fn current_tray_state(app: &AppHandle) -> TrayIconState {
    app.try_state::<CurrentTrayState>()
        .map(|s| s.0.lock().unwrap().clone())
        .unwrap_or(TrayIconState::Idle)
}

#[derive(Clone, Debug, PartialEq)]
pub enum AppTheme {
    Dark,
    Light,
    Colored, // Pink/colored theme for Linux
}

/// Gets the current app theme, with Linux defaulting to Colored theme
pub fn get_current_theme(app: &AppHandle) -> AppTheme {
    if cfg!(target_os = "linux") {
        // On Linux, always use the colored theme
        AppTheme::Colored
    } else {
        // On other platforms, map system theme to our app theme
        if let Some(main_window) = app.get_webview_window("main") {
            match main_window.theme().unwrap_or(Theme::Dark) {
                Theme::Light => AppTheme::Light,
                Theme::Dark => AppTheme::Dark,
                _ => AppTheme::Dark, // Default fallback
            }
        } else {
            AppTheme::Dark
        }
    }
}

/// Gets the appropriate icon path for the given theme and state
pub fn get_icon_path(theme: AppTheme, state: TrayIconState) -> &'static str {
    match (theme, state) {
        // Dark theme uses light icons
        (AppTheme::Dark, TrayIconState::Idle) => "resources/tray_idle.png",
        (AppTheme::Dark, TrayIconState::Recording) => "resources/tray_recording.png",
        (AppTheme::Dark, TrayIconState::Transcribing) => "resources/tray_transcribing.png",
        // Light theme uses dark icons
        (AppTheme::Light, TrayIconState::Idle) => "resources/tray_idle_dark.png",
        (AppTheme::Light, TrayIconState::Recording) => "resources/tray_recording_dark.png",
        (AppTheme::Light, TrayIconState::Transcribing) => "resources/tray_transcribing_dark.png",
        // Colored theme uses pink icons (for Linux)
        (AppTheme::Colored, TrayIconState::Idle) => "resources/handy.png",
        (AppTheme::Colored, TrayIconState::Recording) => "resources/recording.png",
        (AppTheme::Colored, TrayIconState::Transcribing) => "resources/transcribing.png",
    }
}

pub fn change_tray_icon(app: &AppHandle, icon: TrayIconState) {
    // Remember the current state so event-driven menu rebuilds (settings changes,
    // model state) can preserve the recording/transcribing menu instead of Idle.
    if let Some(current) = app.try_state::<CurrentTrayState>() {
        *current.0.lock().unwrap() = icon.clone();
    }

    let tray = app.state::<TrayIcon>();
    let theme = get_current_theme(app);

    let icon_path = get_icon_path(theme, icon.clone());

    let _ = tray.set_icon(Some(
        Image::from_path(
            app.path()
                .resolve(icon_path, tauri::path::BaseDirectory::Resource)
                .expect("failed to resolve"),
        )
        .expect("failed to set icon"),
    ));

    // Update menu based on state
    update_tray_menu(app, &icon, None);
}

pub fn tray_tooltip() -> String {
    version_label()
}

/// The name the app presents to the user. Kept in step with tauri.conf.json's
/// productName by `tray_tooltip_names_the_product_not_the_upstream_fork`.
const APP_NAME: &str = "AudioBud";

fn version_label() -> String {
    if cfg!(debug_assertions) {
        format!("{APP_NAME} v{} (Dev)", env!("CARGO_PKG_VERSION"))
    } else {
        format!("{APP_NAME} v{}", env!("CARGO_PKG_VERSION"))
    }
}

pub fn update_tray_menu(app: &AppHandle, state: &TrayIconState, locale: Option<&str>) {
    let settings = settings::get_settings(app);

    let locale = locale.unwrap_or(&settings.app_language);
    let strings = get_tray_translations(Some(locale.to_string()));

    // Platform-specific accelerators
    #[cfg(target_os = "macos")]
    let (settings_accelerator, quit_accelerator) = (Some("Cmd+,"), Some("Cmd+Q"));
    #[cfg(not(target_os = "macos"))]
    let (settings_accelerator, quit_accelerator) = (Some("Ctrl+,"), Some("Ctrl+Q"));

    // Create common menu items
    let version_label = version_label();
    let version_i = MenuItem::with_id(app, "version", &version_label, false, None::<&str>)
        .expect("failed to create version item");
    let settings_i = MenuItem::with_id(
        app,
        "settings",
        &strings.settings,
        true,
        settings_accelerator,
    )
    .expect("failed to create settings item");
    let check_updates_i = MenuItem::with_id(
        app,
        "check_updates",
        &strings.check_updates,
        crate::update_checks_action_enabled(settings.update_checks_enabled),
        None::<&str>,
    )
    .expect("failed to create check updates item");
    let copy_last_transcript_i = MenuItem::with_id(
        app,
        "copy_last_transcript",
        &strings.copy_last_transcript,
        true,
        None::<&str>,
    )
    .expect("failed to create copy last transcript item");
    let model_loaded = app.state::<Arc<TranscriptionManager>>().is_model_loaded();
    let quit_i = MenuItem::with_id(app, "quit", &strings.quit, true, quit_accelerator)
        .expect("failed to create quit item");
    let separator = || PredefinedMenuItem::separator(app).expect("failed to create separator");

    // Build model submenu — label is the active model name
    let model_manager = app.state::<Arc<ModelManager>>();
    let models = model_manager.get_available_models();
    let current_model_id = &settings.selected_model;

    let mut downloaded: Vec<_> = models.into_iter().filter(|m| m.is_downloaded).collect();
    downloaded.sort_by(|a, b| a.name.cmp(&b.name));

    let submenu_label = downloaded
        .iter()
        .find(|m| m.id == *current_model_id)
        .map(|m| m.name.clone())
        .unwrap_or_else(|| strings.model.clone());

    let model_submenu = {
        let submenu = Submenu::with_id(app, "model_submenu", &submenu_label, true)
            .expect("failed to create model submenu");

        for model in &downloaded {
            let is_active = model.id == *current_model_id;
            let item_id = format!("model_select:{}", model.id);
            let item =
                CheckMenuItem::with_id(app, &item_id, &model.name, true, is_active, None::<&str>)
                    .expect("failed to create model item");
            let _ = submenu.append(&item);
        }

        submenu
    };

    let unload_model_i = MenuItem::with_id(
        app,
        "unload_model",
        &strings.unload_model,
        model_loaded,
        None::<&str>,
    )
    .expect("failed to create unload model item");

    // Quick toggles for high-traffic boolean settings (issue #12). They reuse the
    // existing settings; their checked state is read fresh on every menu rebuild, and
    // the `toggle:` handler in lib.rs flips the setting and rebuilds the menu.
    let toggle_ptt_i = CheckMenuItem::with_id(
        app,
        "toggle:push_to_talk",
        &strings.push_to_talk,
        true,
        settings.push_to_talk,
        None::<&str>,
    )
    .expect("failed to create push-to-talk toggle item");
    let toggle_mute_i = CheckMenuItem::with_id(
        app,
        "toggle:mute_while_recording",
        &strings.mute_while_recording,
        true,
        settings.mute_while_recording,
        None::<&str>,
    )
    .expect("failed to create mute-while-recording toggle item");
    let toggle_space_i = CheckMenuItem::with_id(
        app,
        "toggle:append_trailing_space",
        &strings.append_trailing_space,
        true,
        settings.append_trailing_space,
        None::<&str>,
    )
    .expect("failed to create append-trailing-space toggle item");
    let toggle_auto_submit_i = CheckMenuItem::with_id(
        app,
        "toggle:auto_submit",
        &strings.auto_submit,
        true,
        settings.auto_submit,
        None::<&str>,
    )
    .expect("failed to create auto-submit toggle item");
    // The overlay isn't a plain bool: visible = any position other than None.
    // Toggling hides it (None) or restores the last visible placement.
    let toggle_overlay_i = CheckMenuItem::with_id(
        app,
        "toggle:overlay_visible",
        &strings.show_overlay,
        true,
        settings.overlay_position != OverlayPosition::None,
        None::<&str>,
    )
    .expect("failed to create show-overlay toggle item");

    // Target lock (#120), Windows-only for now (#119). Checked while a window is
    // locked; the `toggle:` handler in lib.rs flips it and rebuilds the menu.
    // The label carries the locked window's name too (#255) so the tray never
    // disagrees with the overlay/settings indicator about what is locked --
    // including while the lock is stale (#266 review): `LostLockNotice` is
    // consulted only when nothing is locked, so the tray keeps naming the
    // window that was just lost instead of silently reverting to the plain
    // unlocked item the instant `PinnedTarget` clears.
    //
    // `locked()` is read exactly once into `target_lock_identity` and both the
    // label and the checkmark derive from that one value (#266 review): two
    // separate reads here could tear if a delivery on another thread dropped
    // the lock in between them, showing a checked item with a stale label or
    // vice versa.
    #[cfg(target_os = "windows")]
    let target_lock_identity = app
        .try_state::<crate::output_target::PinnedTarget>()
        .and_then(|lock| lock.locked());
    #[cfg(target_os = "windows")]
    let target_lock_locked_label =
        target_lock_identity.map(crate::output_target::backend::window_label);
    #[cfg(target_os = "windows")]
    let target_lock_lost_label = if target_lock_locked_label.is_none() {
        app.try_state::<crate::output_target::LostLockNotice>()
            .and_then(|notice| notice.get())
    } else {
        None
    };
    #[cfg(target_os = "windows")]
    let (target_lock_label, target_lock_checked) = target_lock_menu_label(
        target_lock_locked_label,
        target_lock_lost_label,
        &strings.lock_to_window,
        &strings.lock_lost,
    );
    #[cfg(target_os = "windows")]
    let toggle_target_lock_i = CheckMenuItem::with_id(
        app,
        "toggle:target_lock",
        &target_lock_label,
        true,
        target_lock_checked,
        None::<&str>,
    )
    .expect("failed to create target-lock toggle item");

    // Output-mode submenu: switch dictation between a formatted transcript (punctuation, casing,
    // and digit/currency number formatting) and a raw transcript (verbatim, lowercased). The two
    // items act as radio buttons over the `raw_output` setting; the checked one is derived fresh on
    // every rebuild, and the `output_mode:` handler in lib.rs applies the switch.
    let output_mode_submenu = {
        let submenu = Submenu::with_id(app, "output_mode_submenu", &strings.output_mode, true)
            .expect("failed to create output mode submenu");
        let formatted_i = CheckMenuItem::with_id(
            app,
            "output_mode:formatted",
            &strings.formatted,
            true,
            !settings.raw_output,
            None::<&str>,
        )
        .expect("failed to create formatted output item");
        let raw_i = CheckMenuItem::with_id(
            app,
            "output_mode:raw",
            &strings.raw_transcript,
            true,
            settings.raw_output,
            None::<&str>,
        )
        .expect("failed to create raw output item");
        let _ = submenu.append(&formatted_i);
        let _ = submenu.append(&raw_i);
        submenu
    };

    let menu = match state {
        TrayIconState::Recording | TrayIconState::Transcribing => {
            let cancel_i = MenuItem::with_id(app, "cancel", &strings.cancel, true, None::<&str>)
                .expect("failed to create cancel item");
            Menu::with_items(
                app,
                &[
                    &version_i,
                    &separator(),
                    &cancel_i,
                    &separator(),
                    &copy_last_transcript_i,
                    &separator(),
                    &settings_i,
                    &check_updates_i,
                    &separator(),
                    &quit_i,
                ],
            )
            .expect("failed to create menu")
        }
        TrayIconState::Idle => {
            // Built as a list rather than one array literal because the
            // target-lock toggle is Windows-only. Separators are bound to
            // locals so they outlive the borrows collected here.
            let separators: Vec<_> = (0..6).map(|_| separator()).collect();
            let mut items: Vec<&dyn tauri::menu::IsMenuItem<tauri::Wry>> = vec![
                &version_i,
                &separators[0],
                &copy_last_transcript_i,
                &separators[1],
                &model_submenu,
                &unload_model_i,
                &separators[2],
                &output_mode_submenu,
                &separators[3],
                &toggle_ptt_i,
                &toggle_mute_i,
                &toggle_space_i,
                &toggle_auto_submit_i,
                &toggle_overlay_i,
            ];
            #[cfg(target_os = "windows")]
            items.push(&toggle_target_lock_i);
            items.extend([
                &separators[4] as &dyn tauri::menu::IsMenuItem<tauri::Wry>,
                &settings_i,
                &check_updates_i,
                &separators[5],
                &quit_i,
            ]);
            Menu::with_items(app, &items).expect("failed to create menu")
        }
    };

    let tray = app.state::<TrayIcon>();
    let _ = tray.set_menu(Some(menu));
    let _ = tray.set_icon_as_template(true);
    let _ = tray.set_tooltip(Some(version_label));
}

/// Truncate a tray label's variable portion so a long window title cannot
/// stretch the menu row. Mirrors the spirit of the frontend indicator's
/// `truncateName` (`output-target-indicator.ts`) without sharing code across
/// the Rust/TS boundary -- the tray and the overlay/settings indicator only
/// need to agree on *what* is locked, not on byte-identical truncation.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn truncate_tray_label(name: &str, max_chars: usize) -> String {
    let chars: Vec<char> = name.chars().collect();
    if chars.len() <= max_chars {
        name.to_string()
    } else {
        let mut truncated: String = chars[..max_chars].iter().collect();
        truncated.push_str("...");
        truncated
    }
}

/// Derive the target-lock tray item's label and checkmark from a single read
/// of the lock state (#266 review).
///
/// Platform-independent and deliberately fed already-resolved data rather
/// than a `WindowIdentity` or a `PinnedTarget`/`LostLockNotice` handle, so the
/// tearing fix -- one read, both outputs derived from it -- and the lost-label
/// composition are unit-testable without a window system.
///
/// `locked` is the currently-locked window's label, if any. `lost` is
/// consulted only when nothing is locked, and is the tray's memory of the
/// most recent loss (`LostLockNotice`): showing it keeps the tray agreeing
/// with the overlay's stale indicator instead of silently reverting to the
/// plain unlocked item the instant the lock is dropped.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn target_lock_menu_label(
    locked: Option<crate::output_target::WindowLabel>,
    lost: Option<crate::output_target::WindowLabel>,
    lock_to_window: &str,
    lock_lost: &str,
) -> (String, bool) {
    fn name_or(app_name: Option<String>, title: Option<String>, fallback: &str) -> String {
        app_name.or(title).unwrap_or_else(|| fallback.to_string())
    }

    match locked {
        Some((app_name, title)) => {
            let name = name_or(app_name, title, lock_to_window);
            (
                format!("{} — {}", lock_to_window, truncate_tray_label(&name, 28)),
                true,
            )
        }
        None => match lost {
            Some((app_name, title)) => {
                let name = name_or(app_name, title, lock_lost);
                (
                    format!("{} — {}", lock_lost, truncate_tray_label(&name, 28)),
                    false,
                )
            }
            None => (lock_to_window.to_string(), false),
        },
    }
}

fn last_transcript_text(entry: &HistoryEntry) -> &str {
    entry
        .post_processed_text
        .as_deref()
        .unwrap_or(&entry.transcription_text)
}

pub fn set_tray_visibility(app: &AppHandle, visible: bool) {
    let tray = app.state::<TrayIcon>();
    if let Err(e) = tray.set_visible(visible) {
        error!("Failed to set tray visibility: {}", e);
    } else {
        info!("Tray visibility set to: {}", visible);
    }
}

pub fn copy_last_transcript(app: &AppHandle) {
    let history_manager = app.state::<Arc<HistoryManager>>();
    let entry = match history_manager.get_latest_completed_entry() {
        Ok(Some(entry)) => entry,
        Ok(None) => {
            warn!("No completed transcription history entries available for tray copy.");
            return;
        }
        Err(err) => {
            error!(
                "Failed to fetch last completed transcription entry: {}",
                err
            );
            return;
        }
    };

    let text = last_transcript_text(&entry);
    if text.trim().is_empty() {
        warn!("Last completed transcription is empty; skipping tray copy.");
        return;
    }

    if let Err(err) = app.clipboard().write_text(text) {
        error!("Failed to copy last transcript to clipboard: {}", err);
        return;
    }

    info!("Copied last transcript to clipboard via tray.");
}

#[cfg(test)]
mod tests {
    use super::{last_transcript_text, target_lock_menu_label, truncate_tray_label};
    use crate::managers::history::HistoryEntry;

    #[test]
    fn truncate_tray_label_leaves_short_names_alone() {
        assert_eq!(truncate_tray_label("Terminal", 28), "Terminal");
    }

    #[test]
    fn truncate_tray_label_shortens_long_names() {
        let long_name = "a".repeat(40);
        let truncated = truncate_tray_label(&long_name, 28);
        assert_eq!(truncated, format!("{}...", "a".repeat(28)));
    }

    #[test]
    fn target_lock_menu_label_when_unlocked_and_never_lost() {
        let (label, checked) = target_lock_menu_label(None, None, "Lock to window", "Lock lost");
        assert_eq!(label, "Lock to window");
        assert!(!checked);
    }

    #[test]
    fn target_lock_menu_label_when_locked_ignores_any_lost_notice() {
        // A `lost` value can only be stale leftover state here -- a fresh lock
        // always clears the notice first (#266 review) -- but the derivation
        // must still prefer `locked` over it rather than accidentally letting
        // a leftover notice contradict an active lock.
        let (label, checked) = target_lock_menu_label(
            Some((Some("Terminal".to_string()), None)),
            Some((Some("Old window".to_string()), None)),
            "Lock to window",
            "Lock lost",
        );
        assert_eq!(label, "Lock to window — Terminal");
        assert!(checked);
    }

    #[test]
    fn target_lock_menu_label_when_locked_falls_back_to_the_title() {
        let (label, checked) = target_lock_menu_label(
            Some((None, Some("Untitled document - Notepad".to_string()))),
            None,
            "Lock to window",
            "Lock lost",
        );
        assert_eq!(label, "Lock to window — Untitled document - Notepad");
        assert!(checked);
    }

    #[test]
    fn target_lock_menu_label_when_stale_shows_the_lost_window_unchecked() {
        // The core of #266's finding: the tray must keep naming the window
        // that was lost, not silently revert to the plain unlocked label the
        // instant `PinnedTarget` clears -- otherwise the tray and the
        // overlay's stale indicator visibly disagree.
        let (label, checked) = target_lock_menu_label(
            None,
            Some((Some("Terminal".to_string()), None)),
            "Lock to window",
            "Lock lost",
        );
        assert_eq!(label, "Lock lost — Terminal");
        assert!(!checked);
    }

    #[test]
    fn target_lock_menu_label_when_stale_with_no_name_falls_back_to_the_lost_word() {
        let (label, checked) =
            target_lock_menu_label(None, Some((None, None)), "Lock to window", "Lock lost");
        assert_eq!(label, "Lock lost — Lock lost");
        assert!(!checked);
    }

    #[test]
    fn tray_tooltip_names_the_product_not_the_upstream_fork() {
        // The tooltip is the most visible place the app names itself, and it kept
        // saying "Handy" for three releases after the fork. Pin it to the one source
        // of truth (tauri.conf.json's productName) rather than to a literal, so a
        // future rename cannot strand it again.
        let conf: serde_json::Value = serde_json::from_str(include_str!("../tauri.conf.json"))
            .expect("tauri.conf.json parses");
        let product = conf["productName"]
            .as_str()
            .expect("productName is a string");

        let tooltip = super::tray_tooltip();
        assert!(
            tooltip.starts_with(product),
            "tooltip {tooltip:?} does not start with productName {product:?}"
        );
        assert!(
            !tooltip.contains("Handy"),
            "tooltip still names the upstream fork: {tooltip:?}"
        );
    }

    fn build_entry(transcription: &str, post_processed: Option<&str>) -> HistoryEntry {
        HistoryEntry {
            id: 1,
            file_name: "handy-1.wav".to_string(),
            timestamp: 0,
            saved: false,
            title: "Recording".to_string(),
            transcription_text: transcription.to_string(),
            post_processed_text: post_processed.map(|text| text.to_string()),
            post_process_prompt: None,
            post_process_requested: false,
            raw_requested: false,
        }
    }

    #[test]
    fn uses_post_processed_text_when_available() {
        let entry = build_entry("raw", Some("processed"));
        assert_eq!(last_transcript_text(&entry), "processed");
    }

    #[test]
    fn falls_back_to_raw_transcription() {
        let entry = build_entry("raw", None);
        assert_eq!(last_transcript_text(&entry), "raw");
    }
}
