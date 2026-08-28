use crate::audio_toolkit::{list_input_devices, vad::SmoothedVad, AudioRecorder, SileroVad};
use crate::helpers::clamshell;
use crate::settings::{get_settings, AppSettings};
use crate::utils;
use log::{debug, error, info};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};
use tauri::Manager;

const STREAM_IDLE_TIMEOUT: Duration = Duration::from_secs(30);

fn set_mute(mute: bool) -> Result<(), String> {
    // Expected behavior:
    // - Windows: works on most systems using standard audio drivers.
    // - Linux: works on many systems (PipeWire, PulseAudio, ALSA),
    //   but some distros may lack the tools used.
    // - macOS: works on most standard setups via AppleScript.
    // Callers log failures but do not abort recording.

    #[cfg(target_os = "windows")]
    {
        unsafe {
            use windows::Win32::{
                Media::Audio::{
                    eMultimedia, eRender, Endpoints::IAudioEndpointVolume, IMMDeviceEnumerator,
                    MMDeviceEnumerator,
                },
                System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED},
            };

            // Initialize the COM library for this thread.
            // If already initialized (e.g., by another library like Tauri), this does nothing.
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);

            let all_devices: IMMDeviceEnumerator =
                CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).map_err(|error| {
                    format!("Failed to create audio device enumerator: {error}")
                })?;
            let default_device = all_devices
                .GetDefaultAudioEndpoint(eRender, eMultimedia)
                .map_err(|error| format!("Failed to get default audio endpoint: {error}"))?;
            let volume_interface = default_device
                .Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None)
                .map_err(|error| format!("Failed to activate audio endpoint volume: {error}"))?;

            volume_interface
                .SetMute(mute, std::ptr::null())
                .map_err(|error| format!("Failed to change system mute: {error}"))?;
        }
        Ok(())
    }

    #[cfg(target_os = "linux")]
    {
        use std::process::Command;

        let mute_val = if mute { "1" } else { "0" };
        let amixer_state = if mute { "mute" } else { "unmute" };

        // Try multiple backends to increase compatibility
        // 1. PipeWire (wpctl)
        if Command::new("wpctl")
            .args(["set-mute", "@DEFAULT_AUDIO_SINK@", mute_val])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
        {
            return Ok(());
        }

        // 2. PulseAudio (pactl)
        if Command::new("pactl")
            .args(["set-sink-mute", "@DEFAULT_SINK@", mute_val])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
        {
            return Ok(());
        }

        // 3. ALSA (amixer)
        let output = Command::new("amixer")
            .args(["set", "Master", amixer_state])
            .output()
            .map_err(|error| format!("Failed to run a system mute command: {error}"))?;
        if output.status.success() {
            Ok(())
        } else {
            Err(format!(
                "System mute command failed with status {}",
                output.status
            ))
        }
    }

    #[cfg(target_os = "macos")]
    {
        use std::process::Command;
        let script = format!(
            "set volume output muted {}",
            if mute { "true" } else { "false" }
        );
        let output = Command::new("osascript")
            .args(["-e", &script])
            .output()
            .map_err(|error| format!("Failed to run the system mute script: {error}"))?;
        if output.status.success() {
            Ok(())
        } else {
            Err(format!(
                "System mute script failed with status {}",
                output.status
            ))
        }
    }

    #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
    Ok(())
}

const WHISPER_SAMPLE_RATE: usize = 16000;

/* ──────────────────────────────────────────────────────────────── */

#[derive(Clone, Debug)]
pub enum RecordingState {
    Idle,
    Recording { binding_id: String },
}

#[derive(Clone, Debug)]
pub enum MicrophoneMode {
    AlwaysOn,
    OnDemand,
}

fn should_emit_mic_level(is_recording: bool, monitoring_requested: bool) -> bool {
    is_recording || monitoring_requested
}

/* ──────────────────────────────────────────────────────────────── */

/// Serializes microphone stream and system-mute transitions.
///
/// When a method also needs another manager lock, it takes this lifecycle gate
/// first. Native volume work runs while the gate is held, but never while a
/// state, recorder, mode, monitoring, or recording mutex is held.
#[derive(Default)]
struct AudioLifecycle {
    operation: Mutex<()>,
    is_open: AtomicBool,
    did_mute: AtomicBool,
}

struct AudioLifecycleGuard<'a> {
    lifecycle: &'a AudioLifecycle,
    _operation: MutexGuard<'a, ()>,
}

impl AudioLifecycle {
    fn lock(&self) -> AudioLifecycleGuard<'_> {
        AudioLifecycleGuard {
            lifecycle: self,
            _operation: self
                .operation
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()),
        }
    }

    fn is_open(&self) -> bool {
        self.is_open.load(Ordering::SeqCst)
    }

    #[cfg(test)]
    fn did_mute(&self) -> bool {
        self.did_mute.load(Ordering::SeqCst)
    }
}

impl AudioLifecycleGuard<'_> {
    fn is_open(&self) -> bool {
        self.lifecycle.is_open()
    }

    fn open<E>(&self, open: impl FnOnce() -> Result<(), E>) -> Result<bool, E> {
        if self.is_open() {
            return Ok(false);
        }

        open()?;
        self.lifecycle.is_open.store(true, Ordering::SeqCst);
        Ok(true)
    }

    fn close<E>(
        &self,
        unmute: impl FnOnce() -> Result<(), E>,
        close: impl FnOnce(),
    ) -> Result<bool, E> {
        if !self.is_open() {
            return Ok(false);
        }

        let did_mute = self.lifecycle.did_mute.load(Ordering::SeqCst);
        let unmute_result = if did_mute { unmute() } else { Ok(()) };
        if did_mute && unmute_result.is_ok() {
            self.lifecycle.did_mute.store(false, Ordering::SeqCst);
        }

        close();
        self.lifecycle.is_open.store(false, Ordering::SeqCst);
        unmute_result.map(|()| true)
    }

    fn apply_mute<E>(
        &self,
        enabled: bool,
        mute: impl FnOnce() -> Result<(), E>,
    ) -> Result<bool, E> {
        if !enabled || !self.is_open() || self.lifecycle.did_mute.load(Ordering::SeqCst) {
            return Ok(false);
        }

        mute()?;
        self.lifecycle.did_mute.store(true, Ordering::SeqCst);
        Ok(true)
    }

    fn remove_mute<E>(&self, unmute: impl FnOnce() -> Result<(), E>) -> Result<bool, E> {
        if !self.lifecycle.did_mute.load(Ordering::SeqCst) {
            return Ok(false);
        }

        unmute()?;
        self.lifecycle.did_mute.store(false, Ordering::SeqCst);
        Ok(true)
    }
}

#[derive(Debug)]
struct ActiveMuteIntent {
    binding_id: String,
    generation: u64,
}

/// Identifies the recording that is allowed to run a delayed mute operation.
///
/// Audio feedback runs on another thread. A recording can stop while that
/// thread is still sleeping or playing the start sound, so an open stream is
/// not enough proof that mute still belongs to the same recording.
#[derive(Default)]
struct RecordingMuteIntent {
    next_generation: AtomicU64,
    active: Mutex<Option<ActiveMuteIntent>>,
}

impl RecordingMuteIntent {
    fn arm(&self, binding_id: &str) -> u64 {
        let generation = self.next_generation.fetch_add(1, Ordering::SeqCst) + 1;
        *self
            .active
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(ActiveMuteIntent {
            binding_id: binding_id.to_string(),
            generation,
        });
        generation
    }

    fn is_active(&self, generation: u64) -> bool {
        self.active
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .as_ref()
            .is_some_and(|active| active.generation == generation)
    }

    fn disarm(&self, binding_id: &str) -> bool {
        let mut active = self
            .active
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if active
            .as_ref()
            .is_some_and(|intent| intent.binding_id == binding_id)
        {
            *active = None;
            true
        } else {
            false
        }
    }
}

/* ──────────────────────────────────────────────────────────────── */

/// Returns the VAD model path to hand to the VAD engine.
///
/// The path stays a `Path` end to end. The previous handoff round-tripped
/// through `&str` (`.to_str().unwrap()`), which loses paths that are not
/// valid UTF-8 and broke model loading on Windows profiles with non-ASCII
/// characters in the user path (issue #56).
fn vad_engine_path(vad_path: &Path) -> Result<&Path, anyhow::Error> {
    if !vad_path.exists() {
        return Err(anyhow::anyhow!(
            "VAD model not found at {}",
            vad_path.display()
        ));
    }
    Ok(vad_path)
}

fn create_audio_recorder(
    vad_path: &Path,
    app_handle: &tauri::AppHandle,
    is_recording: Arc<Mutex<bool>>,
    monitoring_requested: Arc<AtomicBool>,
) -> Result<AudioRecorder, anyhow::Error> {
    let silero = SileroVad::new(vad_engine_path(vad_path)?, 0.3)
        .map_err(|e| anyhow::anyhow!("Failed to create SileroVad: {}", e))?;
    let smoothed_vad = SmoothedVad::new(Box::new(silero), 15, 15, 2);

    // Recorder with VAD plus a spectrum-level callback that forwards updates to
    // the frontend.
    let recorder = AudioRecorder::new()
        .map_err(|e| anyhow::anyhow!("Failed to create AudioRecorder: {}", e))?
        .with_vad(Box::new(smoothed_vad))
        .with_level_callback({
            let app_handle = app_handle.clone();
            move |levels| {
                let is_recording = *is_recording.lock().unwrap();
                let monitoring_requested = monitoring_requested.load(Ordering::Relaxed);
                if should_emit_mic_level(is_recording, monitoring_requested) {
                    utils::emit_levels(&app_handle, &levels);
                }
            }
        });

    Ok(recorder)
}

/* ──────────────────────────────────────────────────────────────── */

#[derive(Clone)]
pub struct AudioRecordingManager {
    state: Arc<Mutex<RecordingState>>,
    mode: Arc<Mutex<MicrophoneMode>>,
    app_handle: tauri::AppHandle,

    recorder: Arc<Mutex<Option<AudioRecorder>>>,
    lifecycle: Arc<AudioLifecycle>,
    is_recording: Arc<Mutex<bool>>,
    // True when the live settings-screen level meter opened the stream itself
    // (so it knows it owns the close on the way out).
    monitoring: Arc<Mutex<bool>>,
    // True whenever the settings meter requested live levels, including when an
    // always-on stream was already open and the meter does not own that stream.
    monitoring_requested: Arc<AtomicBool>,
    close_generation: Arc<AtomicU64>,
    mute_intent: Arc<RecordingMuteIntent>,
}

impl AudioRecordingManager {
    /* ---------- construction ------------------------------------------------ */

    pub fn new(app: &tauri::AppHandle) -> Result<Self, anyhow::Error> {
        let settings = get_settings(app);
        let mode = if settings.always_on_microphone {
            MicrophoneMode::AlwaysOn
        } else {
            MicrophoneMode::OnDemand
        };

        let manager = Self {
            state: Arc::new(Mutex::new(RecordingState::Idle)),
            mode: Arc::new(Mutex::new(mode.clone())),
            app_handle: app.clone(),

            recorder: Arc::new(Mutex::new(None)),
            lifecycle: Arc::new(AudioLifecycle::default()),
            is_recording: Arc::new(Mutex::new(false)),
            monitoring: Arc::new(Mutex::new(false)),
            monitoring_requested: Arc::new(AtomicBool::new(false)),
            close_generation: Arc::new(AtomicU64::new(0)),
            mute_intent: Arc::new(RecordingMuteIntent::default()),
        };

        // Always-on?  Open immediately.
        if matches!(mode, MicrophoneMode::AlwaysOn) {
            manager.start_microphone_stream()?;
        }

        Ok(manager)
    }

    /* ---------- helper methods --------------------------------------------- */

    fn get_effective_microphone_device(&self, settings: &AppSettings) -> Option<cpal::Device> {
        // Check if we're in clamshell mode and have a clamshell microphone configured
        let use_clamshell_mic = if let Ok(is_clamshell) = clamshell::is_clamshell() {
            is_clamshell && settings.clamshell_microphone.is_some()
        } else {
            false
        };

        let device_name = if use_clamshell_mic {
            settings.clamshell_microphone.as_ref().unwrap()
        } else {
            settings.selected_microphone.as_ref()?
        };

        // Find the device by name
        match list_input_devices() {
            Ok(devices) => devices
                .into_iter()
                .find(|d| d.name == *device_name)
                .map(|d| d.device),
            Err(e) => {
                debug!("Failed to list devices, using default: {}", e);
                None
            }
        }
    }

    fn schedule_lazy_close(&self) {
        let gen = self.close_generation.fetch_add(1, Ordering::SeqCst) + 1;
        let app = self.app_handle.clone();
        std::thread::spawn(move || {
            std::thread::sleep(STREAM_IDLE_TIMEOUT);
            let rm = app.state::<Arc<AudioRecordingManager>>();
            // The lifecycle gate serializes this check and close against a new
            // recording. Release the state mutex before native volume work.
            let lifecycle = rm.lifecycle.lock();
            let is_idle = matches!(*rm.state.lock().unwrap(), RecordingState::Idle);
            if rm.close_generation.load(Ordering::SeqCst) == gen && is_idle {
                info!(
                    "Closing idle microphone stream after {:?}",
                    STREAM_IDLE_TIMEOUT
                );
                rm.stop_microphone_stream_locked(&lifecycle);
            }
        });
    }

    /* ---------- microphone life-cycle -------------------------------------- */

    /// Applies mute only while the recording that scheduled it is still active.
    pub fn apply_mute_for_recording(&self, generation: u64) {
        let settings = get_settings(&self.app_handle);
        let lifecycle = self.lifecycle.lock();
        if !self.mute_intent.is_active(generation) {
            debug!("Skipped delayed mute for inactive recording generation {generation}");
            return;
        }
        match lifecycle.apply_mute(settings.mute_while_recording, || set_mute(true)) {
            Ok(true) => debug!("Mute applied"),
            Ok(false) => {}
            Err(error) => error!("Failed to apply system mute: {error}"),
        }
    }

    /// Invalidates delayed mute work and removes mute for this recording.
    pub fn finish_mute_for_recording(&self, binding_id: &str) {
        let lifecycle = self.lifecycle.lock();
        self.finish_mute_for_recording_locked(binding_id, &lifecycle);
    }

    fn finish_mute_for_recording_locked(
        &self,
        binding_id: &str,
        lifecycle: &AudioLifecycleGuard<'_>,
    ) {
        if !self.mute_intent.disarm(binding_id) {
            return;
        }
        match lifecycle.remove_mute(|| set_mute(false)) {
            Ok(true) => debug!("Mute removed"),
            Ok(false) => {}
            Err(error) => error!("Failed to remove system mute: {error}"),
        }
    }

    pub fn preload_vad(&self) -> Result<(), anyhow::Error> {
        let lifecycle = self.lifecycle.lock();
        self.preload_vad_locked(&lifecycle)
    }

    fn preload_vad_locked(
        &self,
        _lifecycle: &AudioLifecycleGuard<'_>,
    ) -> Result<(), anyhow::Error> {
        let mut recorder_opt = self.recorder.lock().unwrap();
        if recorder_opt.is_none() {
            let vad_path = self
                .app_handle
                .path()
                .resolve(
                    "resources/models/silero_vad_v4.onnx",
                    tauri::path::BaseDirectory::Resource,
                )
                .map_err(|e| anyhow::anyhow!("Failed to resolve VAD path: {}", e))?;
            *recorder_opt = Some(create_audio_recorder(
                &vad_path,
                &self.app_handle,
                Arc::clone(&self.is_recording),
                Arc::clone(&self.monitoring_requested),
            )?);
        }
        Ok(())
    }

    pub fn start_microphone_stream(&self) -> Result<(), anyhow::Error> {
        let lifecycle = self.lifecycle.lock();
        self.start_microphone_stream_locked(&lifecycle)
    }

    fn start_microphone_stream_locked(
        &self,
        lifecycle: &AudioLifecycleGuard<'_>,
    ) -> Result<(), anyhow::Error> {
        if lifecycle.is_open() {
            debug!("Microphone stream already active");
            return Ok(());
        }

        let start_time = Instant::now();
        lifecycle.open(|| {
            // Get the selected device from settings, considering clamshell mode.
            let settings = get_settings(&self.app_handle);
            let selected_device = self.get_effective_microphone_device(&settings);

            // If no device was selected and none exist, return a clear error
            // instead of a backend-specific cpal error.
            if selected_device.is_none() {
                let has_any_device = list_input_devices()
                    .map(|devices| !devices.is_empty())
                    .unwrap_or(false);
                if !has_any_device {
                    return Err(anyhow::anyhow!("No input device found"));
                }
            }

            self.preload_vad_locked(lifecycle)?;
            if let Some(recorder) = self.recorder.lock().unwrap().as_mut() {
                recorder
                    .open(selected_device)
                    .map_err(|error| anyhow::anyhow!("Failed to open recorder: {error}"))?;
            }
            Ok(())
        })?;

        // This timing covers through cpal's stream.play() returning — i.e. the
        // point cpal surfaces as "stream running." It does NOT guarantee the
        // host audio device is producing samples yet; the first input callback
        // fires asynchronously one buffer period later (hardware dependent,
        // typically ~10–200ms on macOS, longer on Bluetooth/USB).
        info!(
            "Microphone stream initialized in {:?}",
            start_time.elapsed()
        );
        Ok(())
    }

    fn stop_microphone_stream_locked(&self, lifecycle: &AudioLifecycleGuard<'_>) {
        let close_result = lifecycle.close(
            || set_mute(false),
            || {
                if let Some(recorder) = self.recorder.lock().unwrap().as_mut() {
                    // If still recording, stop first.
                    {
                        let mut is_recording = self.is_recording.lock().unwrap();
                        if *is_recording {
                            let _ = recorder.stop();
                            *is_recording = false;
                        }
                    }
                    let _ = recorder.close();
                }
            },
        );
        match close_result {
            Ok(true) => debug!("Microphone stream stopped"),
            Ok(false) => {}
            Err(error) => {
                error!("Failed to remove system mute while closing microphone stream: {error}");
                debug!("Microphone stream stopped");
            }
        }
    }

    /* ---------- live level monitoring (settings meter) --------------------- */

    /// Toggle the live input meter used by the settings screen. Enabling opens
    /// the mic stream (which makes `mic-level` events flow) without starting a
    /// recording; disabling closes the stream only if this monitor opened it and
    /// nothing else still needs it (no active recording, not always-on mode).
    pub fn set_monitoring(&self, enable: bool) -> Result<(), anyhow::Error> {
        let lifecycle = self.lifecycle.lock();
        if enable {
            self.monitoring_requested.store(true, Ordering::Relaxed);
            // Cancel any pending lazy close from a just-finished recording so our
            // fresh monitor stream is not torn down underneath us.
            self.close_generation.fetch_add(1, Ordering::SeqCst);
            // We own the close only if the stream was not already open for some
            // other reason (always-on mode, or a recording in progress).
            let already_open = lifecycle.is_open();
            *self.monitoring.lock().unwrap() = !already_open;
            if !already_open {
                if let Err(error) = self.start_microphone_stream_locked(&lifecycle) {
                    *self.monitoring.lock().unwrap() = false;
                    self.monitoring_requested.store(false, Ordering::Relaxed);
                    return Err(error);
                }
            }
        } else {
            self.monitoring_requested.store(false, Ordering::Relaxed);
            let we_opened = *self.monitoring.lock().unwrap();
            *self.monitoring.lock().unwrap() = false;
            let recording = *self.is_recording.lock().unwrap();
            let on_demand = matches!(*self.mode.lock().unwrap(), MicrophoneMode::OnDemand);
            // Never close a stream we did not open, one feeding a live recording,
            // or the persistent always-on stream.
            if we_opened && !recording && on_demand {
                self.stop_microphone_stream_locked(&lifecycle);
            }
        }
        Ok(())
    }

    /* ---------- mode switching --------------------------------------------- */

    pub fn update_mode(&self, new_mode: MicrophoneMode) -> Result<(), anyhow::Error> {
        let lifecycle = self.lifecycle.lock();
        let cur_mode = self.mode.lock().unwrap().clone();

        match (cur_mode, &new_mode) {
            (MicrophoneMode::AlwaysOn, MicrophoneMode::OnDemand) => {
                if matches!(*self.state.lock().unwrap(), RecordingState::Idle) {
                    self.close_generation.fetch_add(1, Ordering::SeqCst);
                    self.stop_microphone_stream_locked(&lifecycle);
                }
            }
            (MicrophoneMode::OnDemand, MicrophoneMode::AlwaysOn) => {
                self.close_generation.fetch_add(1, Ordering::SeqCst);
                self.start_microphone_stream_locked(&lifecycle)?;
            }
            _ => {}
        }

        *self.mode.lock().unwrap() = new_mode;
        Ok(())
    }

    /* ---------- recording --------------------------------------------------- */

    pub fn try_start_recording(&self, binding_id: &str) -> Result<u64, String> {
        let lifecycle = self.lifecycle.lock();
        let mut state = self.state.lock().unwrap();

        if let RecordingState::Idle = *state {
            // Ensure microphone is open in on-demand mode
            if matches!(*self.mode.lock().unwrap(), MicrophoneMode::OnDemand) {
                // Cancel any pending lazy close
                self.close_generation.fetch_add(1, Ordering::SeqCst);
                if let Err(e) = self.start_microphone_stream_locked(&lifecycle) {
                    let msg = format!("{e}");
                    error!("Failed to open microphone stream: {msg}");
                    return Err(msg);
                }
            }

            if let Some(rec) = self.recorder.lock().unwrap().as_ref() {
                if rec.start().is_ok() {
                    *self.is_recording.lock().unwrap() = true;
                    *state = RecordingState::Recording {
                        binding_id: binding_id.to_string(),
                    };
                    let mute_generation = self.mute_intent.arm(binding_id);
                    debug!("Recording started for binding {binding_id}");
                    return Ok(mute_generation);
                }
            }
            Err("Recorder not available".to_string())
        } else {
            Err("Already recording".to_string())
        }
    }

    pub fn update_selected_device(&self) -> Result<(), anyhow::Error> {
        let lifecycle = self.lifecycle.lock();
        // If currently open, restart the microphone stream to use the new device
        if lifecycle.is_open() {
            self.close_generation.fetch_add(1, Ordering::SeqCst);
            self.stop_microphone_stream_locked(&lifecycle);
            self.start_microphone_stream_locked(&lifecycle)?;
        }
        Ok(())
    }

    pub fn stop_recording(&self, binding_id: &str) -> Option<Vec<f32>> {
        let lifecycle = self.lifecycle.lock();
        let mut state = self.state.lock().unwrap();

        match *state {
            RecordingState::Recording {
                binding_id: ref active,
            } if active == binding_id => {
                *state = RecordingState::Idle;
                drop(state);
                self.finish_mute_for_recording_locked(binding_id, &lifecycle);

                // Optionally keep recording for a bit longer to capture trailing audio
                let settings = get_settings(&self.app_handle);
                if settings.extra_recording_buffer_ms > 0 {
                    debug!(
                        "Extra recording buffer: sleeping {}ms before stopping",
                        settings.extra_recording_buffer_ms
                    );
                    std::thread::sleep(Duration::from_millis(settings.extra_recording_buffer_ms));
                }

                let samples = if let Some(rec) = self.recorder.lock().unwrap().as_ref() {
                    match rec.stop() {
                        Ok(buf) => buf,
                        Err(e) => {
                            error!("stop() failed: {e}");
                            Vec::new()
                        }
                    }
                } else {
                    error!("Recorder not available");
                    Vec::new()
                };

                *self.is_recording.lock().unwrap() = false;

                // In on-demand mode, close the mic (lazily if the setting is enabled)
                if matches!(*self.mode.lock().unwrap(), MicrophoneMode::OnDemand) {
                    if get_settings(&self.app_handle).lazy_stream_close {
                        self.schedule_lazy_close();
                    } else {
                        self.stop_microphone_stream_locked(&lifecycle);
                    }
                }

                // Pad if very short
                let s_len = samples.len();
                // debug!("Got {} samples", s_len);
                if s_len < WHISPER_SAMPLE_RATE && s_len > 0 {
                    let mut padded = samples;
                    padded.resize(WHISPER_SAMPLE_RATE * 5 / 4, 0.0);
                    Some(padded)
                } else {
                    Some(samples)
                }
            }
            _ => None,
        }
    }
    pub fn is_recording(&self) -> bool {
        matches!(
            *self.state.lock().unwrap(),
            RecordingState::Recording { .. }
        )
    }

    /// Cancel any ongoing recording without returning audio samples
    pub fn cancel_recording(&self) {
        let lifecycle = self.lifecycle.lock();
        let mut state = self.state.lock().unwrap();

        if let RecordingState::Recording { ref binding_id } = *state {
            let binding_id = binding_id.clone();
            *state = RecordingState::Idle;
            drop(state);
            self.finish_mute_for_recording_locked(&binding_id, &lifecycle);

            if let Some(rec) = self.recorder.lock().unwrap().as_ref() {
                let _ = rec.stop(); // Discard the result
            }

            *self.is_recording.lock().unwrap() = false;

            // In on-demand mode, close the mic (lazily if the setting is enabled)
            if matches!(*self.mode.lock().unwrap(), MicrophoneMode::OnDemand) {
                if get_settings(&self.app_handle).lazy_stream_close {
                    self.schedule_lazy_close();
                } else {
                    self.stop_microphone_stream_locked(&lifecycle);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{should_emit_mic_level, vad_engine_path, AudioLifecycle, RecordingMuteIntent};
    use std::ffi::OsString;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::Ordering;
    use std::sync::{mpsc, Arc};
    use std::time::Duration;

    const TEST_TIMEOUT: Duration = Duration::from_secs(2);

    /// Creates a directory named `dir_name` under the OS temp dir with an
    /// empty stand-in model file inside, mirroring a resource dir that lives
    /// under a user-profile path. Returns (dir, model_path).
    fn model_in_dir(dir_name: OsString) -> (PathBuf, PathBuf) {
        let dir = std::env::temp_dir().join(dir_name);
        fs::create_dir_all(&dir).expect("test setup: failed to create temp dir");
        let model = dir.join("silero_vad_v4.onnx");
        fs::File::create(&model).expect("test setup: failed to create stand-in model file");
        (dir, model)
    }

    #[test]
    fn mic_level_emission_requires_recording_or_monitor_session() {
        assert!(!should_emit_mic_level(false, false));
        assert!(should_emit_mic_level(true, false));
        assert!(should_emit_mic_level(false, true));
        assert!(should_emit_mic_level(true, true));
    }

    #[test]
    fn quick_stop_invalidates_delayed_mute() {
        let intent = RecordingMuteIntent::default();
        let generation = intent.arm("transcribe");

        assert!(intent.is_active(generation));
        assert!(intent.disarm("transcribe"));
        assert!(!intent.is_active(generation));
    }

    #[test]
    fn old_delayed_mute_cannot_attach_to_new_recording() {
        let intent = RecordingMuteIntent::default();
        let old_generation = intent.arm("transcribe");
        assert!(intent.disarm("transcribe"));

        let new_generation = intent.arm("transcribe");

        assert!(!intent.is_active(old_generation));
        assert!(intent.is_active(new_generation));
    }

    #[test]
    fn unrelated_stop_cannot_invalidate_active_mute() {
        let intent = RecordingMuteIntent::default();
        let generation = intent.arm("transcribe");

        assert!(!intent.disarm("transcribe-with-post-process"));
        assert!(intent.is_active(generation));
    }

    #[test]
    fn concurrent_open_and_close_are_serialized() {
        let lifecycle = Arc::new(AudioLifecycle::default());
        let (open_started_tx, open_started_rx) = mpsc::channel();
        let (release_open_tx, release_open_rx) = mpsc::channel();
        let (close_started_tx, close_started_rx) = mpsc::channel();

        let opening_lifecycle = Arc::clone(&lifecycle);
        let opening = std::thread::spawn(move || {
            let lifecycle = opening_lifecycle.lock();
            lifecycle
                .open(|| {
                    open_started_tx.send(()).unwrap();
                    release_open_rx.recv_timeout(TEST_TIMEOUT).unwrap();
                    Ok::<(), ()>(())
                })
                .unwrap();
        });

        open_started_rx.recv_timeout(TEST_TIMEOUT).unwrap();
        assert!(matches!(
            lifecycle.operation.try_lock(),
            Err(std::sync::TryLockError::WouldBlock)
        ));
        let closing_lifecycle = Arc::clone(&lifecycle);
        let closing = std::thread::spawn(move || {
            let lifecycle = closing_lifecycle.lock();
            lifecycle
                .close(|| Ok::<(), ()>(()), || close_started_tx.send(()).unwrap())
                .unwrap();
        });

        assert!(close_started_rx.try_recv().is_err());
        release_open_tx.send(()).unwrap();
        opening.join().unwrap();
        closing.join().unwrap();

        close_started_rx.recv_timeout(TEST_TIMEOUT).unwrap();
        assert!(!lifecycle.is_open());
    }

    #[test]
    fn concurrent_mute_and_unmute_are_serialized() {
        let lifecycle = Arc::new(AudioLifecycle::default());
        {
            let lifecycle = lifecycle.lock();
            lifecycle.open(|| Ok::<(), ()>(())).unwrap();
        }

        let (mute_started_tx, mute_started_rx) = mpsc::channel();
        let (release_mute_tx, release_mute_rx) = mpsc::channel();
        let (unmute_started_tx, unmute_started_rx) = mpsc::channel();

        let muting_lifecycle = Arc::clone(&lifecycle);
        let muting = std::thread::spawn(move || {
            let lifecycle = muting_lifecycle.lock();
            lifecycle
                .apply_mute(true, || {
                    mute_started_tx.send(()).unwrap();
                    release_mute_rx.recv_timeout(TEST_TIMEOUT).unwrap();
                    Ok::<(), ()>(())
                })
                .unwrap();
        });

        mute_started_rx.recv_timeout(TEST_TIMEOUT).unwrap();
        assert!(matches!(
            lifecycle.operation.try_lock(),
            Err(std::sync::TryLockError::WouldBlock)
        ));
        let unmuting_lifecycle = Arc::clone(&lifecycle);
        let unmuting = std::thread::spawn(move || {
            let lifecycle = unmuting_lifecycle.lock();
            lifecycle
                .remove_mute(|| {
                    unmute_started_tx.send(()).unwrap();
                    Ok::<(), ()>(())
                })
                .unwrap();
        });

        assert!(unmute_started_rx.try_recv().is_err());
        release_mute_tx.send(()).unwrap();
        muting.join().unwrap();
        unmuting.join().unwrap();

        unmute_started_rx.recv_timeout(TEST_TIMEOUT).unwrap();
        assert!(!lifecycle.did_mute());
    }

    #[test]
    fn failed_native_mute_does_not_commit_mute_state() {
        let lifecycle = AudioLifecycle::default();
        {
            let lifecycle = lifecycle.lock();
            lifecycle.open(|| Ok::<(), &str>(())).unwrap();
            assert_eq!(
                lifecycle.apply_mute(true, || Err("simulated mute failure")),
                Err("simulated mute failure")
            );
        }

        assert!(!lifecycle.did_mute());
        let lifecycle_guard = lifecycle.lock();
        assert_eq!(
            lifecycle_guard.apply_mute(true, || Ok::<(), &str>(())),
            Ok(true)
        );
        assert!(lifecycle.did_mute());
    }

    #[test]
    fn failed_unmute_during_close_is_retryable_after_stream_close() {
        let lifecycle = AudioLifecycle::default();
        let close_calls = std::sync::atomic::AtomicUsize::new(0);
        {
            let lifecycle = lifecycle.lock();
            lifecycle.open(|| Ok::<(), &str>(())).unwrap();
            lifecycle.apply_mute(true, || Ok::<(), &str>(())).unwrap();
            assert_eq!(
                lifecycle.close(
                    || Err("simulated unmute failure"),
                    || {
                        close_calls.fetch_add(1, Ordering::SeqCst);
                    },
                ),
                Err("simulated unmute failure")
            );
        }

        assert_eq!(close_calls.load(Ordering::SeqCst), 1);
        assert!(!lifecycle.is_open());
        assert!(lifecycle.did_mute());

        {
            let lifecycle_guard = lifecycle.lock();
            lifecycle_guard.open(|| Ok::<(), &str>(())).unwrap();
        }
        assert!(lifecycle.did_mute());

        {
            let lifecycle_guard = lifecycle.lock();
            assert_eq!(
                lifecycle_guard.remove_mute(|| Err("simulated retry failure")),
                Err("simulated retry failure")
            );
        }
        assert!(lifecycle.did_mute());

        {
            let lifecycle_guard = lifecycle.lock();
            assert_eq!(lifecycle_guard.remove_mute(|| Ok::<(), &str>(())), Ok(true));
        }
        assert!(!lifecycle.did_mute());
    }

    // Regression test for issue #56: a Windows profile path with Cyrillic,
    // CJK, or accented Latin characters must survive the VAD path handoff.
    #[test]
    fn vad_engine_path_preserves_unicode_paths() {
        let (dir, model) = model_in_dir(OsString::from(
            "audiobud-vad-\u{041f}\u{043e}\u{043b}\u{044c}-\u{7528}\u{6237}-An\u{e7}a",
        ));

        let handed_off =
            vad_engine_path(&model).expect("unicode path must be accepted, not rejected");
        assert_eq!(handed_off, model.as_path());

        fs::remove_dir_all(&dir).ok();
    }

    // A path that is not valid UTF-8 (possible on Windows and Linux) must be
    // handed to the engine untouched instead of panicking in a &str round-trip.
    // macOS is excluded: APFS rejects non-UTF-8 file names at creation.
    #[test]
    #[cfg(any(windows, target_os = "linux"))]
    fn vad_engine_path_survives_non_utf8_paths() {
        #[cfg(windows)]
        let dir_name = {
            use std::os::windows::ffi::OsStringExt;
            let mut wide: Vec<u16> = "audiobud-vad-".encode_utf16().collect();
            wide.push(0xD800); // unpaired surrogate: valid in Windows paths, not valid UTF-8
            OsString::from_wide(&wide)
        };
        #[cfg(target_os = "linux")]
        let dir_name = {
            use std::os::unix::ffi::OsStringExt;
            OsString::from_vec(b"audiobud-vad-\xff".to_vec())
        };

        let (dir, model) = model_in_dir(dir_name);
        assert!(
            model.to_str().is_none(),
            "test setup: path was expected to be non-UTF-8"
        );

        let handed_off =
            vad_engine_path(&model).expect("non-UTF-8 path must be accepted, not rejected");
        assert_eq!(handed_off, model.as_path());

        fs::remove_dir_all(&dir).ok();
    }

    // End-to-end check for issue #56's reported case: a VALID Unicode
    // (Cyrillic + CJK + accented Latin) directory, exercised through the real
    // ONNX load stack (vad-rs -> ort -> onnxruntime CreateSession) — the same
    // file-open call the Parakeet engine sessions use. CI downloads the model
    // (see .github/workflows/ci.yml) and the test fails there if it is absent —
    // a silent skip would let the regression test pass without running. Local
    // checkouts without the model skip with a notice (AGENTS.md model setup).
    #[test]
    fn silero_vad_loads_from_unicode_directory() {
        use crate::audio_toolkit::SileroVad;

        let source =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/models/silero_vad_v4.onnx");
        if !source.is_file() {
            assert!(
                std::env::var_os("CI").is_none(),
                "silero_vad_v4.onnx is missing in CI; the workflow must download it before cargo test"
            );
            eprintln!(
                "skipping silero_vad_loads_from_unicode_directory: \
                 silero_vad_v4.onnx not downloaded (see AGENTS.md model setup)"
            );
            return;
        }

        let dir = std::env::temp_dir().join(
            "audiobud-vad-\u{0410}\u{043b}\u{0435}\u{043a}\u{0441}\u{0430}\u{043d}\u{0434}\u{0440}-\u{7528}\u{6237}-Fran\u{e7}aise",
        );
        fs::create_dir_all(&dir).expect("test setup: failed to create unicode temp dir");
        let model = dir.join("silero_vad_v4.onnx");
        fs::copy(&source, &model).expect("test setup: failed to copy VAD model");

        let result = SileroVad::new(&model, 0.3);
        fs::remove_dir_all(&dir).ok();
        result.expect("silero VAD must load from a unicode (Cyrillic/CJK) directory");
    }

    #[test]
    fn vad_engine_path_reports_missing_model() {
        let missing = std::env::temp_dir().join("audiobud-vad-missing/silero_vad_v4.onnx");
        let err = vad_engine_path(&missing).expect_err("missing model file must be an error");
        assert!(err.to_string().contains("VAD model not found"));
    }
}
