//! The language contract shared by deterministic text-pipeline stages.
//!
//! A selected dictation language is a usable signal. `auto` is not: the current
//! transcription result does not expose the engine's detected language. Keeping
//! that distinction explicit prevents an English-only formatter from silently
//! treating an unknown or non-English transcript as English.

/// The language of the text entering the formatting pipeline.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextPipelineLanguage(Option<String>);

impl TextPipelineLanguage {
    /// Derives the output language from the selected settings and model contract.
    /// Translation always emits English. A selectable model can retain an explicit
    /// dictation language. An auto-detect-only model uses only a fixed language.
    pub fn from_transcription_settings(
        selected_language: &str,
        translation_is_effective: bool,
        supports_language_selection: bool,
        fixed_model_language: Option<&str>,
    ) -> Self {
        if translation_is_effective {
            return Self::english();
        }
        if supports_language_selection {
            let selected = Self::known(selected_language);
            if selected.tag().is_some() {
                return selected;
            }
        }
        fixed_model_language.map_or_else(Self::unknown, Self::known)
    }

    pub fn known(language_tag: &str) -> Self {
        let tag = language_tag.trim();
        if tag.is_empty() || tag.eq_ignore_ascii_case("auto") {
            Self::unknown()
        } else {
            Self(Some(tag.to_string()))
        }
    }

    pub fn unknown() -> Self {
        Self(None)
    }

    pub fn english() -> Self {
        Self(Some("en".to_string()))
    }

    pub fn tag(&self) -> Option<&str> {
        self.0.as_deref()
    }

    pub fn base_language(&self) -> Option<&str> {
        self.tag()
            .map(|tag| tag.split(&['-', '_'][..]).next().unwrap_or(tag))
    }

    pub fn is_english(&self) -> bool {
        self.base_language()
            .is_some_and(|language| language.eq_ignore_ascii_case("en"))
    }
}

#[cfg(test)]
mod tests {
    use super::TextPipelineLanguage;
    use crate::audio_toolkit::text::extract_learned_replacements;
    use crate::audio_toolkit::{
        apply_spoken_punctuation, filter_transcription_output, format_numbers, strip_to_raw_text,
    };

    #[test]
    fn derives_the_effective_output_language_once() {
        assert_eq!(
            TextPipelineLanguage::from_transcription_settings("fr-CA", false, true, None).tag(),
            Some("fr-CA")
        );
        assert_eq!(
            TextPipelineLanguage::from_transcription_settings("auto", false, true, None).tag(),
            None
        );
        assert_eq!(
            TextPipelineLanguage::from_transcription_settings("fr", true, true, None).tag(),
            Some("en")
        );
        assert_eq!(
            TextPipelineLanguage::from_transcription_settings("auto", false, false, Some("ru"))
                .tag(),
            Some("ru")
        );
        assert_eq!(
            TextPipelineLanguage::from_transcription_settings("fr", false, false, None).tag(),
            None
        );
        assert!(TextPipelineLanguage::known("en-US").is_english());
        assert!(!TextPipelineLanguage::known("pl").is_english());
    }

    #[test]
    fn one_language_signal_survives_the_full_text_pipeline() {
        let english = TextPipelineLanguage::known("en-US");
        let filtered = filter_transcription_output("um twenty five question mark", &english, &None);
        let raw = strip_to_raw_text(&filtered, &english);
        let numbered = format_numbers(&raw, &english);
        assert_eq!(apply_spoken_punctuation(&numbered, &english), "25?");
        assert_eq!(
            extract_learned_replacements("ask clawed", "ask Claude", &english, &[]).len(),
            1
        );

        let french = TextPipelineLanguage::known("fr");
        let filtered = filter_transcription_output("um twenty five question mark", &french, &None);
        let raw = strip_to_raw_text(&filtered, &french);
        let numbered = format_numbers(&raw, &french);
        assert_eq!(
            apply_spoken_punctuation(&numbered, &french),
            "um twenty five question mark"
        );
        assert!(extract_learned_replacements(
            "je vois la maison",
            "je vois le maison",
            &french,
            &[],
        )
        .is_empty());
    }
}
