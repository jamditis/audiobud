use enigo::{Direction, Enigo, Key, Keyboard, Mouse, Settings};
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Manager};

/// Wrapper for Enigo to store in Tauri's managed state.
/// Enigo is wrapped in a Mutex since it requires mutable access.
///
/// The lock protects the instance, not the order deliveries take it in:
/// `std::sync::Mutex` makes no fairness promise, so two transcripts racing for
/// it could be typed out in either order. Delivery order is guaranteed a step
/// earlier instead -- [`crate::delivery_queue`] releases one transcript at a
/// time and [`crate::delivery_worker`] runs them on a single thread, so
/// overlapping dictations never contend for this lock at all (#161, #122).
pub struct EnigoState(pub Mutex<Enigo>);

impl EnigoState {
    pub fn new() -> Result<Self, String> {
        let enigo = Enigo::new(&Settings::default())
            .map_err(|e| format!("Failed to initialize Enigo: {}", e))?;
        Ok(Self(Mutex::new(enigo)))
    }
}

trait KeyInput {
    fn emit_key(&mut self, key: Key, direction: Direction) -> Result<(), String>;
}

impl KeyInput for Enigo {
    fn emit_key(&mut self, key: Key, direction: Direction) -> Result<(), String> {
        self.key(key, direction).map_err(|error| error.to_string())
    }
}

/// Tracks pressed keys and releases them in reverse order on return or unwind.
struct PressedKeys<'a, T: KeyInput> {
    input: &'a mut T,
    pressed: Vec<Key>,
}

impl<'a, T: KeyInput> PressedKeys<'a, T> {
    fn new(input: &'a mut T) -> Self {
        Self {
            input,
            pressed: Vec::new(),
        }
    }

    fn press(&mut self, key: Key) -> Result<(), String> {
        // Track before the backend call. Some backends can apply the event and
        // still return an error, so that path must also attempt a release.
        self.pressed.push(key);
        self.input
            .emit_key(key, Direction::Press)
            .map_err(|error| format!("Failed to press {key:?}: {error}"))
    }

    fn click(&mut self, key: Key) -> Result<(), String> {
        self.input
            .emit_key(key, Direction::Click)
            .map_err(|error| format!("Failed to click {key:?}: {error}"))
    }

    fn release_all(&mut self) -> Result<(), String> {
        while let Some(key) = self.pressed.last().copied() {
            self.input
                .emit_key(key, Direction::Release)
                .map_err(|error| format!("Failed to release {key:?}: {error}"))?;
            self.pressed.pop();
        }
        Ok(())
    }
}

impl<T: KeyInput> Drop for PressedKeys<'_, T> {
    fn drop(&mut self) {
        while let Some(key) = self.pressed.pop() {
            let _ = self.input.emit_key(key, Direction::Release);
        }
    }
}

fn send_click_chord<T: KeyInput>(
    input: &mut T,
    modifiers: &[Key],
    key: Key,
    delay: Duration,
) -> Result<(), String> {
    let mut pressed = PressedKeys::new(input);
    for modifier in modifiers {
        pressed.press(*modifier)?;
    }
    pressed.click(key)?;
    std::thread::sleep(delay);
    pressed.release_all()
}

pub(crate) fn send_pressed_key_chord(
    enigo: &mut Enigo,
    modifiers: &[Key],
    key: Key,
) -> Result<(), String> {
    send_pressed_key_chord_with(enigo, modifiers, key)
}

fn send_pressed_key_chord_with<T: KeyInput>(
    input: &mut T,
    modifiers: &[Key],
    key: Key,
) -> Result<(), String> {
    let mut pressed = PressedKeys::new(input);
    for modifier in modifiers {
        pressed.press(*modifier)?;
    }
    pressed.press(key)?;
    pressed.release_all()
}

/// Get the current mouse cursor position using the managed Enigo instance.
/// Returns None if the state is not available or if getting the location fails.
pub fn get_cursor_position(app_handle: &AppHandle) -> Option<(i32, i32)> {
    let enigo_state = app_handle.try_state::<EnigoState>()?;
    let enigo = enigo_state.0.lock().ok()?;
    enigo.location().ok()
}

/// Sends a Ctrl+V or Cmd+V paste command using platform-specific virtual key codes.
/// This ensures the paste works regardless of keyboard layout (e.g., Russian, AZERTY, DVORAK).
/// Note: On Wayland, this may not work - callers should check for Wayland and use alternative methods.
pub fn send_paste_ctrl_v(enigo: &mut Enigo) -> Result<(), String> {
    // Platform-specific key definitions
    #[cfg(target_os = "macos")]
    let (modifier_key, v_key_code) = (Key::Meta, Key::Other(9));
    #[cfg(target_os = "windows")]
    let (modifier_key, v_key_code) = (Key::Control, Key::Other(0x56)); // VK_V
    #[cfg(target_os = "linux")]
    let (modifier_key, v_key_code) = (Key::Control, Key::Unicode('v'));

    send_click_chord(
        enigo,
        &[modifier_key],
        v_key_code,
        Duration::from_millis(100),
    )
}

/// Sends a Ctrl+Shift+V paste command.
/// This is commonly used in terminal applications on Linux to paste without formatting.
/// Note: On Wayland, this may not work - callers should check for Wayland and use alternative methods.
pub fn send_paste_ctrl_shift_v(enigo: &mut Enigo) -> Result<(), String> {
    // Platform-specific key definitions
    #[cfg(target_os = "macos")]
    let (modifier_key, v_key_code) = (Key::Meta, Key::Other(9)); // Cmd+Shift+V on macOS
    #[cfg(target_os = "windows")]
    let (modifier_key, v_key_code) = (Key::Control, Key::Other(0x56)); // VK_V
    #[cfg(target_os = "linux")]
    let (modifier_key, v_key_code) = (Key::Control, Key::Unicode('v'));

    send_click_chord(
        enigo,
        &[modifier_key, Key::Shift],
        v_key_code,
        Duration::from_millis(100),
    )
}

/// Sends a Shift+Insert paste command (Windows and Linux only).
/// This is more universal for terminal applications and legacy software.
/// Note: On Wayland, this may not work - callers should check for Wayland and use alternative methods.
pub fn send_paste_shift_insert(enigo: &mut Enigo) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    let insert_key_code = Key::Other(0x2D); // VK_INSERT
    #[cfg(not(target_os = "windows"))]
    let insert_key_code = Key::Other(0x76); // XK_Insert (keycode 118 / 0x76, also used as fallback)

    send_click_chord(
        enigo,
        &[Key::Shift],
        insert_key_code,
        Duration::from_millis(100),
    )
}

/// Pastes text directly using the enigo text method.
/// This tries to use system input methods if possible, otherwise simulates keystrokes one by one.
pub fn paste_text_direct(enigo: &mut Enigo, text: &str) -> Result<(), String> {
    enigo
        .text(text)
        .map_err(|e| format!("Failed to send text directly: {}", e))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use enigo::Direction;
    use std::time::Duration;

    #[derive(Default)]
    struct FakeKeyboard {
        events: Vec<(Key, Direction)>,
        pressed: Vec<Key>,
        fail_at: Option<usize>,
    }

    impl FakeKeyboard {
        fn failing_at(event: usize) -> Self {
            Self {
                fail_at: Some(event),
                ..Self::default()
            }
        }
    }

    impl KeyInput for FakeKeyboard {
        fn emit_key(&mut self, key: Key, direction: Direction) -> Result<(), String> {
            self.events.push((key, direction));
            match direction {
                Direction::Press => self.pressed.push(key),
                Direction::Release => {
                    if let Some(position) = self.pressed.iter().rposition(|pressed| *pressed == key)
                    {
                        self.pressed.remove(position);
                    }
                }
                Direction::Click => {}
            }

            if self.fail_at == Some(self.events.len() - 1) {
                Err("simulated input failure".to_string())
            } else {
                Ok(())
            }
        }
    }

    #[test]
    fn click_chord_releases_modifiers_after_each_possible_failure() {
        for fail_at in 0..5 {
            let mut keyboard = FakeKeyboard::failing_at(fail_at);
            let result = send_click_chord(
                &mut keyboard,
                &[Key::Meta, Key::Shift],
                Key::Unicode('v'),
                Duration::ZERO,
            );

            assert!(result.is_err(), "event {fail_at} must fail");
            assert!(
                keyboard.pressed.is_empty(),
                "event {fail_at} left keys pressed: {:?}",
                keyboard.pressed
            );
        }
    }

    #[test]
    fn pressed_key_chord_releases_return_and_modifier_after_each_failure() {
        for fail_at in 0..4 {
            let mut keyboard = FakeKeyboard::failing_at(fail_at);
            let result = send_pressed_key_chord_with(&mut keyboard, &[Key::Meta], Key::Return);

            assert!(result.is_err(), "event {fail_at} must fail");
            assert!(
                keyboard.pressed.is_empty(),
                "event {fail_at} left keys pressed: {:?}",
                keyboard.pressed
            );
        }
    }

    #[test]
    fn cleanup_releases_pressed_keys_in_reverse_order() {
        let mut keyboard = FakeKeyboard::failing_at(2);
        let result = send_click_chord(
            &mut keyboard,
            &[Key::Meta, Key::Shift],
            Key::Unicode('v'),
            Duration::ZERO,
        );

        assert!(result.is_err());
        let releases: Vec<Key> = keyboard
            .events
            .iter()
            .filter_map(|(key, direction)| matches!(direction, Direction::Release).then_some(*key))
            .collect();
        assert_eq!(releases, vec![Key::Shift, Key::Meta]);
    }
}
