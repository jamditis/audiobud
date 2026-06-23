import { chromium } from "playwright";
import { spawn } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(__dirname, "..");
const baseUrl = "http://127.0.0.1:1420";
const screenshotDir = resolve(repoRoot, "screenshots");
const docsAssetDir = resolve(repoRoot, "docs/assets");
const viewport = { width: 680, height: 570 };

const modelLanguages = {
  whisper: [
    "en",
    "zh",
    "de",
    "es",
    "ru",
    "ko",
    "fr",
    "ja",
    "pt",
    "tr",
    "pl",
    "ca",
    "nl",
    "ar",
    "sv",
    "it",
    "id",
    "hi",
    "fi",
    "vi",
    "he",
    "uk",
    "el",
    "ms",
    "cs",
    "ro",
    "da",
    "hu",
    "ta",
    "no",
    "th",
    "ur",
    "hr",
    "bg",
    "lt",
    "la",
    "mi",
    "ml",
    "cy",
    "sk",
    "te",
    "fa",
    "lv",
    "bn",
    "sr",
    "az",
    "sl",
    "kn",
    "et",
    "mk",
    "br",
    "eu",
    "is",
    "hy",
    "ne",
    "mn",
    "bs",
    "kk",
    "sq",
    "sw",
    "gl",
    "mr",
    "pa",
    "si",
    "km",
    "sn",
    "yo",
    "so",
    "af",
    "oc",
    "ka",
    "be",
    "tg",
    "sd",
    "gu",
    "am",
    "yi",
    "lo",
    "uz",
    "fo",
    "ht",
    "ps",
    "tk",
    "nn",
    "mt",
    "sa",
    "lb",
    "my",
    "bo",
    "tl",
    "mg",
    "as",
    "tt",
    "haw",
    "ln",
    "ha",
    "ba",
    "jw",
    "su",
  ],
  parakeetV3: [
    "bg",
    "hr",
    "cs",
    "da",
    "nl",
    "en",
    "et",
    "fi",
    "fr",
    "de",
    "el",
    "hu",
    "it",
    "lv",
    "lt",
    "mt",
    "pl",
    "pt",
    "ro",
    "sk",
    "sl",
    "es",
    "sv",
    "ru",
    "uk",
  ],
};

const models = [
  model({
    id: "parakeet-tdt-0.6b-v3",
    name: "Parakeet V3",
    description: "Fast and accurate. Supports 25 European languages.",
    filename: "parakeet-tdt-0.6b-v3-int8",
    size_mb: 456,
    engine_type: "Parakeet",
    accuracy_score: 0.8,
    speed_score: 0.85,
    is_downloaded: true,
    is_recommended: true,
    is_directory: true,
    supported_languages: modelLanguages.parakeetV3,
  }),
  model({
    id: "canary-1b-v2",
    name: "Canary 1B v2",
    description:
      "Accurate multilingual. 25 European languages. Supports translation.",
    filename: "canary-1b-v2",
    size_mb: 691,
    engine_type: "Canary",
    accuracy_score: 0.85,
    speed_score: 0.7,
    supports_translation: true,
    supports_language_selection: true,
    supported_languages: modelLanguages.parakeetV3,
  }),
  model({
    id: "cohere-int8",
    name: "Cohere",
    description: "A large, slower, but very accurate multilingual model.",
    filename: "cohere-int8",
    size_mb: 1708,
    engine_type: "Cohere",
    accuracy_score: 0.9,
    speed_score: 0.6,
    supports_language_selection: true,
    supported_languages: [
      "en",
      "fr",
      "de",
      "it",
      "es",
      "pt",
      "el",
      "nl",
      "pl",
      "zh",
      "zh-Hans",
      "zh-Hant",
      "ja",
      "ko",
      "vi",
      "ar",
    ],
  }),
  model({
    id: "medium",
    name: "Whisper Medium",
    description: "Good accuracy, medium speed",
    filename: "whisper-medium-q4_1.bin",
    size_mb: 469,
    engine_type: "Whisper",
    accuracy_score: 0.75,
    speed_score: 0.6,
    supports_translation: true,
    supports_language_selection: true,
    supported_languages: modelLanguages.whisper,
  }),
  model({
    id: "sense-voice-int8",
    name: "SenseVoice",
    description: "Very fast. Chinese, English, Japanese, Korean, Cantonese.",
    filename: "sense-voice-int8",
    size_mb: 152,
    engine_type: "SenseVoice",
    accuracy_score: 0.65,
    speed_score: 0.95,
    supports_language_selection: true,
    supported_languages: ["zh", "zh-Hans", "zh-Hant", "en", "yue", "ja", "ko"],
  }),
  model({
    id: "moonshine-small-streaming-en",
    name: "Moonshine V2 Small",
    description: "Fast, English only. Good balance of speed and accuracy.",
    filename: "moonshine-small-streaming-en",
    size_mb: 99,
    engine_type: "MoonshineStreaming",
    accuracy_score: 0.65,
    speed_score: 0.9,
    supported_languages: ["en"],
  }),
];

const settings = {
  bindings: {
    transcribe: shortcut(
      "transcribe",
      "Transcribe",
      "Converts your speech into text.",
      "ctrl+alt+space",
    ),
    transcribe_with_post_process: shortcut(
      "transcribe_with_post_process",
      "Transcribe with Post-Processing",
      "Converts your speech into text and applies AI post-processing.",
      "ctrl+shift+space",
    ),
    transcribe_raw: shortcut(
      "transcribe_raw",
      "Transcribe (raw)",
      "Converts your speech into raw, lowercased, unpunctuated text.",
      "ctrl+alt+r",
    ),
    cancel: shortcut(
      "cancel",
      "Cancel",
      "Cancels the current recording.",
      "escape",
    ),
  },
  push_to_talk: true,
  audio_feedback: true,
  audio_feedback_volume: 0.82,
  sound_theme: "marimba",
  start_hidden: false,
  autostart_enabled: false,
  update_checks_enabled: false,
  selected_model: "parakeet-tdt-0.6b-v3",
  always_on_microphone: false,
  selected_microphone: "Default",
  clamshell_microphone: "Default",
  selected_output_device: "Default",
  translate_to_english: false,
  selected_language: "auto",
  overlay_position: "bottom",
  debug_mode: false,
  log_level: "debug",
  custom_words: ["AudioBud", "Amditis", "Parakeet"],
  word_replacements: [],
  model_unload_timeout: "min_10",
  word_correction_threshold: 0.18,
  history_limit: 5,
  recording_retention_period: "preserve_limit",
  paste_method: "ctrl_v",
  clipboard_handling: "dont_modify",
  auto_submit: false,
  auto_submit_key: "enter",
  post_process_enabled: true,
  post_process_provider_id: "openai",
  post_process_providers: [
    provider("openai", "OpenAI", "https://api.openai.com/v1", true),
    provider("anthropic", "Anthropic", "https://api.anthropic.com/v1", false),
    provider("openrouter", "OpenRouter", "https://openrouter.ai/api/v1", true),
    provider("custom", "Custom", "http://localhost:11434/v1", false, true),
  ],
  post_process_api_keys: {
    openai: "",
    anthropic: "",
    openrouter: "",
    custom: "",
  },
  post_process_models: {
    openai: "gpt-4.1-mini",
    anthropic: "",
    openrouter: "",
    custom: "",
  },
  post_process_prompts: [
    {
      id: "default_improve_transcriptions",
      name: "Improve transcriptions",
      prompt:
        "Clean this transcript, fixing spelling, capitalization, punctuation, number formatting, and filler words. Return only the cleaned transcript.\n\nTranscript:\n${output}",
    },
  ],
  post_process_selected_prompt_id: "default_improve_transcriptions",
  mute_while_recording: false,
  append_trailing_space: true,
  raw_output: false,
  app_language: "en",
  experimental_enabled: false,
  lazy_stream_close: false,
  keyboard_implementation: "tauri",
  show_tray_icon: true,
  paste_delay_ms: 60,
  typing_tool: "auto",
  external_script_path: null,
  custom_filler_words: null,
  whisper_accelerator: "auto",
  ort_accelerator: "directml",
  whisper_gpu_device: -1,
  extra_recording_buffer_ms: 0,
};

function model(overrides) {
  return {
    id: "",
    name: "",
    description: "",
    filename: "",
    url: "https://blob.handy.computer/model.tar.gz",
    sha256: "0".repeat(64),
    size_mb: 0,
    is_downloaded: false,
    is_downloading: false,
    partial_size: 0,
    is_directory: false,
    engine_type: "Whisper",
    accuracy_score: 0,
    speed_score: 0,
    supports_translation: false,
    is_recommended: false,
    supported_languages: [],
    supports_language_selection: false,
    is_custom: false,
    ...overrides,
  };
}

function shortcut(id, name, description, binding) {
  return {
    id,
    name,
    description,
    default_binding: binding,
    current_binding: binding,
  };
}

function provider(
  id,
  label,
  base_url,
  supports_structured_output,
  allow_base_url_edit = false,
) {
  return {
    id,
    label,
    base_url,
    allow_base_url_edit,
    models_endpoint: "/models",
    supports_structured_output,
  };
}

function installTauriMocks(settings, models) {
  window.__TAURI_OS_PLUGIN_INTERNALS__ = {
    platform: "windows",
    os_type: "windows",
    family: "windows",
    arch: "x86_64",
    version: "10.0.22631",
    eol: "\r\n",
    exe_extension: "exe",
  };

  const callbacks = new Map();
  const listeners = new Map();
  let nextCallbackId = 1;
  let nextListenerId = 1;
  let micTimer;

  window.__TAURI_EVENT_PLUGIN_INTERNALS__ = {
    unregisterListener(_event, eventId) {
      listeners.delete(eventId);
    },
  };

  window.__TAURI_INTERNALS__ = {
    transformCallback(callback, once = false) {
      const id = nextCallbackId++;
      callbacks.set(id, { callback, once });
      return id;
    },
    unregisterCallback(id) {
      callbacks.delete(id);
    },
    convertFileSrc(filePath) {
      return filePath;
    },
    async invoke(command, args = {}) {
      switch (command) {
        case "plugin:event|listen": {
          const eventId = nextListenerId++;
          listeners.set(eventId, { event: args.event, handler: args.handler });
          if (args.event === "mic-level") {
            window.setTimeout(
              () => emitEvent("mic-level", sampleMicLevels()),
              80,
            );
            micTimer = window.setInterval(
              () => emitEvent("mic-level", sampleMicLevels()),
              280,
            );
          }
          return eventId;
        }
        case "plugin:event|unlisten":
          listeners.delete(args.eventId);
          if (
            ![...listeners.values()].some(
              (listener) => listener.event === "mic-level",
            )
          ) {
            window.clearInterval(micTimer);
          }
          return null;
        case "plugin:event|emit":
        case "plugin:event|emit_to":
          return null;
        case "plugin:os|locale":
          return "en-US";
        case "plugin:app|version":
          return "0.1.0";
        case "plugin:app|name":
          return "AudioBud";
        case "plugin:app|identifier":
          return "tech.amditis.audiobud";
        case "plugin:updater|check":
          return null;
        case "plugin:dialog|ask":
        case "plugin:dialog|confirm":
          return false;
        case "plugin:dialog|open":
        case "plugin:dialog|save":
          return null;
        case "plugin:dialog|message":
        case "plugin:process|restart":
        case "plugin:opener|open_url":
          return null;
        case "get_app_settings":
        case "get_default_settings":
          return structuredClone(settings);
        case "check_custom_sounds":
          return { start: false, stop: false };
        case "has_any_models_available":
        case "has_any_models_or_downloads":
          return true;
        case "get_available_models":
          return structuredClone(models);
        case "get_current_model":
        case "get_transcription_model_status":
          return settings.selected_model;
        case "get_model_info":
          return structuredClone(
            models.find((item) => item.id === args.modelId) ?? null,
          );
        case "get_available_microphones":
          return [
            { index: "default", name: "Default", is_default: true },
            { index: "1", name: "USB microphone", is_default: false },
          ];
        case "get_available_output_devices":
          return [
            { index: "default", name: "Default", is_default: true },
            { index: "1", name: "Speakers", is_default: false },
          ];
        case "get_windows_microphone_permission_status":
          return {
            supported: true,
            overall_access: "allowed",
            device_access: "allowed",
            app_access: "allowed",
            desktop_app_access: "allowed",
          };
        case "get_app_dir_path":
          return "C:\\Users\\jamditis\\AppData\\Roaming\\tech.amditis.audiobud";
        case "get_log_dir_path":
          return "C:\\Users\\jamditis\\AppData\\Roaming\\tech.amditis.audiobud\\logs";
        case "get_model_load_status":
          return { is_loaded: true, current_model: settings.selected_model };
        case "is_portable":
        case "is_recording":
        case "check_apple_intelligence_available":
        case "is_laptop":
          return false;
        case "get_available_typing_tools":
          return ["auto"];
        default:
          if (
            command.startsWith("change_") ||
            command.startsWith("set_") ||
            command.startsWith("update_") ||
            command.startsWith("delete_") ||
            command.startsWith("open_") ||
            command.startsWith("initialize_") ||
            command.startsWith("reset_") ||
            command === "download_model" ||
            command === "cancel_download" ||
            command === "cancel_operation" ||
            command === "play_test_sound" ||
            command === "unload_model_manually"
          ) {
            return null;
          }
          console.warn(`Unhandled mocked Tauri command: ${command}`, args);
          return null;
      }
    },
  };

  function emitEvent(event, payload) {
    for (const [eventId, listener] of listeners) {
      if (listener.event !== event) continue;
      const entry = callbacks.get(listener.handler);
      if (!entry) continue;
      entry.callback({ event, id: eventId, payload });
      if (entry.once) callbacks.delete(listener.handler);
    }
  }

  function sampleMicLevels() {
    return [
      0.08, 0.14, 0.22, 0.38, 0.48, 0.62, 0.74, 0.83, 0.78, 0.68, 0.54, 0.4,
      0.3, 0.22, 0.16, 0.1,
    ];
  }
}

async function isServerReady() {
  try {
    const response = await fetch(baseUrl);
    return response.ok;
  } catch {
    return false;
  }
}

async function startVite() {
  if (await isServerReady()) return null;

  const viteBin = resolve(repoRoot, "node_modules/vite/bin/vite.js");
  const child = spawn(
    process.execPath,
    [viteBin, "dev", "--host", "127.0.0.1"],
    {
      cwd: repoRoot,
      stdio: ["ignore", "pipe", "pipe"],
    },
  );

  child.stdout.on("data", (chunk) => process.stdout.write(chunk));
  child.stderr.on("data", (chunk) => process.stderr.write(chunk));

  const started = Date.now();
  while (Date.now() - started < 30000) {
    if (await isServerReady()) return child;
    await new Promise((resolveWait) => setTimeout(resolveWait, 250));
  }

  child.kill("SIGTERM");
  throw new Error("Vite dev server did not become ready within 30 seconds.");
}

async function capture() {
  mkdirSync(screenshotDir, { recursive: true });
  mkdirSync(docsAssetDir, { recursive: true });
  const server = await startVite();
  const executablePath =
    process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE ||
    (existsSync("/usr/bin/chromium") ? "/usr/bin/chromium" : undefined);

  const browser = await chromium.launch({
    executablePath,
    headless: true,
  });

  try {
    const context = await browser.newContext({
      colorScheme: "dark",
      deviceScaleFactor: 1,
      viewport,
    });
    await context.addInitScript({
      content: `(${installTauriMocks.toString()})(${JSON.stringify(settings)}, ${JSON.stringify(models)});`,
    });
    const page = await context.newPage();

    page.on("console", (message) => {
      if (message.type() === "error") {
        console.error(`browser console: ${message.text()}`);
      }
    });

    await page.goto(baseUrl, { waitUntil: "networkidle" });
    await page.waitForSelector("nav button", { timeout: 10000 });
    await page.waitForTimeout(500);
    await page.screenshot({
      path: resolve(screenshotDir, "app-general.png"),
    });

    await page.getByRole("button", { name: /models/i }).click();
    await page.waitForSelector("text=Transcription models", { timeout: 10000 });
    await page.waitForTimeout(500);
    await page.screenshot({
      path: resolve(screenshotDir, "models.png"),
    });

    const ogPage = await context.newPage();
    await ogPage.setViewportSize({ width: 1200, height: 630 });
    await ogPage.setContent(ogImageMarkup(), { waitUntil: "load" });
    await ogPage.screenshot({
      path: resolve(docsAssetDir, "og-image.png"),
    });

    copyFileSync(
      resolve(screenshotDir, "app-general.png"),
      resolve(docsAssetDir, "app-general.png"),
    );
    copyFileSync(
      resolve(screenshotDir, "models.png"),
      resolve(docsAssetDir, "models.png"),
    );
    copyFileSync(
      resolve(docsAssetDir, "og-image.png"),
      resolve(repoRoot, "og-image.png"),
    );
  } finally {
    await browser.close();
    if (server) {
      server.kill("SIGTERM");
    }
  }
}

function ogImageMarkup() {
  const screenshotData = readFileSync(
    resolve(screenshotDir, "app-general.png"),
  ).toString("base64");
  return `<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <style>
      @font-face {
        font-family: system-ui;
        src: local("Arial");
      }
      * {
        box-sizing: border-box;
      }
      body {
        margin: 0;
        width: 1200px;
        height: 630px;
        overflow: hidden;
        background:
          linear-gradient(125deg, rgba(16, 27, 19, 0.92), rgba(11, 18, 14, 0.98)),
          radial-gradient(circle at 78% 68%, rgba(132, 209, 80, 0.26), transparent 34%),
          radial-gradient(circle at 22% 18%, rgba(255, 178, 62, 0.2), transparent 32%);
        color: #f3f7ee;
        font-family:
          Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
      }
      .wrap {
        display: grid;
        grid-template-columns: 470px 1fr;
        gap: 56px;
        align-items: center;
        width: 100%;
        height: 100%;
        padding: 58px 78px;
      }
      .brand {
        color: #84d150;
        font-size: 34px;
        font-weight: 900;
        letter-spacing: 0;
        margin-bottom: 26px;
      }
      h1 {
        font-size: 66px;
        line-height: 0.98;
        letter-spacing: 0;
        margin: 0 0 26px;
      }
      p {
        color: rgba(243, 247, 238, 0.78);
        font-size: 27px;
        line-height: 1.32;
        margin: 0;
      }
      .tags {
        display: flex;
        gap: 14px;
        margin-top: 24px;
      }
      .tag {
        border: 1px solid rgba(255, 178, 62, 0.42);
        border-radius: 8px;
        color: #ffcf7a;
        font-size: 18px;
        font-weight: 700;
        padding: 9px 14px;
      }
      .shot {
        border: 1px solid rgba(132, 209, 80, 0.35);
        border-radius: 8px;
        box-shadow: 0 28px 80px rgba(0, 0, 0, 0.48);
        overflow: hidden;
        transform: rotate(-1deg);
      }
      img {
        display: block;
        width: 100%;
        height: auto;
      }
    </style>
  </head>
  <body>
    <main class="wrap">
      <section>
        <div class="brand">AudioBud</div>
        <h1>Local dictation for Windows</h1>
        <p>Press a hotkey, speak, and paste private speech-to-text into the app you already use.</p>
        <div class="tags">
          <span class="tag">offline audio</span>
          <span class="tag">Parakeet V3</span>
          <span class="tag">dark mode</span>
        </div>
      </section>
      <section class="shot">
        <img src="data:image/png;base64,${screenshotData}" alt="" />
      </section>
    </main>
  </body>
</html>`;
}

capture().catch((error) => {
  console.error(error);
  process.exit(1);
});
