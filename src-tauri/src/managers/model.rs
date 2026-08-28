use crate::settings::{get_settings, write_settings};
use anyhow::Result;
use flate2::read::GzDecoder;
use futures_util::StreamExt;
use log::{debug, info, warn};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use specta::Type;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tar::Archive;
use tauri::{AppHandle, Emitter, Manager};

const MODEL_BASE_URL: &str =
    "https://github.com/jamditis/audiobud/releases/download/model-assets-v1";

/// Consecutive progress-emit failures after which the frontend is treated as
/// unreachable and the download is aborted, so the download UI cannot stall
/// forever waiting for events that will never arrive.
const MAX_CONSECUTIVE_PROGRESS_EMIT_FAILURES: u32 = 3;
// Machine-readable lifecycle errors consumed by src/stores/modelStore.ts.
const MODEL_DOWNLOAD_ALREADY_ACTIVE: &str = "model_download_already_active";
const MODEL_DOWNLOAD_CANCELLING: &str = "model_download_cancelling";
const MODEL_DOWNLOAD_NOTIFY_FAILED: &str = "model_download_notify_failed";
const MODEL_DOWNLOAD_NOT_ACTIVE: &str = "model_download_not_active";
const MODEL_DOWNLOAD_CANCEL_TOO_LATE: &str = "model_download_cancel_too_late";
const MODEL_DOWNLOAD_STALLED: &str = "model_download_stalled";

const DOWNLOAD_ACTIVE: u8 = 0;
const DOWNLOAD_CANCELLED: u8 = 1;
const DOWNLOAD_COMMITTING: u8 = 2;

#[derive(Debug, PartialEq, Eq)]
enum CancelRequest {
    Accepted,
    AlreadyCancelled,
    TooLate,
}

#[derive(Debug)]
struct DownloadControl {
    state: AtomicU8,
}

impl DownloadControl {
    fn new() -> Self {
        Self {
            state: AtomicU8::new(DOWNLOAD_ACTIVE),
        }
    }

    fn is_cancelled(&self) -> bool {
        self.state.load(Ordering::Acquire) == DOWNLOAD_CANCELLED
    }

    fn request_cancel(&self) -> CancelRequest {
        match self.state.compare_exchange(
            DOWNLOAD_ACTIVE,
            DOWNLOAD_CANCELLED,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => CancelRequest::Accepted,
            Err(DOWNLOAD_CANCELLED) => CancelRequest::AlreadyCancelled,
            Err(DOWNLOAD_COMMITTING) => CancelRequest::TooLate,
            Err(state) => unreachable!("unexpected download state: {state}"),
        }
    }

    fn begin_commit(&self) -> bool {
        match self.state.compare_exchange(
            DOWNLOAD_ACTIVE,
            DOWNLOAD_COMMITTING,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) | Err(DOWNLOAD_COMMITTING) => true,
            Err(DOWNLOAD_CANCELLED) => false,
            Err(state) => unreachable!("unexpected download state: {state}"),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum DownloadReadDisposition {
    Cancelled,
    Failed(&'static str),
    Continue,
}

fn classify_download_read(cancelled: bool, timed_out: bool) -> DownloadReadDisposition {
    if cancelled {
        DownloadReadDisposition::Cancelled
    } else if timed_out {
        DownloadReadDisposition::Failed(MODEL_DOWNLOAD_STALLED)
    } else {
        DownloadReadDisposition::Continue
    }
}

fn resolve_download_read<T>(
    model_id: &str,
    cancel_flag: &DownloadControl,
    result: std::result::Result<T, reqwest::Error>,
) -> Result<Option<T>> {
    let timed_out = result
        .as_ref()
        .err()
        .map(reqwest::Error::is_timeout)
        .unwrap_or(false);
    match classify_download_read(cancel_flag.is_cancelled(), timed_out) {
        DownloadReadDisposition::Cancelled => Ok(None),
        DownloadReadDisposition::Failed(code) => {
            warn!("Model download read timed out for: {}", model_id);
            Err(anyhow::anyhow!(code))
        }
        DownloadReadDisposition::Continue => result.map(Some).map_err(Into::into),
    }
}

fn model_url(filename: &str) -> String {
    format!("{MODEL_BASE_URL}/{filename}")
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub enum EngineType {
    Whisper,
    Parakeet,
    Moonshine,
    MoonshineStreaming,
    SenseVoice,
    GigaAM,
    Canary,
    Cohere,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub filename: String,
    pub url: Option<String>,
    pub sha256: Option<String>,
    pub size_mb: u64,
    pub is_downloaded: bool,
    pub is_downloading: bool,
    pub partial_size: u64,
    pub is_directory: bool,
    pub engine_type: EngineType,
    pub accuracy_score: f32,        // 0.0 to 1.0, higher is more accurate
    pub speed_score: f32,           // 0.0 to 1.0, higher is faster
    pub supports_translation: bool, // Whether the model supports translating to English
    pub is_recommended: bool,       // Whether this is the recommended model for new users
    pub supported_languages: Vec<String>, // Languages this model can transcribe
    pub supports_language_selection: bool, // Whether the user can explicitly pick a language
    pub is_custom: bool,            // Whether this is a user-provided custom model
}

fn recommended_model_for_platform(model_id: &str, platform: &str) -> bool {
    match platform {
        "windows" => model_id == "parakeet-tdt-0.6b-v3",
        "macos" => model_id == "turbo",
        _ => false,
    }
}

fn is_recommended_model(model_id: &str) -> bool {
    recommended_model_for_platform(model_id, std::env::consts::OS)
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct DownloadProgress {
    pub model_id: String,
    pub downloaded: u64,
    pub total: u64,
    pub percentage: f64,
}

/// Releases a worker's two-part ownership record while holding the same lock
/// order as the claim path. A stale worker must never clear a newer worker's
/// model flag or cancellation handle.
fn release_download_claim(
    available_models: &Mutex<HashMap<String, ModelInfo>>,
    cancel_flags: &Arc<Mutex<HashMap<String, Arc<DownloadControl>>>>,
    model_id: &str,
    cancel_flag: &Arc<DownloadControl>,
    completed: bool,
) -> bool {
    let mut models = available_models.lock().unwrap();
    let mut flags = cancel_flags.lock().unwrap();
    let still_owns_download = flags
        .get(model_id)
        .map(|flag| Arc::ptr_eq(flag, cancel_flag))
        .unwrap_or(false);
    if !still_owns_download {
        return false;
    }

    // Completion and cancellation race at the final filesystem move. While
    // this exact claim is still registered, cancellation wins: leave cleanup
    // armed so it can release the claim after the caller removes the newly
    // installed artifact.
    if completed && cancel_flag.is_cancelled() {
        return false;
    }

    if let Some(model) = models.get_mut(model_id) {
        model.is_downloading = false;
        if completed {
            model.is_downloaded = true;
            model.partial_size = 0;
        }
    }
    flags.remove(model_id);
    true
}

/// RAII guard that cleans up download state (`is_downloading` flag and cancel flag)
/// when dropped, unless explicitly disarmed. This ensures consistent cleanup on
/// every error path without requiring manual cleanup at each `?` or `return Err`.
struct DownloadCleanup<'a> {
    available_models: &'a Mutex<HashMap<String, ModelInfo>>,
    cancel_flags: &'a Arc<Mutex<HashMap<String, Arc<DownloadControl>>>>,
    cancel_flag: Arc<DownloadControl>,
    model_id: String,
    disarmed: bool,
}

impl<'a> Drop for DownloadCleanup<'a> {
    fn drop(&mut self) {
        if self.disarmed {
            return;
        }
        release_download_claim(
            self.available_models,
            self.cancel_flags,
            &self.model_id,
            &self.cancel_flag,
            false,
        );
    }
}

pub struct ModelManager {
    app_handle: AppHandle,
    models_dir: PathBuf,
    available_models: Mutex<HashMap<String, ModelInfo>>,
    cancel_flags: Arc<Mutex<HashMap<String, Arc<DownloadControl>>>>,
    extracting_models: Arc<Mutex<HashSet<String>>>,
}

impl ModelManager {
    pub fn new(app_handle: &AppHandle) -> Result<Self> {
        // Create models directory in app data
        let models_dir = crate::portable::app_data_dir(app_handle)
            .map_err(|e| anyhow::anyhow!("Failed to get app data dir: {}", e))?
            .join("models");

        if !models_dir.exists() {
            fs::create_dir_all(&models_dir)?;
        }

        let mut available_models = HashMap::new();

        // Whisper supported languages (99 languages from tokenizer)
        // Including zh-Hans and zh-Hant variants to match frontend language codes
        let whisper_languages: Vec<String> = vec![
            "en", "zh", "zh-Hans", "zh-Hant", "de", "es", "ru", "ko", "fr", "ja", "pt", "tr", "pl",
            "ca", "nl", "ar", "sv", "it", "id", "hi", "fi", "vi", "he", "uk", "el", "ms", "cs",
            "ro", "da", "hu", "ta", "no", "th", "ur", "hr", "bg", "lt", "la", "mi", "ml", "cy",
            "sk", "te", "fa", "lv", "bn", "sr", "az", "sl", "kn", "et", "mk", "br", "eu", "is",
            "hy", "ne", "mn", "bs", "kk", "sq", "sw", "gl", "mr", "pa", "si", "km", "sn", "yo",
            "so", "af", "oc", "ka", "be", "tg", "sd", "gu", "am", "yi", "lo", "uz", "fo", "ht",
            "ps", "tk", "nn", "mt", "sa", "lb", "my", "bo", "tl", "mg", "as", "tt", "haw", "ln",
            "ha", "ba", "jw", "su", "yue",
        ]
        .into_iter()
        .map(String::from)
        .collect();

        // TODO this should be read from a JSON file or something..
        available_models.insert(
            "small".to_string(),
            ModelInfo {
                id: "small".to_string(),
                name: "Whisper Small".to_string(),
                description: "Fast and fairly accurate.".to_string(),
                filename: "ggml-small.bin".to_string(),
                url: Some(model_url("ggml-small.bin")),
                sha256: Some(
                    "1be3a9b2063867b937e64e2ec7483364a79917e157fa98c5d94b5c1fffea987b".to_string(),
                ),
                size_mb: 465,
                is_downloaded: false,
                is_downloading: false,
                partial_size: 0,
                is_directory: false,
                engine_type: EngineType::Whisper,
                accuracy_score: 0.60,
                speed_score: 0.85,
                supports_translation: true,
                is_recommended: false,
                supported_languages: whisper_languages.clone(),
                supports_language_selection: true,
                is_custom: false,
            },
        );

        // Add downloadable models
        available_models.insert(
            "medium".to_string(),
            ModelInfo {
                id: "medium".to_string(),
                name: "Whisper Medium".to_string(),
                description: "Good accuracy, medium speed".to_string(),
                filename: "whisper-medium-q4_1.bin".to_string(),
                url: Some(model_url("whisper-medium-q4_1.bin")),
                sha256: Some(
                    "79283fc1f9fe12ca3248543fbd54b73292164d8df5a16e095e2bceeaaabddf57".to_string(),
                ),
                size_mb: 469,
                is_downloaded: false,
                is_downloading: false,
                partial_size: 0,
                is_directory: false,
                engine_type: EngineType::Whisper,
                accuracy_score: 0.75,
                speed_score: 0.60,
                supports_translation: true,
                is_recommended: false,
                supported_languages: whisper_languages.clone(),
                supports_language_selection: true,
                is_custom: false,
            },
        );

        available_models.insert(
            "turbo".to_string(),
            ModelInfo {
                id: "turbo".to_string(),
                name: "Whisper Turbo".to_string(),
                description: "Balanced accuracy and speed.".to_string(),
                filename: "ggml-large-v3-turbo.bin".to_string(),
                url: Some(model_url("ggml-large-v3-turbo.bin")),
                sha256: Some(
                    "1fc70f774d38eb169993ac391eea357ef47c88757ef72ee5943879b7e8e2bc69".to_string(),
                ),
                size_mb: 1549,
                is_downloaded: false,
                is_downloading: false,
                partial_size: 0,
                is_directory: false,
                engine_type: EngineType::Whisper,
                accuracy_score: 0.80,
                speed_score: 0.40,
                supports_translation: false, // Turbo doesn't support translation
                is_recommended: is_recommended_model("turbo"),
                supported_languages: whisper_languages.clone(),
                supports_language_selection: true,
                is_custom: false,
            },
        );

        available_models.insert(
            "large".to_string(),
            ModelInfo {
                id: "large".to_string(),
                name: "Whisper Large".to_string(),
                description: "Good accuracy, but slow.".to_string(),
                filename: "ggml-large-v3-q5_0.bin".to_string(),
                url: Some(model_url("ggml-large-v3-q5_0.bin")),
                sha256: Some(
                    "d75795ecff3f83b5faa89d1900604ad8c780abd5739fae406de19f23ecd98ad1".to_string(),
                ),
                size_mb: 1031,
                is_downloaded: false,
                is_downloading: false,
                partial_size: 0,
                is_directory: false,
                engine_type: EngineType::Whisper,
                accuracy_score: 0.85,
                speed_score: 0.30,
                supports_translation: true,
                is_recommended: false,
                supported_languages: whisper_languages.clone(),
                supports_language_selection: true,
                is_custom: false,
            },
        );

        available_models.insert(
            "breeze-asr".to_string(),
            ModelInfo {
                id: "breeze-asr".to_string(),
                name: "Breeze ASR".to_string(),
                description: "Optimized for Taiwanese Mandarin. Code-switching support."
                    .to_string(),
                filename: "breeze-asr-q5_k.bin".to_string(),
                url: Some(model_url("breeze-asr-q5_k.bin")),
                sha256: Some(
                    "8efbf0ce8a3f50fe332b7617da787fb81354b358c288b008d3bdef8359df64c6".to_string(),
                ),
                size_mb: 1030,
                is_downloaded: false,
                is_downloading: false,
                partial_size: 0,
                is_directory: false,
                engine_type: EngineType::Whisper,
                accuracy_score: 0.85,
                speed_score: 0.35,
                supports_translation: false,
                is_recommended: false,
                supported_languages: whisper_languages,
                supports_language_selection: true,
                is_custom: false,
            },
        );

        // Add NVIDIA Parakeet models (directory-based)
        available_models.insert(
            "parakeet-tdt-0.6b-v2".to_string(),
            ModelInfo {
                id: "parakeet-tdt-0.6b-v2".to_string(),
                name: "Parakeet V2".to_string(),
                description: "English only. The best model for English speakers.".to_string(),
                filename: "parakeet-tdt-0.6b-v2-int8".to_string(), // Directory name
                url: Some(model_url("parakeet-v2-int8.tar.gz")),
                sha256: Some(
                    "ac9b9429984dd565b25097337a887bb7f0f8ac393573661c651f0e7d31563991".to_string(),
                ),
                size_mb: 451,
                is_downloaded: false,
                is_downloading: false,
                partial_size: 0,
                is_directory: true,
                engine_type: EngineType::Parakeet,
                accuracy_score: 0.85,
                speed_score: 0.85,
                supports_translation: false,
                is_recommended: false,
                supported_languages: vec!["en".to_string()],
                supports_language_selection: false,
                is_custom: false,
            },
        );

        // Parakeet V3 supported languages (25 EU languages + Russian/Ukrainian):
        // bg, hr, cs, da, nl, en, et, fi, fr, de, el, hu, it, lv, lt, mt, pl, pt, ro, sk, sl, es, sv, ru, uk
        let parakeet_v3_languages: Vec<String> = vec![
            "bg", "hr", "cs", "da", "nl", "en", "et", "fi", "fr", "de", "el", "hu", "it", "lv",
            "lt", "mt", "pl", "pt", "ro", "sk", "sl", "es", "sv", "ru", "uk",
        ]
        .into_iter()
        .map(String::from)
        .collect();

        available_models.insert(
            "parakeet-tdt-0.6b-v3".to_string(),
            ModelInfo {
                id: "parakeet-tdt-0.6b-v3".to_string(),
                name: "Parakeet V3".to_string(),
                description: "Fast and accurate. Supports 25 European languages.".to_string(),
                filename: "parakeet-tdt-0.6b-v3-int8".to_string(), // Directory name
                url: Some(model_url("parakeet-v3-int8.tar.gz")),
                sha256: Some(
                    "43d37191602727524a7d8c6da0eef11c4ba24320f5b4730f1a2497befc2efa77".to_string(),
                ),
                size_mb: 456,
                is_downloaded: false,
                is_downloading: false,
                partial_size: 0,
                is_directory: true,
                engine_type: EngineType::Parakeet,
                accuracy_score: 0.80,
                speed_score: 0.85,
                supports_translation: false,
                is_recommended: is_recommended_model("parakeet-tdt-0.6b-v3"),
                supported_languages: parakeet_v3_languages,
                supports_language_selection: false,
                is_custom: false,
            },
        );

        available_models.insert(
            "moonshine-base".to_string(),
            ModelInfo {
                id: "moonshine-base".to_string(),
                name: "Moonshine Base".to_string(),
                description: "Very fast, English only. Handles accents well.".to_string(),
                filename: "moonshine-base".to_string(),
                url: Some(model_url("moonshine-base.tar.gz")),
                sha256: Some(
                    "04bf6ab012cfceebd4ac7cf88c1b31d027bbdd3cd704649b692e2e935236b7e8".to_string(),
                ),
                size_mb: 55,
                is_downloaded: false,
                is_downloading: false,
                partial_size: 0,
                is_directory: true,
                engine_type: EngineType::Moonshine,
                accuracy_score: 0.70,
                speed_score: 0.90,
                supports_translation: false,
                is_recommended: false,
                supported_languages: vec!["en".to_string()],
                supports_language_selection: false,
                is_custom: false,
            },
        );

        available_models.insert(
            "moonshine-tiny-streaming-en".to_string(),
            ModelInfo {
                id: "moonshine-tiny-streaming-en".to_string(),
                name: "Moonshine V2 Tiny".to_string(),
                description: "Ultra-fast, English only".to_string(),
                filename: "moonshine-tiny-streaming-en".to_string(),
                url: Some(model_url("moonshine-tiny-streaming-en.tar.gz")),
                sha256: Some(
                    "465addcfca9e86117415677dfdc98b21edc53537210333a3ecdb58509a80abaf".to_string(),
                ),
                size_mb: 31,
                is_downloaded: false,
                is_downloading: false,
                partial_size: 0,
                is_directory: true,
                engine_type: EngineType::MoonshineStreaming,
                accuracy_score: 0.55,
                speed_score: 0.95,
                supports_translation: false,
                is_recommended: false,
                supported_languages: vec!["en".to_string()],
                supports_language_selection: false,
                is_custom: false,
            },
        );

        available_models.insert(
            "moonshine-small-streaming-en".to_string(),
            ModelInfo {
                id: "moonshine-small-streaming-en".to_string(),
                name: "Moonshine V2 Small".to_string(),
                description: "Fast, English only. Good balance of speed and accuracy.".to_string(),
                filename: "moonshine-small-streaming-en".to_string(),
                url: Some(model_url("moonshine-small-streaming-en.tar.gz")),
                sha256: Some(
                    "dbb3e1c1832bd88a4ac712f7449a136cc2c9a18c5fe33a12ed1b7cb1cfe9cdd5".to_string(),
                ),
                size_mb: 99,
                is_downloaded: false,
                is_downloading: false,
                partial_size: 0,
                is_directory: true,
                engine_type: EngineType::MoonshineStreaming,
                accuracy_score: 0.65,
                speed_score: 0.90,
                supports_translation: false,
                is_recommended: false,
                supported_languages: vec!["en".to_string()],
                supports_language_selection: false,
                is_custom: false,
            },
        );

        available_models.insert(
            "moonshine-medium-streaming-en".to_string(),
            ModelInfo {
                id: "moonshine-medium-streaming-en".to_string(),
                name: "Moonshine V2 Medium".to_string(),
                description: "English only. High quality.".to_string(),
                filename: "moonshine-medium-streaming-en".to_string(),
                url: Some(model_url("moonshine-medium-streaming-en.tar.gz")),
                sha256: Some(
                    "07a66f3bff1c77e75a2f637e5a263928a08baae3c29c4c053fc968a9a9373d13".to_string(),
                ),
                size_mb: 192,
                is_downloaded: false,
                is_downloading: false,
                partial_size: 0,
                is_directory: true,
                engine_type: EngineType::MoonshineStreaming,
                accuracy_score: 0.75,
                speed_score: 0.80,
                supports_translation: false,
                is_recommended: false,
                supported_languages: vec!["en".to_string()],
                supports_language_selection: false,
                is_custom: false,
            },
        );

        // SenseVoice supported languages
        let sense_voice_languages: Vec<String> =
            vec!["zh", "zh-Hans", "zh-Hant", "en", "yue", "ja", "ko"]
                .into_iter()
                .map(String::from)
                .collect();

        available_models.insert(
            "sense-voice-int8".to_string(),
            ModelInfo {
                id: "sense-voice-int8".to_string(),
                name: "SenseVoice".to_string(),
                description: "Very fast. Chinese, English, Japanese, Korean, Cantonese."
                    .to_string(),
                filename: "sense-voice-int8".to_string(),
                url: Some(model_url("sense-voice-int8.tar.gz")),
                sha256: Some(
                    "171d611fe5d353a50bbb741b6f3ef42559b1565685684e9aa888ef563ba3e8a4".to_string(),
                ),
                size_mb: 152,
                is_downloaded: false,
                is_downloading: false,
                partial_size: 0,
                is_directory: true,
                engine_type: EngineType::SenseVoice,
                accuracy_score: 0.65,
                speed_score: 0.95,
                supports_translation: false,
                is_recommended: false,
                supported_languages: sense_voice_languages,
                supports_language_selection: true,
                is_custom: false,
            },
        );

        // GigaAM v3 supported languages
        let gigaam_languages: Vec<String> = vec!["ru"].into_iter().map(String::from).collect();

        available_models.insert(
            "gigaam-v3-e2e-ctc".to_string(),
            ModelInfo {
                id: "gigaam-v3-e2e-ctc".to_string(),
                name: "GigaAM v3".to_string(),
                description: "Russian speech recognition. Fast and accurate.".to_string(),
                filename: "giga-am-v3-int8".to_string(),
                url: Some(model_url("giga-am-v3-int8.tar.gz")),
                sha256: Some(
                    "d872462268430db140b69b72e0fc4b787b194c1dbe51b58de39444d55b6da45b".to_string(),
                ),
                size_mb: 151,
                is_downloaded: false,
                is_downloading: false,
                partial_size: 0,
                is_directory: true,
                engine_type: EngineType::GigaAM,
                accuracy_score: 0.85,
                speed_score: 0.75,
                supports_translation: false,
                is_recommended: false,
                supported_languages: gigaam_languages,
                supports_language_selection: false,
                is_custom: false,
            },
        );

        // Canary 180m Flash supported languages (4 languages)
        let canary_flash_languages: Vec<String> = vec!["en", "de", "es", "fr"]
            .into_iter()
            .map(String::from)
            .collect();

        available_models.insert(
            "canary-180m-flash".to_string(),
            ModelInfo {
                id: "canary-180m-flash".to_string(),
                name: "Canary 180M Flash".to_string(),
                description: "Very fast. English, German, Spanish, French. Supports translation."
                    .to_string(),
                filename: "canary-180m-flash".to_string(),
                url: Some(model_url("canary-180m-flash.tar.gz")),
                sha256: Some(
                    "6d9cfca6118b296e196eaedc1c8fa9788305a7b0f1feafdb6dc91932ab6e53f7".to_string(),
                ),
                size_mb: 146,
                is_downloaded: false,
                is_downloading: false,
                partial_size: 0,
                is_directory: true,
                engine_type: EngineType::Canary,
                accuracy_score: 0.75,
                speed_score: 0.85,
                supports_translation: true,
                is_recommended: false,
                supported_languages: canary_flash_languages,
                supports_language_selection: true,
                is_custom: false,
            },
        );

        // Canary 1B v2 supported languages (25 EU languages)
        let canary_1b_languages: Vec<String> = vec![
            "bg", "hr", "cs", "da", "nl", "en", "et", "fi", "fr", "de", "el", "hu", "it", "lv",
            "lt", "mt", "pl", "pt", "ro", "sk", "sl", "es", "sv", "ru", "uk",
        ]
        .into_iter()
        .map(String::from)
        .collect();

        available_models.insert(
            "canary-1b-v2".to_string(),
            ModelInfo {
                id: "canary-1b-v2".to_string(),
                name: "Canary 1B v2".to_string(),
                description: "Accurate multilingual. 25 European languages. Supports translation."
                    .to_string(),
                filename: "canary-1b-v2".to_string(),
                url: Some(model_url("canary-1b-v2.tar.gz")),
                sha256: Some(
                    "02305b2a25f9cf3e7deaffa7f94df00efa44f442cd55c101c2cb9c000f904666".to_string(),
                ),
                size_mb: 691,
                is_downloaded: false,
                is_downloading: false,
                partial_size: 0,
                is_directory: true,
                engine_type: EngineType::Canary,
                accuracy_score: 0.85,
                speed_score: 0.70,
                supports_translation: true,
                is_recommended: false,
                supported_languages: canary_1b_languages,
                supports_language_selection: true,
                is_custom: false,
            },
        );

        let cohere_languages: Vec<String> = vec![
            "en", "fr", "de", "it", "es", "pt", "el", "nl", "pl", "zh", "zh-Hans", "zh-Hant", "ja",
            "ko", "vi", "ar",
        ]
        .into_iter()
        .map(String::from)
        .collect();

        available_models.insert(
            "cohere-int8".to_string(),
            ModelInfo {
                id: "cohere-int8".to_string(),
                name: "Cohere".to_string(),
                description: "A large, slower, but very accurate multilingual model.".to_string(),
                filename: "cohere-int8".to_string(),
                url: Some(model_url("cohere-int8.tar.gz")),
                sha256: Some(
                    "ea2257d52434f3644574f187dcdcf666e302cd11b92866116ab8e14cd9c887f0".to_string(),
                ),
                size_mb: 1708,
                is_downloaded: false,
                is_downloading: false,
                partial_size: 0,
                is_directory: true,
                engine_type: EngineType::Cohere,
                accuracy_score: 0.90,
                speed_score: 0.60,
                supports_translation: false,
                is_recommended: false,
                supported_languages: cohere_languages,
                supports_language_selection: true,
                is_custom: false,
            },
        );

        // Auto-discover custom Whisper models (.bin files) in the models directory
        if let Err(e) = Self::discover_custom_whisper_models(&models_dir, &mut available_models) {
            warn!("Failed to discover custom models: {}", e);
        }

        let manager = Self {
            app_handle: app_handle.clone(),
            models_dir,
            available_models: Mutex::new(available_models),
            cancel_flags: Arc::new(Mutex::new(HashMap::new())),
            extracting_models: Arc::new(Mutex::new(HashSet::new())),
        };

        // Migrate any bundled models to user directory
        manager.migrate_bundled_models()?;

        // Migrate GigaAM from single-file to directory format
        manager.migrate_gigaam_to_directory()?;

        // Check which models are already downloaded
        manager.update_download_status()?;

        // Auto-select a model if none is currently selected
        manager.auto_select_model_if_needed()?;

        Ok(manager)
    }

    pub fn get_available_models(&self) -> Vec<ModelInfo> {
        let models = self.available_models.lock().unwrap();
        models.values().cloned().collect()
    }

    pub fn get_model_info(&self, model_id: &str) -> Option<ModelInfo> {
        let models = self.available_models.lock().unwrap();
        models.get(model_id).cloned()
    }

    fn migrate_bundled_models(&self) -> Result<()> {
        // Check for bundled models and copy them to user directory
        let bundled_models = ["ggml-small.bin"]; // Add other bundled models here if any

        for filename in &bundled_models {
            let bundled_path = self.app_handle.path().resolve(
                format!("resources/models/{}", filename),
                tauri::path::BaseDirectory::Resource,
            );

            if let Ok(bundled_path) = bundled_path {
                if bundled_path.exists() {
                    let user_path = self.models_dir.join(filename);

                    // Only copy if user doesn't already have the model
                    if !user_path.exists() {
                        info!("Migrating bundled model {} to user directory", filename);
                        fs::copy(&bundled_path, &user_path)?;
                        info!("Successfully migrated {}", filename);
                    }
                }
            }
        }

        Ok(())
    }

    /// Migrate GigaAM from the old single-file format (giga-am-v3.int8.onnx)
    /// to the new directory format (giga-am-v3-int8/model.int8.onnx + vocab.txt).
    /// This was required by the transcribe-rs 0.3.x upgrade.
    fn migrate_gigaam_to_directory(&self) -> Result<()> {
        let old_file = self.models_dir.join("giga-am-v3.int8.onnx");
        let new_dir = self.models_dir.join("giga-am-v3-int8");

        if !old_file.exists() || new_dir.exists() {
            return Ok(());
        }

        info!("Migrating GigaAM from single-file to directory format");

        let vocab_path = self
            .app_handle
            .path()
            .resolve(
                "resources/models/gigaam_vocab.txt",
                tauri::path::BaseDirectory::Resource,
            )
            .map_err(|e| anyhow::anyhow!("Failed to resolve GigaAM vocab path: {}", e))?;

        info!(
            "Resolved vocab path: {:?} (exists: {})",
            vocab_path,
            vocab_path.exists()
        );
        info!("Old file: {:?} (exists: {})", old_file, old_file.exists());
        info!("New dir: {:?} (exists: {})", new_dir, new_dir.exists());

        fs::create_dir_all(&new_dir)?;
        fs::rename(&old_file, new_dir.join("model.int8.onnx"))?;
        fs::copy(&vocab_path, new_dir.join("vocab.txt"))?;

        // Clean up old partial file if it exists
        let old_partial = self.models_dir.join("giga-am-v3.int8.onnx.partial");
        if old_partial.exists() {
            let _ = fs::remove_file(&old_partial);
        }

        info!("GigaAM migration complete");
        Ok(())
    }

    fn update_download_status(&self) -> Result<()> {
        // Models with a live download task keep their is_downloading flag — the task
        // owns it until it exits. Clearing it here (e.g. via delete_model during an
        // active download) would reopen the single-flight guard.
        let mut models = self.available_models.lock().unwrap();
        // Snapshot claims while holding available_models. Claims and releases
        // take the same available_models -> cancel_flags order, so no worker
        // can appear between this snapshot and the status updates below.
        let active_downloads: HashSet<String> =
            self.cancel_flags.lock().unwrap().keys().cloned().collect();

        for model in models.values_mut() {
            if model.is_directory {
                // For directory-based models, check if the directory exists
                let model_path = self.models_dir.join(&model.filename);
                let partial_path = self.models_dir.join(format!("{}.partial", model.filename));
                let extracting_path = self
                    .models_dir
                    .join(format!("{}.extracting", model.filename));

                // Clean up any leftover .extracting directories from interrupted extractions
                // But only if this model is NOT currently being extracted
                let is_currently_extracting = {
                    let extracting = self.extracting_models.lock().unwrap();
                    extracting.contains(&model.id)
                };
                if extracting_path.exists() && !is_currently_extracting {
                    warn!("Cleaning up interrupted extraction for model: {}", model.id);
                    let _ = fs::remove_dir_all(&extracting_path);
                }

                model.is_downloaded = model_path.exists() && model_path.is_dir();
                if !active_downloads.contains(&model.id) {
                    model.is_downloading = false;
                }

                // Get partial file size if it exists (for the .tar.gz being downloaded)
                if partial_path.exists() {
                    model.partial_size = partial_path.metadata().map(|m| m.len()).unwrap_or(0);
                } else {
                    model.partial_size = 0;
                }
            } else {
                // For file-based models (existing logic)
                let model_path = self.models_dir.join(&model.filename);
                let partial_path = self.models_dir.join(format!("{}.partial", model.filename));

                model.is_downloaded = model_path.exists();
                if !active_downloads.contains(&model.id) {
                    model.is_downloading = false;
                }

                // Get partial file size if it exists
                if partial_path.exists() {
                    model.partial_size = partial_path.metadata().map(|m| m.len()).unwrap_or(0);
                } else {
                    model.partial_size = 0;
                }
            }
        }

        Ok(())
    }

    fn auto_select_model_if_needed(&self) -> Result<()> {
        let mut settings = get_settings(&self.app_handle);

        // Clear stale selection: selected model is set but doesn't exist
        // in available_models (e.g. deleted custom model file)
        if !settings.selected_model.is_empty() {
            let models = self.available_models.lock().unwrap();
            let exists = models.contains_key(&settings.selected_model);
            drop(models);

            if !exists {
                info!(
                    "Selected model '{}' not found in available models, clearing selection",
                    settings.selected_model
                );
                settings.selected_model = String::new();
                // This reconciliation runs during `ModelManager::new`, whose
                // failure would fail app startup entirely; a transient
                // settings-store failure shouldn't crash a running app over
                // clearing a stale selection, so log rather than propagate.
                if let Err(e) = write_settings(&self.app_handle, settings.clone()) {
                    warn!("Failed to clear stale selected_model: {e}");
                }
            }
        }

        // If no model is selected, pick the first downloaded one
        if settings.selected_model.is_empty() {
            // Find the first available (downloaded) model
            let models = self.available_models.lock().unwrap();
            if let Some(available_model) = models.values().find(|model| model.is_downloaded) {
                info!(
                    "Auto-selecting model: {} ({})",
                    available_model.id, available_model.name
                );

                // Update settings with the selected model
                let mut updated_settings = settings;
                updated_settings.selected_model = available_model.id.clone();
                // Same rationale as above: log rather than fail startup.
                if let Err(e) = write_settings(&self.app_handle, updated_settings) {
                    warn!("Failed to persist auto-selected model: {e}");
                }

                info!("Successfully auto-selected model: {}", available_model.id);
            }
        }

        Ok(())
    }

    /// Discover custom Whisper models (.bin files) in the models directory.
    /// Skips files that match predefined model filenames.
    fn discover_custom_whisper_models(
        models_dir: &Path,
        available_models: &mut HashMap<String, ModelInfo>,
    ) -> Result<()> {
        if !models_dir.exists() {
            return Ok(());
        }

        // Collect filenames of predefined Whisper file-based models to skip
        let predefined_filenames: HashSet<String> = available_models
            .values()
            .filter(|m| matches!(m.engine_type, EngineType::Whisper) && !m.is_directory)
            .map(|m| m.filename.clone())
            .collect();

        // Scan models directory for .bin files
        for entry in fs::read_dir(models_dir)? {
            let entry = match entry {
                Ok(e) => e,
                Err(e) => {
                    warn!("Failed to read directory entry: {}", e);
                    continue;
                }
            };

            let path = entry.path();

            // Only process .bin files (not directories)
            if !path.is_file() {
                continue;
            }

            let filename = match path.file_name().and_then(|s| s.to_str()) {
                Some(name) => name.to_string(),
                None => continue,
            };

            // Skip hidden files
            if filename.starts_with('.') {
                continue;
            }

            // Only process .bin files (Whisper GGML format).
            // This also excludes .partial downloads (e.g., "model.bin.partial").
            // If we add discovery for other formats, add a .partial check before this filter.
            if !filename.ends_with(".bin") {
                continue;
            }

            // Skip predefined model files
            if predefined_filenames.contains(&filename) {
                continue;
            }

            // Generate model ID from filename (remove .bin extension)
            let model_id = filename.trim_end_matches(".bin").to_string();

            // Skip if model ID already exists (shouldn't happen, but be safe)
            if available_models.contains_key(&model_id) {
                continue;
            }

            // Generate display name: replace - and _ with space, capitalize words
            let display_name = model_id
                .replace(['-', '_'], " ")
                .split_whitespace()
                .map(|word| {
                    let mut chars = word.chars();
                    match chars.next() {
                        None => String::new(),
                        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                    }
                })
                .collect::<Vec<_>>()
                .join(" ");

            // Get file size in MB
            let size_mb = match path.metadata() {
                Ok(meta) => meta.len() / (1024 * 1024),
                Err(e) => {
                    warn!("Failed to get metadata for {}: {}", filename, e);
                    0
                }
            };

            info!(
                "Discovered custom Whisper model: {} ({}, {} MB)",
                model_id, filename, size_mb
            );

            available_models.insert(
                model_id.clone(),
                ModelInfo {
                    id: model_id,
                    name: display_name,
                    description: "Not officially supported".to_string(),
                    filename,
                    url: None,    // Custom models have no download URL
                    sha256: None, // Custom models skip verification
                    size_mb,
                    is_downloaded: true, // Already present on disk
                    is_downloading: false,
                    partial_size: 0,
                    is_directory: false,
                    engine_type: EngineType::Whisper,
                    accuracy_score: 0.0, // Sentinel: UI hides score bars when both are 0
                    speed_score: 0.0,
                    supports_translation: false,
                    is_recommended: false,
                    supported_languages: vec![],
                    supports_language_selection: true,
                    is_custom: true,
                },
            );
        }

        Ok(())
    }

    /// Verifies the SHA256 of `path` against `expected_sha256` (if provided).
    /// On mismatch or read error the partial file is deleted and an error is returned,
    /// so the next download attempt always starts from a clean state.
    /// When `expected_sha256` is `None` (custom user models) verification is skipped.
    fn verify_sha256(path: &Path, expected_sha256: Option<&str>, model_id: &str) -> Result<()> {
        let Some(expected) = expected_sha256 else {
            return Ok(());
        };
        match Self::compute_sha256(path) {
            Ok(actual) if actual == expected => {
                info!("SHA256 verified for model {}", model_id);
                Ok(())
            }
            Ok(actual) => {
                warn!(
                    "SHA256 mismatch for model {}: expected {}, got {}",
                    model_id, expected, actual
                );
                let _ = fs::remove_file(path);
                Err(anyhow::anyhow!(
                    "Download verification failed for model {}: file is corrupt. Please retry.",
                    model_id
                ))
            }
            Err(e) => {
                let _ = fs::remove_file(path);
                Err(anyhow::anyhow!(
                    "Failed to verify download for model {}: {}. Please retry.",
                    model_id,
                    e
                ))
            }
        }
    }

    /// Computes the SHA256 hex digest of a file, reading in 64KB chunks to handle large models.
    fn compute_sha256(path: &Path) -> Result<String> {
        let mut file = File::open(path)?;
        let mut hasher = Sha256::new();
        let mut buffer = [0u8; 65536];
        loop {
            let n = file.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            hasher.update(&buffer[..n]);
        }
        Ok(format!("{:x}", hasher.finalize()))
    }

    /// Claims one model download under the shared ownership lock order.
    /// Duplicate callers are classified without replacing the active flag;
    /// a caller arriving while cancellation is pending must not join a worker
    /// that is about to exit.
    fn claim_download(
        available_models: &Mutex<HashMap<String, ModelInfo>>,
        cancel_flags: &Arc<Mutex<HashMap<String, Arc<DownloadControl>>>>,
        model_id: &str,
    ) -> Result<std::result::Result<Arc<DownloadControl>, &'static str>> {
        let mut models = available_models.lock().unwrap();
        let mut flags = cancel_flags.lock().unwrap();
        let model = models
            .get_mut(model_id)
            .ok_or_else(|| anyhow::anyhow!("Model not found: {}", model_id))?;

        if model.is_downloading {
            let cancellation_pending = flags
                .get(model_id)
                .map(|flag| flag.is_cancelled())
                .unwrap_or(false);
            return Ok(Err(if cancellation_pending {
                MODEL_DOWNLOAD_CANCELLING
            } else {
                MODEL_DOWNLOAD_ALREADY_ACTIVE
            }));
        }

        let cancel_flag = Arc::new(DownloadControl::new());
        model.is_downloading = true;
        flags.insert(model_id.to_string(), cancel_flag.clone());
        Ok(Ok(cancel_flag))
    }

    /// Emits a download progress event, tracking consecutive failures so callers
    /// can abort the download once the frontend is clearly unreachable.
    fn emit_download_progress(
        &self,
        progress: &DownloadProgress,
        consecutive_failures: &mut u32,
    ) -> Result<()> {
        match self.app_handle.emit("model-download-progress", progress) {
            Ok(()) => {
                *consecutive_failures = 0;
                Ok(())
            }
            Err(e) => {
                *consecutive_failures += 1;
                warn!(
                    "Failed to emit download progress for model {} ({} consecutive failures): {}",
                    progress.model_id, consecutive_failures, e
                );
                Err(anyhow::anyhow!(e.to_string()))
            }
        }
    }

    pub async fn download_model(&self, model_id: &str) -> Result<()> {
        let model_info = {
            let models = self.available_models.lock().unwrap();
            models.get(model_id).cloned()
        };

        let model_info =
            model_info.ok_or_else(|| anyhow::anyhow!("Model not found: {}", model_id))?;

        let url = model_info
            .url
            .ok_or_else(|| anyhow::anyhow!("No download URL for model"))?;
        let model_path = self.models_dir.join(&model_info.filename);
        let partial_path = self
            .models_dir
            .join(format!("{}.partial", model_info.filename));

        // Don't download if complete version already exists
        if model_path.exists() {
            // Clean up any partial file that might exist
            if partial_path.exists() {
                let _ = fs::remove_file(&partial_path);
            }
            self.update_download_status()?;
            return Ok(());
        }

        // Check if we have a partial download to resume
        let mut resume_from = if partial_path.exists() {
            let size = partial_path.metadata()?.len();
            info!("Resuming download of model {} from byte {}", model_id, size);
            size
        } else {
            info!("Starting fresh download of model {} from {}", model_id, url);
            0
        };

        // The model flag and cancel handle form one ownership record. Every
        // dual-lock site uses available_models -> cancel_flags.
        let cancel_flag =
            match Self::claim_download(&self.available_models, &self.cancel_flags, model_id)? {
                Ok(flag) => flag,
                Err(code) => return Err(anyhow::anyhow!(code)),
            };

        // Guard ensures is_downloading and cancel_flags are cleaned up on every
        // error path. Disarmed only on success (which sets is_downloaded = true).
        let mut cleanup = DownloadCleanup {
            available_models: &self.available_models,
            cancel_flags: &self.cancel_flags,
            cancel_flag: cancel_flag.clone(),
            model_id: model_id.to_string(),
            disarmed: false,
        };

        // Create HTTP client with range request for resuming. The 60s read
        // timeout applies per read operation (reqwest 0.12), so a genuinely
        // stalled connection — the server accepts but stops sending bytes —
        // surfaces as a stream error and the task exits through the error
        // cleanup path instead of hanging on stream.next() forever. Slow but
        // progressing downloads are unaffected.
        let client = reqwest::Client::builder()
            .read_timeout(Duration::from_secs(60))
            .build()?;
        let mut request = client.get(&url);

        if resume_from > 0 {
            request = request.header("Range", format!("bytes={}-", resume_from));
        }

        let Some(mut response) =
            resolve_download_read(model_id, &cancel_flag, request.send().await)?
        else {
            info!("Download cancelled for: {}", model_id);
            return Ok(());
        };

        // If we tried to resume but server returned 200 (not 206 Partial Content),
        // the server doesn't support range requests. Delete partial file and restart
        // fresh to avoid file corruption (appending full file to partial).
        if resume_from > 0 && response.status() == reqwest::StatusCode::OK {
            warn!(
                "Server doesn't support range requests for model {}, restarting download",
                model_id
            );
            drop(response);
            let _ = fs::remove_file(&partial_path);

            // Reset resume_from since we're starting fresh
            resume_from = 0;

            // Restart download without range header
            let Some(restarted_response) =
                resolve_download_read(model_id, &cancel_flag, client.get(&url).send().await)?
            else {
                info!("Download cancelled for: {}", model_id);
                return Ok(());
            };
            response = restarted_response;
        }

        // Check for success or partial content status
        if !response.status().is_success()
            && response.status() != reqwest::StatusCode::PARTIAL_CONTENT
        {
            return Err(anyhow::anyhow!(
                "Failed to download model: HTTP {}",
                response.status()
            ));
        }

        let total_size = if resume_from > 0 {
            // For resumed downloads, add the resume point to content length
            resume_from + response.content_length().unwrap_or(0)
        } else {
            response.content_length().unwrap_or(0)
        };

        let mut downloaded = resume_from;
        let mut stream = response.bytes_stream();

        // Open file for appending if resuming, or create new if starting fresh
        let mut file = if resume_from > 0 {
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&partial_path)?
        } else {
            std::fs::File::create(&partial_path)?
        };

        // Emit initial progress
        let initial_progress = DownloadProgress {
            model_id: model_id.to_string(),
            downloaded,
            total: total_size,
            percentage: if total_size > 0 {
                (downloaded as f64 / total_size as f64) * 100.0
            } else {
                0.0
            },
        };
        let mut consecutive_emit_failures = 0u32;
        let _ = self.emit_download_progress(&initial_progress, &mut consecutive_emit_failures);

        // Throttle progress events to max 10/sec (100ms intervals)
        let mut last_emit = Instant::now();
        let throttle_duration = Duration::from_millis(100);

        // Download with progress
        while let Some(chunk_result) = stream.next().await {
            let Some(chunk) = resolve_download_read(model_id, &cancel_flag, chunk_result)? else {
                drop(file);
                info!("Download cancelled for: {}", model_id);
                // Keep partial file for resume functionality. Cancellation
                // wins even if the pending read completed with a timeout.
                // The guard handles lifecycle cleanup on drop.
                return Ok(());
            };

            file.write_all(&chunk)?;
            downloaded += chunk.len() as u64;

            let percentage = if total_size > 0 {
                (downloaded as f64 / total_size as f64) * 100.0
            } else {
                0.0
            };

            // Emit progress event (throttled to avoid UI freeze)
            if last_emit.elapsed() >= throttle_duration {
                let progress = DownloadProgress {
                    model_id: model_id.to_string(),
                    downloaded,
                    total: total_size,
                    percentage,
                };
                if self
                    .emit_download_progress(&progress, &mut consecutive_emit_failures)
                    .is_err()
                    && consecutive_emit_failures >= MAX_CONSECUTIVE_PROGRESS_EMIT_FAILURES
                {
                    // Frontend is unreachable; abort so the download UI does not
                    // stall forever. The cleanup guard resets download state and
                    // the command layer emits model-download-failed on this error.
                    warn!(
                        "Aborting download of model {}: progress events are not reaching the frontend",
                        model_id
                    );
                    return Err(anyhow::anyhow!(MODEL_DOWNLOAD_STALLED));
                }
                last_emit = Instant::now();
            }
        }

        // A cancellation can race with clean EOF and leave no next chunk on
        // which to observe the flag. Do not proceed into verification then.
        if cancel_flag.is_cancelled() {
            drop(file);
            info!("Download cancelled for: {}", model_id);
            // EOF means this partial is complete, including when the server
            // omitted Content-Length. A complete partial cannot be resumed
            // safely, so a retry must start fresh.
            fs::remove_file(&partial_path)?;
            return Ok(());
        }

        // Emit final progress to ensure 100% is shown
        let final_progress = DownloadProgress {
            model_id: model_id.to_string(),
            downloaded,
            total: total_size,
            percentage: if total_size > 0 {
                (downloaded as f64 / total_size as f64) * 100.0
            } else {
                100.0
            },
        };
        let _ = self.emit_download_progress(&final_progress, &mut consecutive_emit_failures);

        file.flush()?;
        drop(file); // Ensure file is closed before moving

        // Honor a cancellation that arrived after the EOF check but before
        // verification began.
        if cancel_flag.is_cancelled() {
            info!("Download cancelled for: {} (before verification)", model_id);
            // The byte stream ended normally, so this partial is complete even
            // when the server omitted Content-Length. A complete partial cannot
            // be range-resumed (the server answers 416); remove it so a retry
            // starts fresh. Mid-stream cancellation still keeps its resumable
            // incomplete partial in the loop above.
            let _ = fs::remove_file(&partial_path);
            // Guard handles is_downloading + cancel_flags cleanup on drop.
            return Ok(());
        }

        // Verify downloaded file size matches expected size
        if total_size > 0 {
            let actual_size = partial_path.metadata()?.len();
            if actual_size != total_size {
                // Download is incomplete/corrupted - delete partial and return error
                let _ = fs::remove_file(&partial_path);
                return Err(anyhow::anyhow!(
                    "Download incomplete: expected {} bytes, got {} bytes",
                    total_size,
                    actual_size
                ));
            }
        }

        // Verify SHA256 checksum. Runs in a blocking thread so the async executor is not
        // stalled while hashing large model files (up to 1.6 GB). On failure the partial
        // is deleted inside verify_sha256 so the next attempt always starts fresh.
        let _ = self.app_handle.emit("model-verification-started", model_id);
        info!("Verifying SHA256 for model {}...", model_id);
        let verify_path = partial_path.clone();
        let verify_expected = model_info.sha256.clone();
        let verify_model_id = model_id.to_string();
        let verify_result = tokio::task::spawn_blocking(move || {
            Self::verify_sha256(&verify_path, verify_expected.as_deref(), &verify_model_id)
        })
        .await
        .map_err(|e| anyhow::anyhow!("SHA256 task panicked: {}", e))?;
        verify_result?;
        let _ = self
            .app_handle
            .emit("model-verification-completed", model_id);

        // Hashing is blocking and can outlive a cancel request. Do not install
        // a verified model after cancellation was acknowledged.
        if cancel_flag.is_cancelled() {
            info!("Download cancelled for: {} (after verification)", model_id);
            let _ = fs::remove_file(&partial_path);
            return Ok(());
        }

        // Handle directory-based models (extract tar.gz) vs file-based models
        if model_info.is_directory {
            // Track that this model is being extracted
            {
                let mut extracting = self.extracting_models.lock().unwrap();
                extracting.insert(model_id.to_string());
            }

            // Emit extraction started event
            let _ = self.app_handle.emit("model-extraction-started", model_id);
            info!("Extracting archive for directory-based model: {}", model_id);

            // Use a temporary extraction directory to ensure atomic operations
            let temp_extract_dir = self
                .models_dir
                .join(format!("{}.extracting", model_info.filename));
            let final_model_dir = self.models_dir.join(&model_info.filename);

            // Clean up any previous incomplete extraction
            if temp_extract_dir.exists() {
                let _ = fs::remove_dir_all(&temp_extract_dir);
            }

            // Create temporary extraction directory
            fs::create_dir_all(&temp_extract_dir)?;

            // Open the downloaded tar.gz file
            let tar_gz = File::open(&partial_path)?;
            let tar = GzDecoder::new(tar_gz);
            let mut archive = Archive::new(tar);

            // Extract to the temporary directory first
            archive.unpack(&temp_extract_dir).map_err(|e| {
                let error_msg = format!("Failed to extract archive: {}", e);
                // Clean up failed extraction
                let _ = fs::remove_dir_all(&temp_extract_dir);
                // Delete the corrupt partial file so the next download attempt starts fresh
                // instead of resuming from a broken archive (issue #858).
                let _ = fs::remove_file(&partial_path);
                // Remove from extracting set
                {
                    let mut extracting = self.extracting_models.lock().unwrap();
                    extracting.remove(model_id);
                }
                let _ = self.app_handle.emit(
                    "model-extraction-failed",
                    &serde_json::json!({
                        "model_id": model_id,
                        "error": error_msg
                    }),
                );
                anyhow::anyhow!(error_msg)
            })?;

            // Atomically choose the winner before the first irreversible
            // installation step. A cancellation acknowledged before this CAS
            // wins and must leave no installed model; once commit wins, later
            // cancel requests are rejected instead of falsely acknowledged.
            if !cancel_flag.begin_commit() {
                info!("Download cancelled for: {} (after extraction)", model_id);
                let _ = fs::remove_dir_all(&temp_extract_dir);
                let _ = fs::remove_file(&partial_path);
                self.extracting_models.lock().unwrap().remove(model_id);
                return Ok(());
            }

            // Find the actual extracted directory (archive might have a nested structure)
            let extracted_dirs: Vec<_> = fs::read_dir(&temp_extract_dir)?
                .filter_map(|entry| entry.ok())
                .filter(|entry| entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false))
                .collect();

            if extracted_dirs.len() == 1 {
                // Single directory extracted, move it to the final location
                let source_dir = extracted_dirs[0].path();
                if final_model_dir.exists() {
                    fs::remove_dir_all(&final_model_dir)?;
                }
                fs::rename(&source_dir, &final_model_dir)?;
                // Clean up temp directory
                let _ = fs::remove_dir_all(&temp_extract_dir);
            } else {
                // Multiple items or no directories, rename the temp directory itself
                if final_model_dir.exists() {
                    fs::remove_dir_all(&final_model_dir)?;
                }
                fs::rename(&temp_extract_dir, &final_model_dir)?;
            }

            info!("Successfully extracted archive for model: {}", model_id);
            // Remove from extracting set
            {
                let mut extracting = self.extracting_models.lock().unwrap();
                extracting.remove(model_id);
            }
            // Emit extraction completed event
            if let Err(e) = self.app_handle.emit("model-extraction-completed", model_id) {
                warn!(
                    "Failed to emit extraction completion for model {}: {}",
                    model_id, e
                );
            }

            // Remove the downloaded tar.gz file
            let _ = fs::remove_file(&partial_path);
        } else {
            if !cancel_flag.begin_commit() {
                info!("Download cancelled for: {} (before installation)", model_id);
                let _ = fs::remove_file(&partial_path);
                return Ok(());
            }
            // Move partial file to final location for file-based models
            fs::rename(&partial_path, &model_path)?;
        }

        // Successful commit releases the exact worker's ownership atomically.
        // Fail closed if a newer worker somehow replaced the control record.
        if !release_download_claim(
            &self.available_models,
            &self.cancel_flags,
            model_id,
            &cancel_flag,
            true,
        ) {
            if cancel_flag.is_cancelled() {
                info!("Download cancelled for: {} (during installation)", model_id);
                if model_info.is_directory {
                    fs::remove_dir_all(&model_path)?;
                } else {
                    fs::remove_file(&model_path)?;
                }
                return Ok(());
            }
            return Err(anyhow::anyhow!(
                "Download ownership changed before completion for model: {}",
                model_id
            ));
        }
        cleanup.disarmed = true;

        // Emit completion event
        if let Err(e) = self.app_handle.emit("model-download-complete", model_id) {
            warn!(
                "Failed to emit download completion for model {}: {}",
                model_id, e
            );
            // Without the completion event the download UI would wait forever,
            // so emit a failure event for any surviving listener to act on.
            let _ = self.app_handle.emit(
                "model-download-failed",
                &serde_json::json!({
                    "model_id": model_id,
                    "error": MODEL_DOWNLOAD_NOTIFY_FAILED
                }),
            );
        }

        info!(
            "Successfully downloaded model {} to {:?}",
            model_id, model_path
        );

        Ok(())
    }

    pub fn delete_model(&self, model_id: &str) -> Result<()> {
        debug!("ModelManager: delete_model called for: {}", model_id);

        let model_info = {
            let models = self.available_models.lock().unwrap();
            models.get(model_id).cloned()
        };

        let model_info =
            model_info.ok_or_else(|| anyhow::anyhow!("Model not found: {}", model_id))?;

        debug!("ModelManager: Found model info: {:?}", model_info);

        let model_path = self.models_dir.join(&model_info.filename);
        let partial_path = self
            .models_dir
            .join(format!("{}.partial", model_info.filename));
        debug!("ModelManager: Model path: {:?}", model_path);
        debug!("ModelManager: Partial path: {:?}", partial_path);

        let mut deleted_something = false;

        if model_info.is_directory {
            // Delete complete model directory if it exists
            if model_path.exists() && model_path.is_dir() {
                info!("Deleting model directory at: {:?}", model_path);
                fs::remove_dir_all(&model_path)?;
                info!("Model directory deleted successfully");
                deleted_something = true;
            }
        } else {
            // Delete complete model file if it exists
            if model_path.exists() {
                info!("Deleting model file at: {:?}", model_path);
                fs::remove_file(&model_path)?;
                info!("Model file deleted successfully");
                deleted_something = true;
            }
        }

        // Delete partial file if it exists (same for both types)
        if partial_path.exists() {
            info!("Deleting partial file at: {:?}", partial_path);
            fs::remove_file(&partial_path)?;
            info!("Partial file deleted successfully");
            deleted_something = true;
        }

        if !deleted_something {
            return Err(anyhow::anyhow!("No model files found to delete"));
        }

        // Custom models should be removed from the list entirely since they
        // have no download URL and can't be re-downloaded
        if model_info.is_custom {
            let mut models = self.available_models.lock().unwrap();
            models.remove(model_id);
            debug!("ModelManager: removed custom model from available models");
        } else {
            // Update download status (marks predefined models as not downloaded)
            self.update_download_status()?;
            debug!("ModelManager: download status updated");
        }

        // Emit event to notify UI
        let _ = self.app_handle.emit("model-deleted", model_id);

        Ok(())
    }

    pub fn get_model_path(&self, model_id: &str) -> Result<PathBuf> {
        let model_info = self
            .get_model_info(model_id)
            .ok_or_else(|| anyhow::anyhow!("Model not found: {}", model_id))?;

        if !model_info.is_downloaded {
            return Err(anyhow::anyhow!("Model not available: {}", model_id));
        }

        // Ensure we don't return partial files/directories
        if model_info.is_downloading {
            return Err(anyhow::anyhow!(
                "Model is currently downloading: {}",
                model_id
            ));
        }

        let model_path = self.models_dir.join(&model_info.filename);
        let partial_path = self
            .models_dir
            .join(format!("{}.partial", model_info.filename));

        if model_info.is_directory {
            // For directory-based models, ensure the directory exists and is complete
            if model_path.exists() && model_path.is_dir() && !partial_path.exists() {
                Ok(model_path)
            } else {
                Err(anyhow::anyhow!(
                    "Complete model directory not found: {}",
                    model_id
                ))
            }
        } else {
            // For file-based models (existing logic)
            if model_path.exists() && !partial_path.exists() {
                Ok(model_path)
            } else {
                Err(anyhow::anyhow!(
                    "Complete model file not found: {}",
                    model_id
                ))
            }
        }
    }

    pub fn cancel_download(&self, model_id: &str) -> Result<()> {
        debug!("ModelManager: cancel_download called for: {}", model_id);

        let request = {
            // Match claim/release lock order so completion and a late cancel
            // observe one coherent lifecycle record.
            let models = self.available_models.lock().unwrap();
            let flags = self.cancel_flags.lock().unwrap();
            if let Some(flag) = flags.get(model_id) {
                flag.request_cancel()
            } else if models
                .get(model_id)
                .map(|model| model.is_downloaded)
                .unwrap_or(false)
            {
                // Commit already released its control record. Treat this the
                // same as losing the commit CAS so the UI keeps its selection.
                CancelRequest::TooLate
            } else {
                // A missing flag means no live download task owns this model's
                // lifecycle — reporting success here would acknowledge a cancel
                // that nothing will honor while the download proceeds.
                return Err(anyhow::anyhow!(MODEL_DOWNLOAD_NOT_ACTIVE));
            }
        };

        match request {
            CancelRequest::Accepted => {
                info!("Cancellation accepted for: {}", model_id);
            }
            CancelRequest::AlreadyCancelled => {
                info!("Cancellation already pending for: {}", model_id);
            }
            CancelRequest::TooLate => {
                return Err(anyhow::anyhow!(MODEL_DOWNLOAD_CANCEL_TOO_LATE));
            }
        }

        // The download task owns is_downloading: it is cleared by the cleanup
        // guard when the task actually observes this flag and exits. Clearing
        // it here would let a retry start while the old task is still
        // running, recreating the double-download race on the same .partial
        // file.

        // Emit cancellation event so all UI components can clear their state
        let _ = self.app_handle.emit("model-download-cancelled", model_id);

        info!("Download cancellation initiated for: {}", model_id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn macos_recommends_whisper_turbo() {
        assert!(recommended_model_for_platform("turbo", "macos"));
        assert!(!recommended_model_for_platform(
            "parakeet-tdt-0.6b-v3",
            "macos"
        ));
    }

    #[test]
    fn windows_recommends_parakeet_v3() {
        assert!(recommended_model_for_platform(
            "parakeet-tdt-0.6b-v3",
            "windows"
        ));
        assert!(!recommended_model_for_platform("turbo", "windows"));
    }

    #[test]
    fn unsupported_platform_has_no_recommended_model() {
        assert!(!recommended_model_for_platform("turbo", "linux"));
        assert!(!recommended_model_for_platform(
            "parakeet-tdt-0.6b-v3",
            "linux"
        ));
    }

    #[test]
    fn test_discover_custom_whisper_models() {
        let temp_dir = TempDir::new().unwrap();
        let models_dir = temp_dir.path().to_path_buf();

        // Create test .bin files
        let mut custom_file = File::create(models_dir.join("my-custom-model.bin")).unwrap();
        custom_file.write_all(b"fake model data").unwrap();

        let mut another_file = File::create(models_dir.join("whisper_medical_v2.bin")).unwrap();
        another_file.write_all(b"another fake model").unwrap();

        // Create files that should be ignored
        File::create(models_dir.join(".hidden-model.bin")).unwrap(); // Hidden file
        File::create(models_dir.join("readme.txt")).unwrap(); // Non-.bin file
        File::create(models_dir.join("ggml-small.bin")).unwrap(); // Predefined filename
        fs::create_dir(models_dir.join("some-directory.bin")).unwrap(); // Directory

        // Set up available_models with a predefined Whisper model
        let mut models = HashMap::new();
        models.insert(
            "small".to_string(),
            ModelInfo {
                id: "small".to_string(),
                name: "Whisper Small".to_string(),
                description: "Test".to_string(),
                filename: "ggml-small.bin".to_string(),
                url: Some("https://example.com".to_string()),
                sha256: None,
                size_mb: 100,
                is_downloaded: false,
                is_downloading: false,
                partial_size: 0,
                is_directory: false,
                engine_type: EngineType::Whisper,
                accuracy_score: 0.5,
                speed_score: 0.5,
                supports_translation: true,
                is_recommended: false,
                supported_languages: vec!["en".to_string()],
                supports_language_selection: true,
                is_custom: false,
            },
        );

        // Discover custom models
        ModelManager::discover_custom_whisper_models(&models_dir, &mut models).unwrap();

        // Should have discovered 2 custom models (my-custom-model and whisper_medical_v2)
        assert!(models.contains_key("my-custom-model"));
        assert!(models.contains_key("whisper_medical_v2"));

        // Verify custom model properties
        let custom = models.get("my-custom-model").unwrap();
        assert_eq!(custom.name, "My Custom Model");
        assert_eq!(custom.filename, "my-custom-model.bin");
        assert!(custom.url.is_none()); // Custom models have no URL
        assert!(custom.is_downloaded);
        assert!(custom.is_custom);
        assert_eq!(custom.accuracy_score, 0.0);
        assert_eq!(custom.speed_score, 0.0);
        assert!(custom.supported_languages.is_empty());

        // Verify underscore handling
        let medical = models.get("whisper_medical_v2").unwrap();
        assert_eq!(medical.name, "Whisper Medical V2");

        // Should NOT have discovered hidden, non-.bin, predefined, or directories
        assert!(!models.contains_key(".hidden-model"));
        assert!(!models.contains_key("readme"));
        assert!(!models.contains_key("some-directory"));
    }

    #[test]
    fn test_discover_custom_models_empty_dir() {
        let temp_dir = TempDir::new().unwrap();
        let models_dir = temp_dir.path().to_path_buf();

        let mut models = HashMap::new();
        let count_before = models.len();

        ModelManager::discover_custom_whisper_models(&models_dir, &mut models).unwrap();

        // No new models should be added
        assert_eq!(models.len(), count_before);
    }

    #[test]
    fn test_discover_custom_models_nonexistent_dir() {
        let models_dir = PathBuf::from("/nonexistent/path/that/does/not/exist");

        let mut models = HashMap::new();
        let count_before = models.len();

        // Should not error, just return Ok
        let result = ModelManager::discover_custom_whisper_models(&models_dir, &mut models);
        assert!(result.is_ok());
        assert_eq!(models.len(), count_before);
    }

    // ── SHA256 verification tests ─────────────────────────────────────────────

    /// Helper: write `data` to a temp file and return (TempDir, path).
    /// TempDir must be kept alive for the duration of the test.
    fn write_temp_file(data: &[u8]) -> (TempDir, std::path::PathBuf) {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("model.partial");
        let mut f = File::create(&path).unwrap();
        f.write_all(data).unwrap();
        (dir, path)
    }

    #[test]
    fn test_verify_sha256_skipped_when_none() {
        // Custom models have no expected hash — verification must be a no-op.
        let (_dir, path) = write_temp_file(b"anything");
        assert!(ModelManager::verify_sha256(&path, None, "custom").is_ok());
        assert!(
            path.exists(),
            "file must be untouched when verification is skipped"
        );
    }

    #[test]
    fn test_verify_sha256_passes_on_correct_hash() {
        // Compute the real hash so the test is self-consistent.
        let (_dir, path) = write_temp_file(b"hello world");
        let actual = ModelManager::compute_sha256(&path).unwrap();
        assert!(
            ModelManager::verify_sha256(&path, Some(&actual), "test_model").is_ok(),
            "should pass when hash matches"
        );
        assert!(
            path.exists(),
            "file must be kept on successful verification"
        );
    }

    #[test]
    fn test_verify_sha256_fails_and_deletes_partial_on_mismatch() {
        let (_dir, path) = write_temp_file(b"this is not the real model");
        let wrong_hash = "0000000000000000000000000000000000000000000000000000000000000000";

        let result = ModelManager::verify_sha256(&path, Some(wrong_hash), "bad_model");

        assert!(result.is_err(), "mismatch must return an error");
        assert!(
            result.unwrap_err().to_string().contains("corrupt"),
            "error message should mention corruption"
        );
        assert!(
            !path.exists(),
            "partial file must be deleted after hash mismatch"
        );
    }

    #[test]
    fn test_verify_sha256_fails_and_deletes_partial_when_file_missing() {
        // Simulate a partial file that was already removed (e.g. disk full mid-download).
        let dir = TempDir::new().unwrap();
        let missing_path = dir.path().join("gone.partial");
        // Don't create the file — it should not exist.

        let result =
            ModelManager::verify_sha256(&missing_path, Some("anyexpectedhash"), "missing_model");

        assert!(result.is_err(), "missing file must return an error");
    }

    #[test]
    fn test_cancelled_read_timeout_is_not_reported_as_a_failure() {
        assert_eq!(
            classify_download_read(true, true),
            DownloadReadDisposition::Cancelled,
            "user cancellation must win when a pending read returns a timeout"
        );
    }

    #[test]
    fn test_read_timeout_uses_a_stable_error_code() {
        assert_eq!(
            classify_download_read(false, true),
            DownloadReadDisposition::Failed(MODEL_DOWNLOAD_STALLED),
        );
    }

    fn download_test_model() -> ModelInfo {
        ModelInfo {
            id: "small".to_string(),
            name: "Whisper Small".to_string(),
            description: "Test".to_string(),
            filename: "ggml-small.bin".to_string(),
            url: Some("https://example.com/model.bin".to_string()),
            sha256: None,
            size_mb: 100,
            is_downloaded: false,
            is_downloading: true,
            partial_size: 0,
            is_directory: false,
            engine_type: EngineType::Whisper,
            accuracy_score: 0.5,
            speed_score: 0.5,
            supports_translation: true,
            is_recommended: false,
            supported_languages: vec!["en".to_string()],
            supports_language_selection: true,
            is_custom: false,
        }
    }

    #[test]
    fn test_retry_during_cancellation_does_not_join_exiting_worker() {
        let models = Mutex::new(HashMap::from([(
            "small".to_string(),
            download_test_model(),
        )]));
        let flag = Arc::new(DownloadControl::new());
        assert_eq!(flag.request_cancel(), CancelRequest::Accepted);
        let flags = Arc::new(Mutex::new(HashMap::from([("small".to_string(), flag)])));

        let claim = ModelManager::claim_download(&models, &flags, "small").unwrap();

        assert_eq!(claim.unwrap_err(), MODEL_DOWNLOAD_CANCELLING);
        assert!(models.lock().unwrap()["small"].is_downloading);
    }

    #[test]
    fn test_stale_cleanup_cannot_release_a_newer_worker() {
        let models = Mutex::new(HashMap::from([(
            "small".to_string(),
            download_test_model(),
        )]));
        let stale_flag = Arc::new(DownloadControl::new());
        let current_flag = Arc::new(DownloadControl::new());
        let flags = Arc::new(Mutex::new(HashMap::from([(
            "small".to_string(),
            current_flag.clone(),
        )])));

        assert!(!release_download_claim(
            &models,
            &flags,
            "small",
            &stale_flag,
            false,
        ));
        assert!(models.lock().unwrap()["small"].is_downloading);
        let stored = flags.lock().unwrap()["small"].clone();
        assert!(Arc::ptr_eq(&stored, &current_flag));
    }

    #[test]
    fn test_completion_cannot_override_an_acknowledged_cancellation() {
        let models = Mutex::new(HashMap::from([(
            "small".to_string(),
            download_test_model(),
        )]));
        let flag = Arc::new(DownloadControl::new());
        assert_eq!(flag.request_cancel(), CancelRequest::Accepted);
        let flags = Arc::new(Mutex::new(HashMap::from([(
            "small".to_string(),
            flag.clone(),
        )])));

        assert!(!release_download_claim(
            &models, &flags, "small", &flag, true,
        ));
        assert!(models.lock().unwrap()["small"].is_downloading);
        assert!(!models.lock().unwrap()["small"].is_downloaded);
        assert!(flags.lock().unwrap().contains_key("small"));
    }

    #[test]
    fn test_acknowledged_cancel_wins_before_commit() {
        let control = DownloadControl::new();

        assert_eq!(control.request_cancel(), CancelRequest::Accepted);
        assert!(!control.begin_commit());
        assert!(control.is_cancelled());
    }

    #[test]
    fn test_commit_rejects_a_late_cancel() {
        let control = DownloadControl::new();

        assert!(control.begin_commit());
        assert_eq!(control.request_cancel(), CancelRequest::TooLate);
        assert!(!control.is_cancelled());
    }

    #[test]
    fn test_repeated_cancel_is_idempotently_acknowledged() {
        let control = DownloadControl::new();

        assert_eq!(control.request_cancel(), CancelRequest::Accepted);
        assert_eq!(control.request_cancel(), CancelRequest::AlreadyCancelled);
    }
}
