import { create } from "zustand";
import { subscribeWithSelector } from "zustand/middleware";
import { produce } from "immer";
import { listen } from "@tauri-apps/api/event";
import { commands, type ModelInfo } from "@/bindings";
import { toast } from "sonner";
import i18n from "@/i18n";

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

// If no progress event arrives during the byte-download phase, the backend's
// progress emits are being lost; surface the same error path as
// model-download-failed instead of showing an indefinite spinner. The timer is
// cleared once verification or extraction starts — those phases emit no
// progress events and have their own UI states.
const DOWNLOAD_STALL_TIMEOUT_MS = 60_000;
// Keep these backend lifecycle codes aligned with managers/model.rs.
const MODEL_DOWNLOAD_ALREADY_ACTIVE = "model_download_already_active";
const MODEL_DOWNLOAD_CANCELLING = "model_download_cancelling";
const MODEL_DOWNLOAD_NOTIFY_FAILED = "model_download_notify_failed";
const MODEL_DOWNLOAD_NOT_ACTIVE = "model_download_not_active";
const MODEL_DOWNLOAD_STALLED = "model_download_stalled";

const stallTimers = new Map<string, ReturnType<typeof setTimeout>>();

// Model ids whose download the stall timer already declared failed. The
// cancelled backend task resolves ok, so without this marker the original
// downloadModel() await would report success for a download the UI already
// failed — leaving callers such as onboarding stuck with every card
// disabled.
const stallFailedDownloads = new Set<string>();

function clearStallTimer(modelId: string) {
  const timer = stallTimers.get(modelId);
  if (timer !== undefined) {
    clearTimeout(timer);
    stallTimers.delete(modelId);
  }
}

function resetStallTimer(modelId: string) {
  clearStallTimer(modelId);
  stallTimers.set(
    modelId,
    setTimeout(() => {
      stallTimers.delete(modelId);
      const state = useModelStore.getState();
      if (modelId in state.cancellingModels) return;
      if (!(modelId in state.downloadingModels)) return;
      // At 100% the byte phase is provably done; verification and extraction
      // emit no progress events, so a dropped phase-start event must not read
      // as a stall.
      const progress = state.downloadProgress[modelId];
      if (progress && progress.percentage >= 100) return;
      stallFailedDownloads.add(modelId);
      // Retract the backend download too: declaring it failed while it keeps
      // writing the .partial file would corrupt a user retry. Advisory only —
      // state cleanup and the toast must not wait on it.
      commands.cancelDownload(modelId).catch(() => {});
      const message = i18n.t("onboarding.downloadFailed");
      useModelStore.setState(
        produce((state: ModelsStore) => {
          delete state.downloadingModels[modelId];
          delete state.verifyingModels[modelId];
          delete state.downloadProgress[modelId];
          delete state.downloadStats[modelId];
          state.error = message;
        }),
      );
      toast.error(message);
    }, DOWNLOAD_STALL_TIMEOUT_MS),
  );
}

function localizedDownloadError(error: string) {
  if (error === MODEL_DOWNLOAD_CANCELLING) {
    return i18n.t("onboarding.cancelPending");
  }
  if (error === MODEL_DOWNLOAD_NOTIFY_FAILED) {
    return i18n.t("onboarding.downloadNotifyFailed");
  }
  if (error === MODEL_DOWNLOAD_STALLED) {
    return i18n.t("onboarding.downloadFailed");
  }
  return error;
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
          const backendDownloading: Record<string, true> = {};
          result.data
            .filter((m) => m.is_downloading)
            .forEach((m) => {
              backendDownloading[m.id] = true;
            });
          const retiredCancellations = Object.keys(
            get().cancellingModels,
          ).filter((id) => !backendDownloading[id]);

          set(
            produce((state) => {
              state.models = result.data;
              state.error = null;

              // A successful cancel is acknowledged before the worker exits.
              // Keep that tombstone authoritative while backend state is
              // briefly stale so refreshes cannot resurrect the spinner.
              Object.keys(state.cancellingModels).forEach((id) => {
                delete state.downloadingModels[id];
                delete state.verifyingModels[id];
                delete state.extractingModels[id];
                delete state.downloadProgress[id];
                delete state.downloadStats[id];
              });

              // Merge: keep frontend state if downloading, add backend state
              Object.keys(backendDownloading).forEach((id) => {
                if (!state.cancellingModels[id]) {
                  state.downloadingModels[id] = true;
                }
              });

              retiredCancellations.forEach((id) => {
                delete state.cancellingModels[id];
                delete state.downloadingModels[id];
                delete state.verifyingModels[id];
                delete state.extractingModels[id];
                delete state.downloadProgress[id];
                delete state.downloadStats[id];
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

          retiredCancellations.forEach((id) => {
            clearStallTimer(id);
            stallFailedDownloads.delete(id);
          });

          // A backend-reported download does not prove which phase it is in: a
          // fresh frontend may have missed the verification/extraction event.
          // Only byte-progress events arm this timer. Here we merely disarm a
          // known completed/phase-transitioned byte stream and leave any live
          // byte timer already maintained by progress events untouched.
          Object.keys(backendDownloading).forEach((id) => {
            const state = get();
            if (
              state.verifyingModels[id] ||
              state.extractingModels[id] ||
              state.downloadProgress[id]?.percentage >= 100
            ) {
              clearStallTimer(id);
            }
          });
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
        // Preserve the lifecycle owned by the active request. A duplicate
        // caller is not a terminal event and must not clear its marker.
        if (modelId in get().downloadingModels) {
          return true;
        }
        if (modelId in get().cancellingModels) {
          await get().loadModels();
          if (modelId in get().cancellingModels) {
            const message = i18n.t("onboarding.cancelPending");
            set({ error: message });
            toast.error(message);
            return false;
          }
        }
        // A new attempt clears any stall marker a previous attempt left behind.
        stallFailedDownloads.delete(modelId);
        set(
          produce((state) => {
            delete state.cancellingModels[modelId];
            state.downloadingModels[modelId] = true;
            state.downloadProgress[modelId] = {
              model_id: modelId,
              downloaded: 0,
              total: 0,
              percentage: 0,
            };
          }),
        );
        resetStallTimer(modelId);
        const result = await commands.downloadModel(modelId);
        if (result.status !== "ok") {
          if (result.error === MODEL_DOWNLOAD_ALREADY_ACTIVE) {
            // A duplicate may be rejoining a worker whose byte phase already
            // ended. Do not let this caller's speculative timer cancel healthy
            // verification or extraction; a later byte-progress event will
            // re-arm the timer if the worker is still downloading bytes.
            clearStallTimer(modelId);
            await get().loadModels();
            return true;
          }
          if (result.error === MODEL_DOWNLOAD_CANCELLING) {
            const message = localizedDownloadError(result.error);
            clearStallTimer(modelId);
            set(
              produce((state) => {
                state.cancellingModels[modelId] = true;
                delete state.downloadingModels[modelId];
                delete state.verifyingModels[modelId];
                delete state.extractingModels[modelId];
                delete state.downloadProgress[modelId];
                delete state.downloadStats[modelId];
                state.error = message;
              }),
            );
            return false;
          }
          // Fallback cleanup in case the model-download-failed event was not received
          // (e.g. listener not yet registered). The event handler is a no-op if it
          // arrives after this cleanup since deleting missing keys is safe.
          clearStallTimer(modelId);
          set(
            produce((state) => {
              delete state.cancellingModels[modelId];
              delete state.downloadingModels[modelId];
              delete state.verifyingModels[modelId];
              delete state.extractingModels[modelId];
              delete state.downloadProgress[modelId];
              delete state.downloadStats[modelId];
            }),
          );
        }
        // The stall timer may have declared this download failed and cancelled
        // it while the await was pending; the cancelled backend task resolves
        // ok, so convert the result back to failure for awaiting callers.
        if (result.status === "ok" && stallFailedDownloads.delete(modelId)) {
          return false;
        }
        return result.status === "ok";
      } catch {
        // model-download-failed event won't fire for JS exceptions (e.g. IPC error),
        // so clean up state here to avoid a stuck progress spinner.
        clearStallTimer(modelId);
        set(
          produce((state) => {
            delete state.cancellingModels[modelId];
            delete state.downloadingModels[modelId];
            delete state.verifyingModels[modelId];
            delete state.extractingModels[modelId];
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
          clearStallTimer(modelId);
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

          // No immediate loadModels() here: the backend task still owns
          // is_downloading until it observes the cancel flag and exits, so a
          // reload now would merge the still-active download back into
          // downloadingModels and re-stick the spinner with no terminal event
          // left to clear it. State was already cleared above and by the
          // model-download-cancelled event; the next natural loadModels
          // reconciles with backend truth.
          return true;
        } else if (result.error === MODEL_DOWNLOAD_NOT_ACTIVE) {
          // The backend has no task to cancel, so the frontend marker is stale.
          // Clear it before reloading; loadModels intentionally preserves a
          // marker that still has frontend progress.
          clearStallTimer(modelId);
          set(
            produce((state) => {
              delete state.cancellingModels[modelId];
              delete state.downloadingModels[modelId];
              delete state.verifyingModels[modelId];
              delete state.extractingModels[modelId];
              delete state.downloadProgress[modelId];
              delete state.downloadStats[modelId];
            }),
          );
          await get().loadModels();
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
        if (progress.model_id in get().cancellingModels) {
          return;
        }
        resetStallTimer(progress.model_id);
        set(
          produce((state) => {
            state.downloadProgress[progress.model_id] = progress;
          }),
        );

        // Update download stats for speed calculation
        const now = Date.now();
        set(
          produce((state) => {
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
        clearStallTimer(modelId);
        set(
          produce((state) => {
            delete state.cancellingModels[modelId];
            delete state.downloadingModels[modelId];
            delete state.verifyingModels[modelId];
            delete state.extractingModels[modelId];
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
          if (error === MODEL_DOWNLOAD_ALREADY_ACTIVE) {
            // The original task still owns this model. Reconcile backend state
            // without applying terminal cleanup from the rejected duplicate.
            get().loadModels();
            return;
          }
          const failureAlreadyReported = stallFailedDownloads.delete(modelId);
          const message = localizedDownloadError(error);
          clearStallTimer(modelId);
          set(
            produce((state) => {
              if (error === MODEL_DOWNLOAD_CANCELLING) {
                state.cancellingModels[modelId] = true;
              } else {
                delete state.cancellingModels[modelId];
              }
              delete state.downloadingModels[modelId];
              delete state.verifyingModels[modelId];
              delete state.extractingModels[modelId];
              delete state.downloadProgress[modelId];
              delete state.downloadStats[modelId];
              if (!failureAlreadyReported) {
                state.error = message;
              }
            }),
          );
          if (!failureAlreadyReported) {
            toast.error(message);
          }
        },
      );

      listen<string>("model-verification-started", (event) => {
        const modelId = event.payload;
        // Byte download is done; verification and extraction emit no progress
        // events and can exceed the stall timeout, so the timer stops here.
        // It stays armed only if this event itself is dropped.
        clearStallTimer(modelId);
        set(
          produce((state) => {
            if (state.cancellingModels[modelId]) {
              return;
            }
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
        // Same as verification: the stall timer only guards the byte stream.
        clearStallTimer(modelId);
        set(
          produce((state) => {
            if (state.cancellingModels[modelId]) {
              return;
            }
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
        clearStallTimer(modelId);
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
