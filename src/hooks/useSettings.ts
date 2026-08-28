import { useCallback } from "react";
import { useShallow } from "zustand/react/shallow";
import { useSettingsStore } from "../stores/settingsStore";
import type {
  AppSettings as Settings,
  AudioDevice,
  OverlayAnchor,
} from "@/bindings";

interface UseSettingsReturn {
  // State
  settings: Settings | null;
  isLoading: boolean;
  isUpdating: (key: string) => boolean;
  audioDevices: AudioDevice[];
  outputDevices: AudioDevice[];
  audioFeedbackEnabled: boolean;
  postProcessModelOptions: Record<string, string[]>;

  // Actions
  updateSetting: <K extends keyof Settings>(
    key: K,
    value: Settings[K],
  ) => Promise<void>;
  resetSetting: (key: keyof Settings) => Promise<void>;
  setOverlayAnchor: (anchor: OverlayAnchor) => Promise<void>;
  resetOverlayPosition: () => Promise<void>;
  refreshSettings: () => Promise<void>;
  refreshAudioDevices: () => Promise<void>;
  refreshOutputDevices: () => Promise<void>;

  // Binding-specific actions
  updateBinding: (id: string, binding: string) => Promise<void>;
  resetBinding: (id: string) => Promise<void>;

  // Convenience getters
  getSetting: <K extends keyof Settings>(key: K) => Settings[K] | undefined;

  // Post-processing helpers
  setPostProcessProvider: (providerId: string) => Promise<void>;
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
}

function handleIgnoredRejection<T>(operation: Promise<T>): Promise<T> {
  void operation.catch(() => {});
  return operation;
}

export const useSettings = (): UseSettingsReturn => {
  const store = useSettingsStore(
    useShallow((state) => ({
      settings: state.settings,
      isLoading: state.isLoading,
      isUpdating: state.isUpdating,
      audioDevices: state.audioDevices,
      outputDevices: state.outputDevices,
      postProcessModelOptions: state.postProcessModelOptions,
      updateSetting: state.updateSetting,
      resetSetting: state.resetSetting,
      setOverlayAnchor: state.setOverlayAnchor,
      resetOverlayPosition: state.resetOverlayPosition,
      refreshSettings: state.refreshSettings,
      refreshAudioDevices: state.refreshAudioDevices,
      refreshOutputDevices: state.refreshOutputDevices,
      updateBinding: state.updateBinding,
      resetBinding: state.resetBinding,
      getSetting: state.getSetting,
      setPostProcessProvider: state.setPostProcessProvider,
      updatePostProcessBaseUrl: state.updatePostProcessBaseUrl,
      updatePostProcessApiKey: state.updatePostProcessApiKey,
      updatePostProcessModel: state.updatePostProcessModel,
      fetchPostProcessModels: state.fetchPostProcessModels,
    })),
  );

  const isUpdating = useCallback(
    (key: string) => store.isUpdating[key] || false,
    [store.isUpdating],
  );
  const updateSetting = useCallback(
    <K extends keyof Settings>(key: K, value: Settings[K]) => {
      return handleIgnoredRejection(store.updateSetting(key, value));
    },
    [store.updateSetting],
  );
  const resetSetting = useCallback(
    (key: keyof Settings) => handleIgnoredRejection(store.resetSetting(key)),
    [store.resetSetting],
  );
  const setOverlayAnchor = useCallback(
    (anchor: OverlayAnchor) =>
      handleIgnoredRejection(store.setOverlayAnchor(anchor)),
    [store.setOverlayAnchor],
  );
  const resetOverlayPosition = useCallback(
    () => handleIgnoredRejection(store.resetOverlayPosition()),
    [store.resetOverlayPosition],
  );

  return {
    settings: store.settings,
    isLoading: store.isLoading,
    isUpdating,
    audioDevices: store.audioDevices,
    outputDevices: store.outputDevices,
    audioFeedbackEnabled: store.settings?.audio_feedback || false,
    postProcessModelOptions: store.postProcessModelOptions,
    updateSetting,
    resetSetting,
    setOverlayAnchor,
    resetOverlayPosition,
    refreshSettings: store.refreshSettings,
    refreshAudioDevices: store.refreshAudioDevices,
    refreshOutputDevices: store.refreshOutputDevices,
    updateBinding: store.updateBinding,
    resetBinding: store.resetBinding,
    getSetting: store.getSetting,
    setPostProcessProvider: store.setPostProcessProvider,
    updatePostProcessBaseUrl: store.updatePostProcessBaseUrl,
    updatePostProcessApiKey: store.updatePostProcessApiKey,
    updatePostProcessModel: store.updatePostProcessModel,
    fetchPostProcessModels: store.fetchPostProcessModels,
  };
};
