#[cfg(windows)]
use crate::clipboard_snapshot::ClipboardBackend;
use crate::clipboard_snapshot::{self, ArboardBackend, ClipboardContent, ClipboardHistory};
use crate::input::{self, EnigoState};
use crate::output_target::backend::{
    self as target_backend, Borrowed, Delivery, FocusHold, FocusLost,
};
#[cfg(target_os = "linux")]
use crate::settings::TypingTool;
use crate::settings::{get_settings, AppSettings, AutoSubmitKey, ClipboardHandling, PasteMethod};
use enigo::{Direction, Enigo, Key, Keyboard};
use log::{info, warn};
use std::process::Command;
use std::time::Duration;
use tauri::{AppHandle, Manager};
use tauri_plugin_clipboard_manager::ClipboardExt;

#[cfg(target_os = "linux")]
use crate::utils::{is_kde_wayland, is_wayland};

/// What was on the clipboard before the transcript overwrote it.
enum SavedClipboard {
    /// Full snapshot (text, HTML, image, file list) via arboard (issue #57).
    Full(ClipboardContent),
    /// Text-only fallback when the arboard backend could not be opened.
    TextOnly(String),
}

/// Why one delivery stopped early.
#[derive(Debug, PartialEq, Eq)]
enum DeliveryError {
    /// The target window went away, so no further keystrokes were sent and the
    /// user has already been told the lock is gone. Nothing is wrong with the
    /// transcript itself, so whatever does not depend on a window -- the
    /// clipboard copy -- still applies (#120).
    Suppressed(String),
    /// The paste machinery failed and the user should be told.
    Failed(String),
}

impl From<String> for DeliveryError {
    fn from(error: String) -> Self {
        DeliveryError::Failed(error)
    }
}

impl From<&str> for DeliveryError {
    fn from(error: &str) -> Self {
        DeliveryError::Failed(error.to_string())
    }
}

impl From<FocusLost> for DeliveryError {
    /// A window that has gone is a settled outcome the user was already told
    /// about, so the delivery is merely suppressed. A window that is still there
    /// but will not come forward is a failure: saying nothing would drop the
    /// transcript in silence, and with `ClipboardHandling::DontModify` there is
    /// no copy left behind to recover it from (#120).
    fn from(lost: FocusLost) -> Self {
        match lost {
            FocusLost::TargetGone => DeliveryError::Suppressed(lost.to_string()),
            FocusLost::ActivationRefused(reason) => DeliveryError::Failed(reason),
        }
    }
}

/// Pastes text using the clipboard: saves current content, writes text, sends paste keystroke, restores clipboard.
fn paste_via_clipboard(
    enigo: &mut Enigo,
    text: &str,
    app_handle: &AppHandle,
    paste_method: &PasteMethod,
    paste_delay_ms: u64,
    hold: &FocusHold,
) -> Result<(), DeliveryError> {
    let clipboard = app_handle.clipboard();

    // Save the full clipboard before overwriting it with the transcript.
    // Saving only the text would destroy images, HTML, and file lists the
    // user had copied (issue #57).
    let mut snapshot_backend = match ArboardBackend::new() {
        Ok(backend) => Some(backend),
        Err(e) => {
            warn!("Falling back to text-only clipboard save/restore: {}", e);
            None
        }
    };
    let saved_clipboard = match snapshot_backend.as_mut() {
        Some(backend) => SavedClipboard::Full(clipboard_snapshot::capture(backend)),
        None => SavedClipboard::TextOnly(clipboard.read_text().unwrap_or_default()),
    };

    // Write text to clipboard first
    // On Wayland, prefer wl-copy for better compatibility (especially with umlauts)
    #[cfg(target_os = "linux")]
    let write_result = if is_wayland() && is_wl_copy_available() {
        info!("Using wl-copy for clipboard write on Wayland");
        write_clipboard_via_wl_copy(text)
    } else {
        clipboard
            .write_text(text)
            .map_err(|e| format!("Failed to write to clipboard: {}", e))
    };

    #[cfg(windows)]
    let write_result = match snapshot_backend.as_mut() {
        Some(backend) => backend.write_text(text, ClipboardHistory::Exclude),
        None => clipboard
            .write_text(text)
            .map_err(|e| format!("Failed to write to clipboard: {}", e)),
    };

    #[cfg(target_os = "macos")]
    let write_result = clipboard
        .write_text(text)
        .map_err(|e| format!("Failed to write to clipboard: {}", e));

    write_result?;

    std::thread::sleep(Duration::from_millis(paste_delay_ms));

    // The clipboard write and the delay above give focus time to move, so the
    // target is re-checked here, immediately before the keystroke (#120).
    let pasted = match hold.ensure() {
        Ok(()) => send_paste_key_combo(enigo, paste_method).map_err(DeliveryError::Failed),
        Err(lost) => Err(lost.into()),
    };

    std::thread::sleep(std::time::Duration::from_millis(50));

    // Restore original clipboard content. This runs even when the keystroke was
    // abandoned, so an aborted delivery does not leave the transcript sitting on
    // the user's clipboard in place of what they had copied.
    restore_saved_clipboard(&saved_clipboard, snapshot_backend.as_mut(), app_handle);

    pasted
}

/// Sends the paste key combination, preferring a Linux-native tool when one is
/// available and falling back to enigo.
fn send_paste_key_combo(enigo: &mut Enigo, paste_method: &PasteMethod) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    let key_combo_sent = try_send_key_combo_linux(paste_method)?;

    #[cfg(not(target_os = "linux"))]
    let key_combo_sent = false;

    if !key_combo_sent {
        match paste_method {
            PasteMethod::CtrlV => input::send_paste_ctrl_v(enigo)?,
            PasteMethod::CtrlShiftV => input::send_paste_ctrl_shift_v(enigo)?,
            PasteMethod::ShiftInsert => input::send_paste_shift_insert(enigo)?,
            _ => return Err("Invalid paste method for clipboard paste".into()),
        }
    }

    Ok(())
}

/// Puts the saved clipboard contents back after the paste keystroke.
/// Restore failures are logged, not propagated: the paste itself succeeded.
fn restore_saved_clipboard(
    saved: &SavedClipboard,
    backend: Option<&mut ArboardBackend>,
    app_handle: &AppHandle,
) {
    match saved {
        SavedClipboard::Full(content) => {
            // On Wayland the transcript was written through wl-copy (same
            // condition as the write path), so the restore must displace
            // that write: text-only content goes back through wl-copy, and
            // everything else clears the wl-copy selection first — the
            // arboard restore below talks to the X11/XWayland selection and
            // cannot displace what wl-copy wrote, which would leave the
            // transcript pasteable.
            #[cfg(target_os = "linux")]
            if is_wayland() && is_wl_copy_available() {
                if content.is_text_only() {
                    if let Some(text) = content.text.as_deref() {
                        let _ = write_clipboard_via_wl_copy(text);
                    }
                    return;
                }
                let _ = clear_clipboard_via_wl_copy();
            }
            if let Some(backend) = backend {
                if let Err(e) =
                    clipboard_snapshot::restore(backend, content, ClipboardHistory::Exclude)
                {
                    warn!("Failed to restore clipboard contents: {}", e);
                }
            }
        }
        SavedClipboard::TextOnly(text) => {
            #[cfg(target_os = "linux")]
            if is_wayland() && is_wl_copy_available() {
                let _ = write_clipboard_via_wl_copy(text);
                return;
            }
            #[cfg(windows)]
            match ArboardBackend::new()
                .and_then(|mut backend| backend.write_text(text, ClipboardHistory::Exclude))
            {
                Ok(()) => return,
                Err(e) => warn!("Failed to restore clipboard text without history: {}", e),
            }
            let _ = app_handle.clipboard().write_text(text);
        }
    }
}

/// Attempts to send a key combination using Linux-native tools.
/// Returns `Ok(true)` if a native tool handled it, `Ok(false)` to fall back to enigo.
#[cfg(target_os = "linux")]
fn try_send_key_combo_linux(paste_method: &PasteMethod) -> Result<bool, String> {
    if is_wayland() {
        // Wayland: prefer wtype (but not on KDE), then dotool, then ydotool
        // Note: wtype doesn't work on KDE (no zwp_virtual_keyboard_manager_v1 support)
        if !is_kde_wayland() && is_wtype_available() {
            info!("Using wtype for key combo");
            send_key_combo_via_wtype(paste_method)?;
            return Ok(true);
        }
        if is_dotool_available() {
            info!("Using dotool for key combo");
            send_key_combo_via_dotool(paste_method)?;
            return Ok(true);
        }
        if is_ydotool_available() {
            info!("Using ydotool for key combo");
            send_key_combo_via_ydotool(paste_method)?;
            return Ok(true);
        }
    } else {
        // X11: prefer xdotool, then ydotool
        if is_xdotool_available() {
            info!("Using xdotool for key combo");
            send_key_combo_via_xdotool(paste_method)?;
            return Ok(true);
        }
        if is_ydotool_available() {
            info!("Using ydotool for key combo");
            send_key_combo_via_ydotool(paste_method)?;
            return Ok(true);
        }
    }

    Ok(false)
}

/// Attempts to type text directly using Linux-native tools.
/// Returns `Ok(true)` if a native tool handled it, `Ok(false)` to fall back to enigo.
#[cfg(target_os = "linux")]
fn try_direct_typing_linux(text: &str, preferred_tool: TypingTool) -> Result<bool, String> {
    // If user specified a tool, try only that one
    if preferred_tool != TypingTool::Auto {
        return match preferred_tool {
            TypingTool::Wtype if is_wtype_available() => {
                info!("Using user-specified wtype");
                type_text_via_wtype(text)?;
                Ok(true)
            }
            TypingTool::Kwtype if is_kwtype_available() => {
                info!("Using user-specified kwtype");
                type_text_via_kwtype(text)?;
                Ok(true)
            }
            TypingTool::Dotool if is_dotool_available() => {
                info!("Using user-specified dotool");
                type_text_via_dotool(text)?;
                Ok(true)
            }
            TypingTool::Ydotool if is_ydotool_available() => {
                info!("Using user-specified ydotool");
                type_text_via_ydotool(text)?;
                Ok(true)
            }
            TypingTool::Xdotool if is_xdotool_available() => {
                info!("Using user-specified xdotool");
                type_text_via_xdotool(text)?;
                Ok(true)
            }
            _ => Err(format!(
                "Typing tool {:?} is not available on this system",
                preferred_tool
            )),
        };
    }

    // Auto mode - existing fallback chain
    if is_wayland() {
        // KDE Wayland: prefer kwtype (uses KDE Fake Input protocol, supports umlauts)
        if is_kde_wayland() && is_kwtype_available() {
            info!("Using kwtype for direct text input on KDE Wayland");
            type_text_via_kwtype(text)?;
            return Ok(true);
        }
        // Wayland: prefer wtype, then dotool, then ydotool
        // Note: wtype doesn't work on KDE (no zwp_virtual_keyboard_manager_v1 support)
        if !is_kde_wayland() && is_wtype_available() {
            info!("Using wtype for direct text input");
            type_text_via_wtype(text)?;
            return Ok(true);
        }
        if is_dotool_available() {
            info!("Using dotool for direct text input");
            type_text_via_dotool(text)?;
            return Ok(true);
        }
        if is_ydotool_available() {
            info!("Using ydotool for direct text input");
            type_text_via_ydotool(text)?;
            return Ok(true);
        }
    } else {
        // X11: prefer xdotool, then ydotool
        if is_xdotool_available() {
            info!("Using xdotool for direct text input");
            type_text_via_xdotool(text)?;
            return Ok(true);
        }
        if is_ydotool_available() {
            info!("Using ydotool for direct text input");
            type_text_via_ydotool(text)?;
            return Ok(true);
        }
    }

    Ok(false)
}

/// Returns the list of available typing tools on this system.
/// Always includes "auto" as the first entry.
#[cfg(target_os = "linux")]
pub fn get_available_typing_tools() -> Vec<String> {
    let mut tools = vec!["auto".to_string()];
    if is_wtype_available() {
        tools.push("wtype".to_string());
    }
    if is_kwtype_available() {
        tools.push("kwtype".to_string());
    }
    if is_dotool_available() {
        tools.push("dotool".to_string());
    }
    if is_ydotool_available() {
        tools.push("ydotool".to_string());
    }
    if is_xdotool_available() {
        tools.push("xdotool".to_string());
    }
    tools
}

/// Check if wtype is available (Wayland text input tool)
#[cfg(target_os = "linux")]
fn is_wtype_available() -> bool {
    Command::new("which")
        .arg("wtype")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

/// Check if dotool is available (another Wayland text input tool)
#[cfg(target_os = "linux")]
fn is_dotool_available() -> bool {
    Command::new("which")
        .arg("dotool")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

/// Check if ydotool is available (uinput-based, works on both Wayland and X11)
#[cfg(target_os = "linux")]
fn is_ydotool_available() -> bool {
    Command::new("which")
        .arg("ydotool")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

#[cfg(target_os = "linux")]
fn is_xdotool_available() -> bool {
    Command::new("which")
        .arg("xdotool")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

/// Check if kwtype is available (KDE Wayland virtual keyboard input tool)
#[cfg(target_os = "linux")]
fn is_kwtype_available() -> bool {
    Command::new("which")
        .arg("kwtype")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

/// Check if wl-copy is available (Wayland clipboard tool)
#[cfg(target_os = "linux")]
fn is_wl_copy_available() -> bool {
    Command::new("which")
        .arg("wl-copy")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

/// Type text directly via wtype on Wayland.
#[cfg(target_os = "linux")]
fn type_text_via_wtype(text: &str) -> Result<(), String> {
    let output = Command::new("wtype")
        .arg("--") // Protect against text starting with -
        .arg(text)
        .output()
        .map_err(|e| format!("Failed to execute wtype: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("wtype failed: {}", stderr));
    }

    Ok(())
}

/// Type text directly via xdotool on X11.
#[cfg(target_os = "linux")]
fn type_text_via_xdotool(text: &str) -> Result<(), String> {
    let output = Command::new("xdotool")
        .arg("type")
        .arg("--clearmodifiers")
        .arg("--")
        .arg(text)
        .output()
        .map_err(|e| format!("Failed to execute xdotool: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("xdotool failed: {}", stderr));
    }

    Ok(())
}

/// Type text directly via dotool (works on both Wayland and X11 via uinput).
#[cfg(target_os = "linux")]
fn type_text_via_dotool(text: &str) -> Result<(), String> {
    use std::io::Write;
    use std::process::Stdio;

    let mut child = Command::new("dotool")
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to spawn dotool: {}", e))?;

    if let Some(mut stdin) = child.stdin.take() {
        // dotool uses "type <text>" command
        writeln!(stdin, "type {}", text)
            .map_err(|e| format!("Failed to write to dotool stdin: {}", e))?;
    }

    let output = child
        .wait_with_output()
        .map_err(|e| format!("Failed to wait for dotool: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("dotool failed: {}", stderr));
    }

    Ok(())
}

/// Type text directly via ydotool (uinput-based, requires ydotoold daemon).
#[cfg(target_os = "linux")]
fn type_text_via_ydotool(text: &str) -> Result<(), String> {
    let output = Command::new("ydotool")
        .arg("type")
        .arg("--")
        .arg(text)
        .output()
        .map_err(|e| format!("Failed to execute ydotool: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("ydotool failed: {}", stderr));
    }

    Ok(())
}

/// Type text directly via kwtype (KDE Wayland virtual keyboard, uses KDE Fake Input protocol).
#[cfg(target_os = "linux")]
fn type_text_via_kwtype(text: &str) -> Result<(), String> {
    let output = Command::new("kwtype")
        .arg("--")
        .arg(text)
        .output()
        .map_err(|e| format!("Failed to execute kwtype: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("kwtype failed: {}", stderr));
    }

    Ok(())
}

/// Clears the Wayland clipboard selection wl-copy wrote the transcript to.
#[cfg(target_os = "linux")]
fn clear_clipboard_via_wl_copy() -> Result<(), String> {
    use std::process::Stdio;
    let status = Command::new("wl-copy")
        .arg("--clear")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|e| format!("Failed to execute wl-copy --clear: {}", e))?;

    if !status.success() {
        return Err("wl-copy --clear failed".into());
    }
    Ok(())
}

/// Write text to clipboard via wl-copy (Wayland clipboard tool).
/// Uses Stdio::null() to avoid blocking on repeated calls — wl-copy forks a
/// daemon that inherits piped fds, causing read_to_end to hang indefinitely.
#[cfg(target_os = "linux")]
fn write_clipboard_via_wl_copy(text: &str) -> Result<(), String> {
    use std::process::Stdio;
    let status = Command::new("wl-copy")
        .arg("--")
        .arg(text)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|e| format!("Failed to execute wl-copy: {}", e))?;

    if !status.success() {
        return Err("wl-copy failed".into());
    }

    Ok(())
}

/// Send a key combination (e.g., Ctrl+V) via wtype on Wayland.
#[cfg(target_os = "linux")]
fn send_key_combo_via_wtype(paste_method: &PasteMethod) -> Result<(), String> {
    let args: Vec<&str> = match paste_method {
        PasteMethod::CtrlV => vec!["-M", "ctrl", "-k", "v"],
        PasteMethod::ShiftInsert => vec!["-M", "shift", "-k", "Insert"],
        PasteMethod::CtrlShiftV => vec!["-M", "ctrl", "-M", "shift", "-k", "v"],
        _ => return Err("Unsupported paste method".into()),
    };

    let output = Command::new("wtype")
        .args(&args)
        .output()
        .map_err(|e| format!("Failed to execute wtype: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("wtype failed: {}", stderr));
    }

    Ok(())
}

/// Send a key combination (e.g., Ctrl+V) via dotool.
#[cfg(target_os = "linux")]
fn send_key_combo_via_dotool(paste_method: &PasteMethod) -> Result<(), String> {
    let command;
    match paste_method {
        PasteMethod::CtrlV => command = "echo key ctrl+v | dotool",
        PasteMethod::ShiftInsert => command = "echo key shift+insert | dotool",
        PasteMethod::CtrlShiftV => command = "echo key ctrl+shift+v | dotool",
        _ => return Err("Unsupported paste method".into()),
    }
    use std::process::Stdio;
    let status = Command::new("sh")
        .arg("-c")
        .arg(command)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|e| format!("Failed to execute dotool: {}", e))?;
    if !status.success() {
        return Err("dotool failed".into());
    }

    Ok(())
}

/// Send a key combination (e.g., Ctrl+V) via ydotool (requires ydotoold daemon).
#[cfg(target_os = "linux")]
fn send_key_combo_via_ydotool(paste_method: &PasteMethod) -> Result<(), String> {
    // ydotool uses Linux input event keycodes with format <keycode>:<pressed>
    // where pressed is 1 for down, 0 for up. Keycodes: ctrl=29, shift=42, v=47, insert=110
    let args: Vec<&str> = match paste_method {
        PasteMethod::CtrlV => vec!["key", "29:1", "47:1", "47:0", "29:0"],
        PasteMethod::ShiftInsert => vec!["key", "42:1", "110:1", "110:0", "42:0"],
        PasteMethod::CtrlShiftV => vec!["key", "29:1", "42:1", "47:1", "47:0", "42:0", "29:0"],
        _ => return Err("Unsupported paste method".into()),
    };

    let output = Command::new("ydotool")
        .args(&args)
        .output()
        .map_err(|e| format!("Failed to execute ydotool: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("ydotool failed: {}", stderr));
    }

    Ok(())
}

/// Send a key combination (e.g., Ctrl+V) via xdotool on X11.
#[cfg(target_os = "linux")]
fn send_key_combo_via_xdotool(paste_method: &PasteMethod) -> Result<(), String> {
    let key_combo = match paste_method {
        PasteMethod::CtrlV => "ctrl+v",
        PasteMethod::CtrlShiftV => "ctrl+shift+v",
        PasteMethod::ShiftInsert => "shift+Insert",
        _ => return Err("Unsupported paste method".into()),
    };

    let output = Command::new("xdotool")
        .arg("key")
        .arg("--clearmodifiers")
        .arg(key_combo)
        .output()
        .map_err(|e| format!("Failed to execute xdotool: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("xdotool failed: {}", stderr));
    }

    Ok(())
}

/// Pastes text by invoking an external script.
/// The script receives the text to paste as a single argument.
fn paste_via_external_script(text: &str, script_path: &str) -> Result<(), String> {
    info!("Pasting via external script: {}", script_path);

    let output = Command::new(script_path)
        .arg(text)
        .output()
        .map_err(|e| format!("Failed to execute external script '{}': {}", script_path, e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        return Err(format!(
            "External script '{}' failed with exit code {:?}. stderr: {}, stdout: {}",
            script_path,
            output.status.code(),
            stderr.trim(),
            stdout.trim()
        ));
    }

    Ok(())
}

/// Types text directly by simulating individual key presses.
fn paste_direct(
    enigo: &mut Enigo,
    text: &str,
    #[cfg(target_os = "linux")] typing_tool: TypingTool,
) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        if try_direct_typing_linux(text, typing_tool)? {
            return Ok(());
        }
        info!("Falling back to enigo for direct text input");
    }

    input::paste_text_direct(enigo, text)
}

fn send_return_key(enigo: &mut Enigo, key_type: AutoSubmitKey) -> Result<(), String> {
    match key_type {
        AutoSubmitKey::Enter => {
            enigo
                .key(Key::Return, Direction::Press)
                .map_err(|e| format!("Failed to press Return key: {}", e))?;
            enigo
                .key(Key::Return, Direction::Release)
                .map_err(|e| format!("Failed to release Return key: {}", e))?;
        }
        AutoSubmitKey::CtrlEnter => {
            enigo
                .key(Key::Control, Direction::Press)
                .map_err(|e| format!("Failed to press Control key: {}", e))?;
            enigo
                .key(Key::Return, Direction::Press)
                .map_err(|e| format!("Failed to press Return key: {}", e))?;
            enigo
                .key(Key::Return, Direction::Release)
                .map_err(|e| format!("Failed to release Return key: {}", e))?;
            enigo
                .key(Key::Control, Direction::Release)
                .map_err(|e| format!("Failed to release Control key: {}", e))?;
        }
        AutoSubmitKey::CmdEnter => {
            enigo
                .key(Key::Meta, Direction::Press)
                .map_err(|e| format!("Failed to press Meta/Cmd key: {}", e))?;
            enigo
                .key(Key::Return, Direction::Press)
                .map_err(|e| format!("Failed to press Return key: {}", e))?;
            enigo
                .key(Key::Return, Direction::Release)
                .map_err(|e| format!("Failed to release Return key: {}", e))?;
            enigo
                .key(Key::Meta, Direction::Release)
                .map_err(|e| format!("Failed to release Meta/Cmd key: {}", e))?;
        }
    }

    Ok(())
}

fn should_send_auto_submit(auto_submit: bool, paste_method: PasteMethod) -> bool {
    auto_submit && paste_method != PasteMethod::None
}

/// Whether the transcript is also copied to the clipboard.
///
/// `delivered` is deliberately ignored: this setting is about the clipboard, so
/// it holds whether or not the text reached a window. A suppressed delivery
/// (#120) that also skipped the copy would silently discard the transcript.
fn should_copy_to_clipboard(handling: ClipboardHandling, delivered: bool) -> bool {
    let _ = delivered;
    handling == ClipboardHandling::CopyToClipboard
}

/// Whether this delivery sends any input to a window, and so needs one to hold
/// focus.
///
/// `PasteMethod::None` types nothing and suppresses auto-submit with it, so a
/// pinned delivery would otherwise steal the user's focus, wait, and hand it
/// back having done nothing. The auto-submit half is spelled out anyway so this
/// stays correct if that rule ever changes.
///
/// This is the narrow local answer for the paste path. Issue #162 gives
/// `PasteMethod` a capability model that says this properly, and supersedes this
/// helper when it lands.
fn delivery_sends_input(paste_method: PasteMethod, auto_submit: bool) -> bool {
    paste_method != PasteMethod::None || should_send_auto_submit(auto_submit, paste_method)
}

/// Send the transcript to the resolved target, and report whether it was
/// delivered. `delivery` is `None` when the target lock was lost, in which case
/// nothing is typed anywhere.
fn deliver_to_target(
    text: &str,
    app_handle: &AppHandle,
    settings: &AppSettings,
    delivery: Option<Delivery>,
) -> Result<bool, String> {
    let Some(delivery) = delivery else {
        return Ok(false);
    };

    let paste_method = settings.paste_method;

    // Get the managed Enigo instance
    let enigo_state = app_handle
        .try_state::<EnigoState>()
        .ok_or("Enigo state not initialized")?;
    let mut enigo = enigo_state
        .0
        .lock()
        .map_err(|e| format!("Failed to lock Enigo: {}", e))?;

    let hold = FocusHold::new(app_handle, delivery);

    // The paste itself, unchanged whichever window it lands in. A pinned target
    // runs it inside a focus borrow; `hold.ensure()` re-checks the target at
    // every keystroke boundary, because focus can move during the writes and
    // waits in between.
    let deliver = |enigo: &mut Enigo| -> Result<(), DeliveryError> {
        match paste_method {
            PasteMethod::None => {
                info!("PasteMethod::None selected - skipping paste action");
            }
            PasteMethod::Direct => {
                hold.ensure()?;
                paste_direct(
                    enigo,
                    text,
                    #[cfg(target_os = "linux")]
                    settings.typing_tool,
                )?;
            }
            PasteMethod::CtrlV | PasteMethod::CtrlShiftV | PasteMethod::ShiftInsert => {
                paste_via_clipboard(
                    enigo,
                    text,
                    app_handle,
                    &paste_method,
                    settings.paste_delay_ms,
                    &hold,
                )?
            }
            PasteMethod::ExternalScript => {
                // The script decides for itself where the text goes, so there is
                // no keystroke here to hold focus for.
                let script_path = settings
                    .external_script_path
                    .as_ref()
                    .filter(|p| !p.is_empty())
                    .ok_or("External script path is not configured")?;
                paste_via_external_script(text, script_path)?;
            }
        }

        if should_send_auto_submit(settings.auto_submit, paste_method) {
            std::thread::sleep(Duration::from_millis(50));
            hold.ensure()?;
            send_return_key(enigo, settings.auto_submit_key)?;
        }

        Ok(())
    };

    let outcome = match delivery {
        Delivery::Foreground => deliver(&mut enigo),
        // Borrowing focus for a delivery that sends nothing would take the
        // user's window away from them for no reason at all.
        Delivery::Pinned(_, _) if !delivery_sends_input(paste_method, settings.auto_submit) => {
            deliver(&mut enigo)
        }
        Delivery::Pinned(identity, source) => {
            match target_backend::borrow_focus(app_handle, identity, source, || deliver(&mut enigo))
            {
                Ok(Borrowed::Delivered(result)) => result,
                // The window died between resolving it and activating it, so
                // nothing was typed and the pick or lock is already cleaned up.
                Ok(Borrowed::Suppressed) => {
                    return Ok(false);
                }
                Err(lost) => Err(lost.into()),
            }
        }
    };

    delivery_outcome(outcome)
}

/// Turn one delivery's result into "was it delivered", or a real error.
///
/// A suppressed delivery is not an error: the target window was lost, nothing
/// was typed anywhere, and the user has already been told the lock is gone.
/// Reporting it as a failure would abandon the rest of the paste path, and with
/// it the clipboard copy that is the transcript's last refuge (#120).
fn delivery_outcome(outcome: Result<(), DeliveryError>) -> Result<bool, String> {
    match outcome {
        Ok(()) => Ok(true),
        Err(DeliveryError::Suppressed(reason)) => {
            warn!("Delivery to the locked window stopped: {}", reason);
            Ok(false)
        }
        Err(DeliveryError::Failed(error)) => Err(error),
    }
}

pub fn paste(text: String, app_handle: AppHandle) -> Result<(), String> {
    let settings = get_settings(&app_handle);
    let paste_method = settings.paste_method;
    let paste_delay_ms = settings.paste_delay_ms;

    // Append trailing space if setting is enabled
    let text = if settings.append_trailing_space {
        format!("{} ", text)
    } else {
        text
    };

    info!(
        "Using paste method: {:?}, delay: {}ms",
        paste_method, paste_delay_ms
    );

    // Where this transcript goes: the foreground window, or the window the user
    // locked (#120). A lock whose window has closed delivers to no window at
    // all, rather than to whatever inherited focus.
    let delivery = target_backend::resolve_paste_target(&app_handle);
    let delivered = deliver_to_target(&text, &app_handle, &settings, delivery)?;

    // The clipboard copy is a setting about the clipboard, not about the window
    // delivery, so it runs even when delivery was suppressed. Otherwise the only
    // copy of a transcript is discarded whenever the lock is lost -- which, with
    // PasteMethod::None, is the entire output.
    if should_copy_to_clipboard(settings.clipboard_handling, delivered) {
        let clipboard = app_handle.clipboard();
        clipboard
            .write_text(&text)
            .map_err(|e| format!("Failed to copy to clipboard: {}", e))?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_submit_requires_setting_enabled() {
        assert!(!should_send_auto_submit(false, PasteMethod::CtrlV));
        assert!(!should_send_auto_submit(false, PasteMethod::Direct));
    }

    #[test]
    fn a_closed_target_suppresses_but_a_refused_one_fails() {
        // Gone: already announced, lock already dropped, nothing typed. The
        // paste path carries on to the clipboard copy without a second alarm.
        assert_eq!(
            DeliveryError::from(FocusLost::TargetGone),
            DeliveryError::Suppressed("the locked window closed during delivery".to_string())
        );
        // Still there but refusing to come forward: the transcript went
        // nowhere and the lock still stands, so the user has to be told --
        // silence here loses the text outright when nothing is copied.
        assert_eq!(
            DeliveryError::from(FocusLost::ActivationRefused("refused".to_string())),
            DeliveryError::Failed("refused".to_string())
        );
    }

    #[test]
    fn a_refused_activation_reaches_the_user_as_an_error() {
        // End to end through the outcome mapping: the refusal must surface,
        // not turn into a quiet "not delivered".
        let refused = delivery_outcome(Err(FocusLost::ActivationRefused(
            "the system refused to activate the target window".to_string(),
        )
        .into()));
        assert_eq!(
            refused,
            Err("the system refused to activate the target window".to_string())
        );
        assert_eq!(
            delivery_outcome(Err(FocusLost::TargetGone.into())),
            Ok(false)
        );
    }

    #[test]
    fn only_a_delivery_that_types_something_needs_focus() {
        // Copy-to-clipboard with no paste method touches no window, so taking
        // focus for it is pure disruption.
        assert!(!delivery_sends_input(PasteMethod::None, false));
        // Auto-submit is suppressed for PasteMethod::None, so it cannot bring
        // the keystroke back on its own.
        assert!(!delivery_sends_input(PasteMethod::None, true));
        assert!(delivery_sends_input(PasteMethod::CtrlV, false));
        assert!(delivery_sends_input(PasteMethod::Direct, false));
        assert!(delivery_sends_input(PasteMethod::ExternalScript, false));
    }

    #[test]
    fn a_lost_target_is_not_a_paste_failure() {
        // Losing the target mid-delivery must not abort the rest of the paste
        // path: the clipboard copy below is the transcript's last refuge, and
        // the user has already seen the "lock lost" notice.
        let suppressed = delivery_outcome(Err(DeliveryError::Suppressed(
            "the locked window closed during delivery".to_string(),
        )));
        assert_eq!(suppressed, Ok(false));
    }

    #[test]
    fn a_broken_paste_is_still_reported() {
        let failed = delivery_outcome(Err(DeliveryError::Failed("enigo exploded".to_string())));
        assert_eq!(failed, Err("enigo exploded".to_string()));
        assert_eq!(delivery_outcome(Ok(())), Ok(true));
    }

    #[test]
    fn a_suppressed_delivery_still_copies_to_the_clipboard() {
        // A lost target lock stops the paste, not the clipboard copy. With
        // PasteMethod::None the copy is the whole output, so skipping it would
        // throw the transcript away.
        assert!(should_copy_to_clipboard(
            ClipboardHandling::CopyToClipboard,
            false
        ));
        assert!(should_copy_to_clipboard(
            ClipboardHandling::CopyToClipboard,
            true
        ));
    }

    #[test]
    fn no_copy_when_the_setting_is_off() {
        assert!(!should_copy_to_clipboard(
            ClipboardHandling::DontModify,
            true
        ));
        assert!(!should_copy_to_clipboard(
            ClipboardHandling::DontModify,
            false
        ));
    }

    #[test]
    fn auto_submit_skips_none_paste_method() {
        assert!(!should_send_auto_submit(true, PasteMethod::None));
    }

    #[test]
    fn auto_submit_runs_for_active_paste_methods() {
        assert!(should_send_auto_submit(true, PasteMethod::CtrlV));
        assert!(should_send_auto_submit(true, PasteMethod::Direct));
        assert!(should_send_auto_submit(true, PasteMethod::CtrlShiftV));
        assert!(should_send_auto_submit(true, PasteMethod::ShiftInsert));
    }
}
