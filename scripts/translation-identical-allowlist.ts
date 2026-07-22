export interface IdenticalValueException {
  source: string;
  locales: "*" | readonly string[];
}

export type IdenticalValueAllowlist = Readonly<
  Record<string, IdenticalValueException>
>;

// Paths use dots between segments and escape literal dots or backslashes in a
// segment with a backslash. For example, the key `model.0` becomes `model\.0`.
// These values are identifiers, shortcuts, format tokens, or technical
// placeholders that remain unchanged regardless of the interface language.
// Binding each path to its current English source makes an English rewrite
// fail closed until the exception is reviewed again.
export const IDENTICAL_VALUE_ALLOWLIST = {
  "onboarding.models.breeze-asr.name": {
    source: "Breeze ASR",
    locales: "*",
  },
  "onboarding.models.canary-180m-flash.name": {
    source: "Canary 180M Flash",
    locales: "*",
  },
  "onboarding.models.canary-1b-v2.name": {
    source: "Canary 1B v2",
    locales: "*",
  },
  "onboarding.models.cohere-int8.name": {
    source: "Cohere",
    locales: "*",
  },
  "onboarding.models.gigaam-v3-e2e-ctc.name": {
    source: "GigaAM v3",
    locales: "*",
  },
  "onboarding.models.large.name": {
    source: "Whisper Large",
    locales: "*",
  },
  "onboarding.models.medium.name": {
    source: "Whisper Medium",
    locales: "*",
  },
  "onboarding.models.moonshine-base.name": {
    source: "Moonshine Base",
    locales: "*",
  },
  "onboarding.models.moonshine-medium-streaming-en.name": {
    source: "Moonshine V2 Medium",
    locales: "*",
  },
  "onboarding.models.moonshine-small-streaming-en.name": {
    source: "Moonshine V2 Small",
    locales: "*",
  },
  "onboarding.models.moonshine-tiny-streaming-en.name": {
    source: "Moonshine V2 Tiny",
    locales: "*",
  },
  "onboarding.models.parakeet-tdt-0\\.6b-v2.name": {
    source: "Parakeet V2",
    locales: "*",
  },
  "onboarding.models.parakeet-tdt-0\\.6b-v3.name": {
    source: "Parakeet V3",
    locales: "*",
  },
  "onboarding.models.sense-voice-int8.name": {
    source: "SenseVoice",
    locales: "*",
  },
  "onboarding.models.small.name": {
    source: "Whisper Small",
    locales: "*",
  },
  "onboarding.models.turbo.name": {
    source: "Whisper Turbo",
    locales: "*",
  },
  "settings.about.acknowledgments.handy.title": {
    source: "Handy",
    locales: "*",
  },
  "settings.about.acknowledgments.parakeet.title": {
    source: "Parakeet",
    locales: "*",
  },
  "settings.about.acknowledgments.whisper.title": {
    source: "Whisper.cpp",
    locales: "*",
  },
  "settings.advanced.autoSubmit.options.cmdEnter": {
    source: "Cmd+Enter",
    locales: "*",
  },
  "settings.advanced.autoSubmit.options.ctrlEnter": {
    source: "Ctrl+Enter",
    locales: "*",
  },
  "settings.advanced.autoSubmit.options.enter": {
    source: "Enter",
    locales: "*",
  },
  "settings.advanced.autoSubmit.options.superEnter": {
    source: "Super+Enter",
    locales: "*",
  },
  "settings.advanced.pasteMethod.externalScriptPlaceholder": {
    source: "/path/to/your/script.sh",
    locales: "*",
  },
  "settings.advanced.wordReplacements.caseBadge": {
    source: "Aa",
    locales: "*",
  },
  "settings.postProcessing.api.apiKey.placeholder": {
    source: "sk-...",
    locales: "*",
  },
  "settings.postProcessing.api.appleIntelligence.title": {
    source: "Apple Intelligence",
    locales: "*",
  },
  "settings.postProcessing.api.baseUrl.placeholder": {
    source: "https://api.openai.com/v1",
    locales: "*",
  },
  "settings.postProcessing.api.model.placeholderApple": {
    source: "Apple Intelligence",
    locales: "*",
  },
  "modelSelector.downloadSpeed": {
    source: "{{speed}} MB/s",
    locales: "*",
  },
  // These words are spelled the same only in the listed languages. Keeping
  // the locale list explicit prevents the same English value from passing in
  // another language without review.
  "overlay.raw": {
    source: "RAW",
    locales: [
      "bg",
      "cs",
      "de",
      "es",
      "it",
      "ja",
      "ko",
      "pl",
      "ru",
      "uk",
      "zh",
      "zh-TW",
    ],
  },
  "common.no": { source: "No", locales: ["es", "it"] },
  "settings.about.version.title": {
    source: "Version",
    locales: ["de", "fr", "sv"],
  },
  "settings.advanced.groups.app": {
    source: "App",
    locales: ["de", "it", "sv"],
  },
  "settings.advanced.groups.experimental": {
    source: "Experimental",
    locales: ["es", "pt"],
  },
  "settings.advanced.groups.transcription": {
    source: "Transcription",
    locales: ["fr"],
  },
  "settings.advanced.pasteMethod.options.direct": {
    source: "Direct",
    locales: ["fr"],
  },
  "settings.debug.title": {
    source: "Debug",
    locales: ["de", "it"],
  },
  "sidebar.debug": {
    source: "Debug",
    locales: ["de", "it"],
  },
  "settings.general.title": {
    source: "General",
    locales: ["es"],
  },
  "sidebar.general": {
    source: "General",
    locales: ["es"],
  },
  "settings.postProcessing.api.model.title": {
    source: "Model",
    locales: ["cs", "pl", "tr"],
  },
  "tray.model": {
    source: "Model",
    locales: ["cs", "pl", "tr"],
  },
  "settings.postProcessing.prompts.title": {
    source: "Prompt",
    locales: ["cs", "de", "es", "fr", "it", "pl", "pt", "sv", "tr", "vi"],
  },
  "settings.sound.microphone.title": {
    source: "Microphone",
    locales: ["fr"],
  },
  "settings.sound.volume.title": {
    source: "Volume",
    locales: ["fr", "it", "pt"],
  },
} satisfies IdenticalValueAllowlist;
