import { create } from "zustand";
import { subscribeWithSelector } from "zustand/middleware";
import { produce } from "immer";
import { listen } from "@tauri-apps/api/event";
import { commands, type ModelInfo } from "@/bindings";
import i18n from "@/i18n";
import { toast } from "sonner";

interface DownloadProgress {
  model_id: string;
  downloaded: number;
  total: number;
  percentage: number;
}

interface DownloadStats {
  startTime: number;
  lastUpdate: number;
  totalDownloaded: number;
  speed: number; // MB/s
}

// Using Record instead of Set/Map for Immer compatibility
interface ModelsStore {
  models: ModelInfo[];
  currentModel: string;
  downloadingModels: Record<string, true>;
  cancellingModels: Record<string, true>;
  verifyingModels: Record<string, true>;
  extractingModels: Record<string, true>;
  downloadProgress: Record<string, DownloadProgress>;
  downloadStats: Record<string, DownloadStats>;
  loading: boolean;
  error: string | null;
  hasAnyModels: boolean;
  isFirstRun: boolean;
  initialized: boolean;

  // Actions
  initialize: () => Promise<void>;
  loadModels: () => Promise<void>;
  loadCurrentModel: () => Promise<void>;
  checkFirstRun: () => Promise<boolean>;
  selectModel: (modelId: string) => Promise<boolean>;
  downloadModel: (modelId: string) => Promise<boolean>;
  cancelDownload: (modelId: string) => Promise<boolean>;
  deleteModel: (modelId: string) => Promise<boolean>;
  getModelInfo: (modelId: string) => ModelInfo | undefined;
  isModelDownloading: (modelId: string) => boolean;
  isModelVerifying: (modelId: string) => boolean;
  isModelExtracting: (modelId: string) => boolean;
  getDownloadProgress: (modelId: string) => DownloadProgress | undefined;

  // Internal setters
  setModels: (models: ModelInfo[]) => void;
  setCurrentModel: (modelId: string) => void;
  setError: (error: string | null) => void;
  setLoading: (loading: boolean) => void;
}

export const useModelStore = create<ModelsStore>()(
  subscribeWithSelector((set, get) => ({
    models: [],
    currentModel: "",
    downloadingModels: {},
    cancellingModels: {},
    verifyingModels: {},
    extractingModels: {},
    downloadProgress: {},
    downloadStats: {},
    loading: true,
    error: null,
    hasAnyModels: false,
    isFirstRun: false,
    initialized: false,

    // Internal setters
    setModels: (models) => set({ models }),
    setCurrentModel: (currentModel) => set({ currentModel }),
    setError: (error) => set({ error }),
    setLoading: (loading) => set({ loading }),

    loadModels: async () => {
      try {
        const result = await commands.getAvailableModels();
        if (result.status === "ok") {
          set(
            produce((state) => {
              state.models = result.data;
              state.error = null;

              // Sync downloading state from backend. A successful cancel is
              // acknowledged before the backend worker necessarily exits, so
              // cancellingModels temporarily masks stale is_downloading=true
              // responses until the worker releases ownership.
              const backendDownloading: Record<string, true> = {};
              result.data
                .filter((m) => m.is_downloading)
                .forEach((m) => {
                  backendDownloading[m.id] = true;
                });

              // Merge: keep frontend state if downloading, add backend state
              Object.keys(backendDownloading).forEach((id) => {
                if (!state.cancellingModels[id]) {
                  state.downloadingModels[id] = true;
                }
              });

              // Once the backend no longer reports the worker, the cancel
              // tombstone has served its purpose and a later retry may start.
              Object.keys(state.cancellingModels).forEach((id) => {
                if (!backendDownloading[id]) {
                  delete state.cancellingModels[id];
                  delete state.downloadingModels[id];
                  delete state.downloadProgress[id];
                  delete state.downloadStats[id];
                }
              });

              // Remove models that backend says are NOT downloading AND
              // frontend doesn't have progress for (completed/cancelled)
              Object.keys(state.downloadingModels).forEach((id) => {
                if (!backendDownloading[id] && !state.downloadProgress[id]) {
                  delete state.downloadingModels[id];
                }
              });
            }),
          );
        } else {
          set({ error: `Failed to load models: ${result.error}` });
        }
      } catch (err) {
        set({ error: `Failed to load models: ${err}` });
      } finally {
        set({ loading: false });
      }
    },

    loadCurrentModel: async () => {
      try {
        const result = await commands.getCurrentModel();
        if (result.status === "ok") {
          set({ currentModel: result.data });
        }
      } catch (err) {
        console.error("Failed to load current model:", err);
      }
    },

    checkFirstRun: async () => {
      try {
        const result = await commands.hasAnyModelsAvailable();
        if (result.status === "ok") {
          const hasModels = result.data;
          set({ hasAnyModels: hasModels, isFirstRun: !hasModels });
          return !hasModels;
        }
        return false;
      } catch (err) {
        console.error("Failed to check model availability:", err);
        return false;
      }
    },

    selectModel: async (modelId: string) => {
      try {
        set({ error: null });
        const result = await commands.setActiveModel(modelId);
        if (result.status === "ok") {
          set({
            currentModel: modelId,
            isFirstRun: false,
            hasAnyModels: true,
          });
          return true;
        } else {
          set({ error: `Failed to switch to model: ${result.error}` });
          return false;
        }
      } catch (err) {
        set({ error: `Failed to switch to model: ${err}` });
        return false;
      }
    },

    downloadModel: async (modelId: string) => {
      try {
        set({ error: null });

        // A repeated click/request joins the existing frontend operation. Do
        // not issue a second IPC call whose "already in progress" response
        // could be mistaken for a terminal failure and clear live state.
        if (modelId in get().downloadingModels) {
          return true;
        }

        // The backend still owns this model until it observes an acknowledged
        // cancellation. Refresh once so the same retry can proceed if that
        // worker has exited; otherwise leave the cancellation suppressed.
        if (modelId in get().cancellingModels) {
          await get().loadModels();
          if (modelId in get().downloadingModels) {
            return true;
          }
          if (modelId in get().cancellingModels) {
            const message = i18n.t("onboarding.downloadFailed");
            set({ error: message });
            toast.error(message);
            return false;
          }
        }

        set(
          produce((state) => {
            state.downloadingModels[modelId] = true;
            state.downloadProgress[modelId] = {
              model_id: modelId,
              downloaded: 0,
              total: 0,
              percentage: 0,
            };
          }),
        );
        const result = await commands.downloadModel(modelId);
        if (result.status !== "ok") {
          // Fallback cleanup in case the model-download-failed event was not received
          // (e.g. listener not yet registered). The event handler is a no-op if it
          // arrives after this cleanup since deleting missing keys is safe.
          set(
            produce((state) => {
              delete state.downloadingModels[modelId];
              delete state.downloadProgress[modelId];
              delete state.downloadStats[modelId];
            }),
          );
        }
        return result.status === "ok";
      } catch {
        // model-download-failed event won't fire for JS exceptions (e.g. IPC error),
        // so clean up state here to avoid a stuck progress spinner.
        set(
          produce((state) => {
            delete state.downloadingModels[modelId];
            delete state.downloadProgress[modelId];
            delete state.downloadStats[modelId];
          }),
        );
        return false;
      }
    },

    cancelDownload: async (modelId: string) => {
      try {
        set({ error: null });
        const result = await commands.cancelDownload(modelId);
        if (result.status === "ok") {
          set(
            produce((state) => {
              state.cancellingModels[modelId] = true;
              delete state.downloadingModels[modelId];
              delete state.downloadProgress[modelId];
              delete state.downloadStats[modelId];
            }),
          );

          // The backend task still owns is_downloading until it observes the
          // cancel flag and exits. cancellingModels prevents any intervening
          // loadModels call from resurrecting the spinner; the first refresh
          // after ownership ends removes that tombstone.
          return true;
        } else {
          set({ error: `Failed to cancel download: ${result.error}` });
          return false;
        }
      } catch (err) {
        set({ error: `Failed to cancel download: ${err}` });
        return false;
      }
    },

    deleteModel: async (modelId: string) => {
      try {
        set({ error: null });
        const result = await commands.deleteModel(modelId);
        if (result.status === "ok") {
          await get().loadModels();
          await get().loadCurrentModel();
          return true;
        } else {
          set({ error: `Failed to delete model: ${result.error}` });
          return false;
        }
      } catch (err) {
        set({ error: `Failed to delete model: ${err}` });
        return false;
      }
    },

    getModelInfo: (modelId: string) => {
      return get().models.find((model) => model.id === modelId);
    },

    isModelDownloading: (modelId: string) => {
      return modelId in get().downloadingModels;
    },

    isModelVerifying: (modelId: string) => {
      return modelId in get().verifyingModels;
    },

    isModelExtracting: (modelId: string) => {
      return modelId in get().extractingModels;
    },

    getDownloadProgress: (modelId: string) => {
      return get().downloadProgress[modelId];
    },

    initialize: async () => {
      if (get().initialized) return;

      const { loadModels, loadCurrentModel, checkFirstRun } = get();

      // Load initial data
      await Promise.all([loadModels(), loadCurrentModel(), checkFirstRun()]);

      // Set up event listeners
      listen<DownloadProgress>("model-download-progress", (event) => {
        const progress = event.payload;
        set(
          produce((state) => {
            if (state.cancellingModels[progress.model_id]) {
              return;
            }
            state.downloadProgress[progress.model_id] = progress;
          }),
        );

        // Update download stats for speed calculation
        const now = Date.now();
        set(
          produce((state) => {
            if (state.cancellingModels[progress.model_id]) {
              return;
            }
            const current = state.downloadStats[progress.model_id];

            if (!current) {
              state.downloadStats[progress.model_id] = {
                startTime: now,
                lastUpdate: now,
                totalDownloaded: progress.downloaded,
                speed: 0,
              };
            } else {
              const timeDiff = (now - current.lastUpdate) / 1000;
              const bytesDiff = progress.downloaded - current.totalDownloaded;

              if (timeDiff > 0.5) {
                const currentSpeed = bytesDiff / (1024 * 1024) / timeDiff;
                const validCurrentSpeed = Math.max(0, currentSpeed);
                const smoothedSpeed =
                  current.speed > 0
                    ? current.speed * 0.8 + validCurrentSpeed * 0.2
                    : validCurrentSpeed;

                state.downloadStats[progress.model_id] = {
                  startTime: current.startTime,
                  lastUpdate: now,
                  totalDownloaded: progress.downloaded,
                  speed: Math.max(0, smoothedSpeed),
                };
              }
            }
          }),
        );
      });

      listen<string>("model-download-complete", (event) => {
        const modelId = event.payload;
        set(
          produce((state) => {
            delete state.cancellingModels[modelId];
            delete state.downloadingModels[modelId];
            delete state.verifyingModels[modelId];
            delete state.downloadProgress[modelId];
            delete state.downloadStats[modelId];
            // The backend emits completion only after the model is installed,
            // so mark it downloaded now. The onboarding watcher advances on
            // is_downloaded && nothing in flight; without this it can observe
            // the gap between clearing in-flight state and the loadModels()
            // refresh below and abandon a successful download.
            const model = state.models.find((m: ModelInfo) => m.id === modelId);
            if (model) {
              model.is_downloaded = true;
            }
          }),
        );
        get().loadModels();
      });

      listen<{ model_id: string; error: string }>(
        "model-download-failed",
        (event) => {
          const { model_id: modelId, error } = event.payload;
          set(
            produce((state) => {
              delete state.cancellingModels[modelId];
              delete state.downloadingModels[modelId];
              delete state.verifyingModels[modelId];
              delete state.downloadProgress[modelId];
              delete state.downloadStats[modelId];
              state.error = error;
            }),
          );
          toast.error(error);
        },
      );

      listen<string>("model-verification-started", (event) => {
        const modelId = event.payload;
        set(
          produce((state) => {
            state.verifyingModels[modelId] = true;
          }),
        );
      });

      listen<string>("model-verification-completed", (event) => {
        const modelId = event.payload;
        set(
          produce((state) => {
            delete state.verifyingModels[modelId];
          }),
        );
      });

      listen<string>("model-extraction-started", (event) => {
        const modelId = event.payload;
        set(
          produce((state) => {
            state.extractingModels[modelId] = true;
          }),
        );
      });

      listen<string>("model-extraction-completed", (event) => {
        const modelId = event.payload;
        set(
          produce((state) => {
            delete state.extractingModels[modelId];
          }),
        );
        get().loadModels();
      });

      listen<{ model_id: string; error: string }>(
        "model-extraction-failed",
        (event) => {
          const modelId = event.payload.model_id;
          set(
            produce((state) => {
              delete state.extractingModels[modelId];
              state.error = `Failed to extract model: ${event.payload.error}`;
            }),
          );
        },
      );

      listen<string>("model-download-cancelled", (event) => {
        const modelId = event.payload;
        set(
          produce((state) => {
            state.cancellingModels[modelId] = true;
            delete state.downloadingModels[modelId];
            delete state.verifyingModels[modelId];
            delete state.extractingModels[modelId];
            delete state.downloadProgress[modelId];
            delete state.downloadStats[modelId];
          }),
        );
      });

      listen<string>("model-deleted", () => {
        get().loadModels();
        get().loadCurrentModel();
      });

      listen("model-state-changed", () => {
        get().loadModels();
        get().loadCurrentModel();
      });

      set({ initialized: true });
    },
  })),
);
