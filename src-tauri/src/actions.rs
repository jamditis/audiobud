#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
use crate::apple_intelligence;
use crate::audio_feedback::{play_feedback_sound, play_feedback_sound_blocking, SoundType};
use crate::audio_toolkit::constants::WHISPER_SAMPLE_RATE;
use crate::audio_toolkit::{
    apply_spoken_punctuation, format_numbers, is_microphone_access_denied,
    is_no_input_device_error, strip_to_raw_text,
};
use crate::delivery_queue::{DeliveryQueue, EnqueueResult, TranscriptDelivery};
use crate::delivery_worker::DeliveryWorker;
use crate::dictation_context::{ActiveDictations, DictationContext};
use crate::managers::audio::AudioRecordingManager;
use crate::managers::engine_limits::{MODEL_AUTO_LOAD_FAILED_ERROR, WEDGED_ENGINE_ERROR};
use crate::managers::history::HistoryManager;
use crate::managers::transcription::TranscriptionManager;
use crate::managers::watchdog::{transcription_watchdog_timeout, WatchdogOutcome};
use crate::settings::{get_settings, AppSettings, APPLE_INTELLIGENCE_PROVIDER_ID};
use crate::shortcut;
use crate::tray::{change_tray_icon, TrayIconState};
use crate::utils::{
    self, show_processing_overlay, show_recording_overlay, show_transcribing_overlay,
};
use crate::TranscriptionCoordinator;
use ferrous_opencc::{config::BuiltinConfig, OpenCC};
use log::{debug, error, warn};
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tauri::Manager;
use tauri::{AppHandle, Emitter};

#[derive(Clone, serde::Serialize)]
struct RecordingErrorEvent {
    error_type: String,
    detail: Option<String>,
}

/// Payload of the `transcription-timeout` event, shared with the history
/// retry command so both paths surface the same timeout toast.
#[derive(Clone, serde::Serialize)]
pub(crate) struct TranscriptionTimeoutEvent {
    pub(crate) timeout_secs: u64,
}

/// Payload of the `transcription-error` event, emitted when a transcription
/// fails so the user sees why (e.g. the Parakeet engine refusing a recording
/// that exceeds its length limit, issue #169) instead of silence.
#[derive(Clone, serde::Serialize)]
pub(crate) struct TranscriptionErrorEvent {
    pub(crate) message: String,
}

/// Model-load failures already emit `model-state-changed/loading_failed`,
/// which the frontend turns into a specific toast. The manager returns the
/// auto-load sentinel only when that event was emitted; a plain missing-model
/// sentinel can instead follow manual unload/deletion and is not suppressed.
fn transcription_error_already_notified(message: &str) -> bool {
    matches!(message, MODEL_AUTO_LOAD_FAILED_ERROR | WEDGED_ENGINE_ERROR)
}

/// Drop guard that notifies the [`TranscriptionCoordinator`] when the
/// transcription pipeline finishes — whether it completes normally or panics.
struct FinishGuard(AppHandle);
impl Drop for FinishGuard {
    fn drop(&mut self) {
        if let Some(c) = self.0.try_state::<TranscriptionCoordinator>() {
            c.notify_processing_finished();
        }
    }
}

/// Enqueue a finished transcript, under the intent its dictation was started
/// with, and return the text only when the bounded queue cannot accept it.
/// Callers must persist that returned text before dropping their last copy.
fn enqueue_transcript_delivery(
    app: AppHandle,
    text: String,
    context: DictationContext,
) -> Option<String> {
    let Some(queue) = app.try_state::<DeliveryQueue>() else {
        error!("Delivery queue is not initialized");
        let _ = app.emit("paste-error", ());
        utils::hide_recording_overlay(&app);
        change_tray_icon(&app, TrayIconState::Idle);
        return Some(text);
    };

    match queue.enqueue(TranscriptDelivery { text, context }) {
        EnqueueResult::Start(delivery) => {
            schedule_transcript_delivery(app, delivery);
            None
        }
        EnqueueResult::Queued => {
            debug!("Transcript queued for delivery");
            None
        }
        EnqueueResult::Full(delivery) => {
            error!("Delivery queue is full; transcript was not pasted");
            let _ = app.emit("paste-error", ());
            Some(delivery.text)
        }
    }
}

/// Hands the delivery worker to the next queued transcript when this one is
/// done -- including when the paste panics, so one bad delivery cannot leave the
/// queue believing a delivery is still in flight and strand every transcript
/// behind it.
struct DeliveryHandoff(AppHandle);
impl Drop for DeliveryHandoff {
    fn drop(&mut self) {
        finish_transcript_delivery(self.0.clone());
    }
}

/// Send one transcript down the delivery thread (#161).
///
/// Not the main thread: a paste blocks for the paste delay, the keystroke
/// holds, the clipboard restore, and -- with a pinned target -- a foreground
/// switch and hand-back on top, all of which froze the overlay and the tray for
/// as long as it took. Ordering is unaffected, because the queue still releases
/// one transcript at a time and the worker runs them in the order it receives
/// them.
fn schedule_transcript_delivery(app: AppHandle, delivery: TranscriptDelivery) {
    let app_for_delivery = app.clone();
    let job = move || {
        let _handoff = DeliveryHandoff(app_for_delivery.clone());
        let paste_time = Instant::now();
        // The queued context, not a fresh read: by the time a queued transcript
        // is pasted the user may already be dictating somewhere else (#160).
        match utils::paste(delivery.text, app_for_delivery.clone(), delivery.context) {
            Ok(()) => debug!("Text pasted successfully in {:?}", paste_time.elapsed()),
            Err(error) => {
                error!("Failed to paste transcription: {}", error);
                let _ = app_for_delivery.emit("paste-error", ());
            }
        }
    };

    match app.try_state::<DeliveryWorker>() {
        Some(worker) => worker.run(Box::new(job)),
        None => {
            // Deliver it here rather than throw the transcript away: this is
            // the transcription thread, so the UI still stays responsive.
            error!("Delivery worker is not initialized; delivering on this thread");
            job();
        }
    }
}

fn finish_transcript_delivery(app: AppHandle) {
    let next = app
        .try_state::<DeliveryQueue>()
        .and_then(|queue| queue.finish_and_take_next());

    if let Some(delivery) = next {
        schedule_transcript_delivery(app, delivery);
    } else {
        // Put drain and hotkey events on the same serialized command stream.
        // Whichever arrives first is fully handled before the other can update
        // pipeline state or UI, so an older delivery cannot clear newer UI.
        if app
            .try_state::<TranscriptionCoordinator>()
            .is_some_and(|coordinator| coordinator.notify_delivery_drained())
        {
            return;
        }
        clear_transcript_ui_if_delivery_idle(&app);
    }
}

/// Clear transcript UI only after both the processing pipeline and delivery
/// worker have released their work. The coordinator calls this when processing
/// finishes to cover the case where a fast paste drained the queue first.
pub(crate) fn clear_transcript_ui_if_delivery_idle(app: &AppHandle) {
    let delivery_is_idle = app
        .try_state::<DeliveryQueue>()
        .is_none_or(|queue| queue.is_idle());

    if delivery_is_idle {
        utils::hide_recording_overlay(app);
        change_tray_icon(app, TrayIconState::Idle);
    }
}

/// Hand a started dictation's context to the `stop` that will finish it.
fn park_dictation_context(app: &AppHandle, binding_id: &str, context: DictationContext) {
    match app.try_state::<ActiveDictations>() {
        Some(active) => active.begin(binding_id, context),
        // Recoverable: `stop` captures the intent itself when nothing was
        // parked, which costs this one dictation its mid-flight immunity but
        // still delivers it.
        None => warn!("Active dictation registry is not initialized"),
    }
}

/// Drop a context whose recording never started.
fn discard_dictation_context(app: &AppHandle, binding_id: &str) {
    if let Some(active) = app.try_state::<ActiveDictations>() {
        active.discard(binding_id);
    }
}

/// Take back the context this binding's recording was started with.
///
/// `capture_now` covers the case where no start was recorded for the binding --
/// the registry was missing, or the recording was cancelled and this is the key
/// release arriving afterwards. The dictation is still completed, from the state
/// as it stands now, rather than dropped.
fn take_dictation_context(
    app: &AppHandle,
    binding_id: &str,
    capture_now: impl FnOnce() -> DictationContext,
) -> DictationContext {
    match app
        .try_state::<ActiveDictations>()
        .and_then(|active| active.take(binding_id))
    {
        Some(context) => context,
        None => {
            debug!(
                "No dictation context is parked for binding '{}'; capturing it at stop instead",
                binding_id
            );
            capture_now()
        }
    }
}

// Shortcut Action Trait
pub trait ShortcutAction: Send + Sync {
    fn start(&self, app: &AppHandle, binding_id: &str, shortcut_str: &str);
    fn stop(&self, app: &AppHandle, binding_id: &str, shortcut_str: &str);
}

// Transcribe Action
struct TranscribeAction {
    post_process: bool,
    /// Emit a raw transcript (lowercase, unpunctuated). Mutually exclusive with
    /// `post_process`; when set it overrides the persisted `raw_output` setting for this
    /// dictation only. See [`process_transcription_output`].
    raw: bool,
}

/// Field name for structured output JSON schema
const TRANSCRIPTION_FIELD: &str = "transcription";

/// Strip invisible Unicode characters that some LLMs may insert
fn strip_invisible_chars(s: &str) -> String {
    s.replace(['\u{200B}', '\u{200C}', '\u{200D}', '\u{FEFF}'], "")
}

/// Build a system prompt from the user's prompt template.
/// Removes `${output}` placeholder since the transcription is sent as the user message.
fn build_system_prompt(prompt_template: &str) -> String {
    prompt_template.replace("${output}", "").trim().to_string()
}

async fn post_process_transcription(settings: &AppSettings, transcription: &str) -> Option<String> {
    let provider = match settings.active_post_process_provider().cloned() {
        Some(provider) => provider,
        None => {
            debug!("Post-processing enabled but no provider is selected");
            return None;
        }
    };

    let model = settings
        .post_process_models
        .get(&provider.id)
        .cloned()
        .unwrap_or_default();

    if model.trim().is_empty() {
        debug!(
            "Post-processing skipped because provider '{}' has no model configured",
            provider.id
        );
        return None;
    }

    let selected_prompt_id = match &settings.post_process_selected_prompt_id {
        Some(id) => id.clone(),
        None => {
            debug!("Post-processing skipped because no prompt is selected");
            return None;
        }
    };

    let prompt = match settings
        .post_process_prompts
        .iter()
        .find(|prompt| prompt.id == selected_prompt_id)
    {
        Some(prompt) => prompt.prompt.clone(),
        None => {
            debug!(
                "Post-processing skipped because prompt '{}' was not found",
                selected_prompt_id
            );
            return None;
        }
    };

    if prompt.trim().is_empty() {
        debug!("Post-processing skipped because the selected prompt is empty");
        return None;
    }

    debug!(
        "Starting LLM post-processing with provider '{}' (model: {})",
        provider.id, model
    );

    let api_key = settings
        .post_process_api_keys
        .get(&provider.id)
        .cloned()
        .unwrap_or_default();

    // Disable reasoning for providers where post-processing rarely benefits from it.
    // - custom: top-level reasoning_effort (works for local OpenAI-compat servers)
    // - openrouter: nested reasoning object; exclude:true also keeps reasoning text
    //   out of the response so it can't pollute structured-output JSON parsing
    let (reasoning_effort, reasoning) = match provider.id.as_str() {
        "custom" => (Some("none".to_string()), None),
        "openrouter" => (
            None,
            Some(crate::llm_client::ReasoningConfig {
                effort: Some("none".to_string()),
                exclude: Some(true),
            }),
        ),
        _ => (None, None),
    };

    if provider.supports_structured_output {
        debug!("Using structured outputs for provider '{}'", provider.id);

        let system_prompt = build_system_prompt(&prompt);
        let user_content = transcription.to_string();

        // Handle Apple Intelligence separately since it uses native Swift APIs
        if provider.id == APPLE_INTELLIGENCE_PROVIDER_ID {
            #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
            {
                if !apple_intelligence::check_apple_intelligence_availability() {
                    debug!(
                        "Apple Intelligence selected but not currently available on this device"
                    );
                    return None;
                }

                let token_limit = model.trim().parse::<i32>().unwrap_or(0);
                return match apple_intelligence::process_text_with_system_prompt(
                    &system_prompt,
                    &user_content,
                    token_limit,
                ) {
                    Ok(result) => {
                        if result.trim().is_empty() {
                            debug!("Apple Intelligence returned an empty response");
                            None
                        } else {
                            let result = strip_invisible_chars(&result);
                            debug!(
                                "Apple Intelligence post-processing succeeded. Output length: {} chars",
                                result.len()
                            );
                            Some(result)
                        }
                    }
                    Err(err) => {
                        error!("Apple Intelligence post-processing failed: {}", err);
                        None
                    }
                };
            }

            #[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
            {
                debug!("Apple Intelligence provider selected on unsupported platform");
                return None;
            }
        }

        // Define JSON schema for transcription output
        let json_schema = serde_json::json!({
            "type": "object",
            "properties": {
                (TRANSCRIPTION_FIELD): {
                    "type": "string",
                    "description": "The cleaned and processed transcription text"
                }
            },
            "required": [TRANSCRIPTION_FIELD],
            "additionalProperties": false
        });

        match crate::llm_client::send_chat_completion_with_schema(
            &provider,
            api_key.clone(),
            &model,
            crate::llm_client::ChatCompletionOptions {
                user_content,
                system_prompt: Some(system_prompt),
                json_schema: Some(json_schema),
                reasoning_effort: reasoning_effort.clone(),
                reasoning: reasoning.clone(),
            },
        )
        .await
        {
            Ok(Some(content)) => {
                // Parse the JSON response to extract the transcription field
                match serde_json::from_str::<serde_json::Value>(&content) {
                    Ok(json) => {
                        if let Some(transcription_value) =
                            json.get(TRANSCRIPTION_FIELD).and_then(|t| t.as_str())
                        {
                            let result = strip_invisible_chars(transcription_value);
                            debug!(
                                "Structured output post-processing succeeded for provider '{}'. Output length: {} chars",
                                provider.id,
                                result.len()
                            );
                            return Some(result);
                        } else {
                            error!("Structured output response missing 'transcription' field");
                            return Some(strip_invisible_chars(&content));
                        }
                    }
                    Err(e) => {
                        error!(
                            "Failed to parse structured output JSON: {}. Returning raw content.",
                            e
                        );
                        return Some(strip_invisible_chars(&content));
                    }
                }
            }
            Ok(None) => {
                error!("LLM API response has no content");
                return None;
            }
            Err(e) => {
                warn!(
                    "Structured output failed for provider '{}': {}. Falling back to legacy mode.",
                    provider.id, e
                );
                // Fall through to legacy mode below
            }
        }
    }

    // Legacy mode: Replace ${output} variable in the prompt with the actual text
    let processed_prompt = prompt.replace("${output}", transcription);
    debug!("Processed prompt length: {} chars", processed_prompt.len());

    match crate::llm_client::send_chat_completion(
        &provider,
        api_key,
        &model,
        processed_prompt,
        reasoning_effort,
        reasoning,
    )
    .await
    {
        Ok(Some(content)) => {
            let content = strip_invisible_chars(&content);
            debug!(
                "LLM post-processing succeeded for provider '{}'. Output length: {} chars",
                provider.id,
                content.len()
            );
            Some(content)
        }
        Ok(None) => {
            error!("LLM API response has no content");
            None
        }
        Err(e) => {
            error!(
                "LLM post-processing failed for provider '{}': {}. Falling back to original transcription.",
                provider.id,
                e
            );
            None
        }
    }
}

async fn maybe_convert_chinese_variant(
    settings: &AppSettings,
    transcription: &str,
) -> Option<String> {
    // Check if language is set to Simplified or Traditional Chinese
    let is_simplified = settings.selected_language == "zh-Hans";
    let is_traditional = settings.selected_language == "zh-Hant";

    if !is_simplified && !is_traditional {
        debug!("selected_language is not Simplified or Traditional Chinese; skipping translation");
        return None;
    }

    debug!(
        "Starting Chinese translation using OpenCC for language: {}",
        settings.selected_language
    );

    // Use OpenCC to convert based on selected language
    let config = if is_simplified {
        // Convert Traditional Chinese to Simplified Chinese
        BuiltinConfig::Tw2sp
    } else {
        // Convert Simplified Chinese to Traditional Chinese
        BuiltinConfig::S2tw
    };

    match OpenCC::from_config(config) {
        Ok(converter) => {
            let converted = converter.convert(transcription);
            debug!(
                "OpenCC translation completed. Input length: {}, Output length: {}",
                transcription.len(),
                converted.len()
            );
            Some(converted)
        }
        Err(e) => {
            error!("Failed to initialize OpenCC converter: {}. Falling back to original transcription.", e);
            None
        }
    }
}

pub(crate) struct ProcessedTranscription {
    pub final_text: String,
    pub post_processed_text: Option<String>,
    pub post_process_prompt: Option<String>,
}

/// Decides whether raw-text formatting should force English casing for the standalone pronoun "I".
/// This is `true` only when the output is known to be English: translate-to-English makes the engine
/// emit English regardless of source language, and an explicitly selected English dictation language
/// is likewise definitely English. For auto-detect (`transcribe-rs` does not report the detected
/// language) or an explicit non-English language it is `false`, and `strip_to_raw_text` then keeps
/// the engine's own casing of an "i"/"I" token -- correct for English (engines capitalize "I") yet
/// not wrongly capitalizing languages that use a lowercase standalone "i".
fn force_english_i_casing(translate_to_english: bool, selected_language: &str) -> bool {
    if translate_to_english {
        return true;
    }
    let base = selected_language
        .split(&['-', '_'][..])
        .next()
        .unwrap_or(selected_language);
    base == "en"
}

pub(crate) async fn process_transcription_output(
    app: &AppHandle,
    transcription: &str,
    post_process: bool,
    effective_raw: bool,
) -> ProcessedTranscription {
    let settings = get_settings(app);
    let mut final_text = transcription.to_string();
    let mut post_processed_text: Option<String> = None;
    let mut post_process_prompt: Option<String> = None;

    if let Some(converted_text) = maybe_convert_chinese_variant(&settings, transcription).await {
        final_text = converted_text;
    }

    if effective_raw {
        // Raw output and LLM post-processing are contradictory, so raw wins and post-processing
        // is skipped. Apply the deterministic raw transform as the final stage. The raw text is the
        // entry's primary output (see the save sites), so it is left in `final_text` rather than
        // being recorded as a separate `post_processed_text` variant.
        let force_english_i =
            force_english_i_casing(settings.translate_to_english, &settings.selected_language);
        final_text = strip_to_raw_text(&final_text, force_english_i);
        // Raw mode has no model to tidy the text, so spoken punctuation and numbers are the
        // only way to dictate anything with a "?" or a "$25" in it. Both stay behind their own
        // setting: format_raw_output turns raw formatting on at all, and format_numbers keeps
        // meaning the same thing here as it does on the normal path.
        if settings.format_raw_output {
            if settings.format_numbers {
                final_text = format_numbers(&final_text);
            }
            final_text = apply_spoken_punctuation(&final_text);
        }
    } else if post_process {
        if let Some(processed_text) = post_process_transcription(&settings, &final_text).await {
            post_processed_text = Some(processed_text.clone());
            final_text = processed_text;

            if let Some(prompt_id) = &settings.post_process_selected_prompt_id {
                if let Some(prompt) = settings
                    .post_process_prompts
                    .iter()
                    .find(|prompt| &prompt.id == prompt_id)
                {
                    post_process_prompt = Some(prompt.prompt.clone());
                }
            }
        }
    } else {
        // Normal path (neither raw nor LLM post-processing). Rewrite spelled-out numbers as digits
        // and symbols ("$25", "10%", "3.5") so amounts, currencies, and decimals read naturally.
        // Raw output is deliberately verbatim, and the LLM path handles its own formatting, so both
        // skip this step.
        if settings.format_numbers {
            final_text = format_numbers(&final_text);
        }
        // Record the formatted variant for history whenever a deterministic transform (Chinese
        // conversion, number formatting) changed the text from the verbatim transcript.
        if final_text != transcription {
            post_processed_text = Some(final_text.clone());
        }
    }

    ProcessedTranscription {
        final_text,
        post_processed_text,
        post_process_prompt,
    }
}

impl ShortcutAction for TranscribeAction {
    fn start(&self, app: &AppHandle, binding_id: &str, _shortcut_str: &str) {
        let start_time = Instant::now();
        debug!("TranscribeAction::start called for binding: {}", binding_id);

        // Load model in the background
        let tm = app.state::<Arc<TranscriptionManager>>();
        let rm = app.state::<Arc<AudioRecordingManager>>();

        // Load ASR model and VAD model in parallel
        tm.initiate_model_load();
        let rm_clone = Arc::clone(&rm);
        std::thread::spawn(move || {
            if let Err(e) = rm_clone.preload_vad() {
                debug!("VAD pre-load failed: {}", e);
            }
        });

        let binding_id = binding_id.to_string();

        // This is the one point where a dictation's intent is decided (#160):
        // what the user asked for with this shortcut, resolved against the
        // settings and the output target as they stand right now. Every later
        // stage -- the overlay badge, transcription, history, the paste -- reads
        // this context instead of consulting the live state again, so a toggle
        // flipped while the user is still speaking governs the next dictation
        // rather than this one.
        let settings = get_settings(app);
        let is_always_on = settings.always_on_microphone;
        let context = DictationContext::capture(
            self.raw,
            self.post_process,
            settings.raw_output,
            crate::output_target::backend::capture_delivery(app),
            // Where this dictation falls in the order they were started, so a
            // one-shot pick reaches the dictation it was made for (#124).
            crate::dictation_context::next_sequence(app),
        );

        change_tray_icon(app, TrayIconState::Recording);
        // The RAW badge shows the resolved decision, so it always matches what
        // will actually be emitted (issue #24).
        show_recording_overlay(app, context.effective_raw());

        debug!("Microphone mode - always_on: {}", is_always_on);

        // Park the context for `stop`, which runs on a later call stack. It is
        // stored before recording begins so the hand-off cannot lose a race with
        // a very short press.
        park_dictation_context(app, &binding_id, context);

        let mut recording_error: Option<String> = None;
        if is_always_on {
            // Always-on mode: Play audio feedback immediately, then apply mute after sound finishes
            debug!("Always-on mode: Playing audio feedback immediately");
            let rm_clone = Arc::clone(&rm);
            let app_clone = app.clone();
            // The blocking helper exits immediately if audio feedback is disabled,
            // so we can always reuse this thread to ensure mute happens right after playback.
            std::thread::spawn(move || {
                play_feedback_sound_blocking(&app_clone, SoundType::Start);
                rm_clone.apply_mute();
            });

            if let Err(e) = rm.try_start_recording(&binding_id) {
                debug!("Recording failed: {}", e);
                recording_error = Some(e);
            }
        } else {
            // On-demand mode: Start recording first, then play audio feedback, then apply mute
            // This allows the microphone to be activated before playing the sound
            debug!("On-demand mode: Starting recording first, then audio feedback");
            let recording_start_time = Instant::now();
            match rm.try_start_recording(&binding_id) {
                Ok(()) => {
                    debug!("Recording started in {:?}", recording_start_time.elapsed());
                    // Small delay to ensure microphone stream is active
                    let app_clone = app.clone();
                    let rm_clone = Arc::clone(&rm);
                    std::thread::spawn(move || {
                        std::thread::sleep(std::time::Duration::from_millis(100));
                        debug!("Handling delayed audio feedback/mute sequence");
                        // Helper handles disabled audio feedback by returning early, so we reuse it
                        // to keep mute sequencing consistent in every mode.
                        play_feedback_sound_blocking(&app_clone, SoundType::Start);
                        rm_clone.apply_mute();
                    });
                }
                Err(e) => {
                    debug!("Failed to start recording: {}", e);
                    recording_error = Some(e);
                }
            }
        }

        if recording_error.is_none() {
            // Dynamically register the cancel shortcut in a separate task to avoid deadlock
            shortcut::register_cancel_shortcut(app);
        } else {
            // Starting failed (for example due to blocked microphone permissions).
            // Revert UI state so we don't stay stuck in the recording overlay,
            // and drop the context: this dictation never happened.
            discard_dictation_context(app, &binding_id);
            utils::hide_recording_overlay(app);
            change_tray_icon(app, TrayIconState::Idle);
            if let Some(err) = recording_error {
                let error_type = if is_microphone_access_denied(&err) {
                    "microphone_permission_denied"
                } else if is_no_input_device_error(&err) {
                    "no_input_device"
                } else {
                    "unknown"
                };
                let _ = app.emit(
                    "recording-error",
                    RecordingErrorEvent {
                        error_type: error_type.to_string(),
                        detail: Some(err),
                    },
                );
            }
        }

        debug!(
            "TranscribeAction::start completed in {:?}",
            start_time.elapsed()
        );
    }

    fn stop(&self, app: &AppHandle, binding_id: &str, _shortcut_str: &str) {
        // Unregister the cancel shortcut when transcription stops
        shortcut::unregister_cancel_shortcut(app);

        let stop_time = Instant::now();
        debug!("TranscribeAction::stop called for binding: {}", binding_id);

        let ah = app.clone();
        let rm = Arc::clone(&app.state::<Arc<AudioRecordingManager>>());
        let tm = Arc::clone(&app.state::<Arc<TranscriptionManager>>());
        let hm = Arc::clone(&app.state::<Arc<HistoryManager>>());

        // The intent this dictation was started with. Everything below -- the
        // overlay badge, the output processing, the history record, the paste --
        // reads it, so they cannot disagree with each other or with what the
        // user asked for when they began speaking (#160).
        let context = take_dictation_context(app, binding_id, || {
            DictationContext::capture(
                self.raw,
                self.post_process,
                get_settings(app).raw_output,
                crate::output_target::backend::capture_delivery(app),
                crate::dictation_context::next_sequence(app),
            )
        });
        let post_process = context.post_process_requested();
        let effective_raw = context.effective_raw();

        change_tray_icon(app, TrayIconState::Transcribing);
        show_transcribing_overlay(app, effective_raw);

        // Unmute before playing audio feedback so the stop sound is audible
        rm.remove_mute();

        // Play audio feedback for recording stop
        play_feedback_sound(app, SoundType::Stop);

        let binding_id = binding_id.to_string(); // Clone binding_id for the async task

        tauri::async_runtime::spawn(async move {
            let _guard = FinishGuard(ah.clone());
            debug!(
                "Starting async transcription task for binding: {}",
                binding_id
            );

            let stop_recording_time = Instant::now();
            if let Some(samples) = rm.stop_recording(&binding_id) {
                debug!(
                    "Recording stopped and samples retrieved in {:?}, sample count: {}",
                    stop_recording_time.elapsed(),
                    samples.len()
                );

                if samples.is_empty() {
                    debug!("Recording produced no audio samples; skipping persistence");
                    utils::hide_recording_overlay(&ah);
                    change_tray_icon(&ah, TrayIconState::Idle);
                } else {
                    // Save WAV concurrently with transcription
                    let sample_count = samples.len();
                    let file_name = format!("handy-{}.wav", chrono::Utc::now().timestamp());
                    let wav_path = hm.recordings_dir().join(&file_name);
                    let wav_path_for_verify = wav_path.clone();
                    let samples_for_wav = samples.clone();
                    let wav_handle = tauri::async_runtime::spawn_blocking(move || {
                        crate::audio_toolkit::save_wav_file(&wav_path, &samples_for_wav)
                    });

                    // Transcribe concurrently with WAV save, under a watchdog
                    // so a wedged engine can't pin the "transcribing" UI state
                    // forever (issue #58). On timeout the result is treated as
                    // a normal transcription error, which reuses the existing
                    // recovery path below (overlay hidden, tray back to idle,
                    // empty history entry saved for retry).
                    let transcription_time = Instant::now();
                    let watchdog_timeout =
                        transcription_watchdog_timeout(sample_count, WHISPER_SAMPLE_RATE);
                    // Set when the failure already emitted its own specific
                    // event, so the generic `transcription-error` emit in the
                    // Err arm below doesn't double up the toasts.
                    let mut error_already_notified = false;
                    let transcription_result =
                        match tm.transcribe_with_watchdog(samples, watchdog_timeout) {
                            WatchdogOutcome::Completed(result) => result,
                            WatchdogOutcome::TimedOut => {
                                let timeout_secs = watchdog_timeout.as_secs();
                                let _ = ah.emit(
                                    "transcription-timeout",
                                    TranscriptionTimeoutEvent { timeout_secs },
                                );
                                error_already_notified = true;
                                Err(anyhow::anyhow!(
                                    "Transcription timed out after {}s",
                                    timeout_secs
                                ))
                            }
                            WatchdogOutcome::Panicked => Err(anyhow::anyhow!(
                                "Transcription worker panicked before producing a result"
                            )),
                        };

                    // Await WAV save and verify
                    let wav_saved = match wav_handle.await {
                        Ok(Ok(())) => {
                            match crate::audio_toolkit::verify_wav_file(
                                &wav_path_for_verify,
                                sample_count,
                            ) {
                                Ok(()) => true,
                                Err(e) => {
                                    error!("WAV verification failed: {}", e);
                                    false
                                }
                            }
                        }
                        Ok(Err(e)) => {
                            error!("Failed to save WAV file: {}", e);
                            false
                        }
                        Err(e) => {
                            error!("WAV save task panicked: {}", e);
                            false
                        }
                    };

                    match transcription_result {
                        Ok(transcription) => {
                            debug!(
                                "Transcription completed in {:?}: '{}'",
                                transcription_time.elapsed(),
                                transcription
                            );

                            if post_process {
                                show_processing_overlay(&ah, effective_raw);
                            }
                            let processed = process_transcription_output(
                                &ah,
                                &transcription,
                                post_process,
                                effective_raw,
                            )
                            .await;

                            if processed.final_text.is_empty() {
                                if wav_saved {
                                    if let Err(err) = hm.save_entry(
                                        file_name,
                                        if effective_raw {
                                            processed.final_text
                                        } else {
                                            transcription
                                        },
                                        post_process,
                                        effective_raw,
                                        processed.post_processed_text,
                                        processed.post_process_prompt,
                                    ) {
                                        error!("Failed to save history entry: {}", err);
                                    }
                                }
                                utils::hide_recording_overlay(&ah);
                                change_tray_icon(&ah, TrayIconState::Idle);
                            } else {
                                let final_text = processed.final_text;
                                let overflow = enqueue_transcript_delivery(
                                    ah.clone(),
                                    final_text.clone(),
                                    context,
                                );

                                if let Some(recovery_text) = overflow {
                                    // Queue saturation must not destroy the only copy of a
                                    // completed transcript. A recovery entry is starred so even a
                                    // zero history limit cannot immediately trim it. The text row
                                    // remains useful when the concurrent WAV write failed.
                                    let recovery_processed_text =
                                        if effective_raw || recovery_text == transcription {
                                            processed.post_processed_text
                                        } else {
                                            // Deterministic transforms such as Chinese-script
                                            // conversion can change final_text even when requested LLM
                                            // post-processing fails and leaves post_processed_text empty.
                                            Some(recovery_text.clone())
                                        };
                                    let primary_text = if effective_raw {
                                        recovery_text
                                    } else {
                                        transcription
                                    };
                                    if let Err(err) = hm.save_delivery_recovery(
                                        file_name,
                                        primary_text,
                                        post_process,
                                        effective_raw,
                                        recovery_processed_text,
                                        processed.post_process_prompt,
                                    ) {
                                        error!(
                                            "Failed to preserve queue overflow in history: {}",
                                            err
                                        );
                                    }
                                } else if wav_saved {
                                    // In raw mode the emitted (raw) text is the entry's primary
                                    // text so history shows and copies what the user actually
                                    // received; other modes keep the verbatim transcription as
                                    // primary and store any LLM-processed variant separately.
                                    let primary_text = if effective_raw {
                                        final_text
                                    } else {
                                        transcription
                                    };
                                    if let Err(err) = hm.save_entry(
                                        file_name,
                                        primary_text,
                                        post_process,
                                        effective_raw,
                                        processed.post_processed_text,
                                        processed.post_process_prompt,
                                    ) {
                                        error!("Failed to save history entry: {}", err);
                                    }
                                }
                            }
                        }
                        Err(err) => {
                            debug!("Global Shortcut Transcription error: {}", err);
                            // Surface the failure to the user (e.g. Parakeet
                            // refusing a recording past its length limit,
                            // issue #169) instead of only logging it.
                            let error_message = err.to_string();
                            if !error_already_notified
                                && !transcription_error_already_notified(&error_message)
                            {
                                let _ = ah.emit(
                                    "transcription-error",
                                    TranscriptionErrorEvent {
                                        message: error_message,
                                    },
                                );
                            }
                            // Save entry with empty text so user can retry
                            if wav_saved {
                                if let Err(save_err) = hm.save_entry(
                                    file_name,
                                    String::new(),
                                    post_process,
                                    effective_raw,
                                    None,
                                    None,
                                ) {
                                    error!("Failed to save failed history entry: {}", save_err);
                                }
                            }
                            utils::hide_recording_overlay(&ah);
                            change_tray_icon(&ah, TrayIconState::Idle);
                        }
                    }
                }
            } else {
                debug!("No samples retrieved from recording stop");
                utils::hide_recording_overlay(&ah);
                change_tray_icon(&ah, TrayIconState::Idle);
            }
        });

        debug!(
            "TranscribeAction::stop completed in {:?}",
            stop_time.elapsed()
        );
    }
}

// Cancel Action
struct CancelAction;

impl ShortcutAction for CancelAction {
    fn start(&self, app: &AppHandle, _binding_id: &str, _shortcut_str: &str) {
        // The cancel binding defaults to Escape, which is also how the one-shot
        // picker is dismissed (#124) -- and a global shortcut fires whatever the
        // focused window does with the key, so backing out of the picker would
        // otherwise throw away the recording underneath it. The picker owns that
        // gesture whichever side of the race gets there first: this closes a
        // picker still up, and stands down for one just dismissed by the same
        // key press. The recording keeps going either way.
        if crate::window_picker::backend::cancel_belongs_to_picker(app) {
            return;
        }
        utils::cancel_current_operation(app);
    }

    fn stop(&self, _app: &AppHandle, _binding_id: &str, _shortcut_str: &str) {
        // Nothing to do on stop for cancel
    }
}

// Target-lock toggle (#120). Pressing locks delivery to the window focused at
// that moment; pressing again releases it. Windows-only for now (#119), so the
// binding is registered only there.
#[cfg(target_os = "windows")]
struct ToggleTargetLockAction;

#[cfg(target_os = "windows")]
impl ShortcutAction for ToggleTargetLockAction {
    fn start(&self, app: &AppHandle, _binding_id: &str, _shortcut_str: &str) {
        crate::output_target::backend::toggle_target_lock(
            app,
            crate::output_target::CaptureSource::Shortcut,
        );
    }

    fn stop(&self, _app: &AppHandle, _binding_id: &str, _shortcut_str: &str) {
        // The lock flips on press; the release does nothing.
    }
}

// One-shot window picker (#124). Pressing opens the picker; choosing a window
// routes the NEXT transcript there and nothing after it. Windows-only for now
// (#119), like the target lock, so the binding is registered only there.
#[cfg(target_os = "windows")]
struct PickOutputWindowAction;

#[cfg(target_os = "windows")]
impl ShortcutAction for PickOutputWindowAction {
    fn start(&self, app: &AppHandle, _binding_id: &str, _shortcut_str: &str) {
        crate::window_picker::backend::open_picker(app);
    }

    fn stop(&self, _app: &AppHandle, _binding_id: &str, _shortcut_str: &str) {
        // The picker opens on press; the release does nothing.
    }
}

// Test Action
struct TestAction;

impl ShortcutAction for TestAction {
    fn start(&self, app: &AppHandle, binding_id: &str, shortcut_str: &str) {
        log::info!(
            "Shortcut ID '{}': Started - {} (App: {})", // Changed "Pressed" to "Started" for consistency
            binding_id,
            shortcut_str,
            app.package_info().name
        );
    }

    fn stop(&self, app: &AppHandle, binding_id: &str, shortcut_str: &str) {
        log::info!(
            "Shortcut ID '{}': Stopped - {} (App: {})", // Changed "Released" to "Stopped" for consistency
            binding_id,
            shortcut_str,
            app.package_info().name
        );
    }
}

// Static Action Map
pub static ACTION_MAP: Lazy<HashMap<String, Arc<dyn ShortcutAction>>> = Lazy::new(|| {
    let mut map = HashMap::new();
    map.insert(
        "transcribe".to_string(),
        Arc::new(TranscribeAction {
            post_process: false,
            raw: false,
        }) as Arc<dyn ShortcutAction>,
    );
    map.insert(
        "transcribe_with_post_process".to_string(),
        Arc::new(TranscribeAction {
            post_process: true,
            raw: false,
        }) as Arc<dyn ShortcutAction>,
    );
    map.insert(
        "transcribe_raw".to_string(),
        Arc::new(TranscribeAction {
            post_process: false,
            raw: true,
        }) as Arc<dyn ShortcutAction>,
    );
    map.insert(
        "cancel".to_string(),
        Arc::new(CancelAction) as Arc<dyn ShortcutAction>,
    );
    #[cfg(target_os = "windows")]
    map.insert(
        "toggle_target_lock".to_string(),
        Arc::new(ToggleTargetLockAction) as Arc<dyn ShortcutAction>,
    );
    #[cfg(target_os = "windows")]
    map.insert(
        "pick_output_window".to_string(),
        Arc::new(PickOutputWindowAction) as Arc<dyn ShortcutAction>,
    );
    map.insert(
        "test".to_string(),
        Arc::new(TestAction) as Arc<dyn ShortcutAction>,
    );
    map
});

#[cfg(test)]
mod tests {
    use super::{force_english_i_casing, transcription_error_already_notified};
    use crate::managers::engine_limits::{
        MODEL_AUTO_LOAD_FAILED_ERROR, MODEL_NOT_LOADED_ERROR, WEDGED_ENGINE_ERROR,
    };

    #[test]
    fn suppresses_generic_toasts_for_model_failures_with_specific_events() {
        // A missing engine can also be caused by a manual unload or active
        // model deletion, neither of which emits `loading_failed`.
        assert!(!transcription_error_already_notified(
            MODEL_NOT_LOADED_ERROR
        ));
        assert!(transcription_error_already_notified(
            MODEL_AUTO_LOAD_FAILED_ERROR
        ));
        assert!(transcription_error_already_notified(WEDGED_ENGINE_ERROR));
        assert!(!transcription_error_already_notified(
            "parakeet_input_too_long:391"
        ));
    }

    #[test]
    fn force_english_i_casing_forces_when_translating() {
        // Translation emits English regardless of the selected dictation language.
        assert!(force_english_i_casing(true, "de"));
        assert!(force_english_i_casing(true, "auto"));
    }

    #[test]
    fn force_english_i_casing_forces_for_explicit_english() {
        // An explicitly selected English language forces English "I" casing, including region tags.
        assert!(force_english_i_casing(false, "en"));
        assert!(force_english_i_casing(false, "en-US"));
    }

    #[test]
    fn force_english_i_casing_defers_for_auto_and_non_english() {
        // Auto-detect can't tell us the language, and an explicit non-English language is not
        // English, so neither forces English rules -- strip_to_raw_text preserves the engine casing.
        assert!(!force_english_i_casing(false, "auto"));
        assert!(!force_english_i_casing(false, "fr"));
        assert!(!force_english_i_casing(false, "de"));
    }
}
