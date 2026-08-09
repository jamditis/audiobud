//! Input-length limits for engines whose models bake in a maximum sequence
//! length. Kept out of transcription.rs so CI (which swaps that file for
//! transcription_mock.rs) still compiles and runs these tests.

use crate::audio_toolkit::constants::WHISPER_SAMPLE_RATE;
use std::sync::atomic::{AtomicBool, Ordering};

/// Maximum input length (mono 16 kHz samples) the Parakeet engines transcribe
/// completely.
///
/// The shipped parakeet-tdt-0.6b ONNX exports carry NeMo Conformer positional
/// embeddings covering 5000 encoder frames. At 8x subsampling and a 10 ms
/// frame step that is 5000 * 8 * 10 ms = 400 s of audio; past it the encoder
/// output degrades to blanks and the tail of the recording is silently
/// dropped from the transcript (issue #169). The limit sits 10 s below the
/// theoretical cutoff to leave room for mel padding.
pub const PARAKEET_MAX_INPUT_SAMPLES: usize = 390 * 16000;

/// Stable error contract parsed by the frontend so the explanation can be
/// localized instead of displaying backend English.
pub const PARAKEET_INPUT_TOO_LONG_PREFIX: &str = "parakeet_input_too_long:";

// Shared transcription failure contracts live here rather than in
// transcription.rs because CI replaces that manager with
// transcription_mock.rs. Callers that compile in both modes must not import
// definitions from the swapped implementation.
/// Error used when a transcription or model load is refused because an
/// earlier transcription timed out and its worker still holds an engine.
/// Kept consistent with the `errors.transcriptionTimeout` toast copy: both
/// point at restarting AudioBud, since retrying or switching models is
/// refused while the engine is stuck.
pub(crate) const WEDGED_ENGINE_ERROR: &str =
    "The transcription engine is stuck from an earlier timeout. Restart AudioBud to recover.";
pub(crate) const MODEL_NOT_LOADED_ERROR: &str = "Model is not loaded for transcription.";
pub(crate) const MODEL_AUTO_LOAD_FAILED_ERROR: &str =
    "Model failed to load after auto-load attempt. Please check your model settings.";

/// Tracks whether the most recent missing engine was preceded by a
/// `model-state-changed/loading_failed` event. A missing engine can also be
/// caused by a manual unload or active-model deletion; those paths have not
/// already notified the user and must retain the generic transcription error.
#[derive(Default)]
pub(crate) struct LoadFailureNotification {
    emitted: AtomicBool,
}

impl LoadFailureNotification {
    pub(crate) fn clear(&self) {
        self.emitted.store(false, Ordering::SeqCst);
    }

    pub(crate) fn record_emission(&self, emitted: bool) {
        if emitted {
            self.emitted.store(true, Ordering::SeqCst);
        }
    }

    pub(crate) fn take_missing_engine_error(&self) -> &'static str {
        if self.emitted.swap(false, Ordering::SeqCst) {
            MODEL_AUTO_LOAD_FAILED_ERROR
        } else {
            MODEL_NOT_LOADED_ERROR
        }
    }
}

/// Refuses Parakeet input that would be silently truncated, so the failure
/// surfaces to the user instead of producing an incomplete transcript that
/// looks complete.
pub fn check_parakeet_input_length(sample_count: usize) -> Result<(), String> {
    if sample_count <= PARAKEET_MAX_INPUT_SAMPLES {
        return Ok(());
    }

    let total_seconds = sample_count / WHISPER_SAMPLE_RATE as usize;
    Err(format!(
        "{}{}",
        PARAKEET_INPUT_TOO_LONG_PREFIX, total_seconds
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_input_just_under_the_limit() {
        assert!(check_parakeet_input_length(PARAKEET_MAX_INPUT_SAMPLES - 1).is_ok());
    }

    #[test]
    fn accepts_input_exactly_at_the_limit() {
        assert!(check_parakeet_input_length(PARAKEET_MAX_INPUT_SAMPLES).is_ok());
    }

    #[test]
    fn rejects_input_just_over_the_limit() {
        let message = check_parakeet_input_length(PARAKEET_MAX_INPUT_SAMPLES + 1)
            .expect_err("input past the limit must be refused");
        assert_eq!(message, "parakeet_input_too_long:390");
    }

    #[test]
    fn only_classifies_missing_engine_as_notified_after_a_load_failure_event() {
        let notification = LoadFailureNotification::default();

        assert_eq!(
            notification.take_missing_engine_error(),
            MODEL_NOT_LOADED_ERROR
        );

        notification.record_emission(false);
        assert_eq!(
            notification.take_missing_engine_error(),
            MODEL_NOT_LOADED_ERROR,
            "a failed event emit must not suppress the visible error"
        );

        notification.record_emission(true);
        assert_eq!(
            notification.take_missing_engine_error(),
            MODEL_AUTO_LOAD_FAILED_ERROR
        );
        assert_eq!(
            notification.take_missing_engine_error(),
            MODEL_NOT_LOADED_ERROR,
            "the notification applies to one matching transcription failure"
        );

        notification.record_emission(true);
        notification.clear();
        assert_eq!(
            notification.take_missing_engine_error(),
            MODEL_NOT_LOADED_ERROR,
            "a manual unload clears stale load-failure notification state"
        );
    }
}
