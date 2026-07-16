use crate::audio_toolkit::{apply_custom_words, apply_replacements, filter_transcription_output};
use crate::managers::audio::AudioRecordingManager;
use crate::managers::model::{EngineType, ModelManager};
use crate::settings::{
    get_settings, ModelUnloadTimeout, OrtAcceleratorSetting, WhisperAcceleratorSetting,
};
use anyhow::Result;
use log::{debug, error, info, warn};
use serde::Serialize;
use specta::Type;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock};
use std::thread;
use std::time::{Duration, SystemTime};
use tauri::{AppHandle, Emitter, Manager};
use transcribe_rs::{
    onnx::{
        canary::CanaryModel,
        cohere::CohereModel,
        gigaam::GigaAMModel,
        moonshine::{MoonshineModel, MoonshineVariant, StreamingModel},
        parakeet::{ParakeetModel, ParakeetParams, TimestampGranularity},
        sense_voice::{SenseVoiceModel, SenseVoiceParams},
        Quantization,
    },
    whisper_cpp::{WhisperEngine, WhisperInferenceParams},
    SpeechModel, TranscribeOptions,
};

#[derive(Clone, Debug, Serialize)]
pub struct ModelStateEvent {
    pub event_type: String,
    pub model_id: Option<String>,
    pub model_name: Option<String>,
    pub error: Option<String>,
}

enum LoadedEngine {
    Whisper(WhisperEngine),
    Parakeet(ParakeetModel),
    Moonshine(MoonshineModel),
    MoonshineStreaming(StreamingModel),
    SenseVoice(SenseVoiceModel),
    GigaAM(GigaAMModel),
    Canary(CanaryModel),
    Cohere(CohereModel),
}

/// Inner state of a [`GenerationGate`]: the stored value plus a counter that
/// bumps on every install/clear.
struct GateInner<T> {
    value: Option<T>,
    generation: u64,
}

/// Slot for the loaded engine, guarded by a generation counter (issue #58).
///
/// A transcription takes the engine out of the slot and puts it back when it
/// finishes. If the call wedges and its watchdog fires, the engine can come
/// back much later — after the model was unloaded or a different one loaded.
/// Every install/clear bumps the generation, and a take records the
/// generation observed, so a late restore whose generation no longer matches
/// is rejected and the stale engine is dropped instead of clobbering the slot.
pub(crate) struct GenerationGate<T> {
    inner: Mutex<GateInner<T>>,
}

impl<T> GenerationGate<T> {
    pub(crate) fn new() -> Self {
        Self {
            inner: Mutex::new(GateInner {
                value: None,
                generation: 0,
            }),
        }
    }

    /// Lock the slot, recovering from poison if a holder panicked.
    fn lock(&self) -> MutexGuard<'_, GateInner<T>> {
        self.inner.lock().unwrap_or_else(|poisoned| {
            warn!("Engine slot mutex was poisoned by a previous panic, recovering");
            poisoned.into_inner()
        })
    }

    pub(crate) fn is_occupied(&self) -> bool {
        self.lock().value.is_some()
    }

    /// Put a new value in the slot, replacing any previous one.
    pub(crate) fn install(&self, value: T) {
        let mut inner = self.lock();
        inner.generation += 1;
        inner.value = Some(value);
    }

    /// Empty the slot. Bumps the generation even when already empty, so a
    /// value currently taken out (a running transcription) cannot be restored
    /// after the unload.
    pub(crate) fn clear(&self) {
        let mut inner = self.lock();
        inner.generation += 1;
        inner.value = None;
    }

    /// Take the value out together with the generation observed at take time.
    pub(crate) fn take(&self) -> Option<(T, u64)> {
        let mut inner = self.lock();
        let generation = inner.generation;
        inner.value.take().map(|value| (value, generation))
    }

    /// Put a taken value back only if the slot generation is unchanged since
    /// the take. Returns `false` (dropping the stale value) if the slot was
    /// cleared or refilled in the meantime.
    #[must_use]
    pub(crate) fn try_restore(&self, value: T, taken_generation: u64) -> bool {
        let mut inner = self.lock();
        if inner.generation == taken_generation {
            inner.value = Some(value);
            true
        } else {
            false
        }
    }
}

/// RAII guard that clears the `is_loading` flag and notifies waiters on drop.
/// Ensures the loading flag is always reset, even on early returns or panics.
pub struct LoadingGuard {
    is_loading: Arc<Mutex<bool>>,
    loading_condvar: Arc<Condvar>,
}

impl Drop for LoadingGuard {
    fn drop(&mut self) {
        let mut is_loading = self.is_loading.lock().unwrap();
        *is_loading = false;
        self.loading_condvar.notify_all();
    }
}

/// Error used when a transcription or model load is refused because an
/// earlier transcription timed out and its worker still holds an engine.
const WEDGED_ENGINE_ERROR: &str = "A previous transcription timed out and its engine is still \
     busy. Restart AudioBud to recover.";

#[derive(Clone)]
pub struct TranscriptionManager {
    engine: Arc<GenerationGate<LoadedEngine>>,
    model_manager: Arc<ModelManager>,
    app_handle: AppHandle,
    current_model_id: Arc<Mutex<Option<String>>>,
    last_activity: Arc<AtomicU64>,
    shutdown_signal: Arc<AtomicBool>,
    watcher_handle: Arc<Mutex<Option<thread::JoinHandle<()>>>>,
    is_loading: Arc<Mutex<bool>>,
    loading_condvar: Arc<Condvar>,
    /// Number of transcribe calls whose watchdog fired and whose worker has
    /// not resolved yet (issue #58). While nonzero, new transcriptions and
    /// model loads are refused so retries cannot stack additional engines on
    /// top of the one the wedged worker still holds.
    wedged_workers: Arc<AtomicUsize>,
}

impl TranscriptionManager {
    pub fn new(app_handle: &AppHandle, model_manager: Arc<ModelManager>) -> Result<Self> {
        let manager = Self {
            engine: Arc::new(GenerationGate::new()),
            model_manager,
            app_handle: app_handle.clone(),
            current_model_id: Arc::new(Mutex::new(None)),
            last_activity: Arc::new(AtomicU64::new(Self::now_ms())),
            shutdown_signal: Arc::new(AtomicBool::new(false)),
            watcher_handle: Arc::new(Mutex::new(None)),
            is_loading: Arc::new(Mutex::new(false)),
            loading_condvar: Arc::new(Condvar::new()),
            wedged_workers: Arc::new(AtomicUsize::new(0)),
        };

        // Start the idle watcher
        {
            let app_handle_cloned = app_handle.clone();
            let manager_cloned = manager.clone();
            let shutdown_signal = manager.shutdown_signal.clone();
            let handle = thread::spawn(move || {
                debug!("Idle watcher thread started");
                while !shutdown_signal.load(Ordering::Relaxed) {
                    thread::sleep(Duration::from_secs(10)); // Check every 10 seconds

                    // Check shutdown signal again after sleep
                    if shutdown_signal.load(Ordering::Relaxed) {
                        break;
                    }

                    let settings = get_settings(&app_handle_cloned);
                    let timeout = settings.model_unload_timeout;

                    // Skip Immediately — that variant is handled by
                    // maybe_unload_immediately() after each transcription.
                    // Treating it as 0s here would unload the model mid-recording.
                    if timeout == ModelUnloadTimeout::Immediately {
                        continue;
                    }

                    // While recording, keep the idle timer fresh so the
                    // model is never unloaded mid-session.
                    let is_recording = app_handle_cloned
                        .try_state::<Arc<AudioRecordingManager>>()
                        .map_or(false, |a| a.is_recording());
                    if is_recording {
                        manager_cloned.touch_activity();
                        continue;
                    }

                    if let Some(limit_seconds) = timeout.to_seconds() {
                        let last = manager_cloned.last_activity.load(Ordering::Relaxed);
                        let now_ms = TranscriptionManager::now_ms();
                        let idle_ms = now_ms.saturating_sub(last);
                        let limit_ms = limit_seconds * 1000;

                        if idle_ms > limit_ms {
                            // idle -> unload
                            if manager_cloned.is_model_loaded() {
                                let unload_start = std::time::Instant::now();
                                info!(
                                    "Model idle for {}s (limit: {}s), unloading",
                                    idle_ms / 1000,
                                    limit_seconds
                                );
                                match manager_cloned.unload_model() {
                                    Ok(()) => {
                                        let unload_duration = unload_start.elapsed();
                                        info!(
                                            "Model unloaded due to inactivity (took {}ms)",
                                            unload_duration.as_millis()
                                        );
                                    }
                                    Err(e) => {
                                        error!("Failed to unload idle model: {}", e);
                                    }
                                }
                            }
                        }
                    }
                }
                debug!("Idle watcher thread shutting down gracefully");
            });
            *manager.watcher_handle.lock().unwrap() = Some(handle);
        }

        Ok(manager)
    }

    pub fn is_model_loaded(&self) -> bool {
        self.engine.is_occupied()
    }

    /// Whether an earlier transcription timed out and its worker is still
    /// running (holding an engine). See `wedged_workers`.
    pub fn is_wedged(&self) -> bool {
        self.wedged_workers.load(Ordering::SeqCst) > 0
    }

    /// Atomically check whether a model load is in progress and, if not, mark
    /// one as starting. Returns a [`LoadingGuard`] whose [`Drop`] impl will
    /// clear the flag and wake waiters. Returns `None` if a load is already in
    /// progress.
    pub fn try_start_loading(&self) -> Option<LoadingGuard> {
        let mut is_loading = self.is_loading.lock().unwrap();
        if *is_loading {
            return None;
        }
        *is_loading = true;
        Some(LoadingGuard {
            is_loading: self.is_loading.clone(),
            loading_condvar: self.loading_condvar.clone(),
        })
    }

    pub fn unload_model(&self) -> Result<()> {
        let unload_start = std::time::Instant::now();
        debug!("Starting to unload model");

        // Dropping the engine frees all resources. clear() also bumps the
        // slot generation, so a transcription currently holding the engine
        // cannot restore it after this unload.
        self.engine.clear();
        {
            let mut current_model = self.current_model_id.lock().unwrap();
            *current_model = None;
        }

        // Emit unloaded event
        let _ = self.app_handle.emit(
            "model-state-changed",
            ModelStateEvent {
                event_type: "unloaded".to_string(),
                model_id: None,
                model_name: None,
                error: None,
            },
        );

        let unload_duration = unload_start.elapsed();
        debug!(
            "Model unloaded manually (took {}ms)",
            unload_duration.as_millis()
        );
        Ok(())
    }

    fn now_ms() -> u64 {
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
    }

    /// Reset the idle timer to now.
    fn touch_activity(&self) {
        self.last_activity.store(Self::now_ms(), Ordering::Relaxed);
    }

    /// Unloads the model immediately if the setting is enabled and the model is loaded
    pub fn maybe_unload_immediately(&self, context: &str) {
        let settings = get_settings(&self.app_handle);
        if settings.model_unload_timeout == ModelUnloadTimeout::Immediately
            && self.is_model_loaded()
        {
            info!("Immediately unloading model after {}", context);
            if let Err(e) = self.unload_model() {
                warn!("Failed to immediately unload model: {}", e);
            }
        }
    }

    pub fn load_model(&self, model_id: &str) -> Result<()> {
        let load_start = std::time::Instant::now();
        debug!("Starting to load model: {}", model_id);

        // Refuse to stack another engine while a wedged transcription still
        // holds one (issue #58) — repeated retries would otherwise pile up
        // engines and exhaust VRAM/RAM. The loading_failed event surfaces the
        // refusal as the usual model-load toast.
        if self.is_wedged() {
            let _ = self.app_handle.emit(
                "model-state-changed",
                ModelStateEvent {
                    event_type: "loading_failed".to_string(),
                    model_id: Some(model_id.to_string()),
                    model_name: None,
                    error: Some(WEDGED_ENGINE_ERROR.to_string()),
                },
            );
            return Err(anyhow::anyhow!(WEDGED_ENGINE_ERROR));
        }

        // Emit loading started event
        let _ = self.app_handle.emit(
            "model-state-changed",
            ModelStateEvent {
                event_type: "loading_started".to_string(),
                model_id: Some(model_id.to_string()),
                model_name: None,
                error: None,
            },
        );

        let model_info = self
            .model_manager
            .get_model_info(model_id)
            .ok_or_else(|| anyhow::anyhow!("Model not found: {}", model_id))?;

        if !model_info.is_downloaded {
            let error_msg = "Model not downloaded";
            let _ = self.app_handle.emit(
                "model-state-changed",
                ModelStateEvent {
                    event_type: "loading_failed".to_string(),
                    model_id: Some(model_id.to_string()),
                    model_name: Some(model_info.name.clone()),
                    error: Some(error_msg.to_string()),
                },
            );
            return Err(anyhow::anyhow!(error_msg));
        }

        let model_path = self.model_manager.get_model_path(model_id)?;

        // Create appropriate engine based on model type
        let emit_loading_failed = |error_msg: &str| {
            let _ = self.app_handle.emit(
                "model-state-changed",
                ModelStateEvent {
                    event_type: "loading_failed".to_string(),
                    model_id: Some(model_id.to_string()),
                    model_name: Some(model_info.name.clone()),
                    error: Some(error_msg.to_string()),
                },
            );
        };

        let loaded_engine = match model_info.engine_type {
            EngineType::Whisper => {
                let engine = WhisperEngine::load(&model_path).map_err(|e| {
                    let error_msg = format!("Failed to load whisper model {}: {}", model_id, e);
                    emit_loading_failed(&error_msg);
                    anyhow::anyhow!(error_msg)
                })?;
                LoadedEngine::Whisper(engine)
            }
            EngineType::Parakeet => {
                let engine =
                    ParakeetModel::load(&model_path, &Quantization::Int8).map_err(|e| {
                        let error_msg =
                            format!("Failed to load parakeet model {}: {}", model_id, e);
                        emit_loading_failed(&error_msg);
                        anyhow::anyhow!(error_msg)
                    })?;
                LoadedEngine::Parakeet(engine)
            }
            EngineType::Moonshine => {
                let engine = MoonshineModel::load(
                    &model_path,
                    MoonshineVariant::Base,
                    &Quantization::default(),
                )
                .map_err(|e| {
                    let error_msg = format!("Failed to load moonshine model {}: {}", model_id, e);
                    emit_loading_failed(&error_msg);
                    anyhow::anyhow!(error_msg)
                })?;
                LoadedEngine::Moonshine(engine)
            }
            EngineType::MoonshineStreaming => {
                let engine = StreamingModel::load(&model_path, 0, &Quantization::default())
                    .map_err(|e| {
                        let error_msg = format!(
                            "Failed to load moonshine streaming model {}: {}",
                            model_id, e
                        );
                        emit_loading_failed(&error_msg);
                        anyhow::anyhow!(error_msg)
                    })?;
                LoadedEngine::MoonshineStreaming(engine)
            }
            EngineType::SenseVoice => {
                let engine =
                    SenseVoiceModel::load(&model_path, &Quantization::Int8).map_err(|e| {
                        let error_msg =
                            format!("Failed to load SenseVoice model {}: {}", model_id, e);
                        emit_loading_failed(&error_msg);
                        anyhow::anyhow!(error_msg)
                    })?;
                LoadedEngine::SenseVoice(engine)
            }
            EngineType::GigaAM => {
                let engine = GigaAMModel::load(&model_path, &Quantization::Int8).map_err(|e| {
                    let error_msg = format!("Failed to load gigaam model {}: {}", model_id, e);
                    emit_loading_failed(&error_msg);
                    anyhow::anyhow!(error_msg)
                })?;
                LoadedEngine::GigaAM(engine)
            }
            EngineType::Canary => {
                let engine = CanaryModel::load(&model_path, &Quantization::Int8).map_err(|e| {
                    let error_msg = format!("Failed to load canary model {}: {}", model_id, e);
                    emit_loading_failed(&error_msg);
                    anyhow::anyhow!(error_msg)
                })?;
                LoadedEngine::Canary(engine)
            }
            EngineType::Cohere => {
                let engine = CohereModel::load(&model_path, &Quantization::Int8).map_err(|e| {
                    let error_msg = format!("Failed to load cohere model {}: {}", model_id, e);
                    emit_loading_failed(&error_msg);
                    anyhow::anyhow!(error_msg)
                })?;
                LoadedEngine::Cohere(engine)
            }
        };

        // Update the current engine and model ID
        self.engine.install(loaded_engine);
        {
            let mut current_model = self.current_model_id.lock().unwrap();
            *current_model = Some(model_id.to_string());
        }

        // Reset idle timer so the watcher doesn't immediately unload a just-loaded model
        self.touch_activity();

        // Emit loading completed event
        let _ = self.app_handle.emit(
            "model-state-changed",
            ModelStateEvent {
                event_type: "loading_completed".to_string(),
                model_id: Some(model_id.to_string()),
                model_name: Some(model_info.name.clone()),
                error: None,
            },
        );

        let load_duration = load_start.elapsed();
        debug!(
            "Successfully loaded transcription model: {} (took {}ms)",
            model_id,
            load_duration.as_millis()
        );
        Ok(())
    }

    /// Kicks off the model loading in a background thread if it's not already loaded
    pub fn initiate_model_load(&self) {
        let mut is_loading = self.is_loading.lock().unwrap();
        if *is_loading || self.is_model_loaded() {
            return;
        }

        *is_loading = true;
        let self_clone = self.clone();
        thread::spawn(move || {
            let settings = get_settings(&self_clone.app_handle);
            if let Err(e) = self_clone.load_model(&settings.selected_model) {
                error!("Failed to load model: {}", e);
            }
            let mut is_loading = self_clone.is_loading.lock().unwrap();
            *is_loading = false;
            self_clone.loading_condvar.notify_all();
        });
    }

    pub fn get_current_model(&self) -> Option<String> {
        let current_model = self.current_model_id.lock().unwrap();
        current_model.clone()
    }

    pub fn transcribe(&self, audio: Vec<f32>) -> Result<String> {
        #[cfg(debug_assertions)]
        if std::env::var("HANDY_FORCE_TRANSCRIPTION_FAILURE").is_ok() {
            return Err(anyhow::anyhow!(
                "Simulated transcription failure (HANDY_FORCE_TRANSCRIPTION_FAILURE)"
            ));
        }

        // Manual repro hook for the issue #58 watchdog: simulate a wedged
        // engine so the timeout/recovery path can be exercised in a dev build.
        #[cfg(debug_assertions)]
        if std::env::var("HANDY_FORCE_TRANSCRIPTION_HANG").is_ok() {
            warn!("Simulating wedged engine (HANDY_FORCE_TRANSCRIPTION_HANG); blocking forever");
            loop {
                thread::sleep(Duration::from_secs(60));
            }
        }

        // Update last activity timestamp
        self.touch_activity();

        let st = std::time::Instant::now();

        debug!("Audio vector length: {}", audio.len());

        if audio.is_empty() {
            debug!("Empty audio vector");
            self.maybe_unload_immediately("empty audio");
            return Ok(String::new());
        }

        // Check if model is loaded, if not try to load it
        {
            // If the model is loading, wait for it to complete.
            let mut is_loading = self.is_loading.lock().unwrap();
            while *is_loading {
                is_loading = self.loading_condvar.wait(is_loading).unwrap();
            }

            if !self.engine.is_occupied() {
                return Err(anyhow::anyhow!("Model is not loaded for transcription."));
            }
        }

        // Get current settings for configuration
        let settings = get_settings(&self.app_handle);

        // When opt-in personalization (issue #16) is enabled, fold the learned dictionary into the
        // effective lists handed to the existing matcher: learned words bias/fuzzy-correct exactly
        // like `custom_words`, and learned replacements run deterministically like
        // `word_replacements`. When disabled these are the user-authored lists verbatim, so the
        // pipeline behaves identically to before.
        let effective_custom_words: Vec<String> = if settings.personalization.enabled {
            settings
                .custom_words
                .iter()
                .chain(settings.personalization.learned_words.iter())
                .cloned()
                .collect()
        } else {
            settings.custom_words.clone()
        };
        let effective_replacements: Vec<crate::settings::WordReplacement> =
            if settings.personalization.enabled {
                settings
                    .word_replacements
                    .iter()
                    .chain(settings.personalization.learned_replacements.iter())
                    .cloned()
                    .collect()
            } else {
                settings.word_replacements.clone()
            };

        // Validate selected language against the model's supported languages.
        // If the language isn't supported, fall back to "auto" to prevent errors.
        let validated_language = if settings.selected_language == "auto" {
            "auto".to_string()
        } else {
            let is_supported = self
                .model_manager
                .get_model_info(&settings.selected_model)
                .map(|info| {
                    info.supported_languages.is_empty()
                        || info
                            .supported_languages
                            .contains(&settings.selected_language)
                })
                .unwrap_or(true);

            if is_supported {
                settings.selected_language.clone()
            } else {
                warn!(
                    "Language '{}' not supported by current model, falling back to auto-detect",
                    settings.selected_language
                );
                "auto".to_string()
            }
        };

        // Perform transcription with the appropriate engine.
        // We use catch_unwind to prevent engine panics from poisoning the mutex,
        // which would make the app hang indefinitely on subsequent operations.
        let result = {
            // Take the engine out so we own it during transcription, together
            // with the slot generation observed at take time.
            // If the engine panics, we simply don't put it back (effectively unloading it)
            // instead of poisoning the mutex.
            let (mut engine, taken_generation) = match self.engine.take() {
                Some(taken) => taken,
                None => {
                    return Err(anyhow::anyhow!(
                        "Model failed to load after auto-load attempt. Please check your model settings."
                    ));
                }
            };

            // take() released the slot lock — no mutex held during the engine call

            let transcribe_result = catch_unwind(AssertUnwindSafe(
                || -> Result<transcribe_rs::TranscriptionResult> {
                    match &mut engine {
                        LoadedEngine::Whisper(whisper_engine) => {
                            let whisper_language = if validated_language == "auto" {
                                None
                            } else {
                                let normalized = if validated_language == "zh-Hans"
                                    || validated_language == "zh-Hant"
                                {
                                    "zh".to_string()
                                } else {
                                    validated_language.clone()
                                };
                                Some(normalized)
                            };

                            let params = WhisperInferenceParams {
                                language: whisper_language,
                                translate: settings.translate_to_english,
                                initial_prompt: if effective_custom_words.is_empty() {
                                    None
                                } else {
                                    Some(effective_custom_words.join(", "))
                                },
                                ..Default::default()
                            };

                            whisper_engine
                                .transcribe_with(&audio, &params)
                                .map_err(|e| anyhow::anyhow!("Whisper transcription failed: {}", e))
                        }
                        LoadedEngine::Parakeet(parakeet_engine) => {
                            let params = ParakeetParams {
                                timestamp_granularity: Some(TimestampGranularity::Segment),
                                ..Default::default()
                            };
                            parakeet_engine
                                .transcribe_with(&audio, &params)
                                .map_err(|e| {
                                    anyhow::anyhow!("Parakeet transcription failed: {}", e)
                                })
                        }
                        LoadedEngine::Moonshine(moonshine_engine) => moonshine_engine
                            .transcribe(&audio, &TranscribeOptions::default())
                            .map_err(|e| anyhow::anyhow!("Moonshine transcription failed: {}", e)),
                        LoadedEngine::MoonshineStreaming(streaming_engine) => streaming_engine
                            .transcribe(&audio, &TranscribeOptions::default())
                            .map_err(|e| {
                                anyhow::anyhow!("Moonshine streaming transcription failed: {}", e)
                            }),
                        LoadedEngine::SenseVoice(sense_voice_engine) => {
                            let language = match validated_language.as_str() {
                                "zh" | "zh-Hans" | "zh-Hant" => Some("zh".to_string()),
                                "en" => Some("en".to_string()),
                                "ja" => Some("ja".to_string()),
                                "ko" => Some("ko".to_string()),
                                "yue" => Some("yue".to_string()),
                                _ => None,
                            };
                            let params = SenseVoiceParams {
                                language,
                                use_itn: Some(true),
                            };
                            sense_voice_engine
                                .transcribe_with(&audio, &params)
                                .map_err(|e| {
                                    anyhow::anyhow!("SenseVoice transcription failed: {}", e)
                                })
                        }
                        LoadedEngine::GigaAM(gigaam_engine) => gigaam_engine
                            .transcribe(&audio, &TranscribeOptions::default())
                            .map_err(|e| anyhow::anyhow!("GigaAM transcription failed: {}", e)),
                        LoadedEngine::Canary(canary_engine) => {
                            let lang = if validated_language == "auto" {
                                None
                            } else {
                                Some(validated_language.clone())
                            };
                            let options = TranscribeOptions {
                                language: lang,
                                translate: settings.translate_to_english,
                                ..Default::default()
                            };
                            canary_engine
                                .transcribe(&audio, &options)
                                .map_err(|e| anyhow::anyhow!("Canary transcription failed: {}", e))
                        }
                        LoadedEngine::Cohere(cohere_engine) => {
                            let lang = if validated_language == "auto" {
                                None
                            } else if validated_language == "zh-Hans"
                                || validated_language == "zh-Hant"
                            {
                                Some("zh".to_string())
                            } else {
                                Some(validated_language.clone())
                            };
                            let options = TranscribeOptions {
                                language: lang,
                                ..Default::default()
                            };
                            cohere_engine
                                .transcribe(&audio, &options)
                                .map_err(|e| anyhow::anyhow!("Cohere transcription failed: {}", e))
                        }
                    }
                },
            ));

            match transcribe_result {
                Ok(inner_result) => {
                    // Success or normal error — put the engine back, unless
                    // the slot changed while we were out (model unloaded or a
                    // different one loaded, e.g. after this call wedged and
                    // its watchdog fired). Restoring then would clobber the
                    // current engine or resurrect an unloaded one, so the
                    // stale engine is dropped instead (issue #58).
                    if !self.engine.try_restore(engine, taken_generation) {
                        warn!(
                            "Engine slot changed while a transcription was running; \
                             dropping the stale engine instead of restoring it"
                        );
                    }
                    inner_result?
                }
                Err(panic_payload) => {
                    // Engine panicked — do NOT put it back (it's in an unknown state).
                    // The engine is dropped here, effectively unloading it.
                    let panic_msg = if let Some(s) = panic_payload.downcast_ref::<&str>() {
                        s.to_string()
                    } else if let Some(s) = panic_payload.downcast_ref::<String>() {
                        s.clone()
                    } else {
                        "unknown panic".to_string()
                    };
                    error!(
                        "Transcription engine panicked: {}. Model has been unloaded.",
                        panic_msg
                    );

                    // Clear the model ID so it will be reloaded on next attempt
                    {
                        let mut current_model = self
                            .current_model_id
                            .lock()
                            .unwrap_or_else(|e| e.into_inner());
                        *current_model = None;
                    }

                    let _ = self.app_handle.emit(
                        "model-state-changed",
                        ModelStateEvent {
                            event_type: "unloaded".to_string(),
                            model_id: None,
                            model_name: None,
                            error: Some(format!("Engine panicked: {}", panic_msg)),
                        },
                    );

                    return Err(anyhow::anyhow!(
                        "Transcription engine panicked: {}. The model has been unloaded and will reload on next attempt.",
                        panic_msg
                    ));
                }
            }
        };

        // Apply word correction if custom words are configured.
        // Skip for Whisper models since custom words are already passed as initial_prompt.
        let is_whisper = self
            .model_manager
            .get_model_info(&settings.selected_model)
            .map(|info| matches!(info.engine_type, EngineType::Whisper))
            .unwrap_or(false);

        let corrected_result = if !effective_custom_words.is_empty() && !is_whisper {
            apply_custom_words(
                &result.text,
                &effective_custom_words,
                settings.word_correction_threshold,
            )
        } else {
            result.text
        };

        // Apply deterministic literal replacements. Unlike the fuzzy dictionary these are exact
        // and safe, so they run for every engine (Whisper still mishears) -- this is the path
        // that fixes large mishears like "clawed" -> "Claude".
        let corrected_result = if effective_replacements.is_empty() {
            corrected_result
        } else {
            apply_replacements(&corrected_result, &effective_replacements)
        };

        // Filter out filler words and hallucinations
        let filtered_result = filter_transcription_output(
            &corrected_result,
            &settings.app_language,
            &settings.custom_filler_words,
        );

        let et = std::time::Instant::now();
        let translation_note = if settings.translate_to_english {
            " (translated)"
        } else {
            ""
        };
        info!(
            "Transcription completed in {}ms{}",
            (et - st).as_millis(),
            translation_note
        );

        let final_result = filtered_result;

        if final_result.is_empty() {
            info!("Transcription result is empty");
        } else {
            info!("Transcription result: {}", final_result);
        }

        self.maybe_unload_immediately("transcription");

        Ok(final_result)
    }

    /// Watchdog-guarded transcription (issue #58).
    ///
    /// While an earlier timed-out call is still running (wedged) this refuses
    /// immediately instead of starting another engine call: the wedged worker
    /// still holds the engine it took out of the slot, and stacking more
    /// engines on retries would exhaust VRAM/RAM. Model loads are refused for
    /// the same reason (see [`Self::load_model`]). If the wedged worker ever
    /// resolves, the count clears and — when the slot generation is unchanged
    /// — its engine is restored, so the manager recovers without a restart.
    pub fn transcribe_with_watchdog(
        &self,
        audio: Vec<f32>,
        timeout: Duration,
    ) -> WatchdogOutcome<Result<String>> {
        if self.is_wedged() {
            return WatchdogOutcome::Completed(Err(anyhow::anyhow!(WEDGED_ENGINE_ERROR)));
        }
        let manager = self.clone();
        run_with_watchdog(
            "transcription",
            timeout,
            Arc::clone(&self.wedged_workers),
            move || manager.transcribe(audio),
        )
    }
}

/// Shortest watchdog deadline: even a tiny clip gets this long before the
/// pipeline gives up on the engine (issue #58).
pub(crate) const TRANSCRIPTION_TIMEOUT_FLOOR: Duration = Duration::from_secs(120);

/// Longest watchdog deadline. A legitimate transcription that needs more than
/// this has an unusable UX anyway; bounding it keeps a wedged engine from
/// pinning the "transcribing" state for the rest of the session.
pub(crate) const TRANSCRIPTION_TIMEOUT_CEILING: Duration = Duration::from_secs(600);

/// Budget per second of recorded audio. 10x realtime covers a large Whisper
/// model on a slow CPU with headroom; anything slower is indistinguishable
/// from a hang for the user.
const TRANSCRIPTION_TIMEOUT_REALTIME_FACTOR: u64 = 10;

/// Watchdog deadline for a transcription of `sample_count` mono samples at
/// `sample_rate` Hz: 10x the audio duration, clamped to a generous
/// floor/ceiling so short clips aren't cut off early and long ones can't
/// wedge the UI forever.
pub(crate) fn transcription_watchdog_timeout(sample_count: usize, sample_rate: u32) -> Duration {
    let audio_secs = (sample_count as u64).div_ceil(u64::from(sample_rate.max(1)));
    Duration::from_secs(audio_secs.saturating_mul(TRANSCRIPTION_TIMEOUT_REALTIME_FACTOR))
        .clamp(TRANSCRIPTION_TIMEOUT_FLOOR, TRANSCRIPTION_TIMEOUT_CEILING)
}

/// How a watchdog-guarded call ended (issue #58).
#[derive(Debug)]
pub enum WatchdogOutcome<T> {
    /// The call finished within the deadline; its result is inside.
    Completed(T),
    /// The deadline passed with the worker still running. It is counted in
    /// the wedged-worker counter until it resolves.
    TimedOut,
    /// The worker panicked before producing a result. Distinct from
    /// [`WatchdogOutcome::TimedOut`] so callers don't report a false timeout.
    Panicked,
}

/// Handshake between the watchdog and its worker so the wedged-worker count
/// stays exact even when the worker finishes right at the deadline.
struct WatchdogState {
    finished: bool,
    timed_out: bool,
}

/// Run `f` and give up if it does not finish within `timeout` (issue #58).
/// The caller must treat anything but `Completed` as an error and recover the
/// UI/state itself.
///
/// Limitation: a wedged worker thread cannot be killed. On timeout it is left
/// running detached and `wedged_workers` is incremented until it resolves
/// (its late result is then logged and discarded, and the count decremented).
/// The counter lets the owner refuse new work while a wedged worker still
/// holds resources.
pub(crate) fn run_with_watchdog<T: Send + 'static>(
    operation: &'static str,
    timeout: Duration,
    wedged_workers: Arc<AtomicUsize>,
    f: impl FnOnce() -> T + Send + 'static,
) -> WatchdogOutcome<T> {
    let (tx, rx) = std::sync::mpsc::channel();
    let state = Arc::new(Mutex::new(WatchdogState {
        finished: false,
        timed_out: false,
    }));

    let worker_state = Arc::clone(&state);
    let worker_wedged = Arc::clone(&wedged_workers);
    thread::spawn(move || {
        let result = catch_unwind(AssertUnwindSafe(f));
        let panicked = result.is_err();
        if let Ok(value) = result {
            // Send before marking finished: once the watchdog observes
            // `finished`, a successful result is guaranteed to be in the
            // channel (so an empty channel + finished means a panic).
            let _ = tx.send(value);
        }
        let timed_out = {
            let mut st = worker_state.lock().unwrap_or_else(|p| p.into_inner());
            st.finished = true;
            if st.timed_out {
                // Increment and decrement both happen under this lock, so the
                // count can never underflow.
                worker_wedged.fetch_sub(1, Ordering::SeqCst);
            }
            st.timed_out
        };
        if timed_out {
            warn!(
                "{} resolved after its watchdog already fired; late {} discarded",
                operation,
                if panicked { "panic" } else { "result" }
            );
        }
        if panicked {
            error!("{} worker thread panicked", operation);
        }
        // tx drops here; a watchdog still waiting sees Disconnected on panic.
    });

    match rx.recv_timeout(timeout) {
        Ok(result) => WatchdogOutcome::Completed(result),
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
            // The worker panicked. transcribe() catches engine panics itself,
            // so this is unexpected — but it still must not wedge the pipeline.
            error!(
                "{} worker thread panicked before producing a result",
                operation
            );
            WatchdogOutcome::Panicked
        }
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
            let mut st = state.lock().unwrap_or_else(|p| p.into_inner());
            if st.finished {
                // The worker finished right at the deadline. On success its
                // result is already in the channel; nothing there means it
                // panicked.
                drop(st);
                match rx.try_recv() {
                    Ok(result) => WatchdogOutcome::Completed(result),
                    Err(_) => WatchdogOutcome::Panicked,
                }
            } else {
                st.timed_out = true;
                wedged_workers.fetch_add(1, Ordering::SeqCst);
                drop(st);
                error!(
                    "{} watchdog fired after {:?}; the engine appears wedged. The worker \
                     thread cannot be killed and is left running detached; further \
                     transcriptions and model loads are refused until it resolves.",
                    operation, timeout
                );
                WatchdogOutcome::TimedOut
            }
        }
    }
}

/// Apply the user's accelerator preferences to the transcribe-rs global atomics.
/// Called on startup and whenever the user changes the setting.
pub fn apply_accelerator_settings(app: &tauri::AppHandle) {
    use transcribe_rs::accel;

    let settings = get_settings(app);

    let whisper_pref = match settings.whisper_accelerator {
        WhisperAcceleratorSetting::Auto => accel::WhisperAccelerator::Auto,
        WhisperAcceleratorSetting::Cpu => accel::WhisperAccelerator::CpuOnly,
        WhisperAcceleratorSetting::Gpu => accel::WhisperAccelerator::Gpu,
    };
    accel::set_whisper_accelerator(whisper_pref);
    accel::set_whisper_gpu_device(settings.whisper_gpu_device);
    info!(
        "Whisper accelerator set to: {}, gpu_device: {}",
        whisper_pref,
        if settings.whisper_gpu_device == accel::GPU_DEVICE_AUTO {
            "auto".to_string()
        } else {
            settings.whisper_gpu_device.to_string()
        }
    );

    let ort_pref = match settings.ort_accelerator {
        OrtAcceleratorSetting::Auto => accel::OrtAccelerator::Auto,
        OrtAcceleratorSetting::Cpu => accel::OrtAccelerator::CpuOnly,
        OrtAcceleratorSetting::Cuda => accel::OrtAccelerator::Cuda,
        OrtAcceleratorSetting::DirectMl => accel::OrtAccelerator::DirectMl,
        OrtAcceleratorSetting::Rocm => accel::OrtAccelerator::Rocm,
    };
    accel::set_ort_accelerator(ort_pref);
    info!("ORT accelerator set to: {}", ort_pref);
}

#[derive(Serialize, Clone, Debug, Type)]
pub struct GpuDeviceOption {
    pub id: i32,
    pub name: String,
    pub total_vram_mb: usize,
}

static GPU_DEVICES: OnceLock<Vec<GpuDeviceOption>> = OnceLock::new();

fn cached_gpu_devices() -> &'static [GpuDeviceOption] {
    use transcribe_rs::whisper_cpp::gpu::list_gpu_devices;

    GPU_DEVICES.get_or_init(|| {
        // ggml's Vulkan backend uses FMA3 instructions internally.
        // On older CPUs without FMA3 (e.g. Sandy Bridge Xeons) this causes
        // a SIGILL crash that cannot be caught. Skip enumeration entirely
        // on those CPUs — GPU-accelerated whisper won't work there anyway.
        #[cfg(target_arch = "x86_64")]
        if !std::arch::is_x86_feature_detected!("fma") {
            warn!("CPU lacks FMA3 support — skipping GPU device enumeration");
            return Vec::new();
        }

        list_gpu_devices()
            .into_iter()
            .map(|d| GpuDeviceOption {
                id: d.id,
                name: d.name,
                total_vram_mb: d.total_vram / (1024 * 1024),
            })
            .collect()
    })
}

#[derive(Serialize, Clone, Debug, Type)]
pub struct AvailableAccelerators {
    pub whisper: Vec<String>,
    pub ort: Vec<String>,
    pub gpu_devices: Vec<GpuDeviceOption>,
}

/// Return which accelerators are compiled into this build.
pub fn get_available_accelerators() -> AvailableAccelerators {
    use transcribe_rs::accel::OrtAccelerator;

    let ort_options: Vec<String> = OrtAccelerator::available()
        .into_iter()
        .map(|a| a.to_string())
        .collect();

    let whisper_options = vec!["auto".to_string(), "cpu".to_string(), "gpu".to_string()];

    AvailableAccelerators {
        whisper: whisper_options,
        ort: ort_options,
        gpu_devices: cached_gpu_devices().to_vec(),
    }
}

impl Drop for TranscriptionManager {
    fn drop(&mut self) {
        // Skip shutdown unless this is the very last clone. TranscriptionManager
        // is cloned by initiate_model_load() and the watcher thread — those
        // clones dropping must not kill the watcher. The watcher thread holds
        // its own clone, so engine's strong_count is always >= 2 while the
        // watcher is alive. When it reaches 1, only this instance remains
        // and we can safely shut down.
        if Arc::strong_count(&self.engine) > 1 {
            return;
        }

        // Signal the watcher thread to shutdown
        self.shutdown_signal.store(true, Ordering::Relaxed);

        // Wait for the thread to finish gracefully
        if let Some(handle) = self.watcher_handle.lock().unwrap().take() {
            if let Err(e) = handle.join() {
                warn!("Failed to join idle watcher thread: {:?}", e);
            } else {
                debug!("Idle watcher thread joined successfully");
            }
        }
    }
}

#[cfg(test)]
mod watchdog_tests {
    use super::{
        run_with_watchdog, transcription_watchdog_timeout, GenerationGate, WatchdogOutcome,
        TRANSCRIPTION_TIMEOUT_CEILING, TRANSCRIPTION_TIMEOUT_FLOOR,
    };
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    fn counter() -> Arc<AtomicUsize> {
        Arc::new(AtomicUsize::new(0))
    }

    #[test]
    fn watchdog_passes_through_a_result_that_arrives_in_time() {
        let wedged = counter();
        let outcome =
            run_with_watchdog("test", Duration::from_secs(5), Arc::clone(&wedged), || {
                "hello".to_string()
            });
        assert!(matches!(outcome, WatchdogOutcome::Completed(ref s) if s == "hello"));
        assert_eq!(wedged.load(Ordering::SeqCst), 0);
    }

    /// Reproduces issue #58: with no watchdog, a wedged engine call blocks the
    /// pipeline forever and the "transcribing" UI state is never cleared. The
    /// watchdog must report `TimedOut` at its deadline instead of waiting for
    /// the wedged call. The stand-in for a wedged engine sleeps well past the
    /// deadline (rather than literally forever) so a failing run still
    /// terminates.
    #[test]
    fn watchdog_times_out_on_a_wedged_transcribe_call() {
        let start = Instant::now();
        let outcome = run_with_watchdog("test", Duration::from_millis(100), counter(), || {
            std::thread::sleep(Duration::from_secs(3));
        });
        assert!(
            matches!(outcome, WatchdogOutcome::TimedOut),
            "a wedged transcribe call must time out, not produce a result"
        );
        assert!(
            start.elapsed() < Duration::from_secs(2),
            "the watchdog must fire at its deadline instead of waiting out the wedged call"
        );
    }

    /// A wedged worker is counted while it is still running so the owner can
    /// refuse new engine loads/transcriptions (no engine stacking), and the
    /// count clears when the worker finally resolves.
    #[test]
    fn watchdog_counts_a_wedged_worker_and_clears_it_when_it_resolves() {
        let wedged = counter();
        let outcome = run_with_watchdog(
            "test",
            Duration::from_millis(50),
            Arc::clone(&wedged),
            || std::thread::sleep(Duration::from_millis(400)),
        );
        assert!(matches!(outcome, WatchdogOutcome::TimedOut));
        assert_eq!(
            wedged.load(Ordering::SeqCst),
            1,
            "a timed-out worker must be counted as wedged while it is still running"
        );

        // The worker resolves ~350ms later; poll until the count clears.
        let deadline = Instant::now() + Duration::from_secs(5);
        while wedged.load(Ordering::SeqCst) != 0 {
            assert!(
                Instant::now() < deadline,
                "the wedged count must clear once the late worker resolves"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// A worker panic must be reported as `Panicked`, not as a false
    /// "timed out after N seconds", and must not leave a wedged count behind.
    #[test]
    fn watchdog_reports_a_worker_panic_as_panicked_not_timed_out() {
        let wedged = counter();
        let outcome: WatchdogOutcome<()> =
            run_with_watchdog("test", Duration::from_secs(5), Arc::clone(&wedged), || {
                panic!("simulated engine panic")
            });
        assert!(
            matches!(outcome, WatchdogOutcome::Panicked),
            "a worker panic must surface as Panicked, not TimedOut or Completed"
        );
        assert_eq!(wedged.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn watchdog_timeout_scales_with_audio_duration_within_bounds() {
        const RATE: u32 = 16_000;
        // Short clip: clamped up to the floor.
        assert_eq!(
            transcription_watchdog_timeout(10 * RATE as usize, RATE),
            TRANSCRIPTION_TIMEOUT_FLOOR
        );
        // 30s of audio: 10x realtime = 300s, between floor and ceiling.
        assert_eq!(
            transcription_watchdog_timeout(30 * RATE as usize, RATE),
            Duration::from_secs(300)
        );
        // Very long recording: clamped down to the ceiling.
        assert_eq!(
            transcription_watchdog_timeout(600 * RATE as usize, RATE),
            TRANSCRIPTION_TIMEOUT_CEILING
        );
        // Empty audio still gets the floor (the caller guards this case anyway).
        assert_eq!(
            transcription_watchdog_timeout(0, RATE),
            TRANSCRIPTION_TIMEOUT_FLOOR
        );
    }

    #[test]
    fn generation_gate_restores_when_the_slot_is_untouched() {
        let gate: GenerationGate<u32> = GenerationGate::new();
        gate.install(7);
        let (value, generation) = gate.take().expect("installed value must be takeable");
        assert!(!gate.is_occupied());
        assert!(
            gate.try_restore(value, generation),
            "a take/restore with no intervening slot change must succeed"
        );
        assert!(gate.is_occupied());
    }

    /// A late-returning transcription must not resurrect its engine after the
    /// model was unloaded while it was out.
    #[test]
    fn generation_gate_rejects_a_restore_after_clear() {
        let gate: GenerationGate<u32> = GenerationGate::new();
        gate.install(7);
        let (value, generation) = gate.take().expect("installed value must be takeable");
        gate.clear();
        assert!(
            !gate.try_restore(value, generation),
            "restoring after an unload must be rejected"
        );
        assert!(!gate.is_occupied());
    }

    /// A late-returning transcription must not clobber an engine that was
    /// loaded while it was out (e.g. a fresh model loaded after its watchdog
    /// fired).
    #[test]
    fn generation_gate_rejects_a_restore_after_a_new_install() {
        let gate: GenerationGate<u32> = GenerationGate::new();
        gate.install(7);
        let (stale, generation) = gate.take().expect("installed value must be takeable");
        gate.install(8);
        assert!(
            !gate.try_restore(stale, generation),
            "restoring over a newly installed value must be rejected"
        );
        let (current, _) = gate
            .take()
            .expect("the new value must still be in the slot");
        assert_eq!(current, 8, "the newly installed value must win");
    }
}
