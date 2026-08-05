//! Input-length limits for engines whose models bake in a maximum sequence
//! length. Kept out of transcription.rs so CI (which swaps that file for
//! transcription_mock.rs) still compiles and runs these tests.

use crate::audio_toolkit::constants::WHISPER_SAMPLE_RATE;

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

/// Refuses Parakeet input that would be silently truncated, so the failure
/// surfaces to the user instead of producing an incomplete transcript that
/// looks complete.
pub fn check_parakeet_input_length(sample_count: usize) -> Result<(), String> {
    if sample_count <= PARAKEET_MAX_INPUT_SAMPLES {
        return Ok(());
    }

    let total_seconds = sample_count / WHISPER_SAMPLE_RATE as usize;
    Err(format!(
        "This recording is {}:{:02} long. The Parakeet engine cannot transcribe \
         recordings longer than about 6:30; audio past that point would be \
         silently dropped from the transcript. Switch to a Whisper model for \
         long recordings.",
        total_seconds / 60,
        total_seconds % 60,
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
        assert!(message.contains("6:30"));
        assert!(message.contains("silently dropped"));
        assert!(message.contains("Whisper"));
    }
}
