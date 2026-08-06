import { beforeEach, describe, expect, mock, test } from "bun:test";
type CommandResult<T> =
  | { status: "ok"; data: T }
  | { status: "error"; error: string };

type EventHandler = (event: { payload: unknown }) => void;

const eventHandlers = new Map<string, EventHandler>();
const errorToasts: string[] = [];
let downloadModelCalls = 0;
let getAvailableModelsResult: CommandResult<unknown[]> = {
  status: "ok",
  data: [],
};
let cancelDownloadResult: CommandResult<null> = {
  status: "ok",
  data: null,
};
let downloadModelResult: CommandResult<null> = {
  status: "error",
  error: "Download already in progress for model: small",
};

mock.module("@tauri-apps/api/event", () => ({
  listen: (eventName: string, handler: EventHandler) => {
    eventHandlers.set(eventName, handler);
    return Promise.resolve(() => {});
  },
}));

mock.module("@/bindings", () => ({
  commands: {
    getAvailableModels: async () => getAvailableModelsResult,
    getCurrentModel: async () => ({ status: "ok", data: "" }),
    hasAnyModelsAvailable: async () => ({ status: "ok", data: false }),
    setActiveModel: async () => ({ status: "ok", data: null }),
    downloadModel: async () => {
      downloadModelCalls += 1;
      return downloadModelResult;
    },
    cancelDownload: async () => cancelDownloadResult,
    deleteModel: async () => ({ status: "ok", data: null }),
  },
}));

mock.module("@/i18n", () => ({
  default: {
    t: (key: string) => {
      if (key === "onboarding.downloadFailed") {
        return "Localized download failure";
      }
      if (key === "onboarding.cancelPending") {
        return "Localized cancellation pending";
      }
      if (key === "onboarding.downloadNotifyFailed") {
        return "Localized download notification failure";
      }
      return key;
    },
  },
}));

mock.module("sonner", () => ({
  toast: {
    error: (message: string) => errorToasts.push(message),
  },
}));

const { useModelStore } = await import("./modelStore");

function model(id: string, isDownloading: boolean) {
  return {
    id,
    name: id,
    description: "",
    filename: `${id}.bin`,
    url: null,
    sha256: null,
    size_mb: 1,
    is_downloaded: false,
    is_downloading: isDownloading,
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
  };
}

beforeEach(() => {
  eventHandlers.clear();
  errorToasts.length = 0;
  downloadModelCalls = 0;
  getAvailableModelsResult = { status: "ok", data: [] };
  cancelDownloadResult = { status: "ok", data: null };
  downloadModelResult = {
    status: "error",
    error: "Download already in progress for model: small",
  };
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
  test("a fresh store does not guess that a rehydrated download is still in the byte phase", async () => {
    getAvailableModelsResult = {
      status: "ok",
      data: [model("verifying", true), model("extracting", true)],
    };

    const originalSetTimeout = globalThis.setTimeout;
    let timersArmed = 0;
    globalThis.setTimeout = ((..._args: Parameters<typeof setTimeout>) => {
      timersArmed += 1;
      return 1;
    }) as typeof setTimeout;

    try {
      await useModelStore.getState().loadModels();
    } finally {
      globalThis.setTimeout = originalSetTimeout;
    }

    expect(timersArmed).toBe(0);
  });

  test("a duplicate start preserves the active same-model download marker", async () => {
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
    expect(await useModelStore.getState().downloadModel("small")).toBe(true);
    expect(downloadModelCalls).toBe(0);
    expect(useModelStore.getState().downloadingModels.small).toBe(true);
    expect(useModelStore.getState().downloadProgress.small?.downloaded).toBe(
      50,
    );
  });

  test("a backend duplicate response is not treated as a terminal failure", async () => {
    downloadModelResult = {
      status: "error",
      error: "model_download_already_active",
    };

    expect(await useModelStore.getState().downloadModel("small")).toBe(true);
    expect(downloadModelCalls).toBe(1);
    expect(useModelStore.getState().downloadingModels.small).toBe(true);
  });

  test("a backend duplicate response disarms its speculative stall timer", async () => {
    downloadModelResult = {
      status: "error",
      error: "model_download_already_active",
    };
    const originalSetTimeout = globalThis.setTimeout;
    const originalClearTimeout = globalThis.clearTimeout;
    const timerToken = 987_654;
    let cleared = false;
    globalThis.setTimeout = (() => timerToken) as typeof setTimeout;
    globalThis.clearTimeout = ((token: ReturnType<typeof setTimeout>) => {
      if (token === timerToken) cleared = true;
    }) as typeof clearTimeout;

    try {
      expect(await useModelStore.getState().downloadModel("rejoined")).toBe(
        true,
      );
    } finally {
      globalThis.setTimeout = originalSetTimeout;
      globalThis.clearTimeout = originalClearTimeout;
    }

    expect(cleared).toBe(true);
  });

  test("a backend duplicate failure event preserves the original lifecycle", async () => {
    await useModelStore.getState().initialize();
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

    const handler = eventHandlers.get("model-download-failed");
    handler?.({
      payload: {
        model_id: "small",
        error: "model_download_already_active",
      },
    });

    expect(useModelStore.getState().downloadingModels.small).toBe(true);
    expect(errorToasts).toEqual([]);
  });

  test("cancel clears stale frontend state when the backend has no live task", async () => {
    cancelDownloadResult = {
      status: "error",
      error: "model_download_not_active",
    };
    getAvailableModelsResult = {
      status: "ok",
      data: [model("small", false)],
    };
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

    expect(await useModelStore.getState().cancelDownload("small")).toBe(true);
    expect(useModelStore.getState().downloadingModels.small).toBeUndefined();
    expect(useModelStore.getState().downloadProgress.small).toBeUndefined();
    expect(useModelStore.getState().error).toBeNull();
  });

  test("cancel tombstone masks stale backend state until the worker exits", async () => {
    getAvailableModelsResult = {
      status: "ok",
      data: [model("small", true)],
    };
    useModelStore.setState({ downloadingModels: { small: true } });

    expect(await useModelStore.getState().cancelDownload("small")).toBe(true);
    await useModelStore.getState().loadModels();

    expect(useModelStore.getState().cancellingModels.small).toBe(true);
    expect(useModelStore.getState().downloadingModels.small).toBeUndefined();

    getAvailableModelsResult = {
      status: "ok",
      data: [model("small", false)],
    };
    await useModelStore.getState().loadModels();
    expect(useModelStore.getState().cancellingModels.small).toBeUndefined();
  });

  test("retry while cancellation is pending fails without joining the old worker", async () => {
    getAvailableModelsResult = {
      status: "ok",
      data: [model("small", true)],
    };
    useModelStore.setState({ cancellingModels: { small: true } });

    expect(await useModelStore.getState().downloadModel("small")).toBe(false);

    expect(downloadModelCalls).toBe(0);
    expect(useModelStore.getState().downloadingModels.small).toBeUndefined();
    expect(useModelStore.getState().error).toBe(
      "Localized cancellation pending",
    );
    expect(errorToasts).toEqual(["Localized cancellation pending"]);
  });

  test("backend cancellation-pending response is not treated as success", async () => {
    downloadModelResult = {
      status: "error",
      error: "model_download_cancelling",
    };

    expect(await useModelStore.getState().downloadModel("small")).toBe(false);

    expect(downloadModelCalls).toBe(1);
    expect(useModelStore.getState().cancellingModels.small).toBe(true);
    expect(useModelStore.getState().downloadingModels.small).toBeUndefined();
    expect(useModelStore.getState().error).toBe(
      "Localized cancellation pending",
    );
  });

  test("a stable stalled-download code is localized before reaching the user", async () => {
    await useModelStore.getState().initialize();
    useModelStore.setState({ downloadingModels: { small: true } });

    const handler = eventHandlers.get("model-download-failed");
    expect(handler).toBeDefined();
    handler?.({
      payload: {
        model_id: "small",
        error: "model_download_stalled",
      },
    });

    expect(useModelStore.getState().error).toBe("Localized download failure");
    expect(errorToasts).toEqual(["Localized download failure"]);
  });

  test("a completion-notification failure is localized before reaching the user", async () => {
    await useModelStore.getState().initialize();
    useModelStore.setState({ downloadingModels: { small: true } });

    eventHandlers.get("model-download-failed")?.({
      payload: {
        model_id: "small",
        error: "model_download_notify_failed",
      },
    });

    expect(useModelStore.getState().error).toBe(
      "Localized download notification failure",
    );
    expect(errorToasts).toEqual(["Localized download notification failure"]);
  });

  test("frontend and backend stall signals produce only one toast", async () => {
    await useModelStore.getState().initialize();
    useModelStore.setState({ downloadingModels: { stall: true } });

    const originalSetTimeout = globalThis.setTimeout;
    let fireTimer: (() => void) | null = null;
    globalThis.setTimeout = ((handler: TimerHandler, ..._args: unknown[]) => {
      if (typeof handler === "function") {
        fireTimer = handler;
      }
      return 1;
    }) as typeof setTimeout;

    try {
      eventHandlers.get("model-download-progress")?.({
        payload: {
          model_id: "stall",
          downloaded: 1,
          total: 10,
          percentage: 10,
        },
      });
      const timerCallback = fireTimer as (() => void) | null;
      timerCallback?.();

      eventHandlers.get("model-download-failed")?.({
        payload: {
          model_id: "stall",
          error: "model_download_stalled",
        },
      });
    } finally {
      globalThis.setTimeout = originalSetTimeout;
    }

    expect(errorToasts).toEqual(["Localized download failure"]);
  });

  test("late phase events cannot resurrect a cancelling download", async () => {
    await useModelStore.getState().initialize();
    useModelStore.setState({ cancellingModels: { small: true } });

    eventHandlers.get("model-download-progress")?.({
      payload: {
        model_id: "small",
        downloaded: 10,
        total: 100,
        percentage: 10,
      },
    });
    eventHandlers.get("model-verification-started")?.({ payload: "small" });
    eventHandlers.get("model-extraction-started")?.({ payload: "small" });

    expect(useModelStore.getState().downloadProgress.small).toBeUndefined();
    expect(useModelStore.getState().verifyingModels.small).toBeUndefined();
    expect(useModelStore.getState().extractingModels.small).toBeUndefined();
  });
});
