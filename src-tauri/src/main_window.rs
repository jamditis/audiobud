use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

pub const MAIN_WINDOW_LABEL: &str = "main";

#[derive(Default)]
pub struct MainWindowState {
    creating: AtomicBool,
    show_requested: AtomicBool,
    ready: AtomicBool,
    pending_update_check: AtomicBool,
}

impl MainWindowState {
    fn claim_creation(&self) -> bool {
        self.creating
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    }
}

pub fn initialize(app: &AppHandle) {
    app.manage(MainWindowState::default());
}

pub fn required_at_launch(
    settings_start_hidden: bool,
    cli_start_hidden: bool,
    permission_onboarding: bool,
    tray_available: bool,
) -> bool {
    permission_onboarding || !(settings_start_hidden || cli_start_hidden) || !tray_available
}

fn show_existing(app: &AppHandle) -> bool {
    let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) else {
        return false;
    };

    if let Err(error) = window.unminimize() {
        log::error!("Failed to unminimize settings window: {error}");
    }
    if let Err(error) = window.show() {
        log::error!("Failed to show settings window: {error}");
    }
    if let Err(error) = window.set_focus() {
        log::error!("Failed to focus settings window: {error}");
    }
    #[cfg(target_os = "macos")]
    if let Err(error) = app.set_activation_policy(tauri::ActivationPolicy::Regular) {
        log::error!("Failed to show the AudioBud dock icon: {error}");
    }

    true
}

fn build(app: &AppHandle) -> Result<(), tauri::Error> {
    let mut builder =
        WebviewWindowBuilder::new(app, MAIN_WINDOW_LABEL, WebviewUrl::App("/".into()))
            .title("AudioBud")
            .inner_size(680.0, 570.0)
            .min_inner_size(680.0, 570.0)
            .resizable(true)
            .maximizable(false)
            .visible(false);

    if let Some(data_dir) = crate::portable::data_dir() {
        builder = builder.data_directory(data_dir.join("webview"));
    }

    builder.build().map(|_| ())
}

/// Create the settings webview if needed, then show and focus it. Creation is
/// claimed atomically so simultaneous tray and second-instance requests still
/// build one window.
pub fn show(app: &AppHandle) {
    let state = app.state::<MainWindowState>();
    state.show_requested.store(true, Ordering::SeqCst);

    if show_existing(app) {
        state.show_requested.store(false, Ordering::SeqCst);
        return;
    }
    if !state.claim_creation() {
        return;
    }

    let app_for_create = app.clone();
    if let Err(error) = app.run_on_main_thread(move || {
        let result = if app_for_create
            .get_webview_window(MAIN_WINDOW_LABEL)
            .is_some()
        {
            Ok(())
        } else {
            build(&app_for_create)
        };

        let state = app_for_create.state::<MainWindowState>();
        state.creating.store(false, Ordering::SeqCst);
        match result {
            Ok(()) => {
                if state.show_requested.swap(false, Ordering::SeqCst) {
                    show_existing(&app_for_create);
                }
            }
            Err(error) => {
                log::error!("Failed to create settings window: {error}");
            }
        }
    }) {
        state.creating.store(false, Ordering::SeqCst);
        log::error!("Failed to schedule settings window creation: {error}");
    }
}

fn flush_update_check(app: &AppHandle) {
    let state = app.state::<MainWindowState>();
    if state.ready.load(Ordering::SeqCst)
        && state.pending_update_check.swap(false, Ordering::SeqCst)
        && app.emit("check-for-updates", ()).is_err()
    {
        state.pending_update_check.store(true, Ordering::SeqCst);
    }
}

/// Open Settings and deliver an update-check request once that webview says
/// its event listeners are ready.
pub fn show_for_update_check(app: &AppHandle) {
    let state = app.state::<MainWindowState>();
    state.pending_update_check.store(true, Ordering::SeqCst);
    show(app);
    flush_update_check(app);
}

/// Mark the settings React surface ready. The returned flag is a durable
/// fallback for a request queued before its event listener existed.
#[tauri::command]
#[specta::specta]
pub fn main_window_ready(app: AppHandle) -> bool {
    let state = app.state::<MainWindowState>();
    state.ready.store(true, Ordering::SeqCst);
    state.pending_update_check.swap(false, Ordering::SeqCst)
}

#[cfg(test)]
mod tests {
    use super::{required_at_launch, MainWindowState};
    use std::sync::Arc;

    #[test]
    fn simultaneous_requests_claim_one_creation() {
        let state = Arc::new(MainWindowState::default());
        let mut requests = Vec::new();
        for _ in 0..8 {
            let state = Arc::clone(&state);
            requests.push(std::thread::spawn(move || state.claim_creation()));
        }

        let winners = requests
            .into_iter()
            .map(|request| request.join().expect("window claimant did not panic"))
            .filter(|won| *won)
            .count();
        assert_eq!(winners, 1);
    }

    #[test]
    fn hidden_launch_skips_the_webview_only_when_tray_is_usable() {
        assert!(!required_at_launch(true, false, false, true));
        assert!(!required_at_launch(false, true, false, true));
        assert!(required_at_launch(true, false, false, false));
        assert!(required_at_launch(true, false, true, true));
        assert!(required_at_launch(false, false, false, true));
    }
}
