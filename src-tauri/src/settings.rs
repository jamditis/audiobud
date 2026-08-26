use log::{debug, warn};
use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use specta::Type;
use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, RwLock};
use tauri::{AppHandle, Emitter};
use tauri_plugin_store::{Store, StoreExt};

pub const APPLE_INTELLIGENCE_PROVIDER_ID: &str = "apple_intelligence";
pub const APPLE_INTELLIGENCE_DEFAULT_MODEL_ID: &str = "Apple Intelligence";

#[derive(Serialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

// Custom deserializer to handle both old numeric format (1-5) and new string format ("trace", "debug", etc.)
impl<'de> Deserialize<'de> for LogLevel {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct LogLevelVisitor;

        impl<'de> Visitor<'de> for LogLevelVisitor {
            type Value = LogLevel;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a string or integer representing log level")
            }

            fn visit_str<E: de::Error>(self, value: &str) -> Result<LogLevel, E> {
                match value.to_lowercase().as_str() {
                    "trace" => Ok(LogLevel::Trace),
                    "debug" => Ok(LogLevel::Debug),
                    "info" => Ok(LogLevel::Info),
                    "warn" => Ok(LogLevel::Warn),
                    "error" => Ok(LogLevel::Error),
                    _ => Err(E::unknown_variant(
                        value,
                        &["trace", "debug", "info", "warn", "error"],
                    )),
                }
            }

            fn visit_u64<E: de::Error>(self, value: u64) -> Result<LogLevel, E> {
                match value {
                    1 => Ok(LogLevel::Trace),
                    2 => Ok(LogLevel::Debug),
                    3 => Ok(LogLevel::Info),
                    4 => Ok(LogLevel::Warn),
                    5 => Ok(LogLevel::Error),
                    _ => Err(E::invalid_value(de::Unexpected::Unsigned(value), &"1-5")),
                }
            }
        }

        deserializer.deserialize_any(LogLevelVisitor)
    }
}

impl From<LogLevel> for tauri_plugin_log::LogLevel {
    fn from(level: LogLevel) -> Self {
        match level {
            LogLevel::Trace => tauri_plugin_log::LogLevel::Trace,
            LogLevel::Debug => tauri_plugin_log::LogLevel::Debug,
            LogLevel::Info => tauri_plugin_log::LogLevel::Info,
            LogLevel::Warn => tauri_plugin_log::LogLevel::Warn,
            LogLevel::Error => tauri_plugin_log::LogLevel::Error,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Type)]
pub struct ShortcutBinding {
    pub id: String,
    pub name: String,
    pub description: String,
    pub default_binding: String,
    pub current_binding: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, Type)]
pub struct LLMPrompt {
    pub id: String,
    pub name: String,
    pub prompt: String,
}

fn default_true() -> bool {
    true
}

/// A deterministic literal text replacement applied after fuzzy custom-word correction.
///
/// Unlike the fuzzy dictionary, this maps an exact heard phrase to an exact output, which is
/// the only safe way to fix large mishears the fuzzy matcher cannot (and must not) guess at,
/// e.g. "clawed" -> "Claude" (50% edit distance, phonetically distinct). Replacements run for
/// every engine and are applied in order.
#[derive(Serialize, Deserialize, Debug, Clone, Type)]
pub struct WordReplacement {
    /// The text to find, as heard/transcribed. May contain spaces for multi-word phrases.
    pub from: String,
    /// The replacement text. An empty string deletes the matched text.
    pub to: String,
    /// Match only on whole-word boundaries (default true). When false, matches substrings too.
    #[serde(default = "default_true")]
    pub whole_word: bool,
    /// Match case-sensitively (default false). When false, matching ignores case and the
    /// replacement adapts to the matched text's case pattern.
    #[serde(default)]
    pub case_sensitive: bool,
    /// Keep the replacement's exact casing instead of adapting it to the matched text.
    /// Used by learned names, brands, and acronyms whose casing carries meaning.
    #[serde(default)]
    pub preserve_replacement_case: bool,
}

/// Opt-in, on-device personalization data (issue #16, Tier 1).
///
/// Kept in a separate store from the user-authored `custom_words`/`word_replacements` so it can be
/// inspected, exported, and reset on its own without ever touching hand-authored entries. All
/// processing is local; nothing leaves the device. When `enabled` is false (the default) none of
/// this data affects transcription and no history mining is surfaced.
#[derive(Serialize, Deserialize, Debug, Clone, Default, Type)]
pub struct PersonalizationData {
    /// Opt-in master switch. Off by default.
    #[serde(default)]
    pub enabled: bool,
    /// Words the user accepted from history-mined suggestions. Applied like `custom_words` (fuzzy)
    /// when `enabled`.
    #[serde(default)]
    pub learned_words: Vec<String>,
    /// Learned heard->meant corrections captured from in-app transcript edits (issue #16 PR2).
    /// Defined now for a forward-compatible data model; empty until the capture surface ships.
    /// Applied like `word_replacements` (deterministic) when `enabled`.
    #[serde(default)]
    pub learned_replacements: Vec<WordReplacement>,
    /// Mined suggestions the user dismissed, so they are never surfaced again.
    #[serde(default)]
    pub dismissed_suggestions: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Type)]
pub struct PostProcessProvider {
    pub id: String,
    pub label: String,
    pub base_url: String,
    #[serde(default)]
    pub allow_base_url_edit: bool,
    #[serde(default)]
    pub models_endpoint: Option<String>,
    #[serde(default)]
    pub supports_structured_output: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "lowercase")]
pub enum OverlayPosition {
    None,
    Top,
    Bottom,
}

/// A 3x3 grid of placement anchors on a monitor's work area. Used by #9's
/// reposition feature: the user picks an anchor (and can drag to nudge), and
/// the overlay is placed relative to that anchor on whichever monitor has the
/// cursor, then clamped fully on-screen.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "lowercase")]
pub enum OverlayAnchor {
    TopLeft,
    TopCenter,
    TopRight,
    MiddleLeft,
    MiddleCenter,
    MiddleRight,
    BottomLeft,
    BottomCenter,
    BottomRight,
}

/// A user-chosen overlay placement that overrides the centered Top/Bottom
/// default: an anchor on the active monitor plus a logical-pixel nudge (dx, dy)
/// from that anchor, set by dragging the bug. When `overlay_custom_position` is
/// `None`, the overlay uses the default centered Top/Bottom placement.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Type)]
pub struct OverlayCustomPosition {
    pub anchor: OverlayAnchor,
    pub dx: f64,
    pub dy: f64,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, Default, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum ModelUnloadTimeout {
    Never,
    Immediately,
    Min2,
    #[default]
    Min5,
    Min10,
    Min15,
    Hour1,
    Sec15, // Debug mode only
}

/// How a finished transcript gets delivered to the target application.
///
/// `None` and `ExternalScript` never touch a window: `None` is a no-op, and
/// `ExternalScript` hands the transcript to an arbitrary program on `argv[1]`
/// (`clipboard::paste_via_external_script`). The other four -- `Direct` via
/// `enigo.text()`, and `CtrlV` / `CtrlShiftV` / `ShiftInsert` via synthesized
/// keystrokes -- all resolve to `SendInput`-style injection against the
/// foreground window's input queue, so they only land correctly when that
/// window is focused. See [`PasteMethod::requires_focus`] for the capability
/// this drives (issue #162): target-lock (#120) is meaningless for a method
/// that has no window to lock.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum PasteMethod {
    CtrlV,
    Direct,
    None,
    ShiftInsert,
    CtrlShiftV,
    ExternalScript,
}

impl PasteMethod {
    /// Whether this method must act on a focused window to work.
    ///
    /// `false` for `None` (no-op) and `ExternalScript` (runs a program, not a
    /// window-directed injection) -- both are unaffected by which window is
    /// focused, so a focus-borrowing feature like target-lock (#120) has
    /// nothing to do for them. `true` for every other variant: each is a form
    /// of keystroke/text injection into the foreground window's input queue,
    /// so it needs that window focused to land in the right place.
    ///
    /// Callers should use this instead of matching on individual variants when
    /// the question is "does this method need a focused window", so a future
    /// variant is covered by construction rather than by remembering to add it
    /// to every such match (#123).
    pub fn requires_focus(&self) -> bool {
        !matches!(self, PasteMethod::None | PasteMethod::ExternalScript)
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, Default, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum ClipboardHandling {
    #[default]
    DontModify,
    CopyToClipboard,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, Default, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum AutoSubmitKey {
    #[default]
    Enter,
    CtrlEnter,
    CmdEnter,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum RecordingRetentionPeriod {
    Never,
    PreserveLimit,
    Days3,
    Weeks2,
    Months3,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum KeyboardImplementation {
    Tauri,
    HandyKeys,
}

impl Default for KeyboardImplementation {
    fn default() -> Self {
        #[cfg(target_os = "linux")]
        return KeyboardImplementation::Tauri;
        #[cfg(not(target_os = "linux"))]
        return KeyboardImplementation::HandyKeys;
    }
}

impl Default for PasteMethod {
    fn default() -> Self {
        // Default to CtrlV for macOS and Windows, Direct for Linux
        #[cfg(target_os = "linux")]
        return PasteMethod::Direct;
        #[cfg(not(target_os = "linux"))]
        return PasteMethod::CtrlV;
    }
}

impl ModelUnloadTimeout {
    pub fn to_minutes(self) -> Option<u64> {
        match self {
            ModelUnloadTimeout::Never => None,
            ModelUnloadTimeout::Immediately => Some(0), // Special case for immediate unloading
            ModelUnloadTimeout::Min2 => Some(2),
            ModelUnloadTimeout::Min5 => Some(5),
            ModelUnloadTimeout::Min10 => Some(10),
            ModelUnloadTimeout::Min15 => Some(15),
            ModelUnloadTimeout::Hour1 => Some(60),
            ModelUnloadTimeout::Sec15 => Some(0), // Special case for debug - handled separately
        }
    }

    pub fn to_seconds(self) -> Option<u64> {
        match self {
            ModelUnloadTimeout::Never => None,
            ModelUnloadTimeout::Immediately => Some(0), // Special case for immediate unloading
            ModelUnloadTimeout::Sec15 => Some(15),
            _ => self.to_minutes().map(|m| m * 60),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum SoundTheme {
    Marimba,
    Pop,
    Custom,
}

impl SoundTheme {
    fn as_str(self) -> &'static str {
        match self {
            SoundTheme::Marimba => "marimba",
            SoundTheme::Pop => "pop",
            SoundTheme::Custom => "custom",
        }
    }

    pub fn to_start_path(self) -> String {
        format!("resources/{}_start.wav", self.as_str())
    }

    pub fn to_stop_path(self) -> String {
        format!("resources/{}_stop.wav", self.as_str())
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, Default, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum TypingTool {
    #[default]
    Auto,
    Wtype,
    Kwtype,
    Dotool,
    Ydotool,
    Xdotool,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, Default, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum WhisperAcceleratorSetting {
    #[default]
    Auto,
    Cpu,
    Gpu,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, Default, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum OrtAcceleratorSetting {
    #[default]
    Auto,
    Cpu,
    Cuda,
    #[serde(rename = "directml")]
    DirectMl,
    Rocm,
}

#[derive(Clone, Serialize, Deserialize, Type)]
#[serde(transparent)]
pub(crate) struct SecretMap(HashMap<String, String>);

impl fmt::Debug for SecretMap {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let redacted: HashMap<&String, &str> = self
            .0
            .iter()
            .map(|(k, v)| (k, if v.is_empty() { "" } else { "[REDACTED]" }))
            .collect();
        redacted.fmt(f)
    }
}

impl std::ops::Deref for SecretMap {
    type Target = HashMap<String, String>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for SecretMap {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

/* still handy for composing the initial JSON in the store ------------- */
#[derive(Serialize, Deserialize, Debug, Clone, Type)]
pub struct AppSettings {
    pub bindings: HashMap<String, ShortcutBinding>,
    pub push_to_talk: bool,
    pub audio_feedback: bool,
    #[serde(default = "default_audio_feedback_volume")]
    pub audio_feedback_volume: f32,
    #[serde(default = "default_sound_theme")]
    pub sound_theme: SoundTheme,
    #[serde(default = "default_start_hidden")]
    pub start_hidden: bool,
    #[serde(default = "default_autostart_enabled")]
    pub autostart_enabled: bool,
    #[serde(default = "default_update_checks_enabled")]
    pub update_checks_enabled: bool,
    #[serde(default = "default_model")]
    pub selected_model: String,
    #[serde(default = "default_always_on_microphone")]
    pub always_on_microphone: bool,
    #[serde(default)]
    pub selected_microphone: Option<String>,
    #[serde(default)]
    pub clamshell_microphone: Option<String>,
    #[serde(default)]
    pub selected_output_device: Option<String>,
    #[serde(default = "default_translate_to_english")]
    pub translate_to_english: bool,
    #[serde(default = "default_selected_language")]
    pub selected_language: String,
    #[serde(default = "default_overlay_position")]
    pub overlay_position: OverlayPosition,
    /// User-chosen precise overlay placement (anchor + drag nudge). When set it
    /// overrides the centered Top/Bottom placement; `None` = default placement.
    #[serde(default)]
    pub overlay_custom_position: Option<OverlayCustomPosition>,
    /// Last visible overlay placement, remembered when the tray show/hide
    /// toggle hides the overlay (sets `overlay_position` to None) so re-showing
    /// restores the user's Top/Bottom choice instead of forcing the default.
    #[serde(default)]
    pub overlay_restore_position: Option<OverlayPosition>,
    #[serde(default = "default_debug_mode")]
    pub debug_mode: bool,
    #[serde(default = "default_log_level")]
    pub log_level: LogLevel,
    #[serde(default)]
    pub custom_words: Vec<String>,
    /// Deterministic literal heard->meant replacements, applied after fuzzy custom-word
    /// correction and before filler removal, for every engine. See [`WordReplacement`].
    #[serde(default)]
    pub word_replacements: Vec<WordReplacement>,
    #[serde(default)]
    pub model_unload_timeout: ModelUnloadTimeout,
    #[serde(default = "default_word_correction_threshold")]
    pub word_correction_threshold: f64,
    #[serde(default = "default_history_limit")]
    pub history_limit: usize,
    #[serde(default = "default_recording_retention_period")]
    pub recording_retention_period: RecordingRetentionPeriod,
    #[serde(default)]
    pub paste_method: PasteMethod,
    #[serde(default)]
    pub clipboard_handling: ClipboardHandling,
    #[serde(default = "default_auto_submit")]
    pub auto_submit: bool,
    #[serde(default)]
    pub auto_submit_key: AutoSubmitKey,
    #[serde(default = "default_post_process_enabled")]
    pub post_process_enabled: bool,
    #[serde(default = "default_post_process_provider_id")]
    pub post_process_provider_id: String,
    #[serde(default = "default_post_process_providers")]
    pub post_process_providers: Vec<PostProcessProvider>,
    #[serde(default = "default_post_process_api_keys")]
    pub post_process_api_keys: SecretMap,
    #[serde(default = "default_post_process_models")]
    pub post_process_models: HashMap<String, String>,
    #[serde(default = "default_post_process_prompts")]
    pub post_process_prompts: Vec<LLMPrompt>,
    #[serde(default)]
    pub post_process_selected_prompt_id: Option<String>,
    #[serde(default)]
    pub mute_while_recording: bool,
    #[serde(default)]
    pub append_trailing_space: bool,
    /// When true, transcriptions are emitted as raw lowercased, unpunctuated text (issue #19).
    /// A per-dictation shortcut / CLI flag can override this at runtime without persisting.
    #[serde(default)]
    pub raw_output: bool,
    /// When true (default), a transcript has its spelled-out numbers rewritten as digits and
    /// symbols — "twenty five dollars" -> "$25", "ten percent" -> "10%", "three point five" ->
    /// "3.5". Applied on the normal dictation path, and on raw output when `format_raw_output`
    /// is also on. The LLM post-processing path does its own formatting and is left untouched.
    /// See [`crate::audio_toolkit::format_numbers`].
    #[serde(default = "default_format_numbers")]
    pub format_numbers: bool,
    /// When true, raw output interprets spoken punctuation ("question mark" -> "?") and applies
    /// `format_numbers` if that is also on, so raw mode is usable for dictation with no model in
    /// the loop (issue #66).
    ///
    /// Defaults to false because turning it on rewrites text that raw mode historically passed
    /// through verbatim. The advanced settings toggle lets each user opt in without changing
    /// existing raw-mode installs on upgrade.
    ///
    /// Turning it off is also how you type the command words themselves, since there is no
    /// escape word. See [`crate::audio_toolkit::apply_spoken_punctuation`].
    #[serde(default = "default_format_raw_output")]
    pub format_raw_output: bool,
    #[serde(default = "default_app_language")]
    pub app_language: String,
    #[serde(default)]
    pub experimental_enabled: bool,
    #[serde(default)]
    pub lazy_stream_close: bool,
    #[serde(default)]
    pub keyboard_implementation: KeyboardImplementation,
    #[serde(default = "default_show_tray_icon")]
    pub show_tray_icon: bool,
    #[serde(default = "default_paste_delay_ms")]
    pub paste_delay_ms: u64,
    #[serde(default = "default_typing_tool")]
    pub typing_tool: TypingTool,
    pub external_script_path: Option<String>,
    #[serde(default)]
    pub custom_filler_words: Option<Vec<String>>,
    #[serde(default)]
    pub whisper_accelerator: WhisperAcceleratorSetting,
    #[serde(default)]
    pub ort_accelerator: OrtAcceleratorSetting,
    #[serde(default = "default_whisper_gpu_device")]
    pub whisper_gpu_device: i32,
    #[serde(default)]
    pub extra_recording_buffer_ms: u64,
    /// Opt-in, on-device personalization (issue #16, Tier 1). Off by default. See
    /// [`PersonalizationData`].
    #[serde(default)]
    pub personalization: PersonalizationData,
}

fn default_model() -> String {
    "".to_string()
}

fn default_always_on_microphone() -> bool {
    false
}

fn default_translate_to_english() -> bool {
    false
}

fn default_format_numbers() -> bool {
    true
}

fn default_format_raw_output() -> bool {
    false
}

fn default_start_hidden() -> bool {
    false
}

fn default_autostart_enabled() -> bool {
    false
}

fn default_update_checks_enabled() -> bool {
    // Package detection runs after Tauri resolves the installed executable.
    // Keep the serialized default off until that one-time migration determines
    // whether this is an installed NSIS package rather than MSI or portable.
    false
}

fn migrate_update_checks_v0_4_2(
    settings: &mut AppSettings,
    migration_complete: bool,
    update_channel_available: bool,
) -> Option<bool> {
    if migration_complete || !update_channel_available {
        return None;
    }

    // Every release through v0.4.1 forced this value off. Enable the new feed
    // once an installed NSIS package is actually running. MSI, portable, and
    // non-Windows packages must not consume the migration because they can
    // share AppData with a later NSIS install. The durable marker then preserves
    // any later user opt-out.
    settings.update_checks_enabled = true;
    Some(true)
}

fn default_selected_language() -> String {
    "auto".to_string()
}

fn default_overlay_position() -> OverlayPosition {
    #[cfg(target_os = "linux")]
    return OverlayPosition::None;
    #[cfg(not(target_os = "linux"))]
    return OverlayPosition::Bottom;
}

fn default_debug_mode() -> bool {
    false
}

fn default_log_level() -> LogLevel {
    LogLevel::Debug
}

fn default_word_correction_threshold() -> f64 {
    0.18
}

fn default_paste_delay_ms() -> u64 {
    60
}

fn default_auto_submit() -> bool {
    false
}

fn default_history_limit() -> usize {
    5
}

fn default_recording_retention_period() -> RecordingRetentionPeriod {
    RecordingRetentionPeriod::PreserveLimit
}

fn default_audio_feedback_volume() -> f32 {
    1.0
}

fn default_sound_theme() -> SoundTheme {
    SoundTheme::Marimba
}

fn default_post_process_enabled() -> bool {
    false
}

fn default_app_language() -> String {
    tauri_plugin_os::locale()
        .map(|l| l.replace('_', "-"))
        .unwrap_or_else(|| "en".to_string())
}

fn default_show_tray_icon() -> bool {
    true
}

fn default_post_process_provider_id() -> String {
    "openai".to_string()
}

fn default_post_process_providers() -> Vec<PostProcessProvider> {
    let mut providers = vec![
        PostProcessProvider {
            id: "openai".to_string(),
            label: "OpenAI".to_string(),
            base_url: "https://api.openai.com/v1".to_string(),
            allow_base_url_edit: false,
            models_endpoint: Some("/models".to_string()),
            supports_structured_output: true,
        },
        PostProcessProvider {
            id: "zai".to_string(),
            label: "Z.AI".to_string(),
            base_url: "https://api.z.ai/api/paas/v4".to_string(),
            allow_base_url_edit: false,
            models_endpoint: Some("/models".to_string()),
            supports_structured_output: true,
        },
        PostProcessProvider {
            id: "openrouter".to_string(),
            label: "OpenRouter".to_string(),
            base_url: "https://openrouter.ai/api/v1".to_string(),
            allow_base_url_edit: false,
            models_endpoint: Some("/models".to_string()),
            supports_structured_output: true,
        },
        PostProcessProvider {
            id: "anthropic".to_string(),
            label: "Anthropic".to_string(),
            base_url: "https://api.anthropic.com/v1".to_string(),
            allow_base_url_edit: false,
            models_endpoint: Some("/models".to_string()),
            supports_structured_output: false,
        },
        PostProcessProvider {
            id: "groq".to_string(),
            label: "Groq".to_string(),
            base_url: "https://api.groq.com/openai/v1".to_string(),
            allow_base_url_edit: false,
            models_endpoint: Some("/models".to_string()),
            supports_structured_output: false,
        },
        PostProcessProvider {
            id: "cerebras".to_string(),
            label: "Cerebras".to_string(),
            base_url: "https://api.cerebras.ai/v1".to_string(),
            allow_base_url_edit: false,
            models_endpoint: Some("/models".to_string()),
            supports_structured_output: true,
        },
    ];

    // Note: We always include Apple Intelligence on macOS ARM64 without checking availability
    // at startup. The availability check is deferred to when the user actually tries to use it
    // (in actions.rs). This prevents crashes on macOS 26.x beta where accessing
    // SystemLanguageModel.default during early app initialization causes SIGABRT.
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        providers.push(PostProcessProvider {
            id: APPLE_INTELLIGENCE_PROVIDER_ID.to_string(),
            label: "Apple Intelligence".to_string(),
            base_url: "apple-intelligence://local".to_string(),
            allow_base_url_edit: false,
            models_endpoint: None,
            supports_structured_output: true,
        });
    }

    // AWS Bedrock via Mantle (OpenAI-compatible endpoint)
    providers.push(PostProcessProvider {
        id: "bedrock_mantle".to_string(),
        label: "AWS Bedrock (Mantle)".to_string(),
        base_url: "https://bedrock-mantle.us-east-1.api.aws/v1".to_string(),
        allow_base_url_edit: false,
        models_endpoint: Some("/models".to_string()),
        supports_structured_output: true,
    });

    // Custom provider always comes last
    providers.push(PostProcessProvider {
        id: "custom".to_string(),
        label: "Custom".to_string(),
        base_url: "http://localhost:11434/v1".to_string(),
        allow_base_url_edit: true,
        models_endpoint: Some("/models".to_string()),
        supports_structured_output: false,
    });

    providers
}

fn default_post_process_api_keys() -> SecretMap {
    let mut map = HashMap::new();
    for provider in default_post_process_providers() {
        map.insert(provider.id, String::new());
    }
    SecretMap(map)
}

fn default_model_for_provider(provider_id: &str) -> String {
    if provider_id == APPLE_INTELLIGENCE_PROVIDER_ID {
        return APPLE_INTELLIGENCE_DEFAULT_MODEL_ID.to_string();
    }
    String::new()
}

fn default_post_process_models() -> HashMap<String, String> {
    let mut map = HashMap::new();
    for provider in default_post_process_providers() {
        map.insert(
            provider.id.clone(),
            default_model_for_provider(&provider.id),
        );
    }
    map
}

fn default_post_process_prompts() -> Vec<LLMPrompt> {
    vec![LLMPrompt {
        id: "default_improve_transcriptions".to_string(),
        name: "Improve Transcriptions".to_string(),
        prompt: "Clean this transcript:\n1. Fix spelling, capitalization, and punctuation errors\n2. Convert number words to digits (twenty-five → 25, ten percent → 10%, five dollars → $5)\n3. Replace spoken punctuation with symbols (period → ., comma → ,, question mark → ?)\n4. Remove filler words (um, uh, like as filler)\n5. Keep the language in the original version (if it was french, keep it in french for example)\n\nPreserve exact meaning and word order. Do not paraphrase or reorder content.\n\nReturn only the cleaned transcript.\n\nTranscript:\n${output}".to_string(),
    }]
}

fn default_whisper_gpu_device() -> i32 {
    -1 // auto
}

fn default_typing_tool() -> TypingTool {
    TypingTool::Auto
}

fn ensure_post_process_defaults(settings: &mut AppSettings) -> bool {
    let mut changed = false;
    for provider in default_post_process_providers() {
        // Use match to do a single lookup - either sync existing or add new
        match settings
            .post_process_providers
            .iter_mut()
            .find(|p| p.id == provider.id)
        {
            Some(existing) => {
                // Sync supports_structured_output field for existing providers (migration)
                if existing.supports_structured_output != provider.supports_structured_output {
                    debug!(
                        "Updating supports_structured_output for provider '{}' from {} to {}",
                        provider.id,
                        existing.supports_structured_output,
                        provider.supports_structured_output
                    );
                    existing.supports_structured_output = provider.supports_structured_output;
                    changed = true;
                }
            }
            None => {
                // Provider doesn't exist, add it
                settings.post_process_providers.push(provider.clone());
                changed = true;
            }
        }

        if !settings.post_process_api_keys.contains_key(&provider.id) {
            settings
                .post_process_api_keys
                .insert(provider.id.clone(), String::new());
            changed = true;
        }

        let default_model = default_model_for_provider(&provider.id);
        match settings.post_process_models.get_mut(&provider.id) {
            Some(existing) => {
                if existing.is_empty() && !default_model.is_empty() {
                    *existing = default_model.clone();
                    changed = true;
                }
            }
            None => {
                settings
                    .post_process_models
                    .insert(provider.id.clone(), default_model);
                changed = true;
            }
        }
    }

    changed
}

pub const SETTINGS_STORE_PATH: &str = "settings_store.json";
const UPDATE_CHECKS_V0_4_2_MIGRATION_KEY: &str = "update_checks_v0_4_2_migrated";

pub fn get_default_settings() -> AppSettings {
    #[cfg(target_os = "windows")]
    let default_shortcut = "ctrl+alt+space";
    #[cfg(target_os = "macos")]
    let default_shortcut = "option+space";
    #[cfg(target_os = "linux")]
    let default_shortcut = "ctrl+space";
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    let default_shortcut = "alt+space";

    let mut bindings = HashMap::new();
    bindings.insert(
        "transcribe".to_string(),
        ShortcutBinding {
            id: "transcribe".to_string(),
            name: "Transcribe".to_string(),
            description: "Converts your speech into text.".to_string(),
            default_binding: default_shortcut.to_string(),
            current_binding: default_shortcut.to_string(),
        },
    );
    #[cfg(target_os = "windows")]
    let default_post_process_shortcut = "ctrl+shift+space";
    #[cfg(target_os = "macos")]
    let default_post_process_shortcut = "option+shift+space";
    #[cfg(target_os = "linux")]
    let default_post_process_shortcut = "ctrl+shift+space";
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    let default_post_process_shortcut = "alt+shift+space";

    bindings.insert(
        "transcribe_with_post_process".to_string(),
        ShortcutBinding {
            id: "transcribe_with_post_process".to_string(),
            name: "Transcribe with Post-Processing".to_string(),
            description: "Converts your speech into text and applies AI post-processing."
                .to_string(),
            default_binding: default_post_process_shortcut.to_string(),
            current_binding: default_post_process_shortcut.to_string(),
        },
    );
    #[cfg(target_os = "windows")]
    let default_raw_shortcut = "ctrl+alt+r";
    #[cfg(target_os = "macos")]
    let default_raw_shortcut = "option+shift+r";
    #[cfg(target_os = "linux")]
    let default_raw_shortcut = "ctrl+alt+r";
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    let default_raw_shortcut = "alt+r";

    bindings.insert(
        "transcribe_raw".to_string(),
        ShortcutBinding {
            id: "transcribe_raw".to_string(),
            name: "Transcribe (raw)".to_string(),
            description: "Converts your speech into raw, lowercased, unpunctuated text."
                .to_string(),
            default_binding: default_raw_shortcut.to_string(),
            current_binding: default_raw_shortcut.to_string(),
        },
    );

    // Target lock (#120): pin dictation to one window. Windows-only for now
    // (#119), so other platforms get no binding rather than a dead shortcut.
    #[cfg(target_os = "windows")]
    bindings.insert(
        "toggle_target_lock".to_string(),
        ShortcutBinding {
            id: "toggle_target_lock".to_string(),
            name: "Lock to window".to_string(),
            description: "Sends your text to one window until you unlock it.".to_string(),
            default_binding: "ctrl+alt+l".to_string(),
            current_binding: "ctrl+alt+l".to_string(),
        },
    );

    // One-shot window picker (#124): send just the next transcript to a window
    // you choose. Windows-only for now (#119), like the target lock.
    #[cfg(target_os = "windows")]
    bindings.insert(
        "pick_output_window".to_string(),
        ShortcutBinding {
            id: "pick_output_window".to_string(),
            name: "Send to window once".to_string(),
            description: "Pick a window for your next text only.".to_string(),
            default_binding: "ctrl+alt+w".to_string(),
            current_binding: "ctrl+alt+w".to_string(),
        },
    );

    bindings.insert(
        "cancel".to_string(),
        ShortcutBinding {
            id: "cancel".to_string(),
            name: "Cancel".to_string(),
            description: "Cancels the current recording.".to_string(),
            default_binding: "escape".to_string(),
            current_binding: "escape".to_string(),
        },
    );

    // Default engine: Parakeet V3 on Windows (milestone-A benchmark winner, see
    // bench/RESULTS.md). Other platforms keep upstream's empty default, which opens
    // the model picker on first run.
    #[cfg(target_os = "windows")]
    let default_model = "parakeet-tdt-0.6b-v3";
    #[cfg(not(target_os = "windows"))]
    let default_model = "";

    AppSettings {
        bindings,
        push_to_talk: false,
        audio_feedback: false,
        audio_feedback_volume: default_audio_feedback_volume(),
        sound_theme: default_sound_theme(),
        start_hidden: default_start_hidden(),
        autostart_enabled: default_autostart_enabled(),
        update_checks_enabled: default_update_checks_enabled(),
        selected_model: default_model.to_string(),
        always_on_microphone: false,
        selected_microphone: None,
        clamshell_microphone: None,
        selected_output_device: None,
        translate_to_english: false,
        selected_language: "auto".to_string(),
        overlay_position: default_overlay_position(),
        overlay_custom_position: None,
        overlay_restore_position: None,
        debug_mode: false,
        log_level: default_log_level(),
        custom_words: Vec::new(),
        word_replacements: Vec::new(),
        model_unload_timeout: ModelUnloadTimeout::default(),
        word_correction_threshold: default_word_correction_threshold(),
        history_limit: default_history_limit(),
        recording_retention_period: default_recording_retention_period(),
        paste_method: PasteMethod::default(),
        clipboard_handling: ClipboardHandling::default(),
        auto_submit: default_auto_submit(),
        auto_submit_key: AutoSubmitKey::default(),
        post_process_enabled: default_post_process_enabled(),
        post_process_provider_id: default_post_process_provider_id(),
        post_process_providers: default_post_process_providers(),
        post_process_api_keys: default_post_process_api_keys(),
        post_process_models: default_post_process_models(),
        post_process_prompts: default_post_process_prompts(),
        post_process_selected_prompt_id: None,
        mute_while_recording: false,
        append_trailing_space: false,
        raw_output: false,
        format_numbers: default_format_numbers(),
        format_raw_output: default_format_raw_output(),
        app_language: default_app_language(),
        experimental_enabled: false,
        lazy_stream_close: false,
        keyboard_implementation: KeyboardImplementation::default(),
        show_tray_icon: default_show_tray_icon(),
        paste_delay_ms: default_paste_delay_ms(),
        typing_tool: default_typing_tool(),
        external_script_path: None,
        custom_filler_words: None,
        whisper_accelerator: WhisperAcceleratorSetting::default(),
        ort_accelerator: OrtAcceleratorSetting::default(),
        whisper_gpu_device: default_whisper_gpu_device(),
        extra_recording_buffer_ms: 0,
        personalization: PersonalizationData::default(),
    }
}

impl AppSettings {
    pub fn active_post_process_provider(&self) -> Option<&PostProcessProvider> {
        self.post_process_providers
            .iter()
            .find(|provider| provider.id == self.post_process_provider_id)
    }

    pub fn post_process_provider(&self, provider_id: &str) -> Option<&PostProcessProvider> {
        self.post_process_providers
            .iter()
            .find(|provider| provider.id == provider_id)
    }

    pub fn post_process_provider_mut(
        &mut self,
        provider_id: &str,
    ) -> Option<&mut PostProcessProvider> {
        self.post_process_providers
            .iter_mut()
            .find(|provider| provider.id == provider_id)
    }
}

/// Process-wide cache of the deserialized [`AppSettings`].
///
/// Before this existed, every one of the ~130 `get_settings` call sites re-opened
/// the Tauri store and re-deserialized the whole settings object, including on the
/// paste hot path (issue #166). Reads now hit the cache; the only invalidation
/// point is [`write_settings`], which is the single funnel every mutation in the
/// app already goes through, so the cache cannot drift from the persisted store.
pub(crate) struct SettingsCache {
    inner: RwLock<Option<AppSettings>>,
}

impl SettingsCache {
    pub(crate) const fn new() -> Self {
        Self {
            inner: RwLock::new(None),
        }
    }

    /// Return the cached settings, loading (and caching) them with `load` on a miss.
    ///
    /// `load` runs without the lock held: it touches the filesystem, and holding a
    /// write lock across that would serialize every reader behind disk I/O. A
    /// concurrent miss can therefore run `load` twice; both produce the same value,
    /// so the last write wins harmlessly.
    ///
    /// `load` always populates the cache with its result, so it must not be
    /// used for a load that can fail to reach the store: `get_settings` and
    /// `load_or_create_app_settings` handle their store-open failure directly
    /// instead of going through this helper, so defaults from an unavailable
    /// store are never cached (see the P2 fix for issue #166's settings
    /// refactor). Kept for the cache-only tests below that exercise the
    /// miss/hit/invalidate contract in isolation.
    #[cfg(test)]
    pub(crate) fn get_or_load(&self, load: impl FnOnce() -> AppSettings) -> AppSettings {
        if let Some(cached) = self.peek() {
            return cached;
        }
        let loaded = load();
        self.store(&loaded);
        loaded
    }

    pub(crate) fn peek(&self) -> Option<AppSettings> {
        self.inner
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    /// Unconditionally overwrite the cached value. Production code always
    /// goes through `write_through` (a write plus a publish, atomically) or
    /// `fill_if_empty` (a loader's best-effort fill); this raw setter is kept
    /// for the cache-only tests below that need to seed a starting value.
    #[cfg(test)]
    pub(crate) fn store(&self, settings: &AppSettings) {
        *self
            .inner
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(settings.clone());
    }

    /// Publish `settings` to the cache only if nothing is cached yet.
    ///
    /// For a cold-cache *loader* (`get_settings`, `load_or_create_app_settings`):
    /// the loader reads the store without holding any lock, so a concurrent
    /// `write_through` can persist and publish a newer value while the loader
    /// is still mid-read. An unconditional `store` after that would clobber
    /// the newer value with the loader's now-stale read, and a later
    /// read-modify-write would then serialize that stale snapshot back over
    /// the store, silently erasing the write that already landed. Checking
    /// emptiness under the same write lock `write_through` uses makes the two
    /// mutually exclusive: whichever finishes last either wins the empty slot
    /// or finds it already filled and no-ops, so a loader can never overwrite
    /// a write that beat it to the cache.
    pub(crate) fn fill_if_empty(&self, settings: &AppSettings) {
        let mut guard = self
            .inner
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if guard.is_none() {
            *guard = Some(settings.clone());
        }
    }

    /// Run `persist` (the store write) and publish `settings` to the cache as
    /// one step under the cache's write lock.
    ///
    /// Locking rule: the store write and the cache update must happen while
    /// the same write-guard is held. Two concurrent writers calling `store`
    /// and then a separate cache update could interleave as store(A),
    /// store(B), cache(B), cache(A), leaving the cache pinned to the older
    /// value A forever even though the store holds B. Serializing both steps
    /// under one lock makes the last writer to acquire the lock win for both
    /// the store and the cache, together. Callers must not read the cache
    /// (directly or via effects) until this call returns, so the read lock
    /// taken by `peek` is never requested while this write lock is held.
    pub(crate) fn write_through(&self, persist: impl FnOnce(), settings: &AppSettings) {
        let mut guard = self
            .inner
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        persist();
        *guard = Some(settings.clone());
    }

    pub(crate) fn invalidate(&self) {
        *self
            .inner
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
    }
}

static SETTINGS_CACHE: SettingsCache = SettingsCache::new();

/// Open the settings store, logging instead of aborting the process when it
/// cannot be initialized. Callers fall back to defaults; an unwritable store
/// costs persistence for the session, which beats killing a running dictation
/// app mid-session (issue #166).
fn settings_store(app: &AppHandle) -> Option<Arc<Store<tauri::Wry>>> {
    match app.store(crate::portable::store_path(SETTINGS_STORE_PATH)) {
        Ok(store) => Some(store),
        Err(e) => {
            warn!("Failed to initialize the settings store: {e}");
            None
        }
    }
}

/// Apply a JSON value to a single [`AppSettings`] field, addressed by its
/// serialized key.
///
/// This is the typed core of the generic settings mutator that replaced ~33
/// near-identical `change_*_setting` commands (issue #166). Patching through
/// `serde_json` rather than a hand-written match means the stored shape is the
/// authority: an unknown key or a value of the wrong type is rejected here
/// rather than silently coerced, and adding a field to `AppSettings` makes it
/// settable with no extra command, no `collect_commands!` entry, and no new
/// branch in this function.
pub fn apply_setting_value(
    settings: &mut AppSettings,
    key: &str,
    value: serde_json::Value,
) -> Result<(), String> {
    let mut object = match serde_json::to_value(&*settings) {
        Ok(serde_json::Value::Object(map)) => map,
        Ok(_) => return Err("Settings did not serialize to a JSON object".to_string()),
        Err(e) => return Err(format!("Failed to serialize settings: {e}")),
    };

    if !object.contains_key(key) {
        return Err(format!("Unknown setting '{key}'"));
    }
    object.insert(key.to_string(), value);

    let updated: AppSettings = serde_json::from_value(serde_json::Value::Object(object))
        .map_err(|e| format!("Invalid value for setting '{key}': {e}"))?;

    let previous = std::mem::replace(settings, updated);
    normalize_after_change(settings, key, &previous);
    Ok(())
}

/// Keep fields that are derived from the changed one consistent.
///
/// These are the adjustments the hand-written commands made around their one
/// assignment; they live here so the generic mutator reproduces them exactly.
fn normalize_after_change(settings: &mut AppSettings, key: &str, previous: &AppSettings) {
    if key == "overlay_position" {
        // Keep the restore slot (read by the tray show/hide toggle) in sync so it
        // always holds the most recent visible placement. Choosing Top/Bottom
        // records it; hiding via the dropdown ("none") remembers the outgoing
        // placement, so a dropdown-hide followed by a tray-show restores the
        // position the user last picked instead of an older value or the default.
        if settings.overlay_position != OverlayPosition::None {
            settings.overlay_restore_position = Some(settings.overlay_position);
        } else if previous.overlay_position != OverlayPosition::None {
            settings.overlay_restore_position = Some(previous.overlay_position);
        }
        // Picking a coarse position (or hiding the overlay) supersedes any fine
        // grid/drag placement from #9, so clear it and fall back to the centered
        // Top/Bottom default.
        settings.overlay_custom_position = None;
    }
}

pub fn load_or_create_app_settings(app: &AppHandle) -> AppSettings {
    // Initialize store
    let Some(store) = settings_store(app) else {
        // The store failed to open, which may be transient (e.g. a startup
        // race). Return defaults for this call but leave the cache empty
        // instead of caching them, so the next read retries the store rather
        // than being stuck serving defaults — and so a later write can't
        // clobber a store that recovers with defaults-plus-one-field.
        return get_default_settings();
    };

    let mut settings = if let Some(settings_value) = store.get("settings") {
        // Parse the entire settings object
        match serde_json::from_value::<AppSettings>(settings_value) {
            Ok(mut settings) => {
                debug!("Found existing settings: {:?}", settings);
                let default_settings = get_default_settings();
                let mut updated = false;

                // Merge default bindings into existing settings
                for (key, value) in default_settings.bindings {
                    if let std::collections::hash_map::Entry::Vacant(entry) =
                        settings.bindings.entry(key)
                    {
                        debug!("Adding missing binding: {}", entry.key());
                        entry.insert(value);
                        updated = true;
                    }
                }

                if updated {
                    debug!("Settings updated with new bindings");
                    store.set("settings", serde_json::to_value(&settings).unwrap());
                }

                settings
            }
            Err(e) => {
                warn!("Failed to parse settings: {}", e);
                // Fall back to default settings if parsing fails
                let default_settings = get_default_settings();
                store.set("settings", serde_json::to_value(&default_settings).unwrap());
                default_settings
            }
        }
    } else {
        let default_settings = get_default_settings();
        store.set("settings", serde_json::to_value(&default_settings).unwrap());
        default_settings
    };

    if ensure_post_process_defaults(&mut settings) {
        store.set("settings", serde_json::to_value(&settings).unwrap());
    }

    let update_checks_migrated = store
        .get(UPDATE_CHECKS_V0_4_2_MIGRATION_KEY)
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    if let Some(enabled) = migrate_update_checks_v0_4_2(
        &mut settings,
        update_checks_migrated,
        crate::update_channel_available(),
    ) {
        store.set("settings", serde_json::to_value(&settings).unwrap());
        store.set(UPDATE_CHECKS_V0_4_2_MIGRATION_KEY, true);
        let _ = app.emit(
            "settings-changed",
            serde_json::json!({ "setting": "update_checks_enabled", "value": enabled }),
        );
        debug!("Configured signed update checks for the v0.4.2 package migration: {enabled}");
    }

    // A loader's read isn't lock-protected, so a concurrent `write_settings`
    // may have already published a newer value while this function was
    // still reading the store: fill only if the slot is still empty, so
    // that write is never clobbered (see `SettingsCache::fill_if_empty`).
    SETTINGS_CACHE.fill_if_empty(&settings);
    settings
}

/// Read the settings, from the in-process cache when it is warm.
///
/// The store is only touched on a cold cache; every mutation funnels through
/// [`write_settings`], which refreshes the cache, so callers still observe their
/// own writes.
pub fn get_settings(app: &AppHandle) -> AppSettings {
    if let Some(cached) = SETTINGS_CACHE.peek() {
        return cached;
    }
    let Some(store) = settings_store(app) else {
        // The store failed to open. Return defaults for this call without
        // caching them (`get_or_load` would cache unconditionally), so the
        // next read retries the store instead of being stuck serving
        // defaults for the rest of the process's life.
        return get_default_settings();
    };
    let settings = read_settings_from_open_store(&store);
    // Same race as `load_or_create_app_settings`: only fill an empty slot so
    // a concurrent write that already published wins instead of being
    // overwritten by this read's stale snapshot.
    SETTINGS_CACHE.fill_if_empty(&settings);
    settings
}

fn read_settings_from_open_store(store: &Store<tauri::Wry>) -> AppSettings {
    let mut settings = if let Some(settings_value) = store.get("settings") {
        serde_json::from_value::<AppSettings>(settings_value).unwrap_or_else(|_| {
            let default_settings = get_default_settings();
            store.set("settings", serde_json::to_value(&default_settings).unwrap());
            default_settings
        })
    } else {
        let default_settings = get_default_settings();
        store.set("settings", serde_json::to_value(&default_settings).unwrap());
        default_settings
    };

    if ensure_post_process_defaults(&mut settings) {
        store.set("settings", serde_json::to_value(&settings).unwrap());
    }

    settings
}

/// Persist `settings`, returning `Err` when the store could not be written.
///
/// A caller that surfaces this to the user (`update_setting` and the other
/// `#[tauri::command]`s that call this directly) must propagate it *before*
/// running any side effect, so the frontend's rollback+toast path never
/// confirms a save that didn't happen. A caller with nowhere to report to
/// (a background manager, an event handler) should log it instead of
/// discarding it silently — never `unwrap`/panic on it, since an unwritable
/// store is a real, recoverable condition (issue #166).
pub fn write_settings(app: &AppHandle, settings: AppSettings) -> Result<(), String> {
    let Some(store) = settings_store(app) else {
        // The store is unavailable, so the new value cannot be persisted. Drop
        // the cache rather than caching an unpersisted value, so the next read
        // reflects whatever is actually on disk.
        SETTINGS_CACHE.invalidate();
        return Err("Settings store is unavailable; the change was not saved".to_string());
    };

    // The store write and the cache publish happen together under the
    // cache's write lock (see `SettingsCache::write_through`) so concurrent
    // writers can't interleave and strand the cache behind the store.
    // Callers (e.g. `apply_setting_change`) only run their side effects after
    // this function returns, so no reader ever waits on this lock from
    // inside it.
    SETTINGS_CACHE.write_through(
        || store.set("settings", serde_json::to_value(&settings).unwrap()),
        &settings,
    );
    Ok(())
}

pub fn get_bindings(app: &AppHandle) -> HashMap<String, ShortcutBinding> {
    let settings = get_settings(app);

    settings.bindings
}

pub fn get_stored_binding(app: &AppHandle, id: &str) -> ShortcutBinding {
    let bindings = get_bindings(app);

    let binding = bindings.get(id).unwrap().clone();

    binding
}

pub fn get_history_limit(app: &AppHandle) -> usize {
    let settings = get_settings(app);
    settings.history_limit
}

pub fn get_recording_retention_period(app: &AppHandle) -> RecordingRetentionPeriod {
    let settings = get_settings(app);
    settings.recording_retention_period
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// The exact shape a released build persists, reduced to the fields that
    /// have no serde default. Anything added to `AppSettings` after this must
    /// stay optional, or an existing install fails to load its settings.
    fn legacy_stored_settings() -> serde_json::Value {
        json!({
            "bindings": {
                "transcribe": {
                    "id": "transcribe",
                    "name": "Transcribe",
                    "description": "Converts your speech into text.",
                    "default_binding": "ctrl+alt+space",
                    "current_binding": "ctrl+alt+f1"
                }
            },
            "push_to_talk": true,
            "audio_feedback": true,
            "external_script_path": null
        })
    }

    #[test]
    fn stored_settings_survive_a_round_trip_unchanged() {
        // The generic mutator patches settings as JSON, so the serialized shape
        // is now load-bearing: serialize -> deserialize -> serialize must be a
        // fixed point, or a write could silently rewrite unrelated fields.
        let settings = get_default_settings();
        let serialized = serde_json::to_value(&settings).expect("settings serialize");
        let reloaded: AppSettings =
            serde_json::from_value(serialized.clone()).expect("settings deserialize");
        let reserialized = serde_json::to_value(&reloaded).expect("settings re-serialize");

        assert_eq!(serialized, reserialized);
    }

    #[test]
    fn settings_from_an_older_install_load_with_defaults_filled_in() {
        let settings: AppSettings =
            serde_json::from_value(legacy_stored_settings()).expect("legacy settings deserialize");

        // The persisted values survive...
        assert!(settings.push_to_talk);
        assert!(settings.audio_feedback);
        assert_eq!(
            settings.bindings["transcribe"].current_binding,
            "ctrl+alt+f1"
        );
        // ...and everything added since gets its default rather than failing the load.
        assert_eq!(settings.selected_language, default_selected_language());
        assert_eq!(settings.paste_method, PasteMethod::default());
        assert!(settings.format_numbers);
        assert!(!settings.personalization.enabled);
    }

    #[test]
    fn settings_written_by_a_newer_install_still_load() {
        // Downgrades happen (a bad release, a portable copy). Unknown keys must
        // be ignored, not fail the whole load back to defaults.
        let mut stored = legacy_stored_settings();
        stored["setting_from_the_future"] = json!("surprise");

        let settings: AppSettings =
            serde_json::from_value(stored).expect("forward-compatible settings deserialize");
        assert!(settings.push_to_talk);
    }

    #[test]
    fn legacy_numeric_log_levels_still_migrate() {
        // Releases before the string encoding stored log_level as 1-5.
        let mut stored = legacy_stored_settings();
        stored["log_level"] = json!(4);

        let settings: AppSettings =
            serde_json::from_value(stored).expect("numeric log level deserializes");
        assert_eq!(settings.log_level, LogLevel::Warn);
        assert_eq!(
            serde_json::to_value(settings.log_level).unwrap(),
            json!("warn"),
            "a migrated level is rewritten in the current string encoding"
        );
    }

    #[test]
    fn generic_mutator_round_trips_a_value() {
        let mut settings = get_default_settings();

        apply_setting_value(&mut settings, "push_to_talk", json!(true)).expect("bool applies");
        apply_setting_value(&mut settings, "paste_delay_ms", json!(250)).expect("number applies");
        apply_setting_value(&mut settings, "selected_language", json!("de"))
            .expect("string applies");
        apply_setting_value(&mut settings, "custom_words", json!(["AudioBud", "Tauri"]))
            .expect("list applies");
        apply_setting_value(&mut settings, "paste_method", json!("shift_insert"))
            .expect("enum applies");
        apply_setting_value(&mut settings, "external_script_path", json!(null))
            .expect("null applies to an Option field");

        assert!(settings.push_to_talk);
        assert_eq!(settings.paste_delay_ms, 250);
        assert_eq!(settings.selected_language, "de");
        assert_eq!(settings.custom_words, vec!["AudioBud", "Tauri"]);
        assert_eq!(settings.paste_method, PasteMethod::ShiftInsert);
        assert_eq!(settings.external_script_path, None);
    }

    #[test]
    fn generic_mutator_leaves_every_other_field_untouched() {
        let mut settings = get_default_settings();
        let before = serde_json::to_value(&settings).unwrap();

        apply_setting_value(&mut settings, "audio_feedback_volume", json!(0.25))
            .expect("volume applies");

        let after = serde_json::to_value(&settings).unwrap();
        let (before, after) = (
            before.as_object().unwrap().clone(),
            after.as_object().unwrap().clone(),
        );
        assert_eq!(before.len(), after.len());
        for (key, value) in &before {
            if key == "audio_feedback_volume" {
                continue;
            }
            assert_eq!(Some(value), after.get(key), "field '{key}' changed");
        }
    }

    #[test]
    fn generic_mutator_rejects_unknown_keys_and_wrong_types() {
        let mut settings = get_default_settings();

        let error = apply_setting_value(&mut settings, "not_a_setting", json!(true))
            .expect_err("unknown key is rejected");
        assert!(error.contains("not_a_setting"), "{error}");

        let error = apply_setting_value(&mut settings, "push_to_talk", json!("yes"))
            .expect_err("wrong type is rejected");
        assert!(error.contains("push_to_talk"), "{error}");
        assert!(
            !settings.push_to_talk,
            "a rejected patch must not partially apply"
        );
    }

    #[test]
    fn changing_the_overlay_position_keeps_the_derived_fields_in_sync() {
        let mut settings = get_default_settings();
        settings.overlay_position = OverlayPosition::Top;
        settings.overlay_custom_position = Some(OverlayCustomPosition {
            anchor: OverlayAnchor::TopLeft,
            dx: 4.0,
            dy: 8.0,
        });

        // Hiding remembers the outgoing placement for the tray show/hide toggle
        // and drops the fine grid placement.
        apply_setting_value(&mut settings, "overlay_position", json!("none")).expect("applies");
        assert_eq!(settings.overlay_position, OverlayPosition::None);
        assert_eq!(
            settings.overlay_restore_position,
            Some(OverlayPosition::Top)
        );
        assert!(settings.overlay_custom_position.is_none());

        // Choosing a visible placement records it as the restore point.
        apply_setting_value(&mut settings, "overlay_position", json!("bottom")).expect("applies");
        assert_eq!(
            settings.overlay_restore_position,
            Some(OverlayPosition::Bottom)
        );
    }

    #[test]
    fn cache_loads_once_and_serves_later_reads() {
        let cache = SettingsCache::new();
        let mut loads = 0;

        let first = cache.get_or_load(|| {
            loads += 1;
            get_default_settings()
        });
        let second = cache.get_or_load(|| {
            loads += 1;
            get_default_settings()
        });

        assert_eq!(loads, 1, "a warm cache must not re-read the store");
        assert_eq!(first.push_to_talk, second.push_to_talk);
    }

    #[test]
    fn writing_settings_refreshes_the_cache() {
        let cache = SettingsCache::new();
        cache.get_or_load(get_default_settings);

        let mut updated = get_default_settings();
        apply_setting_value(&mut updated, "push_to_talk", json!(true)).expect("applies");
        cache.store(&updated);

        let observed = cache.get_or_load(|| panic!("cache must serve the value just written"));
        assert!(
            observed.push_to_talk,
            "a reader must observe the value just written"
        );
    }

    #[test]
    fn write_through_persists_before_publishing_to_the_cache() {
        let cache = SettingsCache::new();
        let mut updated = get_default_settings();
        apply_setting_value(&mut updated, "push_to_talk", json!(true)).expect("applies");

        let mut persisted = false;
        cache.write_through(
            || {
                persisted = true;
            },
            &updated,
        );

        assert!(persisted, "write_through must run the persist step");
        let cached = cache.peek().expect("write_through publishes to the cache");
        assert!(cached.push_to_talk);
    }

    #[test]
    fn write_through_serializes_concurrent_publishers() {
        // Simulates two overlapping `write_settings` calls: interleaving the
        // store write and the cache publish across threads would let the
        // slower writer's cache update win even though the faster writer's
        // store write landed last. Holding one lock across both steps for
        // each writer rules that out — whichever writer's `write_through`
        // call finishes last is authoritative for both the store and the
        // cache, together.
        let cache = Arc::new(SettingsCache::new());
        let mut a = get_default_settings();
        apply_setting_value(&mut a, "push_to_talk", json!(true)).expect("applies");
        let mut b = get_default_settings();
        apply_setting_value(&mut b, "push_to_talk", json!(false)).expect("applies");

        let order = Arc::new(std::sync::Mutex::new(Vec::new()));

        let handles: Vec<_> = [('A', a), ('B', b)]
            .into_iter()
            .map(|(label, settings)| {
                let cache = Arc::clone(&cache);
                let order = Arc::clone(&order);
                std::thread::spawn(move || {
                    cache.write_through(|| order.lock().unwrap().push(label), &settings);
                })
            })
            .collect();
        for handle in handles {
            handle.join().unwrap();
        }

        // Whichever writer persisted last must also be the one the cache
        // reflects; write_through never lets the two disagree.
        let last = *order.lock().unwrap().last().unwrap();
        let cached = cache
            .peek()
            .expect("a write_through call populated the cache");
        assert_eq!(
            cached.push_to_talk,
            last == 'A',
            "the cache must match whichever writer persisted last"
        );
    }

    #[test]
    fn fill_if_empty_never_overwrites_an_existing_value() {
        let cache = SettingsCache::new();
        let mut written = get_default_settings();
        apply_setting_value(&mut written, "push_to_talk", json!(true)).expect("applies");
        cache.write_through(|| {}, &written);

        // A loader that read the store before the write above landed must not
        // clobber it just because its own read finishes later.
        let mut stale_read = get_default_settings();
        apply_setting_value(&mut stale_read, "push_to_talk", json!(false)).expect("applies");
        cache.fill_if_empty(&stale_read);

        let cached = cache.peek().expect("write_through populated the cache");
        assert!(
            cached.push_to_talk,
            "fill_if_empty must not overwrite a value already published by write_through"
        );
    }

    #[test]
    fn a_racing_loader_never_beats_a_concurrent_write() {
        // Models the loader-vs-writer race from the settings refactor
        // (issue #166): `load_or_create_app_settings`/`get_settings` read the
        // store without holding the cache lock, so a concurrent
        // `write_settings` can persist and publish a newer value while the
        // loader is still reading. The loader must never be able to publish
        // its now-stale snapshot over that newer value.
        let cache = Arc::new(SettingsCache::new());

        let mut written = get_default_settings();
        apply_setting_value(&mut written, "push_to_talk", json!(true)).expect("applies");
        let mut stale_read = get_default_settings();
        apply_setting_value(&mut stale_read, "push_to_talk", json!(false)).expect("applies");

        let writer = {
            let cache = Arc::clone(&cache);
            let written = written.clone();
            std::thread::spawn(move || cache.write_through(|| {}, &written))
        };
        writer.join().unwrap();

        // The loader's "read" is modeled as already stale by the time it
        // reaches the cache, which is exactly the case `fill_if_empty` must
        // guard: the slot is no longer empty, so the loader's publish is a
        // no-op instead of overwriting the write that already landed.
        let loader = {
            let cache = Arc::clone(&cache);
            std::thread::spawn(move || cache.fill_if_empty(&stale_read))
        };
        loader.join().unwrap();

        let cached = cache.peek().expect("a write populated the cache");
        assert!(
            cached.push_to_talk,
            "a loader's stale read must never overwrite a write that already published"
        );
    }

    #[test]
    fn invalidating_the_cache_forces_a_reload() {
        let cache = SettingsCache::new();
        cache.get_or_load(get_default_settings);
        cache.invalidate();
        assert!(cache.peek().is_none());

        let mut reloaded = 0;
        cache.get_or_load(|| {
            reloaded += 1;
            get_default_settings()
        });
        assert_eq!(reloaded, 1, "an invalidated cache re-reads the store");
    }

    #[test]
    fn default_settings_disable_auto_submit() {
        let settings = get_default_settings();
        assert!(!settings.auto_submit);
        assert_eq!(settings.auto_submit_key, AutoSubmitKey::Enter);
    }

    #[test]
    fn default_settings_use_toggle_recording() {
        let settings = get_default_settings();
        assert!(!settings.push_to_talk);
    }

    #[test]
    fn debug_output_redacts_api_keys() {
        let mut settings = get_default_settings();
        settings
            .post_process_api_keys
            .insert("openai".to_string(), "sk-proj-secret-key-12345".to_string());
        settings.post_process_api_keys.insert(
            "anthropic".to_string(),
            "sk-ant-secret-key-67890".to_string(),
        );
        settings
            .post_process_api_keys
            .insert("empty_provider".to_string(), "".to_string());

        let debug_output = format!("{:?}", settings);

        assert!(!debug_output.contains("sk-proj-secret-key-12345"));
        assert!(!debug_output.contains("sk-ant-secret-key-67890"));
        assert!(debug_output.contains("[REDACTED]"));
    }

    #[test]
    fn secret_map_debug_redacts_values() {
        let map = SecretMap(HashMap::from([("key".into(), "secret".into())]));
        let out = format!("{:?}", map);
        assert!(!out.contains("secret"));
        assert!(out.contains("[REDACTED]"));
    }

    #[test]
    fn paste_methods_with_no_window_do_not_require_focus() {
        // None is a no-op and ExternalScript hands off to a program: neither
        // touches a window, so target-lock has nothing to do for them (#162).
        assert!(!PasteMethod::None.requires_focus());
        assert!(!PasteMethod::ExternalScript.requires_focus());
    }

    #[test]
    fn paste_methods_that_inject_into_a_window_require_focus() {
        // Direct and the three clipboard-paste key combos all resolve to
        // injection against the foreground window's input queue.
        assert!(PasteMethod::Direct.requires_focus());
        assert!(PasteMethod::CtrlV.requires_focus());
        assert!(PasteMethod::CtrlShiftV.requires_focus());
        assert!(PasteMethod::ShiftInsert.requires_focus());
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_transcribe_default_is_ctrl_alt_space() {
        let settings = get_default_settings();
        let binding = settings
            .bindings
            .get("transcribe")
            .expect("transcribe binding should exist");
        assert_eq!(binding.default_binding, "ctrl+alt+space");
        assert_eq!(binding.current_binding, "ctrl+alt+space");
    }

    // Default engine chosen from the milestone-A benchmark (see bench/RESULTS.md):
    // Parakeet V3 is the smallest model that transcribes reliably on the DirectML path.
    #[cfg(target_os = "windows")]
    #[test]
    fn windows_default_model_is_parakeet_v3() {
        let settings = get_default_settings();
        assert_eq!(settings.selected_model, "parakeet-tdt-0.6b-v3");
    }

    #[test]
    fn default_settings_wait_for_installed_package_detection() {
        let settings = get_default_settings();
        assert!(!settings.update_checks_enabled);
    }

    #[test]
    fn v0_4_2_migration_enables_the_first_signed_feed_on_windows() {
        let mut settings = get_default_settings();
        settings.update_checks_enabled = false;

        assert_eq!(
            migrate_update_checks_v0_4_2(&mut settings, false, true),
            Some(true)
        );
        assert!(settings.update_checks_enabled);
    }

    #[test]
    fn v0_4_2_migration_preserves_a_later_user_opt_out() {
        let mut settings = get_default_settings();
        settings.update_checks_enabled = false;

        assert_eq!(
            migrate_update_checks_v0_4_2(&mut settings, true, true),
            None
        );
        assert!(!settings.update_checks_enabled);
    }

    #[test]
    fn v0_4_2_migration_waits_for_an_nsis_channel() {
        let mut settings = get_default_settings();
        settings.update_checks_enabled = false;

        assert_eq!(
            migrate_update_checks_v0_4_2(&mut settings, false, false),
            None
        );
        assert!(!settings.update_checks_enabled);

        assert_eq!(
            migrate_update_checks_v0_4_2(&mut settings, false, true),
            Some(true)
        );
        assert!(settings.update_checks_enabled);
    }
}
