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
use crate::output_target::{WindowHandle, WindowIdentity};

use super::{
    arm_pick, is_stale_selection, offer_rows, offered_candidates, resolve_gesture,
    visible_candidates, LastPick, OfferedWindow, PendingPick, PendingRoute, PickArmed,
    PickDelivery, PickerGesture, PickerSession, PickerWindow, RawWindow,
};

/// The picker window's Tauri label.
pub const PICKER_WINDOW: &str = "window_picker";

/// Emitted when a picked window was gone by the time the transcript fired, so
/// the paste was suppressed rather than sent to whatever inherited the handle
/// (#254). The frontend turns this into a brief notice.
pub const WINDOW_PICK_LOST_EVENT: &str = "window-pick-lost";

/// Emitted when a transcript finished while the picker was open, so it was not
/// pasted: the picker holds the foreground, and a foreground paste would land in
/// AudioBud's own window (#164).
pub const PICKER_OPEN_EVENT: &str = "window-pick-in-progress";

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
    // Placeholder only: the overlay sets the real, translated title once i18n
    // has loaded, since the translations live in the frontend bundle.
    .title("AudioBud")
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
///
/// DESTROY, not close: `close()` only asks, and this app's window-event handler
/// answers every close request by hiding the window instead of letting it go
/// (so the settings window survives its close button). A picker hidden that way
/// would stay registered and keep reading as open, holding off every later
/// paste (see [`pick_in_progress`]). The picker is transient -- one pick, one
/// window -- so it is destroyed outright and rebuilt next time, which also means
/// each pick starts from a freshly enumerated list.
pub fn close_picker(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(PICKER_WINDOW) {
        if let Err(e) = window.destroy() {
            warn!("Failed to close the window picker: {}", e);
        }
    }
}

/// The rows to offer, remembered pick first.
///
/// Enumerates the OS windows, drops the hidden, untitled and own ones, promotes
/// the last pick to row 0 so a repeat route is a single confirm, and records the
/// result as the session's offer so a gesture is only ever resolved against the
/// rows the user actually saw.
pub fn offered_rows(app: &AppHandle) -> Vec<PickerWindow> {
    let enumerated = enumerate_windows();
    let candidates = visible_candidates(enumerated.windows, &enumerated.excluded);
    // Each row keeps the identity enumeration read for it, so a pick can demand
    // that the handle still belongs to the window the user was shown (#254).
    let mut offered = super::offered_windows(candidates, &enumerated.identities);

    if let Some(last) = app.try_state::<LastPick>() {
        last.promote_offered(&mut offered);
    }

    let rows = offer_rows(&offered);
    if let Some(session) = app.try_state::<PickerSession>() {
        session.offer(offered);
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
    let offered: Vec<OfferedWindow> = app
        .try_state::<PickerSession>()
        .map(|session| session.offered())
        .unwrap_or_default();

    let outcome = resolve_gesture(gesture.to_gesture(), &offered_candidates(&offered));
    let (armed, route) = arm_pick(outcome, &offered, identify_window);

    if let Some(pending) = app.try_state::<PendingPick>() {
        match route {
            // A foreground send is armed too, so it outranks a target lock for
            // this one transcript (#120).
            Some(route) => pending.arm(route),
            // A dismissal, or a window that closed or was recycled under the
            // user's click, clears any earlier pick rather than leave one armed
            // that the user did not just confirm.
            None => pending.clear(),
        }
    }

    if let (Some(last), Some(PendingRoute::Window(window))) = (app.try_state::<LastPick>(), route) {
        last.remember(window);
    }

    match route {
        Some(PendingRoute::Window(window)) => {
            info!("Next transcript goes to window {:#x}", window.handle.0)
        }
        Some(PendingRoute::Foreground) => {
            info!("Next transcript follows the foreground, whatever is locked")
        }
        None => info!("Window pick cancelled; nothing was armed"),
    }

    // A row the user clicked that could not be honored -- its window closed, or
    // its handle was recycled, since it was offered. Closing now would look
    // exactly like a pick that worked, so the picker STAYS OPEN with its offer
    // still standing; the overlay says what happened and re-lists the windows,
    // which replaces that offer with a fresh one.
    if is_stale_selection(&gesture, armed) {
        warn!("The chosen window is gone; the picker stays open for another try");
        return armed;
    }

    if let Some(session) = app.try_state::<PickerSession>() {
        session.clear();
    }

    close_picker(app);
    armed
}

/// Forget the rows a picker window was offering, whatever ended it. Leaves any
/// armed route alone: a pick that WAS made arms its route and then destroys the
/// window on its way out.
pub fn forget_offer(app: &AppHandle) {
    if let Some(session) = app.try_state::<PickerSession>() {
        session.clear();
    }
}

/// End a pick that was abandoned rather than answered -- the picker window went
/// away without a gesture (Alt+F4, the window menu). Leaves nothing armed and no
/// session standing, so the next transcript follows the usual rules.
pub fn abandon_pick(app: &AppHandle) {
    let (Some(session), Some(pending)) = (
        app.try_state::<PickerSession>(),
        app.try_state::<PendingPick>(),
    ) else {
        return;
    };
    if session.is_open() {
        info!("The window picker was closed without a pick; nothing was armed");
    }
    super::abandon_pick(&session, &pending);
}

/// Whether a pick is in progress, so the paste path can hold off (#164).
///
/// The picker window is the authority while it is VISIBLE -- it holds the
/// foreground then, including when it offered no rows at all. A window that is
/// merely registered proves nothing: something else may have hidden it, and a
/// hidden picker that still read as open would suppress every later transcript
/// for the rest of the run. The decision itself is [`super::picker_is_open`],
/// which is where its reasoning is tested.
pub fn pick_in_progress(app: &AppHandle) -> bool {
    let window_visible = app
        .get_webview_window(PICKER_WINDOW)
        // A window whose visibility cannot be read is treated as up: it exists,
        // and refusing one paste is safer than typing into the picker.
        .map(|window| window.is_visible().unwrap_or(true));
    let session_open = app
        .try_state::<PickerSession>()
        .is_some_and(|session| session.is_open());
    super::picker_is_open(window_visible, session_open)
}

/// Tell the user a transcript was not pasted because the picker was open, and
/// say so in the log. The transcript still reaches the clipboard and history;
/// only the keystrokes are withheld.
pub fn announce_pick_in_progress(app: &AppHandle) {
    warn!("The window picker is open; the transcript was not pasted");
    let _ = app.emit(PICKER_OPEN_EVENT, ());
}

/// Tell the user a picked window went away mid-delivery.
///
/// The lock's own cleanup is deliberately NOT reused here: a one-shot pick holds
/// no lock, so clearing [`crate::output_target::PinnedTarget`] would take down a
/// lock the user set separately and is still relying on, and the "locked window
/// is gone" notice would be a lie.
pub fn announce_pick_lost(app: &AppHandle) {
    warn!("The picked window is gone; the transcript was not pasted");
    let _ = app.emit(WINDOW_PICK_LOST_EVENT, ());
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
        announce_pick_lost(app);
    }

    Some(delivery)
}

/// The full identity of `handle` right now, or `None` if no window has it any
/// more. This is the target lock's own probe, so the picker and the lock read
/// window identity the same way, class fingerprint included (#254).
fn identify_window(handle: WindowHandle) -> Option<WindowIdentity> {
    target_backend::probe_identity(handle)
}

/// One pass of the OS window list.
#[derive(Default)]
pub struct Enumerated {
    /// The windows worth offering.
    pub windows: Vec<RawWindow>,
    /// Their identities AS READ DURING THIS PASS, one per window. A pick is
    /// honored only if the handle still carries the same identity later (#254).
    pub identities: Vec<WindowIdentity>,
    /// Handles the shared eligibility rule rejected, for [`visible_candidates`]
    /// to exclude as well.
    pub excluded: Vec<WindowHandle>,
}

#[cfg(windows)]
pub use imp::enumerate_windows;

#[cfg(not(windows))]
pub use fallback::enumerate_windows;

#[cfg(windows)]
mod imp {
    use super::{identify_window, Enumerated, RawWindow, WindowHandle};
    use crate::output_target::{is_eligible_target, WindowFacts};
    // BOOL lives in windows::core in windows 0.61, not in Win32::Foundation
    // where the docs file it.
    use windows::core::{BOOL, PWSTR};
    use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, MAX_PATH, TRUE};
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetClassNameW, GetWindowTextLengthW, GetWindowTextW, IsWindowVisible,
    };

    /// Walk the top-level windows with `EnumWindows`, reading each one's title,
    /// visibility, owning application AND identity. Filtering is left to the
    /// core's [`super::visible_candidates`] so the rules stay in one tested
    /// place.
    pub fn enumerate_windows() -> Enumerated {
        let mut collected = Enumerated::default();
        let lparam = LPARAM(&mut collected as *mut Enumerated as isize);
        // A failed enumeration is not fatal: the picker shows its empty state.
        if let Err(e) = unsafe { EnumWindows(Some(collect), lparam) } {
            log::warn!("Could not enumerate windows for the picker: {}", e);
        }
        collected
    }

    unsafe extern "system" fn collect(hwnd: HWND, lparam: LPARAM) -> BOOL {
        // Safety: `lparam` is the `Enumerated` this call passed to EnumWindows,
        // which outlives the enumeration and is not shared with another thread.
        let collected = &mut *(lparam.0 as *mut Enumerated);
        let handle = WindowHandle(hwnd.0 as isize);

        // A window whose identity cannot be read has already closed.
        let Some(identity) = identify_window(handle) else {
            return TRUE;
        };

        // The same eligibility rule the target lock uses, so the picker offers
        // exactly what can be locked onto: no AudioBud windows (#164), no shell
        // surfaces, nothing hidden or untitled.
        let title = window_title(hwnd);
        let class_name = class_name_of(hwnd);
        let visible = unsafe { IsWindowVisible(hwnd) }.as_bool();
        let facts = WindowFacts {
            identity,
            class_name: &class_name,
            has_title: !title.trim().is_empty(),
            visible,
        };
        if !is_eligible_target(&facts, std::process::id()) {
            collected.excluded.push(handle);
            return TRUE;
        }

        collected.windows.push(RawWindow {
            handle,
            title,
            app: app_name(identity.process_id),
            visible,
        });
        collected.identities.push(identity);
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

    /// The window's class name, empty when it cannot be read. Only the shared
    /// eligibility rule reads it, to spot the shell's own surfaces.
    fn class_name_of(hwnd: HWND) -> String {
        // 256 matches the documented maximum length of a registered class name.
        let mut buffer = [0u16; 256];
        let written = unsafe { GetClassNameW(hwnd, &mut buffer) };
        if written <= 0 {
            return String::new();
        }
        String::from_utf16_lossy(&buffer[..written as usize])
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
    use super::Enumerated;

    /// No window enumeration on this platform yet (#119). An empty list makes
    /// the picker show its empty state rather than offer rows it cannot honor.
    pub fn enumerate_windows() -> Enumerated {
        Enumerated::default()
    }
}
