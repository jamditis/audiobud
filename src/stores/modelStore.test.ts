import { beforeEach, describe, expect, mock, test } from "bun:test";
import type { ModelInfo } from "@/bindings";

let availableModels: ModelInfo[] = [];
let downloadCalls = 0;
const errorToasts: string[] = [];
let firstDownloadResult: Promise<
  { status: "ok"; data: null } | { status: "error"; error: string }
>;

mock.module("@/bindings", () => ({
  commands: {
    getAvailableModels: async () => ({
      status: "ok" as const,
      data: availableModels,
    }),
    getCurrentModel: async () => ({ status: "ok" as const, data: "" }),
    hasAnyModelsAvailable: async () => ({
      status: "ok" as const,
      data: false,
    }),
    downloadModel: async () => {
      downloadCalls += 1;
      if (downloadCalls === 1) return firstDownloadResult;
      return {
        status: "error" as const,
        error: "Download already in progress",
      };
    },
    cancelDownload: async () => ({ status: "ok" as const, data: null }),
  },
}));

mock.module("@tauri-apps/api/event", () => ({
  listen: async () => () => {},
}));

mock.module("sonner", () => ({
  toast: { error: (message: string) => errorToasts.push(message) },
}));

mock.module("@/i18n", () => ({
  default: {
    t: (key: string) =>
      key === "onboarding.downloadFailed" ? "Localized download failure" : key,
  },
}));

const { useModelStore } = await import("./modelStore");

const model = (isDownloading: boolean): ModelInfo => ({
  id: "small",
  name: "Whisper Small",
  description: "Test model",
  filename: "ggml-small.bin",
  url: "https://example.com/ggml-small.bin",
  sha256: null,
  size_mb: 100,
  is_downloaded: false,
  is_downloading: isDownloading,
  partial_size: 0,
  is_directory: false,
  engine_type: "Whisper",
  accuracy_score: 0.5,
  speed_score: 0.5,
  supports_translation: true,
  is_recommended: true,
  supported_languages: ["en"],
  supports_language_selection: true,
  is_custom: false,
});

beforeEach(() => {
  availableModels = [];
  downloadCalls = 0;
  errorToasts.length = 0;
  firstDownloadResult = Promise.resolve({ status: "ok", data: null });
  useModelStore.setState({
    models: [],
    currentModel: "",
    downloadingModels: {},
    cancellingModels: {},
    verifyingModels: {},
    extractingModels: {},
    downloadProgress: {},
    downloadStats: {},
    loading: false,
    error: null,
    hasAnyModels: false,
    isFirstRun: false,
    initialized: false,
  });
});

describe("model download lifecycle", () => {
  test("a duplicate request preserves the original in-flight download", async () => {
    let finishFirst: (
      result: { status: "ok"; data: null } | { status: "error"; error: string },
    ) => void = () => {};
    firstDownloadResult = new Promise((resolve) => {
      finishFirst = resolve;
    });

    const original = useModelStore.getState().downloadModel("small");
    expect(useModelStore.getState().downloadingModels.small).toBe(true);

    const duplicate = await useModelStore.getState().downloadModel("small");

    expect(duplicate).toBe(true);
    expect(downloadCalls).toBe(1);
    expect(useModelStore.getState().downloadingModels.small).toBe(true);

    finishFirst({ status: "ok", data: null });
    await original;
  });

  test("backend lag after cancellation cannot resurrect a cancelled download", async () => {
    useModelStore.setState({
      downloadingModels: { small: true },
      downloadProgress: {
        small: {
          model_id: "small",
          downloaded: 50,
          total: 100,
          percentage: 50,
        },
      },
    });
    availableModels = [model(true)];

    expect(await useModelStore.getState().cancelDownload("small")).toBe(true);
    await useModelStore.getState().loadModels();

    expect(useModelStore.getState().downloadingModels.small).toBeUndefined();
    expect(useModelStore.getState().downloadProgress.small).toBeUndefined();
    expect(useModelStore.getState().cancellingModels.small).toBe(true);

    availableModels = [model(false)];
    await useModelStore.getState().loadModels();
    expect(useModelStore.getState().cancellingModels.small).toBeUndefined();
  });

  test("a retry can start after the cancelled backend worker exits", async () => {
    useModelStore.setState({ cancellingModels: { small: true } });
    availableModels = [model(false)];

    expect(await useModelStore.getState().downloadModel("small")).toBe(true);

    expect(downloadCalls).toBe(1);
    expect(useModelStore.getState().cancellingModels.small).toBeUndefined();
    expect(useModelStore.getState().downloadingModels.small).toBe(true);
  });

  test("a retry reports that the previous cancellation is still pending", async () => {
    useModelStore.setState({ cancellingModels: { small: true } });
    availableModels = [model(true)];

    expect(await useModelStore.getState().downloadModel("small")).toBe(false);

    expect(downloadCalls).toBe(0);
    expect(useModelStore.getState().error).toBe("Localized download failure");
    expect(errorToasts).toEqual(["Localized download failure"]);
  });
});
