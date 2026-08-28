//! Handy-keys based keyboard shortcut implementation
//!
//! This module provides an alternative to Tauri's global-shortcut plugin
//! using the handy-keys library for more control over keyboard events.
//!
//! ## Architecture
//!
//! The implementation uses a dedicated manager thread that owns the `HotkeyManager`:
//!
//! ```text
//! ┌─────────────────┐     commands      ┌──────────────────────┐
//! │   Main Thread   │ ───────────────▶ │   Manager Thread     │
//! │                 │   (via channel)   │                      │
//! │ - register()    │                   │ - owns HotkeyManager │
//! │ - unregister()  │                   │ - polls for events   │
//! └─────────────────┘                   │ - dispatches actions │
//!                                       └──────────────────────┘
//! ```
//!
//! This design ensures thread-safety since `HotkeyManager` is only accessed
//! from a single thread. Commands (register/unregister) are sent via an mpsc
//! channel and responses are synchronously awaited.
//!
//! ## Recording Mode
//!
//! For UI key capture, a separate `KeyboardListener` is created on-demand and
//! polled from a dedicated recording thread. Events are emitted to the frontend
//! via Tauri's event system.

use handy_keys::{
    Error as HandyKeysError, Hotkey, HotkeyId, HotkeyManager, HotkeyState, KeyboardListener,
};
use log::{debug, error, info};
use serde::Serialize;
use specta::Type;
use std::collections::HashMap;
use std::hash::Hash;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

use crate::settings::{self, get_settings, ShortcutBinding};

use super::handler::handle_shortcut_event;

const MANAGER_STARTUP_TIMEOUT: Duration = Duration::from_secs(5);

/// Commands that can be sent to the hotkey manager thread
enum ManagerCommand {
    Register {
        binding_id: String,
        hotkey_string: String,
        response: Sender<Result<(), super::ShortcutRegistrationError>>,
    },
    Unregister {
        binding_id: String,
        response: Sender<Result<(), String>>,
    },
    Shutdown,
}

/// State for the handy-keys shortcut manager
pub struct HandyKeysState {
    /// Channel to send commands to the manager thread (wrapped in Mutex for Sync)
    command_sender: Mutex<Sender<ManagerCommand>>,
    /// Handle to the manager thread (wrapped in Mutex for Sync, allows proper join on drop)
    thread_handle: Mutex<Option<JoinHandle<()>>>,
    /// Recording listener for UI key capture (only active during recording)
    recording_listener: Mutex<Option<KeyboardListener>>,
    /// Flag indicating if we're in recording mode
    is_recording: AtomicBool,
    /// The binding ID being recorded (if any)
    recording_binding_id: Mutex<Option<String>>,
    /// Flag to stop recording loop
    recording_running: Arc<AtomicBool>,
}

/// Key event sent to frontend during recording mode
#[derive(Debug, Clone, Serialize, Type)]
pub struct FrontendKeyEvent {
    /// Currently pressed modifier keys
    pub modifiers: Vec<String>,
    /// The key that was pressed (if any)
    pub key: Option<String>,
    /// Whether this is a key down event
    pub is_key_down: bool,
    /// The full hotkey string (e.g., "option+space")
    pub hotkey_string: String,
}

impl HandyKeysState {
    /// Create a new HandyKeysState
    pub fn new(app: AppHandle) -> Result<Self, String> {
        let (cmd_tx, cmd_rx) = mpsc::channel::<ManagerCommand>();
        let (startup_tx, startup_rx) = mpsc::channel::<Result<(), String>>();
        let startup_cancelled = Arc::new(AtomicBool::new(false));

        // Start the manager thread
        let app_clone = app.clone();
        let thread_cancelled = Arc::clone(&startup_cancelled);
        let thread_handle = thread::spawn(move || {
            Self::manager_thread(cmd_rx, app_clone, startup_tx, thread_cancelled);
        });

        if let Err(error) =
            wait_for_manager_startup(startup_rx, &startup_cancelled, MANAGER_STARTUP_TIMEOUT)
        {
            // The manager checks cancellation before it publishes startup and
            // exits without entering its command loop. Dropping the handle
            // keeps this error path bounded while native startup unwinds.
            drop(thread_handle);
            return Err(error);
        }

        Ok(Self {
            command_sender: Mutex::new(cmd_tx),
            thread_handle: Mutex::new(Some(thread_handle)),
            recording_listener: Mutex::new(None),
            is_recording: AtomicBool::new(false),
            recording_binding_id: Mutex::new(None),
            recording_running: Arc::new(AtomicBool::new(false)),
        })
    }

    /// The main manager thread - owns the HotkeyManager and processes commands
    fn manager_thread(
        cmd_rx: Receiver<ManagerCommand>,
        app: AppHandle,
        startup_tx: Sender<Result<(), String>>,
        startup_cancelled: Arc<AtomicBool>,
    ) {
        info!("handy-keys manager thread started");

        if startup_cancelled.load(Ordering::SeqCst) {
            return;
        }

        // Create the HotkeyManager in this thread
        let manager = match HotkeyManager::new_with_blocking() {
            Ok(m) => m,
            Err(e) => {
                let message = format!("Failed to create HotkeyManager: {e}");
                error!("{message}");
                let _ = startup_tx.send(Err(message));
                return;
            }
        };

        if startup_cancelled.load(Ordering::SeqCst) {
            return;
        }

        if startup_tx.send(Ok(())).is_err() {
            return;
        }

        // Maps binding IDs to HotkeyIds and hotkey strings
        let mut binding_to_hotkey: HashMap<String, HotkeyId> = HashMap::new();
        let mut hotkey_to_binding: HashMap<HotkeyId, (String, String)> = HashMap::new(); // (binding_id, hotkey_string)

        loop {
            // Check for hotkey events (non-blocking)
            while let Some(event) = manager.try_recv() {
                if let Some((binding_id, hotkey_string)) = hotkey_to_binding.get(&event.id) {
                    debug!(
                        "handy-keys event: binding={}, hotkey={}, state={:?}",
                        binding_id, hotkey_string, event.state
                    );
                    let is_pressed = event.state == HotkeyState::Pressed;
                    handle_shortcut_event(&app, binding_id, hotkey_string, is_pressed);
                }
            }

            // Check for commands (non-blocking with timeout)
            match cmd_rx.recv_timeout(std::time::Duration::from_millis(10)) {
                Ok(cmd) => match cmd {
                    ManagerCommand::Register {
                        binding_id,
                        hotkey_string,
                        response,
                    } => {
                        let result = Self::do_register(
                            &manager,
                            &mut binding_to_hotkey,
                            &mut hotkey_to_binding,
                            &binding_id,
                            &hotkey_string,
                        );
                        let _ = response.send(result);
                    }
                    ManagerCommand::Unregister {
                        binding_id,
                        response,
                    } => {
                        let result = Self::do_unregister(
                            &manager,
                            &mut binding_to_hotkey,
                            &mut hotkey_to_binding,
                            &binding_id,
                        );
                        let _ = response.send(result);
                    }
                    ManagerCommand::Shutdown => {
                        info!("handy-keys manager thread shutting down");
                        break;
                    }
                },
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    // No command, continue
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    info!("Command channel disconnected, shutting down");
                    break;
                }
            }
        }

        info!("handy-keys manager thread stopped");
    }

    /// Register a hotkey
    fn do_register(
        manager: &HotkeyManager,
        binding_to_hotkey: &mut HashMap<String, HotkeyId>,
        hotkey_to_binding: &mut HashMap<HotkeyId, (String, String)>,
        binding_id: &str,
        hotkey_string: &str,
    ) -> Result<(), super::ShortcutRegistrationError> {
        let hotkey: Hotkey = hotkey_string.parse().map_err(|error| {
            super::ShortcutRegistrationError::before_activation(format!(
                "Failed to parse hotkey '{hotkey_string}': {error}"
            ))
        })?;

        let id = manager
            .register(hotkey)
            .map_err(classify_manager_registration_error)?;

        binding_to_hotkey.insert(binding_id.to_string(), id);
        hotkey_to_binding.insert(id, (binding_id.to_string(), hotkey_string.to_string()));

        debug!(
            "Registered handy-keys shortcut: {} -> {:?}",
            binding_id, hotkey
        );
        Ok(())
    }

    /// Unregister a hotkey
    fn do_unregister(
        manager: &HotkeyManager,
        binding_to_hotkey: &mut HashMap<String, HotkeyId>,
        hotkey_to_binding: &mut HashMap<HotkeyId, (String, String)>,
        binding_id: &str,
    ) -> Result<(), String> {
        unregister_tracked_hotkey(binding_to_hotkey, hotkey_to_binding, binding_id, |id| {
            match manager.unregister(id) {
                Ok(()) => Ok(()),
                Err(HandyKeysError::HotkeyNotFound(_)) => Ok(()),
                Err(error) => Err(format!("Failed to unregister hotkey: {error}")),
            }
        })?;
        debug!("Unregistered handy-keys shortcut: {binding_id}");
        Ok(())
    }

    /// Register a shortcut binding
    fn register(&self, binding: &ShortcutBinding) -> Result<(), super::ShortcutRegistrationError> {
        send_registration_command(&self.command_sender, binding)
    }

    /// Unregister a shortcut binding
    pub fn unregister(&self, binding: &ShortcutBinding) -> Result<(), String> {
        let (tx, rx) = mpsc::channel();
        self.command_sender
            .lock()
            .map_err(|_| "Failed to lock command_sender")?
            .send(ManagerCommand::Unregister {
                binding_id: binding.id.clone(),
                response: tx,
            })
            .map_err(|_| "Failed to send unregister command")?;

        rx.recv()
            .map_err(|_| "Failed to receive unregister response")?
    }

    /// Start recording mode for a specific binding
    pub fn start_recording(&self, app: &AppHandle, binding_id: String) -> Result<(), String> {
        if self.is_recording.load(Ordering::SeqCst) {
            return Err("Already recording".into());
        }

        // Create a new keyboard listener for recording
        let listener = KeyboardListener::new()
            .map_err(|e| format!("Failed to create keyboard listener: {}", e))?;

        {
            let mut recording = self
                .recording_listener
                .lock()
                .map_err(|_| "Failed to lock recording_listener")?;
            *recording = Some(listener);
        }
        {
            let mut binding = self
                .recording_binding_id
                .lock()
                .map_err(|_| "Failed to lock recording_binding_id")?;
            *binding = Some(binding_id);
        }

        self.is_recording.store(true, Ordering::SeqCst);
        self.recording_running.store(true, Ordering::SeqCst);

        // Start a thread to emit key events to the frontend
        let app_clone = app.clone();
        let recording_running = Arc::clone(&self.recording_running);
        thread::spawn(move || {
            Self::recording_loop(app_clone, recording_running);
        });

        debug!("Started handy-keys recording mode");
        Ok(())
    }

    /// Recording loop - emits key events to frontend during recording
    fn recording_loop(app: AppHandle, running: Arc<AtomicBool>) {
        while running.load(Ordering::SeqCst) {
            let event = {
                let state = match app.try_state::<HandyKeysState>() {
                    Some(s) => s,
                    None => break,
                };
                let listener = state.recording_listener.lock().ok();
                listener.as_ref().and_then(|l| l.as_ref()?.try_recv())
            };

            if let Some(key_event) = event {
                // Convert to frontend-friendly format
                let frontend_event = FrontendKeyEvent {
                    modifiers: modifiers_to_strings(key_event.modifiers),
                    key: key_event.key.map(|k| k.to_string().to_lowercase()),
                    is_key_down: key_event.is_key_down,
                    hotkey_string: key_event
                        .as_hotkey()
                        .map(|h| h.to_handy_string())
                        .unwrap_or_default(),
                };

                // Emit to frontend
                if let Err(e) = app.emit("handy-keys-event", &frontend_event) {
                    error!("Failed to emit key event: {}", e);
                }
            } else {
                thread::sleep(std::time::Duration::from_millis(10));
            }
        }

        debug!("Recording loop ended");
    }

    /// Stop recording mode
    pub fn stop_recording(&self) -> Result<(), String> {
        self.is_recording.store(false, Ordering::SeqCst);
        self.recording_running.store(false, Ordering::SeqCst);

        {
            let mut recording = self
                .recording_listener
                .lock()
                .map_err(|_| "Failed to lock recording_listener")?;
            *recording = None;
        }
        {
            let mut binding = self
                .recording_binding_id
                .lock()
                .map_err(|_| "Failed to lock recording_binding_id")?;
            *binding = None;
        }

        debug!("Stopped handy-keys recording mode");
        Ok(())
    }
}

fn send_registration_command(
    command_sender: &Mutex<Sender<ManagerCommand>>,
    binding: &ShortcutBinding,
) -> Result<(), super::ShortcutRegistrationError> {
    let (response_sender, response_receiver) = mpsc::channel();
    command_sender
        .lock()
        .map_err(|_| {
            super::ShortcutRegistrationError::backend_unavailable(
                "Failed to lock command_sender".to_string(),
            )
        })?
        .send(ManagerCommand::Register {
            binding_id: binding.id.clone(),
            hotkey_string: binding.current_binding.clone(),
            response: response_sender,
        })
        .map_err(|_| {
            super::ShortcutRegistrationError::backend_unavailable(
                "Failed to send register command".to_string(),
            )
        })?;

    match response_receiver.recv() {
        Ok(Ok(())) => Ok(()),
        Ok(Err(error)) => Err(error),
        Err(_) => Err(super::ShortcutRegistrationError::backend_unavailable(
            "Failed to receive register response".to_string(),
        )),
    }
}

fn classify_manager_registration_error(error: HandyKeysError) -> super::ShortcutRegistrationError {
    let message = format!("Failed to register hotkey: {error}");
    match error {
        HandyKeysError::HotkeyAlreadyRegistered(_) => {
            super::ShortcutRegistrationError::before_activation(message)
        }
        _ => super::ShortcutRegistrationError::backend_unavailable(message),
    }
}

fn unregister_tracked_hotkey<Id, Unregister>(
    binding_to_hotkey: &mut HashMap<String, Id>,
    hotkey_to_binding: &mut HashMap<Id, (String, String)>,
    binding_id: &str,
    unregister: Unregister,
) -> Result<(), String>
where
    Id: Copy + Eq + Hash,
    Unregister: FnOnce(Id) -> Result<(), String>,
{
    let Some(&id) = binding_to_hotkey.get(binding_id) else {
        return Ok(());
    };

    unregister(id)?;
    binding_to_hotkey.remove(binding_id);
    hotkey_to_binding.remove(&id);
    Ok(())
}

fn wait_for_manager_startup(
    startup_rx: Receiver<Result<(), String>>,
    startup_cancelled: &AtomicBool,
    timeout: Duration,
) -> Result<(), String> {
    match startup_rx.recv_timeout(timeout) {
        Ok(result) => result,
        Err(mpsc::RecvTimeoutError::Timeout) => {
            startup_cancelled.store(true, Ordering::SeqCst);
            Err("HandyKeys manager startup timed out".to_string())
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            startup_cancelled.store(true, Ordering::SeqCst);
            Err("HandyKeys manager stopped during startup".to_string())
        }
    }
}

impl Drop for HandyKeysState {
    fn drop(&mut self) {
        // Signal recording to stop
        self.recording_running.store(false, Ordering::SeqCst);
        self.is_recording.store(false, Ordering::SeqCst);

        // Send shutdown command
        let sender = self
            .command_sender
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let _ = sender.send(ManagerCommand::Shutdown);
        drop(sender);

        // Wait for the manager thread to finish
        let mut handle = self
            .thread_handle
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(h) = handle.take() {
            let _ = h.join();
        }
    }
}

/// Convert handy-keys Modifiers to a list of strings
fn modifiers_to_strings(modifiers: handy_keys::Modifiers) -> Vec<String> {
    let mut result = Vec::new();

    if modifiers.contains(handy_keys::Modifiers::CTRL) {
        result.push("ctrl".to_string());
    }
    if modifiers.contains(handy_keys::Modifiers::OPT) {
        #[cfg(target_os = "macos")]
        result.push("option".to_string());
        #[cfg(not(target_os = "macos"))]
        result.push("alt".to_string());
    }
    if modifiers.contains(handy_keys::Modifiers::SHIFT) {
        result.push("shift".to_string());
    }
    if modifiers.contains(handy_keys::Modifiers::CMD) {
        #[cfg(target_os = "macos")]
        result.push("command".to_string());
        #[cfg(not(target_os = "macos"))]
        result.push("super".to_string());
    }
    if modifiers.contains(handy_keys::Modifiers::FN) {
        result.push("fn".to_string());
    }

    result
}

/// Validate a shortcut string for the HandyKeys implementation.
/// HandyKeys is more permissive: allows modifier-only combos and the fn key.
pub fn validate_shortcut(raw: &str) -> Result<(), String> {
    if raw.trim().is_empty() {
        return Err("Shortcut cannot be empty".into());
    }
    // HandyKeys accepts modifier-only, key-only, and modifier+key combos
    // Just verify the string is parseable
    raw.parse::<Hotkey>()
        .map(|_| ())
        .map_err(|e| format!("Invalid shortcut for HandyKeys: {}", e))
}

fn validate_initial_shortcut(raw: &str) -> Result<(), super::ShortcutRegistrationError> {
    validate_shortcut(raw).map_err(super::ShortcutRegistrationError::before_activation)
}

/// Initialize handy-keys shortcuts
pub fn init_shortcuts(app: &AppHandle) -> Result<(), String> {
    let state = HandyKeysState::new(app.clone())?;
    let bindings = super::configured_initial_bindings(app);
    super::register_initial_bindings(
        bindings,
        |binding| {
            validate_initial_shortcut(&binding.current_binding)?;
            state.register(binding)
        },
        |binding| state.unregister(binding),
    )?;

    app.manage(state);
    info!("handy-keys shortcuts initialized");
    Ok(())
}

/// Register the cancel shortcut (called when recording starts)
pub fn register_cancel_shortcut(app: &AppHandle) {
    // Disabled on Linux due to instability
    #[cfg(target_os = "linux")]
    {
        let _ = app;
        return;
    }

    #[cfg(not(target_os = "linux"))]
    {
        let app_clone = app.clone();
        tauri::async_runtime::spawn(async move {
            if let Some(cancel_binding) = get_settings(&app_clone).bindings.get("cancel").cloned() {
                if let Some(state) = app_clone.try_state::<HandyKeysState>() {
                    if let Err(error) = state.register(&cancel_binding) {
                        error!(
                            "Failed to register cancel shortcut: {}",
                            error.into_message()
                        );
                    }
                }
            }
        });
    }
}

/// Unregister the cancel shortcut (called when recording stops)
pub fn unregister_cancel_shortcut(app: &AppHandle) {
    #[cfg(target_os = "linux")]
    {
        let _ = app;
        return;
    }

    #[cfg(not(target_os = "linux"))]
    {
        let app_clone = app.clone();
        tauri::async_runtime::spawn(async move {
            if let Some(cancel_binding) = get_settings(&app_clone).bindings.get("cancel").cloned() {
                if let Some(state) = app_clone.try_state::<HandyKeysState>() {
                    let _ = state.unregister(&cancel_binding);
                }
            }
        });
    }
}

/// Register a shortcut
pub fn register_shortcut(app: &AppHandle, binding: ShortcutBinding) -> Result<(), String> {
    let state = app
        .try_state::<HandyKeysState>()
        .ok_or("HandyKeysState not initialized")?;
    state
        .register(&binding)
        .map_err(super::ShortcutRegistrationError::into_message)
}

/// Unregister a shortcut
pub fn unregister_shortcut(app: &AppHandle, binding: ShortcutBinding) -> Result<(), String> {
    let state = app
        .try_state::<HandyKeysState>()
        .ok_or("HandyKeysState not initialized")?;
    state.unregister(&binding)
}

/// Start key recording mode
#[tauri::command]
#[specta::specta]
pub fn start_handy_keys_recording(app: AppHandle, binding_id: String) -> Result<(), String> {
    let settings = get_settings(&app);
    if settings.keyboard_implementation != settings::KeyboardImplementation::HandyKeys {
        return Err("handy-keys is not the active keyboard implementation".into());
    }

    let state = app
        .try_state::<HandyKeysState>()
        .ok_or("HandyKeysState not initialized")?;
    state.start_recording(&app, binding_id)
}

/// Stop key recording mode
#[tauri::command]
#[specta::specta]
pub fn stop_handy_keys_recording(app: AppHandle) -> Result<(), String> {
    let settings = get_settings(&app);
    if settings.keyboard_implementation != settings::KeyboardImplementation::HandyKeys {
        return Err("handy-keys is not the active keyboard implementation".into());
    }

    let state = app
        .try_state::<HandyKeysState>()
        .ok_or("HandyKeysState not initialized")?;
    state.stop_recording()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_binding() -> ShortcutBinding {
        ShortcutBinding {
            id: "transcribe".to_string(),
            name: "Transcribe".to_string(),
            description: "Test binding".to_string(),
            default_binding: "option+space".to_string(),
            current_binding: "option+space".to_string(),
        }
    }

    #[test]
    fn failed_cleanup_keeps_tracking_for_a_retry() {
        let mut binding_to_hotkey = HashMap::from([("second".to_string(), 7_u32)]);
        let mut hotkey_to_binding = HashMap::from([(
            7_u32,
            ("second".to_string(), "option+shift+space".to_string()),
        )]);

        let first_result = unregister_tracked_hotkey(
            &mut binding_to_hotkey,
            &mut hotkey_to_binding,
            "second",
            |_| Err("native cleanup failed".to_string()),
        );

        assert_eq!(first_result, Err("native cleanup failed".to_string()));
        assert_eq!(binding_to_hotkey.get("second"), Some(&7_u32));
        assert!(hotkey_to_binding.contains_key(&7_u32));

        unregister_tracked_hotkey(
            &mut binding_to_hotkey,
            &mut hotkey_to_binding,
            "second",
            |_| Ok(()),
        )
        .expect("a later cleanup can retry the retained native id");

        assert!(!binding_to_hotkey.contains_key("second"));
        assert!(!hotkey_to_binding.contains_key(&7_u32));
    }

    #[test]
    fn closed_manager_command_channel_is_backend_unavailable() {
        let (command_sender, command_receiver) = mpsc::channel();
        drop(command_receiver);

        let error = send_registration_command(&Mutex::new(command_sender), &test_binding())
            .expect_err("a stopped manager must reject the command");

        assert_eq!(
            error.stage,
            super::super::ShortcutRegistrationStage::BackendUnavailable
        );
    }

    #[test]
    fn duplicate_manager_registration_is_rejected_before_activation() {
        let error = classify_manager_registration_error(HandyKeysError::HotkeyAlreadyRegistered(
            "option+space".to_string(),
        ));

        assert_eq!(
            error.stage,
            super::super::ShortcutRegistrationStage::RejectedBeforeActivation
        );
    }

    #[test]
    fn poisoned_manager_registration_aborts_initialization() {
        let error = classify_manager_registration_error(HandyKeysError::MutexPoisoned);

        assert_eq!(
            error.stage,
            super::super::ShortcutRegistrationStage::BackendUnavailable
        );
    }

    #[test]
    fn lost_registration_response_is_backend_unavailable() {
        let (command_sender, command_receiver) = mpsc::channel();
        let manager_thread = thread::spawn(move || {
            let ManagerCommand::Register { response, .. } = command_receiver
                .recv()
                .expect("the manager receives the registration command")
            else {
                panic!("expected a registration command");
            };
            drop(response);
        });

        let error = send_registration_command(&Mutex::new(command_sender), &test_binding())
            .expect_err("a lost response leaves the native result unknown");
        manager_thread.join().expect("the manager thread exits");

        assert_eq!(
            error.stage,
            super::super::ShortcutRegistrationStage::BackendUnavailable
        );
    }

    #[test]
    fn failed_handy_keys_owner_is_joined_before_fallback() {
        let (command_sender, command_receiver) = mpsc::channel();
        let teardown_complete = Arc::new(AtomicBool::new(false));
        let worker_teardown_complete = Arc::clone(&teardown_complete);
        let manager_thread = thread::spawn(move || loop {
            match command_receiver
                .recv()
                .expect("the state owns the command sender until teardown")
            {
                ManagerCommand::Register { response, .. } => drop(response),
                ManagerCommand::Unregister { response, .. } => {
                    let _ = response.send(Ok(()));
                }
                ManagerCommand::Shutdown => {
                    worker_teardown_complete.store(true, Ordering::SeqCst);
                    break;
                }
            }
        });

        let handy_error = {
            let state = HandyKeysState {
                command_sender: Mutex::new(command_sender),
                thread_handle: Mutex::new(Some(manager_thread)),
                recording_listener: Mutex::new(None),
                is_recording: AtomicBool::new(false),
                recording_binding_id: Mutex::new(None),
                recording_running: Arc::new(AtomicBool::new(false)),
            };
            let error = state
                .register(&test_binding())
                .expect_err("a lost response makes the backend unavailable");
            assert_eq!(
                error.stage,
                super::super::ShortcutRegistrationStage::BackendUnavailable
            );
            error.into_message()
        };

        let fallback_started = std::cell::Cell::new(false);
        let result = super::super::with_retry_safe_handy_keys_fallback(handy_error, || {
            assert!(teardown_complete.load(Ordering::SeqCst));
            fallback_started.set(true);
            Ok(())
        });

        assert_eq!(result, Ok(()));
        assert!(fallback_started.get());
    }

    #[test]
    fn malformed_initial_shortcuts_fail_before_activation() {
        let error = validate_initial_shortcut("ctrl+definitely_not_a_key")
            .expect_err("the backend parser must reject an unknown key");

        assert_eq!(
            error.stage,
            super::super::ShortcutRegistrationStage::RejectedBeforeActivation
        );
    }

    #[test]
    fn manager_startup_failure_is_returned_to_the_caller() {
        let (startup_tx, startup_rx) = mpsc::channel();
        let cancelled = AtomicBool::new(false);
        startup_tx
            .send(Err("simulated manager startup failure".to_string()))
            .unwrap();

        assert_eq!(
            wait_for_manager_startup(startup_rx, &cancelled, Duration::from_secs(1)),
            Err("simulated manager startup failure".to_string())
        );
    }

    #[test]
    fn closed_startup_channel_is_an_error() {
        let (startup_tx, startup_rx) = mpsc::channel::<Result<(), String>>();
        let cancelled = AtomicBool::new(false);
        drop(startup_tx);

        assert_eq!(
            wait_for_manager_startup(startup_rx, &cancelled, Duration::from_secs(1)),
            Err("HandyKeys manager stopped during startup".to_string())
        );
    }

    #[test]
    fn stalled_startup_times_out_and_cancels_the_manager_thread() {
        let (_startup_tx, startup_rx) = mpsc::channel::<Result<(), String>>();
        let cancelled = Arc::new(AtomicBool::new(false));
        let thread_cancelled = Arc::clone(&cancelled);
        let manager_thread = thread::spawn(move || {
            while !thread_cancelled.load(Ordering::SeqCst) {
                thread::sleep(Duration::from_millis(1));
            }
        });

        assert_eq!(
            wait_for_manager_startup(startup_rx, &cancelled, Duration::from_millis(10),),
            Err("HandyKeys manager startup timed out".to_string())
        );

        manager_thread.join().unwrap();
        assert!(cancelled.load(Ordering::SeqCst));
    }
}
