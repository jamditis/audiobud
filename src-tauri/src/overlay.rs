use crate::input;
use crate::output_target::backend::TranscriptDeliveredEvent;
use crate::settings;
use crate::settings::{OverlayAnchor, OverlayPosition};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize};
use tauri_specta::Event;

#[cfg(not(target_os = "macos"))]
use log::debug;

#[cfg(not(target_os = "macos"))]
use tauri::WebviewWindowBuilder;

#[cfg(target_os = "macos")]
use tauri::WebviewUrl;

#[cfg(target_os = "macos")]
use tauri_nspanel::{tauri_panel, CollectionBehavior, PanelBuilder, PanelLevel};

#[cfg(target_os = "linux")]
use gtk_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

#[cfg(target_os = "linux")]
use std::env;

#[cfg(target_os = "macos")]
tauri_panel! {
    panel!(RecordingOverlayPanel {
        config: {
            can_become_key_window: false,
            is_floating_panel: true
        }
    })
}

const OVERLAY_WIDTH: f64 = 172.0;
const OVERLAY_HEIGHT: f64 = 36.0;

/// How long the native overlay window's hide waits, after `hide-overlay` is
/// emitted, for a plain paste -- just enough for the CSS fade-out the event
/// triggers to finish before the window itself disappears.
const OVERLAY_HIDE_DELAY_MS: u64 = 300;

/// How long it waits instead when a delivery confirmation (#165) was just
/// shown. Must comfortably outlast the overlay's own JS timeout for the
/// confirmation chip (`DELIVERY_CONFIRMATION_MS` in RecordingOverlay.tsx,
/// currently 1800ms), or the native window would vanish out from under a
/// chip the user is still reading (#279 review round 2).
const OVERLAY_HIDE_DELAY_AFTER_CONFIRMATION_MS: u64 = 2200;

/// Bumped by every show-overlay or hide-overlay instruction (#279 review
/// round 2). Each hide spawns a thread that sleeps, then only actually calls
/// `.hide()` if this still reads the value it captured before sleeping: a
/// show that lands while it slept (a new dictation starting during the
/// extended delivery-confirmation delay above) or a newer hide otherwise
/// bumps it first, turning a stale sleeper into a silent no-op instead of
/// taking down a window whose state has since moved on. This keeps the
/// existing single-spawned-thread mechanism rather than adding a cancel
/// token or a second thread.
static OVERLAY_VISIBILITY_EPOCH: AtomicU64 = AtomicU64::new(0);

/// Set by `output_target::backend::announce_delivered` right before the
/// delivery pipeline finishes and calls `hide_recording_overlay` (issue
/// #165). Read (and reset) by the very next hide to decide whether the
/// overlay's confirmation chip needs the longer delay above instead of the
/// quick one meant for a plain paste.
static PENDING_DELIVERY_CONFIRMATION: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy)]
struct PendingOverlayState {
    state: &'static str,
    raw: bool,
}

#[derive(Default)]
pub struct RecordingOverlayLifecycle {
    creating: AtomicBool,
    ready: AtomicBool,
    dispatch: Mutex<()>,
    pending: Mutex<Option<PendingOverlayState>>,
    pending_delivery: Mutex<Option<TranscriptDeliveredEvent>>,
}

impl RecordingOverlayLifecycle {
    fn claim_creation(&self) -> bool {
        self.creating
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    }

    fn set_pending(&self, state: PendingOverlayState) {
        *self
            .pending
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(state);
    }

    fn take_pending(&self) -> Option<PendingOverlayState> {
        self.pending
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take()
    }

    fn dispatch_lock(&self) -> std::sync::MutexGuard<'_, ()> {
        self.dispatch
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn set_pending_delivery(&self, event: TranscriptDeliveredEvent) {
        *self
            .pending_delivery
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(event);
    }

    fn take_pending_delivery(&self) -> Option<TranscriptDeliveredEvent> {
        self.pending_delivery
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take()
    }
}

pub fn initialize(app: &AppHandle) {
    app.manage(RecordingOverlayLifecycle::default());
}

/// Mark that a delivery confirmation chip was just shown on the overlay, so
/// the hide that is about to follow gives it time to actually be read.
pub fn mark_delivery_confirmation_pending() {
    PENDING_DELIVERY_CONFIRMATION.store(true, Ordering::SeqCst);
}

/// Emit a delivery confirmation without racing the overlay's first-listener
/// handshake. Other webviews receive the normal broadcast immediately; a
/// not-yet-ready overlay keeps one replay for its own listener.
pub fn emit_delivery_confirmation(app: &AppHandle, event: TranscriptDeliveredEvent) {
    let lifecycle = app.state::<RecordingOverlayLifecycle>();
    let _dispatch = lifecycle.dispatch_lock();
    if settings::get_settings(app).overlay_position != OverlayPosition::None
        && !lifecycle.ready.load(Ordering::SeqCst)
    {
        lifecycle.set_pending_delivery(event.clone());
    }
    let _ = event.emit(app);
}

#[cfg(target_os = "macos")]
const OVERLAY_TOP_OFFSET: f64 = 46.0;
#[cfg(any(target_os = "windows", target_os = "linux"))]
const OVERLAY_TOP_OFFSET: f64 = 4.0;

#[cfg(target_os = "macos")]
const OVERLAY_BOTTOM_OFFSET: f64 = 15.0;

#[cfg(any(target_os = "windows", target_os = "linux"))]
const OVERLAY_BOTTOM_OFFSET: f64 = 40.0;

/// Horizontal margin from the monitor edge for the left/right column anchors
/// of the #9 reposition grid.
const OVERLAY_SIDE_OFFSET: f64 = 16.0;

#[cfg(target_os = "linux")]
fn update_gtk_layer_shell_anchors(overlay_window: &tauri::webview::WebviewWindow) {
    let window_clone = overlay_window.clone();
    let _ = overlay_window.run_on_main_thread(move || {
        // Try to get the GTK window from the Tauri webview
        if let Ok(gtk_window) = window_clone.gtk_window() {
            let settings = settings::get_settings(window_clone.app_handle());
            match settings.overlay_position {
                OverlayPosition::Top => {
                    gtk_window.set_anchor(Edge::Top, true);
                    gtk_window.set_anchor(Edge::Bottom, false);
                }
                OverlayPosition::Bottom | OverlayPosition::None => {
                    gtk_window.set_anchor(Edge::Bottom, true);
                    gtk_window.set_anchor(Edge::Top, false);
                }
            }
        }
    });
}

/// Returns true when the environment variable is set to a truthy value
/// (e.g. "1", "true", "yes", "on").
/// "0", "false", "no", "off" and empty string are treated as falsy (case-insensitive).
/// Returns false when the variable is not set.
#[cfg(target_os = "linux")]
fn env_flag_enabled(name: &str) -> bool {
    match env::var(name) {
        Ok(v) => !matches!(
            v.trim().to_ascii_lowercase().as_str(),
            "" | "0" | "false" | "no" | "off"
        ),
        Err(_) => false,
    }
}

/// Initializes GTK layer shell for Linux overlay window
/// Returns true if layer shell was successfully initialized, false otherwise
#[cfg(target_os = "linux")]
fn init_gtk_layer_shell(overlay_window: &tauri::webview::WebviewWindow) -> bool {
    if env_flag_enabled("HANDY_NO_GTK_LAYER_SHELL") {
        debug!("Skipping GTK layer shell init (HANDY_NO_GTK_LAYER_SHELL is enabled)");
        return false;
    }

    if !gtk_layer_shell::is_supported() {
        return false;
    }

    // Try to get the GTK window from the Tauri webview
    if let Ok(gtk_window) = overlay_window.gtk_window() {
        // Initialize layer shell
        gtk_window.init_layer_shell();
        gtk_window.set_layer(Layer::Overlay);
        gtk_window.set_keyboard_mode(KeyboardMode::None);
        gtk_window.set_exclusive_zone(0);

        update_gtk_layer_shell_anchors(overlay_window);

        return true;
    }
    false
}

/// Marks the overlay's HWND `WS_EX_NOACTIVATE` (Windows only): the window never
/// takes the foreground, no matter what generates the click.
///
/// `.focused(false)` on the window builder only sets the window's state at
/// creation -- it says nothing about what a later mouse click does. Without
/// this style, clicking anything in the overlay (the target-lock indicator's
/// unlock button, #255/#266) activates the overlay's own HWND like any other
/// window would. If that happens while a lock is held, unlocking then resolves
/// the next paste to "whatever is in the foreground" -- which is now the
/// overlay itself, so the transcript would be typed into the overlay rather
/// than the app the user meant. `WS_EX_NOACTIVATE` stops Windows from ever
/// activating this window; it still receives and delivers the click, so the
/// button keeps working.
///
/// Set once at creation: the style persists across every later show/hide.
#[cfg(target_os = "windows")]
fn set_overlay_noactivate(overlay_window: &tauri::webview::WebviewWindow) {
    use windows::Win32::UI::WindowsAndMessaging::{
        GetWindowLongPtrW, SetWindowLongPtrW, GWL_EXSTYLE, WS_EX_NOACTIVATE,
    };

    let overlay_clone = overlay_window.clone();
    let _ = overlay_clone.clone().run_on_main_thread(move || {
        if let Ok(hwnd) = overlay_clone.hwnd() {
            unsafe {
                let current = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
                let with_noactivate = current | (WS_EX_NOACTIVATE.0 as isize);
                SetWindowLongPtrW(hwnd, GWL_EXSTYLE, with_noactivate);
            }
        }
    });
}

/// Forces a window to be topmost using Win32 API (Windows only)
/// This is more reliable than Tauri's set_always_on_top which can be overridden
#[cfg(target_os = "windows")]
fn force_overlay_topmost(overlay_window: &tauri::webview::WebviewWindow) {
    use windows::Win32::UI::WindowsAndMessaging::{
        SetWindowPos, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW,
    };

    // Clone because run_on_main_thread takes 'static
    let overlay_clone = overlay_window.clone();

    // Make sure the Win32 call happens on the UI thread
    let _ = overlay_clone.clone().run_on_main_thread(move || {
        if let Ok(hwnd) = overlay_clone.hwnd() {
            unsafe {
                // Force Z-order: make this window topmost without changing size/pos or stealing focus
                let _ = SetWindowPos(
                    hwnd,
                    Some(HWND_TOPMOST),
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
                );
            }
        }
    });
}

fn get_monitor_with_cursor(app_handle: &AppHandle) -> Option<tauri::Monitor> {
    if let Some(mouse_location) = input::get_cursor_position(app_handle) {
        if let Ok(monitors) = app_handle.available_monitors() {
            for monitor in monitors {
                // Tauri's monitor position/size are physical pixels, but enigo
                // may return logical coordinates (confirmed on macOS via
                // NSEvent::mouseLocation; on Windows, GetCursorPos behavior
                // depends on the process DPI-awareness context). Dividing by
                // scale_factor normalizes to logical, which is safe regardless:
                // if enigo returns logical it matches directly, and if it returns
                // physical on a scale=1 monitor the division is a no-op.
                let scale = monitor.scale_factor();
                let pos = PhysicalPosition::new(
                    (monitor.position().x as f64 / scale) as i32,
                    (monitor.position().y as f64 / scale) as i32,
                );
                let size = PhysicalSize::new(
                    (monitor.size().width as f64 / scale) as u32,
                    (monitor.size().height as f64 / scale) as u32,
                );
                if is_mouse_within_monitor(mouse_location, &pos, &size) {
                    return Some(monitor);
                }
            }
        }
    }

    app_handle.primary_monitor().ok().flatten()
}

fn is_mouse_within_monitor(
    mouse_pos: (i32, i32),
    monitor_pos: &PhysicalPosition<i32>,
    monitor_size: &PhysicalSize<u32>,
) -> bool {
    let (mouse_x, mouse_y) = mouse_pos;
    let PhysicalPosition {
        x: monitor_x,
        y: monitor_y,
    } = *monitor_pos;
    let PhysicalSize {
        width: monitor_width,
        height: monitor_height,
    } = *monitor_size;

    mouse_x >= monitor_x
        && mouse_x < (monitor_x + monitor_width as i32)
        && mouse_y >= monitor_y
        && mouse_y < (monitor_y + monitor_height as i32)
}

/// Returns overlay position in logical coordinates (points on macOS).
///
/// Uses monitor position/size directly rather than work_area(), which can
/// return incorrect coordinates on macOS for monitors with negative positions.
/// The per-platform OVERLAY_TOP_OFFSET / OVERLAY_BOTTOM_OFFSET constants
/// already account for system chrome (menu bar, taskbar).
///
/// Clamp the overlay's top-left so its full rect stays inside the monitor's
/// logical bounds (so a drag nudge or a resolution change can never strand the
/// bug off-screen).
#[derive(Clone, Copy)]
struct LogicalRect {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

fn clamp_overlay_to_monitor(
    x: f64,
    y: f64,
    monitor: LogicalRect,
    overlay_size: (f64, f64),
) -> (f64, f64) {
    let (overlay_width, overlay_height) = overlay_size;
    let max_x = monitor.x + (monitor.width - overlay_width).max(0.0);
    let max_y = monitor.y + (monitor.height - overlay_height).max(0.0);
    (x.clamp(monitor.x, max_x), y.clamp(monitor.y, max_y))
}

/// Resolve a 3x3 grid anchor plus a logical-pixel nudge (dx, dy) into the
/// overlay's top-left position on the given monitor, clamped fully on-screen.
/// Pure function so the placement math is unit-tested without a running app.
fn resolve_overlay_anchor_position(
    monitor: LogicalRect,
    overlay_size: (f64, f64),
    anchor: OverlayAnchor,
    nudge: (f64, f64),
) -> (f64, f64) {
    let (overlay_width, overlay_height) = overlay_size;
    let (dx, dy) = nudge;
    let left = monitor.x + OVERLAY_SIDE_OFFSET;
    let center_x = monitor.x + (monitor.width - overlay_width) / 2.0;
    let right = monitor.x + monitor.width - overlay_width - OVERLAY_SIDE_OFFSET;

    let top = monitor.y + OVERLAY_TOP_OFFSET;
    let middle_y = monitor.y + (monitor.height - overlay_height) / 2.0;
    let bottom = monitor.y + monitor.height - overlay_height - OVERLAY_BOTTOM_OFFSET;

    let (base_x, base_y) = match anchor {
        OverlayAnchor::TopLeft => (left, top),
        OverlayAnchor::TopCenter => (center_x, top),
        OverlayAnchor::TopRight => (right, top),
        OverlayAnchor::MiddleLeft => (left, middle_y),
        OverlayAnchor::MiddleCenter => (center_x, middle_y),
        OverlayAnchor::MiddleRight => (right, middle_y),
        OverlayAnchor::BottomLeft => (left, bottom),
        OverlayAnchor::BottomCenter => (center_x, bottom),
        OverlayAnchor::BottomRight => (right, bottom),
    };

    clamp_overlay_to_monitor(base_x + dx, base_y + dy, monitor, overlay_size)
}

/// We must use LogicalPosition (not PhysicalPosition) because Tauri/tao
/// converts PhysicalPosition using the scale factor of the monitor the window
/// is *currently* on, which is wrong when moving cross-monitor.
fn calculate_overlay_position(app_handle: &AppHandle) -> Option<(f64, f64)> {
    let monitor = get_monitor_with_cursor(app_handle)?;
    let scale = monitor.scale_factor();
    let monitor_x = monitor.position().x as f64 / scale;
    let monitor_y = monitor.position().y as f64 / scale;
    let monitor_width = monitor.size().width as f64 / scale;
    let monitor_height = monitor.size().height as f64 / scale;

    let settings = settings::get_settings(app_handle);

    // A user-chosen placement (anchor + drag nudge from #9) overrides the
    // centered Top/Bottom default, re-resolved on the cursor's monitor and
    // clamped fully on-screen.
    if let Some(custom) = settings.overlay_custom_position {
        return Some(resolve_overlay_anchor_position(
            LogicalRect {
                x: monitor_x,
                y: monitor_y,
                width: monitor_width,
                height: monitor_height,
            },
            (OVERLAY_WIDTH, OVERLAY_HEIGHT),
            custom.anchor,
            (custom.dx, custom.dy),
        ));
    }

    let x = monitor_x + (monitor_width - OVERLAY_WIDTH) / 2.0;
    let y = match settings.overlay_position {
        OverlayPosition::Top => monitor_y + OVERLAY_TOP_OFFSET,
        OverlayPosition::Bottom | OverlayPosition::None => {
            monitor_y + monitor_height - OVERLAY_HEIGHT - OVERLAY_BOTTOM_OFFSET
        }
    };

    Some((x, y))
}

/// Creates the recording overlay window and keeps it hidden by default.
#[cfg(not(target_os = "macos"))]
fn build_recording_overlay(app_handle: &AppHandle) -> Result<(), String> {
    // On Linux (Wayland), monitor detection often fails, but we don't need exact coordinates
    // for Layer Shell as we use anchors. On other platforms, we require a monitor.
    #[cfg(not(target_os = "linux"))]
    {
        let position = calculate_overlay_position(app_handle);
        if position.is_none() {
            return Err("Failed to determine overlay position".to_string());
        }
    }

    // Position starts unset — update_overlay_position() sets the correct
    // LogicalPosition before the overlay is shown.
    let mut builder = WebviewWindowBuilder::new(
        app_handle,
        "recording_overlay",
        tauri::WebviewUrl::App("src/overlay/index.html".into()),
    )
    .title("Recording")
    .resizable(false)
    .inner_size(OVERLAY_WIDTH, OVERLAY_HEIGHT)
    .shadow(false)
    .maximizable(false)
    .minimizable(false)
    .closable(false)
    .accept_first_mouse(true)
    .decorations(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .transparent(true)
    .focused(false)
    .visible(false);

    if let Some(data_dir) = crate::portable::data_dir() {
        builder = builder.data_directory(data_dir.join("webview"));
    }

    match builder.build() {
        Ok(window) => {
            #[cfg(target_os = "linux")]
            {
                // Try to initialize GTK layer shell, ignore errors if compositor doesn't support it
                if init_gtk_layer_shell(&window) {
                    debug!("GTK layer shell initialized for overlay window");
                } else {
                    debug!("GTK layer shell not available, falling back to regular window");
                }
            }

            #[cfg(target_os = "windows")]
            set_overlay_noactivate(&window);

            debug!("Recording overlay window created successfully (hidden)");
            Ok(())
        }
        Err(error) => Err(format!(
            "Failed to create recording overlay window: {error}"
        )),
    }
}

/// Creates the recording overlay panel and keeps it hidden by default (macOS)
#[cfg(target_os = "macos")]
fn build_recording_overlay(app_handle: &AppHandle) -> Result<(), String> {
    let (x, y) = calculate_overlay_position(app_handle)
        .ok_or_else(|| "Failed to determine overlay position".to_string())?;

    // PanelBuilder creates a Tauri window then converts it to NSPanel.
    // The window remains registered, so get_webview_window() still works.
    PanelBuilder::<_, RecordingOverlayPanel>::new(app_handle, "recording_overlay")
        .url(WebviewUrl::App("src/overlay/index.html".into()))
        .title("Recording")
        .position(tauri::Position::Logical(tauri::LogicalPosition { x, y }))
        .level(PanelLevel::Status)
        .size(tauri::Size::Logical(tauri::LogicalSize {
            width: OVERLAY_WIDTH,
            height: OVERLAY_HEIGHT,
        }))
        .has_shadow(false)
        .transparent(true)
        .no_activate(true)
        .corner_radius(0.0)
        .with_window(|window| window.decorations(false).transparent(true))
        .collection_behavior(
            CollectionBehavior::new()
                .can_join_all_spaces()
                .full_screen_auxiliary(),
        )
        .build()
        .map(|panel| panel.hide())
        .map_err(|error| format!("Failed to create recording overlay panel: {error}"))
}

/// Payload for the `show-overlay` event. `raw` tells the overlay whether the current dictation
/// will be emitted as raw transcript so it can surface a "RAW" indicator (issue #24).
#[derive(Clone, serde::Serialize)]
struct OverlayShowPayload<'a> {
    state: &'a str,
    raw: bool,
}

fn dispatch_pending_state_locked(app_handle: &AppHandle, lifecycle: &RecordingOverlayLifecycle) {
    if !lifecycle.ready.load(Ordering::SeqCst) {
        return;
    }
    let Some(pending) = lifecycle.take_pending() else {
        return;
    };

    if settings::get_settings(app_handle).overlay_position == OverlayPosition::None {
        return;
    }

    update_overlay_position(app_handle);
    if let Some(overlay_window) = app_handle.get_webview_window("recording_overlay") {
        let _ = overlay_window.show();

        #[cfg(target_os = "windows")]
        force_overlay_topmost(&overlay_window);

        let _ = overlay_window.emit(
            "show-overlay",
            OverlayShowPayload {
                state: pending.state,
                raw: pending.raw,
            },
        );
    } else {
        lifecycle.set_pending(pending);
    }
}

fn schedule_native_overlay_hide(
    overlay_window: tauri::webview::WebviewWindow,
    delay_ms: u64,
    epoch: u64,
) {
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(delay_ms));
        if OVERLAY_VISIBILITY_EPOCH.load(Ordering::SeqCst) == epoch {
            let _ = overlay_window.hide();
        }
    });
}

fn dispatch_pending_delivery_locked(
    app_handle: &AppHandle,
    lifecycle: &RecordingOverlayLifecycle,
    event: TranscriptDeliveredEvent,
) {
    if settings::get_settings(app_handle).overlay_position == OverlayPosition::None {
        return;
    }

    update_overlay_position(app_handle);
    let Some(overlay_window) = app_handle.get_webview_window("recording_overlay") else {
        lifecycle.set_pending_delivery(event);
        return;
    };

    // The pre-ready hide already scheduled by the delivery must not take this
    // replayed confirmation down. Give the replay its own full confirmation
    // lifetime and repeat the hide event that the frontend also missed.
    let epoch = OVERLAY_VISIBILITY_EPOCH.fetch_add(1, Ordering::SeqCst) + 1;
    PENDING_DELIVERY_CONFIRMATION.store(false, Ordering::SeqCst);
    let _ = overlay_window.show();

    #[cfg(target_os = "windows")]
    force_overlay_topmost(&overlay_window);

    let _ = overlay_window.emit("transcript-delivered-event", event);
    let _ = overlay_window.emit("hide-overlay", ());
    schedule_native_overlay_hide(
        overlay_window,
        OVERLAY_HIDE_DELAY_AFTER_CONFIRMATION_MS,
        epoch,
    );
}

fn dispatch_pending_state(app_handle: &AppHandle) {
    let lifecycle = app_handle.state::<RecordingOverlayLifecycle>();
    let _dispatch = lifecycle.dispatch_lock();
    dispatch_pending_state_locked(app_handle, &lifecycle);
}

fn ensure_recording_overlay(app_handle: &AppHandle) {
    if app_handle.get_webview_window("recording_overlay").is_some() {
        return;
    }

    let lifecycle = app_handle.state::<RecordingOverlayLifecycle>();
    if !lifecycle.claim_creation() {
        return;
    }

    let app_for_create = app_handle.clone();
    if let Err(error) = app_handle.run_on_main_thread(move || {
        let result = if app_for_create
            .get_webview_window("recording_overlay")
            .is_some()
        {
            Ok(())
        } else {
            build_recording_overlay(&app_for_create)
        };
        let lifecycle = app_for_create.state::<RecordingOverlayLifecycle>();
        lifecycle.creating.store(false, Ordering::SeqCst);
        if let Err(error) = result {
            log::error!("{error}");
            return;
        }
        dispatch_pending_state(&app_for_create);
    }) {
        lifecycle.creating.store(false, Ordering::SeqCst);
        log::error!("Failed to schedule recording overlay creation: {error}");
    }
}

/// The overlay calls this only after all first-state event listeners exist.
#[tauri::command]
#[specta::specta]
pub fn recording_overlay_ready(app: AppHandle) {
    let lifecycle = app.state::<RecordingOverlayLifecycle>();
    let _dispatch = lifecycle.dispatch_lock();
    lifecycle.ready.store(true, Ordering::SeqCst);
    dispatch_pending_state_locked(&app, &lifecycle);
    if let Some(event) = lifecycle.take_pending_delivery() {
        dispatch_pending_delivery_locked(&app, &lifecycle, event);
    }
}

fn show_overlay_state(app_handle: &AppHandle, state: &'static str, raw: bool) {
    let lifecycle = app_handle.state::<RecordingOverlayLifecycle>();
    let dispatch = lifecycle.dispatch_lock();

    // Check if overlay should be shown based on position setting
    let settings = settings::get_settings(app_handle);
    if settings.overlay_position == OverlayPosition::None {
        lifecycle.take_pending();
        return;
    }

    // Supersede any hide still asleep from a previous dictation (#279 review
    // round 2): showing the overlay again means whatever that sleeping hide
    // would have done is stale, whether it is mid-fade or mid-confirmation.
    OVERLAY_VISIBILITY_EPOCH.fetch_add(1, Ordering::SeqCst);

    // A pending confirmation flag left over from an older, superseded
    // delivery must not extend *this* dictation's hide to the long delay
    // meant for a confirmation this show has nothing to do with (#279 review
    // round 6): the older delivery's own confirmation was already dropped by
    // the frontend's active-recording guard, so left set the flag would just
    // be consumed by whichever unrelated hide runs next, keeping the
    // invisible native window alive (and click-intercepting on Windows) for
    // 2.2s instead of the normal 300ms. A show clears it; `announce_delivered`
    // re-arms it only for the confirmation that is actually about to follow.
    PENDING_DELIVERY_CONFIRMATION.store(false, Ordering::SeqCst);

    lifecycle.set_pending(PendingOverlayState { state, raw });
    drop(dispatch);
    ensure_recording_overlay(app_handle);
    dispatch_pending_state(app_handle);
}

/// Shows the recording overlay window with fade-in animation. `raw` reflects whether this
/// dictation will be emitted as raw transcript so the overlay can show a "RAW" badge.
pub fn show_recording_overlay(app_handle: &AppHandle, raw: bool) {
    show_overlay_state(app_handle, "recording", raw);
}

/// Shows the transcribing overlay window. `raw` carries the active output mode for the indicator.
pub fn show_transcribing_overlay(app_handle: &AppHandle, raw: bool) {
    show_overlay_state(app_handle, "transcribing", raw);
}

/// Shows the processing overlay window. `raw` carries the active output mode for the indicator.
pub fn show_processing_overlay(app_handle: &AppHandle, raw: bool) {
    show_overlay_state(app_handle, "processing", raw);
}

/// Updates the overlay window position based on current settings
pub fn update_overlay_position(app_handle: &AppHandle) {
    if let Some(overlay_window) = app_handle.get_webview_window("recording_overlay") {
        #[cfg(target_os = "linux")]
        {
            update_gtk_layer_shell_anchors(&overlay_window);
        }

        if let Some((x, y)) = calculate_overlay_position(app_handle) {
            let _ = overlay_window
                .set_position(tauri::Position::Logical(tauri::LogicalPosition { x, y }));
        }
    }
}

/// Hides the recording overlay window with fade-out animation
pub fn hide_recording_overlay(app_handle: &AppHandle) {
    let lifecycle = app_handle.state::<RecordingOverlayLifecycle>();
    let _dispatch = lifecycle.dispatch_lock();

    // A stop/cancel can arrive while the first overlay webview is still being
    // created. Clear the queued state before touching the window so its later
    // ready signal cannot resurrect a dictation that already ended.
    lifecycle.take_pending();

    // Every hide supersedes an in-flight delayed hide and any pending show,
    // even when the native window does not exist yet.
    let epoch = OVERLAY_VISIBILITY_EPOCH.fetch_add(1, Ordering::SeqCst) + 1;

    // Always hide the overlay regardless of settings - if setting was changed while recording,
    // we still want to hide it properly
    if let Some(overlay_window) = app_handle.get_webview_window("recording_overlay") {
        // Emit event to trigger fade-out animation
        let _ = overlay_window.emit("hide-overlay", ());

        // A delivery confirmation (#165) needs longer on screen than the
        // usual quick fade-out; the flag is reset here so only the hide it
        // was set for is delayed.
        let delay_ms = if PENDING_DELIVERY_CONFIRMATION.swap(false, Ordering::SeqCst) {
            OVERLAY_HIDE_DELAY_AFTER_CONFIRMATION_MS
        } else {
            OVERLAY_HIDE_DELAY_MS
        };

        // Hide the window after a short delay to allow animation to complete.
        // A newer show or hide instruction changes the epoch and makes this
        // sleeper a no-op.
        schedule_native_overlay_hide(overlay_window, delay_ms, epoch);
    }
}

pub fn emit_levels(app_handle: &AppHandle, levels: &Vec<f32>) {
    // emit levels to main app
    let _ = app_handle.emit("mic-level", levels);

    // also emit to the recording overlay if it's open
    if let Some(overlay_window) = app_handle.get_webview_window("recording_overlay") {
        let _ = overlay_window.emit("mic-level", levels);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    // A 1920x1080 monitor at the origin with the standard overlay rect. The
    // assertions reference the offset constants (not raw numbers) so they hold
    // regardless of the platform CI runs on.
    const MW: f64 = 1920.0;
    const MH: f64 = 1080.0;

    fn place(anchor: OverlayAnchor, dx: f64, dy: f64) -> (f64, f64) {
        resolve_overlay_anchor_position(
            LogicalRect {
                x: 0.0,
                y: 0.0,
                width: MW,
                height: MH,
            },
            (OVERLAY_WIDTH, OVERLAY_HEIGHT),
            anchor,
            (dx, dy),
        )
    }

    #[test]
    fn bottom_center_matches_the_legacy_centered_default() {
        // The same spot the pre-#9 centered Bottom placement produced.
        let (x, y) = place(OverlayAnchor::BottomCenter, 0.0, 0.0);
        assert_eq!(x, (MW - OVERLAY_WIDTH) / 2.0);
        assert_eq!(y, MH - OVERLAY_HEIGHT - OVERLAY_BOTTOM_OFFSET);
    }

    #[test]
    fn top_left_uses_side_and_top_offsets() {
        let (x, y) = place(OverlayAnchor::TopLeft, 0.0, 0.0);
        assert_eq!(x, OVERLAY_SIDE_OFFSET);
        assert_eq!(y, OVERLAY_TOP_OFFSET);
    }

    #[test]
    fn top_right_is_flush_to_the_right_margin() {
        let (x, _) = place(OverlayAnchor::TopRight, 0.0, 0.0);
        assert_eq!(x, MW - OVERLAY_WIDTH - OVERLAY_SIDE_OFFSET);
    }

    #[test]
    fn middle_center_is_centered_on_both_axes() {
        let (x, y) = place(OverlayAnchor::MiddleCenter, 0.0, 0.0);
        assert_eq!(x, (MW - OVERLAY_WIDTH) / 2.0);
        assert_eq!(y, (MH - OVERLAY_HEIGHT) / 2.0);
    }

    #[test]
    fn a_nudge_past_the_right_edge_is_clamped_on_screen() {
        let (x, _) = place(OverlayAnchor::TopRight, 9999.0, 0.0);
        assert_eq!(x, MW - OVERLAY_WIDTH); // flush right, fully visible
    }

    #[test]
    fn a_negative_nudge_is_clamped_to_the_top_left_corner() {
        let (x, y) = place(OverlayAnchor::TopLeft, -9999.0, -9999.0);
        assert_eq!(x, 0.0);
        assert_eq!(y, 0.0);
    }

    #[test]
    fn the_anchor_resolves_relative_to_a_secondary_monitor() {
        // A second monitor offset to the right at x=1920 (logical).
        let (x, y) = resolve_overlay_anchor_position(
            LogicalRect {
                x: 1920.0,
                y: 0.0,
                width: 1920.0,
                height: 1080.0,
            },
            (OVERLAY_WIDTH, OVERLAY_HEIGHT),
            OverlayAnchor::BottomCenter,
            (0.0, 0.0),
        );
        assert_eq!(x, 1920.0 + (1920.0 - OVERLAY_WIDTH) / 2.0);
        assert_eq!(y, 1080.0 - OVERLAY_HEIGHT - OVERLAY_BOTTOM_OFFSET);
    }

    #[test]
    fn concurrent_overlay_creation_has_one_winner() {
        let lifecycle = Arc::new(RecordingOverlayLifecycle::default());
        let winners = std::thread::scope(|scope| {
            let handles = (0..16)
                .map(|_| {
                    let lifecycle = Arc::clone(&lifecycle);
                    scope.spawn(move || lifecycle.claim_creation())
                })
                .collect::<Vec<_>>();
            handles
                .into_iter()
                .map(|handle| handle.join().expect("creation claimant did not panic"))
                .filter(|won| *won)
                .count()
        });

        assert_eq!(winners, 1);
    }

    #[test]
    fn pending_overlay_state_keeps_the_latest_transition() {
        let lifecycle = RecordingOverlayLifecycle::default();
        lifecycle.set_pending(PendingOverlayState {
            state: "recording",
            raw: false,
        });
        lifecycle.set_pending(PendingOverlayState {
            state: "processing",
            raw: true,
        });

        let pending = lifecycle.take_pending().expect("pending overlay state");
        assert_eq!(pending.state, "processing");
        assert!(pending.raw);
        assert!(lifecycle.take_pending().is_none());
    }

    #[test]
    fn pending_delivery_keeps_the_latest_confirmation() {
        use crate::output_target::backend::DeliverySource;

        let lifecycle = RecordingOverlayLifecycle::default();
        for title in ["first", "latest"] {
            lifecycle.set_pending_delivery(TranscriptDeliveredEvent {
                app: Some("Editor".to_string()),
                title: Some(title.to_string()),
                source: DeliverySource::Lock,
            });
        }

        let pending = lifecycle
            .take_pending_delivery()
            .expect("pending delivery confirmation");
        assert_eq!(pending.title.as_deref(), Some("latest"));
        assert!(lifecycle.take_pending_delivery().is_none());
    }
}
