//! Platform half of the one-shot picker: enumerate windows, open the picker,
//! arm the pick the paste path consumes (#124, #259).
//!
//! The core in the parent module decides what a gesture means; this module
//! supplies the three things it cannot know without a window system:
//!   - [`enumerate_windows`] -- the top-level windows the OS is showing,
//!   - [`offered_windows`] -- the rows the overlay renders, remembered pick first,
//!   - [`resolve_pick`] -- the gesture the overlay sends back, turned into the
//!     pending pick the next paste delivers to.
//!
//! Windows is the only backend for now (#119), matching the target lock. On
//! other platforms the enumeration is empty, so the picker shows its empty state
//! instead of a blank surface and nothing is ever armed.
//!
//! Everything about window identity is borrowed from the target lock
//! ([`crate::output_target`]): [`accept_capture`] excludes AudioBud's own
//! windows (#164) and [`crate::output_target::backend::window_is_alive`]
//! re-validates the pick at paste time (#254). The picker adds no second copy of
//! either rule.

use log::{debug, info, warn};
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

use crate::output_target::backend as target_backend;
use crate::output_target::{accept_capture, WindowHandle, WindowIdentity};

use super::{
    arm_pick, offer_rows, resolve_gesture, visible_candidates, LastPick, PendingPick, PickArmed,
    PickDelivery, PickerGesture, PickerSession, PickerWindow, RawWindow,
};

/// The picker window's Tauri label.
pub const PICKER_WINDOW: &str = "window_picker";

/// Emitted when a picked window was gone by the time the transcript fired, so
/// the paste was suppressed rather than sent to whatever inherited the handle
/// (#254). The frontend turns this into a brief notice.
pub const WINDOW_PICK_LOST_EVENT: &str = "window-pick-lost";

const PICKER_WIDTH: f64 = 420.0;
const PICKER_HEIGHT: f64 = 360.0;

/// Open the picker for one dictation, from the shortcut or the tray item.
///
/// The window is created on demand and destroyed when the pick ends, so no
/// hidden surface sits around holding a stale window list. Opening it while it
/// is already open just focuses it, since a second press means "I am picking",
/// not "start over".
pub fn open_picker(app: &AppHandle) {
    if let Some(existing) = app.get_webview_window(PICKER_WINDOW) {
        let _ = existing.show();
        let _ = existing.set_focus();
        return;
    }

    let mut builder = WebviewWindowBuilder::new(
        app,
        PICKER_WINDOW,
        WebviewUrl::App("src/window-picker/index.html".into()),
    )
    .title("Send to window")
    .inner_size(PICKER_WIDTH, PICKER_HEIGHT)
    .resizable(false)
    .maximizable(false)
    .minimizable(false)
    .decorations(false)
    .transparent(true)
    .shadow(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .center()
    // Unlike the recording overlay, the picker is driven by the keyboard, so it
    // takes focus. The window it was opened over is captured per row, not from
    // the foreground, so borrowing focus here costs nothing.
    .focused(true);

    if let Some(data_dir) = crate::portable::data_dir() {
        builder = builder.data_directory(data_dir.join("webview"));
    }

    match builder.build() {
        Ok(_) => debug!("Window picker opened"),
        Err(e) => warn!("Failed to open the window picker: {}", e),
    }
}

/// Close the picker, if it is open. Called when a pick ends, whichever way.
pub fn close_picker(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(PICKER_WINDOW) {
        let _ = window.close();
    }
}

/// The rows to offer, remembered pick first.
///
/// Enumerates the OS windows, drops the hidden, untitled and own ones, promotes
/// the last pick to row 0 so a repeat route is a single confirm, and records the
/// result as the session's offer so a gesture is only ever resolved against the
/// rows the user actually saw.
pub fn offered_windows(app: &AppHandle) -> Vec<PickerWindow> {
    let (raw, own) = enumerate_windows();
    let mut candidates = visible_candidates(raw, &own);

    if let Some(last) = app.try_state::<LastPick>() {
        last.promote_to_front(&mut candidates);
    }

    let rows = offer_rows(&candidates);
    if let Some(session) = app.try_state::<PickerSession>() {
        session.offer(candidates);
    }
    rows
}

/// Resolve the overlay's terminal gesture and arm what it asked for.
///
/// The gesture is validated against the offered rows ([`resolve_gesture`]), the
/// chosen window's identity is captured so the paste path can re-check it
/// (#254), and the picker closes either way. Nothing here creates a lasting
/// lock: the armed pick routes one transcript and is then forgotten.
pub fn resolve_pick(app: &AppHandle, gesture: PickerGesture) -> PickArmed {
    let offered = app
        .try_state::<PickerSession>()
        .map(|session| session.offered())
        .unwrap_or_default();

    let outcome = resolve_gesture(gesture.to_gesture(), &offered);
    let (armed, window) = arm_pick(outcome, identify_window);

    if let Some(pending) = app.try_state::<PendingPick>() {
        match window {
            Some(identity) => pending.arm(identity),
            // A dismissal, a foreground send, or a window that closed under the
            // user's click all clear any earlier pick rather than leave one
            // armed that the user did not just confirm.
            None => pending.clear(),
        }
    }

    if let (Some(last), Some(identity)) = (app.try_state::<LastPick>(), window) {
        last.remember(identity.handle);
    }

    if let Some(session) = app.try_state::<PickerSession>() {
        session.clear();
    }

    match armed {
        PickArmed::Window => info!(
            "Next transcript goes to window {:#x}",
            window.map(|w| w.handle.0).unwrap_or_default()
        ),
        PickArmed::Foreground => info!("Next transcript follows the foreground"),
        PickArmed::Cancelled => info!("Window pick cancelled; nothing was armed"),
    }

    close_picker(app);
    armed
}

/// Consume a pending one-shot pick for the paste about to fire.
///
/// `None` means no pick is waiting and the usual target rules apply. The picked
/// window is re-validated through the shared identity check first, so a window
/// that closed (or a handle Windows recycled) suppresses the paste exactly as a
/// lost lock does (#254), emitting [`WINDOW_PICK_LOST_EVENT`] once.
pub fn take_pick_target(app: &AppHandle) -> Option<PickDelivery> {
    let pending = app.try_state::<PendingPick>()?;
    let delivery = pending.take_resolved(target_backend::window_is_alive)?;

    if delivery == PickDelivery::PickLost {
        warn!("The picked window is gone; the transcript was not pasted");
        let _ = app.emit(WINDOW_PICK_LOST_EVENT, ());
    }

    Some(delivery)
}

/// The full identity of `handle` right now, or `None` if no window has it any
/// more. Built on the target lock's probe so the picker and the lock read window
/// identity the same way.
fn identify_window(handle: WindowHandle) -> Option<WindowIdentity> {
    let (process_id, thread_id) = target_backend::probe_identity(handle)?;
    Some(WindowIdentity {
        handle,
        process_id,
        thread_id,
    })
}

/// Whether `identity` belongs to AudioBud, reusing the target lock's rule (#164)
/// so the picker can never offer a row that pastes back into the picker.
fn is_own(identity: WindowIdentity) -> bool {
    accept_capture(identity, std::process::id()).is_err()
}

/// Every top-level window the OS is showing, plus the handles of AudioBud's own
/// windows for [`visible_candidates`] to exclude.
#[cfg(windows)]
pub use imp::enumerate_windows;

#[cfg(not(windows))]
pub use fallback::enumerate_windows;

#[cfg(windows)]
mod imp {
    use super::{identify_window, is_own, RawWindow, WindowHandle};
    // BOOL lives in windows::core in windows 0.61, not in Win32::Foundation
    // where the docs file it.
    use windows::core::{BOOL, PWSTR};
    use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, MAX_PATH, TRUE};
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindowTextLengthW, GetWindowTextW, IsWindowVisible,
    };

    /// What the enumeration callback collects: the offerable windows and the
    /// handles of AudioBud's own, which the core filter excludes (#164).
    #[derive(Default)]
    struct Collected {
        windows: Vec<RawWindow>,
        own: Vec<WindowHandle>,
    }

    /// Walk the top-level windows with `EnumWindows`, reading each one's title,
    /// visibility and owning application. Filtering is left to the core's
    /// [`super::visible_candidates`] so the rules stay in one tested place.
    pub fn enumerate_windows() -> (Vec<RawWindow>, Vec<WindowHandle>) {
        let mut collected = Collected::default();
        let lparam = LPARAM(&mut collected as *mut Collected as isize);
        // A failed enumeration is not fatal: the picker shows its empty state.
        if let Err(e) = unsafe { EnumWindows(Some(collect), lparam) } {
            log::warn!("Could not enumerate windows for the picker: {}", e);
        }
        (collected.windows, collected.own)
    }

    unsafe extern "system" fn collect(hwnd: HWND, lparam: LPARAM) -> BOOL {
        // Safety: `lparam` is the `Collected` this call passed to EnumWindows,
        // which outlives the enumeration and is not shared with another thread.
        let collected = &mut *(lparam.0 as *mut Collected);
        let handle = WindowHandle(hwnd.0 as isize);

        // A window whose identity cannot be read has already closed.
        let Some(identity) = identify_window(handle) else {
            return TRUE;
        };
        if is_own(identity) {
            collected.own.push(handle);
            return TRUE;
        }

        collected.windows.push(RawWindow {
            handle,
            title: window_title(hwnd),
            app: app_name(identity.process_id),
            visible: unsafe { IsWindowVisible(hwnd) }.as_bool(),
        });
        TRUE
    }

    /// The window's title, or an empty string when it has none (the core drops
    /// untitled windows, which have no readable label).
    fn window_title(hwnd: HWND) -> String {
        let length = unsafe { GetWindowTextLengthW(hwnd) };
        if length <= 0 {
            return String::new();
        }
        // One extra cell for the terminating NUL GetWindowTextW writes.
        let mut buffer = vec![0u16; length as usize + 1];
        let copied = unsafe { GetWindowTextW(hwnd, &mut buffer) };
        if copied <= 0 {
            return String::new();
        }
        String::from_utf16_lossy(&buffer[..copied as usize])
    }

    /// The owning application's name -- the executable's file stem, so rows read
    /// "Notepad", not a full path. `None` when the process cannot be opened,
    /// which is normal for elevated or protected processes; the core then falls
    /// back to the title alone.
    fn app_name(process_id: u32) -> Option<String> {
        let process =
            unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process_id) }.ok()?;
        let mut buffer = [0u16; MAX_PATH as usize];
        let mut length = buffer.len() as u32;
        let queried = unsafe {
            QueryFullProcessImageNameW(
                process,
                PROCESS_NAME_WIN32,
                PWSTR(buffer.as_mut_ptr()),
                &mut length,
            )
        };
        unsafe {
            let _ = CloseHandle(process);
        }
        queried.ok()?;

        let path = String::from_utf16_lossy(&buffer[..length as usize]);
        std::path::Path::new(&path)
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
    }
}

#[cfg(not(windows))]
mod fallback {
    use super::{RawWindow, WindowHandle};

    /// No window enumeration on this platform yet (#119). An empty list makes
    /// the picker show its empty state rather than offer rows it cannot honor.
    pub fn enumerate_windows() -> (Vec<RawWindow>, Vec<WindowHandle>) {
        (Vec::new(), Vec::new())
    }
}
