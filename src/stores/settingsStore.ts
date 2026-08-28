import { create } from "zustand";
import { subscribeWithSelector } from "zustand/middleware";
import { listen } from "@tauri-apps/api/event";
import type {
  AppSettings as Settings,
  AudioDevice,
  ModelUnloadTimeout,
  OverlayAnchor,
} from "@/bindings";
import { commands } from "@/bindings";
import { toast } from "sonner";
import i18n from "@/i18n";
import { settingUpdateError } from "./settingUpdateResult";
import {
  createKeyedSerialQueue,
  createOptimisticWriteCoordinator,
  createSettingsLifecycle,
  mergePendingValues,
} from "./settingsCoordination";

// Auto-save acknowledgment. Settings persist immediately (there is no Save
// button), so a brief toast confirms each write. A shared toast id plus a short
// debounce keeps consecutive changes from stacking toasts.
let savedToastTimer: ReturnType<typeof setTimeout> | null = null;
function notifySaved() {
  if (savedToastTimer) clearTimeout(savedToastTimer);
  savedToastTimer = setTimeout(() => {
    toast.success(i18n.t("settings.autosave.saved"), {
      id: "settings-saved",
      duration: 1500,
    });
  }, 500);
}
function notifySaveError() {
  if (savedToastTimer) clearTimeout(savedToastTimer);
  toast.error(i18n.t("settings.autosave.failed"), { id: "settings-saved" });
}

interface SettingsStore {
  settings: Settings | null;
  defaultSettings: Settings | null;
  isLoading: boolean;
  isUpdating: Record<string, boolean>;
  audioDevices: AudioDevice[];
  outputDevices: AudioDevice[];
  customSounds: { start: boolean; stop: boolean };
  postProcessModelOptions: Record<string, string[]>;

  // Actions
  initialize: () => Promise<void>;
  loadDefaultSettings: () => Promise<void>;
  updateSetting: <K extends keyof Settings>(
    key: K,
    value: Settings[K],
  ) => Promise<void>;
  resetSetting: (key: keyof Settings) => Promise<void>;
  setOverlayAnchor: (anchor: OverlayAnchor) => Promise<void>;
  resetOverlayPosition: () => Promise<void>;
  refreshSettings: (required?: boolean) => Promise<void>;
  refreshAudioDevices: () => Promise<void>;
  refreshOutputDevices: () => Promise<void>;
  updateBinding: (id: string, binding: string) => Promise<void>;
  resetBinding: (id: string) => Promise<void>;
  getSetting: <K extends keyof Settings>(key: K) => Settings[K] | undefined;
  playTestSound: (soundType: "start" | "stop") => Promise<void>;
  checkCustomSounds: () => Promise<void>;
  setPostProcessProvider: (providerId: string) => Promise<void>;
  updatePostProcessSetting: (
    settingType: "base_url" | "api_key" | "model",
    providerId: string,
    value: string,
  ) => Promise<void>;
  updatePostProcessBaseUrl: (
    providerId: string,
    baseUrl: string,
  ) => Promise<void>;
  updatePostProcessApiKey: (
    providerId: string,
    apiKey: string,
  ) => Promise<void>;
  updatePostProcessModel: (providerId: string, model: string) => Promise<void>;
  fetchPostProcessModels: (providerId: string) => Promise<string[]>;
  setPostProcessModelOptions: (providerId: string, models: string[]) => void;

  // Internal state setters
  setSettings: (settings: Settings | null) => void;
  setDefaultSettings: (defaultSettings: Settings | null) => void;
  setLoading: (loading: boolean) => void;
  setUpdating: (key: string, updating: boolean) => void;
  setAudioDevices: (devices: AudioDevice[]) => void;
  setOutputDevices: (devices: AudioDevice[]) => void;
  setCustomSounds: (sounds: { start: boolean; stop: boolean }) => void;
}

// Note: Default settings are now fetched from Rust via commands.getDefaultSettings()
// This ensures platform-specific defaults (like overlay_position, shortcuts, paste_method) work correctly

const DEFAULT_AUDIO_DEVICE: AudioDevice = {
  index: "default",
  name: "Default",
  is_default: true,
};

const settingsLifecycle = createSettingsLifecycle((eventName, handler) =>
  listen(eventName, handler),
);
const settingsRefreshQueue = createKeyedSerialQueue();
const settingWrites = createOptimisticWriteCoordinator<Settings>();

// One backend command now persists any plain `AppSettings` field: it takes the
// field name and the JSON encoding of the new value, and type-checks the value
// against the real Rust field (issue #166). Settings whose backend work is more
// than a write — a device switch, a confirmation dialog, a manager call — keep
// their own command below.
const persistSetting =
  <K extends keyof Settings>(key: K) =>
  (value: Settings[K]) =>
    commands.updateSetting(key as string, JSON.stringify(value ?? null));

const settingUpdaters: {
  [K in keyof Settings]?: (value: Settings[K]) => Promise<unknown>;
} = {
  always_on_microphone: (value) =>
    commands.updateMicrophoneMode(value as boolean),
  audio_feedback: persistSetting("audio_feedback"),
  audio_feedback_volume: persistSetting("audio_feedback_volume"),
  sound_theme: persistSetting("sound_theme"),
  start_hidden: persistSetting("start_hidden"),
  autostart_enabled: persistSetting("autostart_enabled"),
  update_checks_enabled: persistSetting("update_checks_enabled"),
  push_to_talk: persistSetting("push_to_talk"),
  selected_microphone: (value) =>
    commands.setSelectedMicrophone(
      (value as string) === "Default" || value === null
        ? "default"
        : (value as string),
    ),
  clamshell_microphone: (value) =>
    commands.setClamshellMicrophone(
      (value as string) === "Default" ? "default" : (value as string),
    ),
  selected_output_device: (value) =>
    commands.setSelectedOutputDevice(
      (value as string) === "Default" || value === null
        ? "default"
        : (value as string),
    ),
  recording_retention_period: (value) =>
    commands.updateRecordingRetentionPeriod(value as string),
  translate_to_english: persistSetting("translate_to_english"),
  selected_language: persistSetting("selected_language"),
  overlay_position: persistSetting("overlay_position"),
  debug_mode: persistSetting("debug_mode"),
  custom_words: persistSetting("custom_words"),
  word_replacements: persistSetting("word_replacements"),
  word_correction_threshold: persistSetting("word_correction_threshold"),
  paste_delay_ms: persistSetting("paste_delay_ms"),
  // Keeps its own command: selecting the external-script method prompts for
  // confirmation before anything is written.
  paste_method: (value) => commands.changePasteMethodSetting(value as string),
  typing_tool: persistSetting("typing_tool"),
  // Keeps its own command for the same confirmation prompt.
  external_script_path: (value) =>
    commands.changeExternalScriptPathSetting(value as string | null),
  clipboard_handling: persistSetting("clipboard_handling"),
  auto_submit: persistSetting("auto_submit"),
  auto_submit_key: persistSetting("auto_submit_key"),
  // The whole per-app profile list is one plain field, so add, edit, and remove
  // are all the same wholesale write (#123).
  output_profiles: persistSetting("output_profiles"),
  history_limit: (value) => commands.updateHistoryLimit(value as number),
  post_process_enabled: persistSetting("post_process_enabled"),
  post_process_selected_prompt_id: (value) =>
    commands.setPostProcessSelectedPrompt(value as string),
  mute_while_recording: persistSetting("mute_while_recording"),
  append_trailing_space: persistSetting("append_trailing_space"),
  raw_output: persistSetting("raw_output"),
  format_numbers: persistSetting("format_numbers"),
  format_raw_output: persistSetting("format_raw_output"),
  log_level: (value) => commands.setLogLevel(value as any),
  app_language: persistSetting("app_language"),
  experimental_enabled: persistSetting("experimental_enabled"),
  lazy_stream_close: persistSetting("lazy_stream_close"),
  show_tray_icon: persistSetting("show_tray_icon"),
  whisper_accelerator: persistSetting("whisper_accelerator"),
  ort_accelerator: persistSetting("ort_accelerator"),
  whisper_gpu_device: persistSetting("whisper_gpu_device"),
  extra_recording_buffer_ms: persistSetting("extra_recording_buffer_ms"),
  model_unload_timeout: (value) =>
    commands.setModelUnloadTimeout(value as ModelUnloadTimeout),
};

async function persistSettingValue<K extends keyof Settings>(
  key: K,
  value: Settings[K],
) {
  const updater = settingUpdaters[key];
  if (!updater) {
    if (key !== "bindings" && key !== "selected_model") {
      console.warn(`No handler for setting: ${String(key)}`);
    }
    return;
  }

  // tauri-specta bindings resolve to { status: "error" } on a Rust Err
  // instead of throwing, so inspect the result explicitly.
  const errorMessage = settingUpdateError(await updater(value));
  if (errorMessage) {
    throw new Error(errorMessage);
  }
}

function applySettingsPatch(
  settings: Settings | null,
  patch: Partial<Settings>,
): Settings | null {
  if (!settings) {
    return null;
  }
  return mergePendingValues(settings, patch);
}

function pendingSettingsPatch(): Partial<Settings> {
  return settingWrites.pendingValues();
}

function reportSettingWriteError(context: string) {
  return (error: unknown, isLatest: boolean) => {
    console.error(`Failed to ${context}:`, error);
    if (isLatest) {
      notifySaveError();
    }
  };
}

function overlayStatePatch(settings: Settings | null): Partial<Settings> {
  return {
    overlay_position: settings?.overlay_position,
    overlay_custom_position: settings?.overlay_custom_position,
  };
}

function overlayAnchorPatch(
  settings: Settings | null,
  anchor: OverlayAnchor,
): Partial<Settings> {
  const overlayPosition =
    settings?.overlay_position === "none"
      ? "none"
      : anchor.startsWith("top")
        ? "top"
        : "bottom";
  return {
    overlay_position: overlayPosition,
    overlay_custom_position: { anchor, dx: 0, dy: 0 },
  };
}

function overlayResetPatch(settings: Settings | null): Partial<Settings> {
  return {
    overlay_position: settings?.overlay_position === "none" ? "none" : "bottom",
    overlay_custom_position: null,
  };
}

export const useSettingsStore = create<SettingsStore>()(
  subscribeWithSelector((set, get) => ({
    settings: null,
    defaultSettings: null,
    isLoading: true,
    isUpdating: {},
    audioDevices: [],
    outputDevices: [],
    customSounds: { start: false, stop: false },
    postProcessModelOptions: {},

    // Internal setters
    setSettings: (settings) => set({ settings }),
    setDefaultSettings: (defaultSettings) => set({ defaultSettings }),
    setLoading: (isLoading) => set({ isLoading }),
    setUpdating: (key, updating) =>
      set((state) => ({
        isUpdating: { ...state.isUpdating, [key]: updating },
      })),
    setAudioDevices: (audioDevices) => set({ audioDevices }),
    setOutputDevices: (outputDevices) => set({ outputDevices }),
    setCustomSounds: (customSounds) => set({ customSounds }),

    // Getters
    getSetting: (key) => get().settings?.[key],

    // Load settings from store
    refreshSettings: async (required = false) => {
      try {
        await settingsRefreshQueue.run("settings", async () => {
          const result = await commands.getAppSettings();
          if (result.status === "error") {
            throw new Error(result.error);
          }

          const settings = result.data;
          const normalizedSettings: Settings = {
            ...settings,
            always_on_microphone: settings.always_on_microphone ?? false,
            selected_microphone: settings.selected_microphone ?? "Default",
            clamshell_microphone: settings.clamshell_microphone ?? "Default",
            selected_output_device:
              settings.selected_output_device ?? "Default",
          };
          set({
            settings: mergePendingValues(
              normalizedSettings,
              pendingSettingsPatch(),
            ),
            isLoading: false,
          });
        });
      } catch (error) {
        set({ isLoading: false });
        if (required) {
          throw error;
        }
        console.error("Failed to load settings:", error);
      }
    },

    // Load audio devices
    refreshAudioDevices: async () => {
      try {
        const result = await commands.getAvailableMicrophones();
        if (result.status === "ok") {
          const devicesWithDefault = [
            DEFAULT_AUDIO_DEVICE,
            ...result.data.filter(
              (d) => d.name !== "Default" && d.name !== "default",
            ),
          ];
          set({ audioDevices: devicesWithDefault });
        } else {
          set({ audioDevices: [DEFAULT_AUDIO_DEVICE] });
        }
      } catch (error) {
        console.error("Failed to load audio devices:", error);
        set({ audioDevices: [DEFAULT_AUDIO_DEVICE] });
      }
    },

    // Load output devices
    refreshOutputDevices: async () => {
      try {
        const result = await commands.getAvailableOutputDevices();
        if (result.status === "ok") {
          const devicesWithDefault = [
            DEFAULT_AUDIO_DEVICE,
            ...result.data.filter(
              (d) => d.name !== "Default" && d.name !== "default",
            ),
          ];
          set({ outputDevices: devicesWithDefault });
        } else {
          set({ outputDevices: [DEFAULT_AUDIO_DEVICE] });
        }
      } catch (error) {
        console.error("Failed to load output devices:", error);
        set({ outputDevices: [DEFAULT_AUDIO_DEVICE] });
      }
    },

    // Play a test sound
    playTestSound: async (soundType: "start" | "stop") => {
      try {
        await commands.playTestSound(soundType);
      } catch (error) {
        console.error(`Failed to play test sound (${soundType}):`, error);
      }
    },

    checkCustomSounds: async () => {
      const sounds = await commands.checkCustomSounds();
      get().setCustomSounds(sounds);
    },

    // Update a specific setting
    updateSetting: async <K extends keyof Settings>(
      key: K,
      value: Settings[K],
    ) => {
      const { settings, setUpdating } = get();
      const updateKey = String(key);
      const clearsCustomOverlay = key === "overlay_position";
      const confirmedPatch = {
        [key]: settings?.[key],
        ...(clearsCustomOverlay
          ? { overlay_custom_position: settings?.overlay_custom_position }
          : {}),
      } as Partial<Settings>;
      const optimisticPatch = {
        [key]: value,
        ...(clearsCustomOverlay ? { overlay_custom_position: null } : {}),
      } as Partial<Settings>;

      await settingWrites.run({
        key: updateKey,
        hasConfirmedValues: settings !== null,
        confirmedValues: confirmedPatch,
        optimisticValues: optimisticPatch,
        persist: () => persistSettingValue(key, value),
        apply: (patch) =>
          set((state) => ({
            settings: applySettingsPatch(state.settings, patch),
          })),
        setUpdating,
        onError: reportSettingWriteError(`update setting ${String(key)}`),
        onSuccess: notifySaved,
      });
    },

    // Reset a setting to its default value
    resetSetting: async (key) => {
      const { defaultSettings } = get();
      if (defaultSettings) {
        const defaultValue = defaultSettings[key];
        if (defaultValue !== undefined) {
          await get().updateSetting(key, defaultValue as any);
        }
      }
    },

    // Set a precise overlay placement (anchor + zero nudge) from the #9 grid.
    // overlay_custom_position is a struct, not a simple key in settingUpdaters,
    // so this routes through its own command then re-reads from the backend
    // (the source of truth), mirroring resetBinding/setPostProcessProvider. The
    // "overlay_position" update key is reused so the grid and the show/hide
    // dropdown share one in-flight lock.
    setOverlayAnchor: async (anchor) => {
      const { settings, setUpdating } = get();
      const updateKey = "overlay_position";
      await settingWrites.run({
        key: updateKey,
        hasConfirmedValues: settings !== null,
        confirmedValues: overlayStatePatch(settings),
        optimisticValues: overlayAnchorPatch(settings, anchor),
        persist: async () => {
          const result = await commands.setOverlayAnchor(anchor);
          if (result.status === "error") {
            throw new Error(result.error);
          }
        },
        apply: (patch) =>
          set((state) => ({
            settings: applySettingsPatch(state.settings, patch),
          })),
        setUpdating,
        afterSuccess: () => get().refreshSettings(),
        onError: reportSettingWriteError("set overlay anchor"),
        onSuccess: notifySaved,
      });
    },

    // Clear any custom overlay placement, returning to the centered default.
    resetOverlayPosition: async () => {
      const { settings, setUpdating } = get();
      const updateKey = "overlay_position";
      await settingWrites.run({
        key: updateKey,
        hasConfirmedValues: settings !== null,
        confirmedValues: overlayStatePatch(settings),
        optimisticValues: overlayResetPatch(settings),
        persist: async () => {
          const result = await commands.resetOverlayPosition();
          if (result.status === "error") {
            throw new Error(result.error);
          }
        },
        apply: (patch) =>
          set((state) => ({
            settings: applySettingsPatch(state.settings, patch),
          })),
        setUpdating,
        afterSuccess: () => get().refreshSettings(),
        onError: reportSettingWriteError("reset overlay position"),
        onSuccess: notifySaved,
      });
    },

    // Update a specific binding
    updateBinding: async (id, binding) => {
      const { settings, setUpdating } = get();
      const updateKey = `binding_${id}`;
      const originalBinding = settings?.bindings?.[id]?.current_binding;

      setUpdating(updateKey, true);

      try {
        // Optimistic update
        set((state) => ({
          settings: state.settings
            ? {
                ...state.settings,
                bindings: {
                  ...state.settings.bindings,
                  [id]: {
                    ...state.settings.bindings[id]!,
                    current_binding: binding,
                  },
                },
              }
            : null,
        }));

        const result = await commands.changeBinding(id, binding);

        // Check if the command executed successfully
        if (result.status === "error") {
          throw new Error(result.error);
        }

        // Check if the binding change was successful
        if (!result.data.success) {
          throw new Error(result.data.error || "Failed to update binding");
        }

        notifySaved();
      } catch (error) {
        console.error(`Failed to update binding ${id}:`, error);

        // Rollback on error
        if (originalBinding && get().settings) {
          set((state) => ({
            settings: state.settings
              ? {
                  ...state.settings,
                  bindings: {
                    ...state.settings.bindings,
                    [id]: {
                      ...state.settings.bindings[id]!,
                      current_binding: originalBinding,
                    },
                  },
                }
              : null,
          }));
        }

        // Re-throw to let the caller know it failed
        throw error;
      } finally {
        setUpdating(updateKey, false);
      }
    },

    // Reset a specific binding
    resetBinding: async (id) => {
      const { setUpdating, refreshSettings } = get();
      const updateKey = `binding_${id}`;

      setUpdating(updateKey, true);

      try {
        await commands.resetBinding(id);
        await refreshSettings();
        notifySaved();
      } catch (error) {
        console.error(`Failed to reset binding ${id}:`, error);
        notifySaveError();
      } finally {
        setUpdating(updateKey, false);
      }
    },

    setPostProcessProvider: async (providerId) => {
      const {
        settings,
        setUpdating,
        refreshSettings,
        setPostProcessModelOptions,
      } = get();
      const updateKey = "post_process_provider_id";
      const previousId = settings?.post_process_provider_id ?? null;

      setUpdating(updateKey, true);

      if (settings) {
        set((state) => ({
          settings: state.settings
            ? { ...state.settings, post_process_provider_id: providerId }
            : null,
        }));
      }

      // Clear cached model options for the new provider so the dropdown
      // doesn't show stale models from a previous fetch or base_url.
      setPostProcessModelOptions(providerId, []);

      try {
        await commands.setPostProcessProvider(providerId);
        await refreshSettings();
      } catch (error) {
        console.error("Failed to set post-process provider:", error);
        if (previousId !== null) {
          set((state) => ({
            settings: state.settings
              ? { ...state.settings, post_process_provider_id: previousId }
              : null,
          }));
        }
      } finally {
        setUpdating(updateKey, false);
      }
    },

    // Generic updater for post-processing provider settings
    updatePostProcessSetting: async (
      settingType: "base_url" | "api_key" | "model",
      providerId: string,
      value: string,
    ) => {
      const { setUpdating, refreshSettings } = get();
      const updateKey = `post_process_${settingType}:${providerId}`;

      setUpdating(updateKey, true);

      try {
        if (settingType === "base_url") {
          await commands.changePostProcessBaseUrlSetting(providerId, value);
        } else if (settingType === "api_key") {
          await commands.changePostProcessApiKeySetting(providerId, value);
        } else if (settingType === "model") {
          await commands.changePostProcessModelSetting(providerId, value);
        }
        await refreshSettings();
      } catch (error) {
        console.error(
          `Failed to update post-process ${settingType.replace("_", " ")}:`,
          error,
        );
      } finally {
        setUpdating(updateKey, false);
      }
    },

    updatePostProcessBaseUrl: async (providerId, baseUrl) => {
      const { setUpdating, refreshSettings } = get();
      const updateKey = `post_process_base_url:${providerId}`;

      setUpdating(updateKey, true);

      try {
        // Persist the new base URL first.
        const urlResult = await commands.changePostProcessBaseUrlSetting(
          providerId,
          baseUrl,
        );
        if (urlResult.status === "error") {
          console.error("Failed to persist base URL:", urlResult.error);
          return;
        }

        // Reset the stored model since the previous value is almost certainly
        // invalid for the new endpoint (e.g. switching Custom from Groq to
        // Cerebras). Only proceed if the reset succeeds.
        const modelResult = await commands.changePostProcessModelSetting(
          providerId,
          "",
        );
        if (modelResult.status === "error") {
          console.error("Failed to reset model setting:", modelResult.error);
          return;
        }

        // Clear cached model options only after both backend writes succeed.
        set((state) => ({
          postProcessModelOptions: {
            ...state.postProcessModelOptions,
            [providerId]: [],
          },
        }));

        // Single refresh after both backend writes.
        await refreshSettings();
      } catch (error) {
        console.error("Failed to update post-process base URL:", error);
      } finally {
        setUpdating(updateKey, false);
      }
    },

    updatePostProcessApiKey: async (providerId, apiKey) => {
      // Clear cached models when API key changes - user should click refresh after
      set((state) => ({
        postProcessModelOptions: {
          ...state.postProcessModelOptions,
          [providerId]: [],
        },
      }));
      return get().updatePostProcessSetting("api_key", providerId, apiKey);
    },

    updatePostProcessModel: async (providerId, model) => {
      return get().updatePostProcessSetting("model", providerId, model);
    },

    fetchPostProcessModels: async (providerId) => {
      const updateKey = `post_process_models_fetch:${providerId}`;
      const { setUpdating, setPostProcessModelOptions } = get();

      setUpdating(updateKey, true);

      try {
        // Call Tauri backend command instead of fetch
        const result = await commands.fetchPostProcessModels(providerId);
        if (result.status === "ok") {
          setPostProcessModelOptions(providerId, result.data);
          return result.data;
        } else {
          console.error("Failed to fetch models:", result.error);
          return [];
        }
      } catch (error) {
        console.error("Failed to fetch models:", error);
        // Don't cache empty array on error - let user retry
        return [];
      } finally {
        setUpdating(updateKey, false);
      }
    },

    setPostProcessModelOptions: (providerId, models) =>
      set((state) => ({
        postProcessModelOptions: {
          ...state.postProcessModelOptions,
          [providerId]: models,
        },
      })),

    // Load default settings from Rust
    loadDefaultSettings: async () => {
      const result = await commands.getDefaultSettings();
      if (result.status === "error") {
        throw new Error(result.error);
      }
      set({ defaultSettings: result.data });
    },

    // Initialize everything once for the application process.
    initialize: () =>
      settingsLifecycle.initialize(
        async () => {
          const { refreshSettings, checkCustomSounds, loadDefaultSettings } =
            get();

          // Audio devices are loaded only after onboarding. Loading them here
          // can trigger a macOS permission dialog before the user is ready.
          const results = await Promise.allSettled([
            loadDefaultSettings(),
            refreshSettings(true),
            checkCustomSounds(),
          ]);
          const failure = results.find(
            (result): result is PromiseRejectedResult =>
              result.status === "rejected",
          );
          if (failure) {
            throw failure.reason;
          }
        },
        () => get().refreshSettings(),
      ),
  })),
);
