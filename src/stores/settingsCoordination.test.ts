import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import {
  createKeyedSerialQueue,
  createOptimisticWriteCoordinator,
  createSettingsLifecycle,
  initializeSettingsWithRetry,
  mergePendingValues,
} from "./settingsCoordination";

function deferred() {
  let resolve!: () => void;
  const promise = new Promise<void>((resolvePromise) => {
    resolve = resolvePromise;
  });
  return { promise, resolve };
}

describe("settings lifecycle", () => {
  test("shares one initialization and registers each backend listener once", async () => {
    const loadGate = deferred();
    const handlers = new Map<string, () => void>();
    const registeredEvents: string[] = [];
    let loadCalls = 0;
    let refreshCalls = 0;

    const lifecycle = createSettingsLifecycle(async (eventName, handler) => {
      registeredEvents.push(eventName);
      handlers.set(eventName, handler);
      return () => {};
    });
    const load = async () => {
      loadCalls += 1;
      await loadGate.promise;
    };
    const refresh = async () => {
      refreshCalls += 1;
    };

    const first = lifecycle.initialize(load, refresh);
    const second = lifecycle.initialize(load, refresh);
    const third = lifecycle.initialize(load, refresh);

    expect(first).toBe(second);
    expect(second).toBe(third);
    expect(loadCalls).toBe(1);
    expect(handlers.size).toBe(0);

    loadGate.resolve();
    await Promise.all([first, second, third]);
    await lifecycle.initialize(load, refresh);

    expect(loadCalls).toBe(1);
    expect(registeredEvents.sort()).toEqual([
      "model-state-changed",
      "settings-changed",
    ]);

    handlers.get("settings-changed")?.();
    await Promise.resolve();
    expect(refreshCalls).toBe(1);

    expect("dispose" in lifecycle).toBe(false);
  });

  test("retries after a required initialization load fails", async () => {
    let loadAttempts = 0;
    let listenerRegistrations = 0;
    const lifecycle = createSettingsLifecycle(async () => {
      listenerRegistrations += 1;
      return () => {};
    });

    const load = async () => {
      loadAttempts += 1;
      if (loadAttempts === 1) {
        throw new Error("required settings load failed");
      }
    };

    await expect(lifecycle.initialize(load, async () => {})).rejects.toThrow(
      "required settings load failed",
    );
    await lifecycle.initialize(load, async () => {});

    expect(loadAttempts).toBe(2);
    expect(listenerRegistrations).toBe(2);
  });

  test("cleans up a partial listener registration before retry", async () => {
    const cleanedUp: string[] = [];
    let failSettingsListener = true;
    const lifecycle = createSettingsLifecycle(async (eventName) => {
      if (eventName === "settings-changed" && failSettingsListener) {
        failSettingsListener = false;
        throw new Error("listener registration failed");
      }
      return () => cleanedUp.push(eventName);
    });

    await expect(
      lifecycle.initialize(
        async () => {},
        async () => {},
      ),
    ).rejects.toThrow("listener registration failed");
    expect(cleanedUp).toEqual(["model-state-changed"]);

    await lifecycle.initialize(
      async () => {},
      async () => {},
    );
  });

  test("automatically retries a failed required load after one second", async () => {
    const delays: number[] = [];
    let attempts = 0;

    await initializeSettingsWithRetry(
      async () => {
        attempts += 1;
        if (attempts === 1) {
          throw new Error("required load failed");
        }
      },
      (retry, delayMs) => {
        delays.push(delayMs);
        retry();
      },
    );

    expect(attempts).toBe(2);
    expect(delays).toEqual([1_000]);
  });

  test("retries listener registration without leaving duplicate listeners", async () => {
    const activeListeners = new Set<string>();
    let settingsListenerAttempts = 0;
    const lifecycle = createSettingsLifecycle(async (eventName) => {
      if (eventName === "settings-changed") {
        settingsListenerAttempts += 1;
        if (settingsListenerAttempts === 1) {
          throw new Error("listener registration failed");
        }
      }
      activeListeners.add(eventName);
      return () => activeListeners.delete(eventName);
    });

    await initializeSettingsWithRetry(
      () =>
        lifecycle.initialize(
          async () => {},
          async () => {},
        ),
      (retry) => retry(),
    );

    expect(settingsListenerAttempts).toBe(2);
    expect([...activeListeners].sort()).toEqual([
      "model-state-changed",
      "settings-changed",
    ]);
  });

  test("stops after three failed initialization attempts", async () => {
    const delays: number[] = [];
    let attempts = 0;

    await expect(
      initializeSettingsWithRetry(
        async () => {
          attempts += 1;
          throw new Error("startup failed");
        },
        (retry, delayMs) => {
          delays.push(delayMs);
          retry();
        },
      ),
    ).rejects.toThrow("startup failed");

    expect(attempts).toBe(3);
    expect(delays).toEqual([1_000, 1_000]);
  });
});

describe("settings write queue", () => {
  test("persists the newest value last when completion gates resolve in reverse", async () => {
    const queue = createKeyedSerialQueue();
    const olderGate = deferred();
    const newerGate = deferred();
    const started: number[] = [];
    let storedValue = 0;

    const olderWrite = queue.run("audio_feedback_volume", async () => {
      started.push(25);
      await olderGate.promise;
      storedValue = 25;
    });
    const newerWrite = queue.run("audio_feedback_volume", async () => {
      started.push(75);
      await newerGate.promise;
      storedValue = 75;
    });

    await Promise.resolve();
    expect(started).toEqual([25]);
    expect(queue.hasPending("audio_feedback_volume")).toBe(true);

    newerGate.resolve();
    olderGate.resolve();
    await Promise.all([olderWrite, newerWrite]);

    expect(started).toEqual([25, 75]);
    expect(storedValue).toBe(75);
    expect(queue.hasPending("audio_feedback_volume")).toBe(false);
  });

  test("does not block writes to different settings", async () => {
    const queue = createKeyedSerialQueue();
    const volumeGate = deferred();
    const started: string[] = [];

    const volumeWrite = queue.run("audio_feedback_volume", async () => {
      started.push("volume");
      await volumeGate.promise;
    });
    const delayWrite = queue.run("paste_delay_ms", async () => {
      started.push("delay");
    });

    await delayWrite;
    expect(started).toEqual(["volume", "delay"]);
    expect(queue.hasPending("audio_feedback_volume")).toBe(true);
    expect(queue.hasPending("paste_delay_ms")).toBe(false);

    volumeGate.resolve();
    await volumeWrite;
  });

  test("rejects a failed write and still runs the next write for that setting", async () => {
    const queue = createKeyedSerialQueue();
    const writes: string[] = [];

    const failed = queue.run("audio_feedback_volume", async () => {
      writes.push("failed");
      throw new Error("backend write failed");
    });
    const retry = queue.run("audio_feedback_volume", async () => {
      writes.push("retry");
    });

    await expect(failed).rejects.toThrow("backend write failed");
    await retry;
    expect(writes).toEqual(["failed", "retry"]);
  });

  test("serializes all overlay position operations through one key", async () => {
    const queue = createKeyedSerialQueue();
    const positionGate = deferred();
    const writes: string[] = [];

    const position = queue.run("overlay_position", async () => {
      writes.push("position");
      await positionGate.promise;
    });
    const anchor = queue.run("overlay_position", async () => {
      writes.push("anchor");
    });
    const reset = queue.run("overlay_position", async () => {
      writes.push("reset");
    });

    await Promise.resolve();
    expect(writes).toEqual(["position"]);

    positionGate.resolve();
    await Promise.all([position, anchor, reset]);
    expect(writes).toEqual(["position", "anchor", "reset"]);
  });
});

describe("optimistic settings writes", () => {
  test("rolls back the newest failure, clears updating, and rejects", async () => {
    const coordinator = createOptimisticWriteCoordinator<{
      volume: number;
    }>();
    let settings = { volume: 0.25 };
    const updating: boolean[] = [];

    const operation = coordinator.run({
      key: "volume",
      hasConfirmedValues: true,
      confirmedValues: { volume: 0.25 },
      optimisticValues: { volume: 0.75 },
      persist: async () => {
        throw new Error("backend write failed");
      },
      apply: (patch) => {
        settings = { ...settings, ...patch };
      },
      setUpdating: (_key, value) => updating.push(value),
    });

    expect(settings.volume).toBe(0.75);
    await expect(operation).rejects.toThrow("backend write failed");
    expect(settings.volume).toBe(0.25);
    expect(updating).toEqual([true, false]);
    expect(coordinator.pendingValues()).toEqual({});
  });

  test("keeps the newest optimistic value when an older write fails", async () => {
    const coordinator = createOptimisticWriteCoordinator<{
      volume: number;
    }>();
    const olderGate = deferred();
    let settings = { volume: 0.25 };
    const updating: boolean[] = [];

    const older = coordinator.run({
      key: "volume",
      hasConfirmedValues: true,
      confirmedValues: { volume: 0.25 },
      optimisticValues: { volume: 0.5 },
      persist: async () => {
        await olderGate.promise;
        throw new Error("older write failed");
      },
      apply: (patch) => {
        settings = { ...settings, ...patch };
      },
      setUpdating: (_key, value) => updating.push(value),
    });
    void older.catch(() => {});
    const newer = coordinator.run({
      key: "volume",
      hasConfirmedValues: true,
      confirmedValues: { volume: 0.5 },
      optimisticValues: { volume: 0.75 },
      persist: async () => {},
      apply: (patch) => {
        settings = { ...settings, ...patch };
      },
      setUpdating: (_key, value) => updating.push(value),
    });

    expect(settings.volume).toBe(0.75);
    expect(coordinator.pendingValues()).toEqual({ volume: 0.75 });

    olderGate.resolve();
    await expect(older).rejects.toThrow("older write failed");
    expect(settings.volume).toBe(0.75);
    await newer;

    expect(settings.volume).toBe(0.75);
    expect(updating).toEqual([true, true, false]);
    expect(coordinator.pendingValues()).toEqual({});
  });

  test("preserves a pending optimistic value during a backend refresh", async () => {
    const coordinator = createOptimisticWriteCoordinator<{
      volume: number;
      delay: number;
    }>();
    const persistGate = deferred();
    let settings = { volume: 0.25, delay: 40 };

    const write = coordinator.run({
      key: "volume",
      hasConfirmedValues: true,
      confirmedValues: { volume: 0.25 },
      optimisticValues: { volume: 0.75 },
      persist: () => persistGate.promise,
      apply: (patch) => {
        settings = { ...settings, ...patch };
      },
      setUpdating: () => {},
    });

    settings = mergePendingValues(
      { volume: 0.25, delay: 60 },
      coordinator.pendingValues(),
    );
    expect(settings).toEqual({ volume: 0.75, delay: 60 });

    persistGate.resolve();
    await write;
    expect(coordinator.pendingValues()).toEqual({});
  });

  test("runs overlay position, anchor, and reset writes through shared state", async () => {
    type OverlayValues = {
      position: string;
      anchor: string | null;
    };
    const coordinator = createOptimisticWriteCoordinator<OverlayValues>();
    const positionGate = deferred();
    const persisted: string[] = [];
    let settings: OverlayValues = { position: "bottom", anchor: null };
    const run = (
      optimisticValues: Partial<OverlayValues>,
      persist: () => Promise<void>,
    ) =>
      coordinator.run({
        key: "overlay_position",
        hasConfirmedValues: true,
        confirmedValues: settings,
        optimisticValues,
        persist,
        apply: (patch) => {
          settings = { ...settings, ...patch };
        },
        setUpdating: () => {},
      });

    const position = run({ position: "top", anchor: null }, async () => {
      persisted.push("position");
      await positionGate.promise;
    });
    const anchor = run({ position: "top", anchor: "topleft" }, async () => {
      persisted.push("anchor");
    });
    const reset = run({ position: "bottom", anchor: null }, async () => {
      persisted.push("reset");
    });

    await Promise.resolve();
    expect(settings).toEqual({ position: "bottom", anchor: null });
    expect(persisted).toEqual(["position"]);

    positionGate.resolve();
    await Promise.all([position, anchor, reset]);
    expect(persisted).toEqual(["position", "anchor", "reset"]);
    expect(settings).toEqual({ position: "bottom", anchor: null });
    expect(coordinator.pendingValues()).toEqual({});
  });
});

describe("settings refresh ordering", () => {
  test("serializes refreshes so they apply in request order", async () => {
    const queue = createKeyedSerialQueue();
    const olderGate = deferred();
    const applied: string[] = [];

    const older = queue.run("settings-refresh", async () => {
      await olderGate.promise;
      applied.push("older");
    });
    const newer = queue.run("settings-refresh", async () => {
      applied.push("newer");
    });

    await Promise.resolve();
    expect(applied).toEqual([]);
    olderGate.resolve();
    await Promise.all([older, newer]);
    expect(applied).toEqual(["older", "newer"]);
  });

  test("keeps pending optimistic values over a backend refresh", () => {
    const refreshed = {
      audio_feedback_volume: 0.25,
      paste_delay_ms: 60,
    };

    expect(
      mergePendingValues(refreshed, { audio_feedback_volume: 0.75 }),
    ).toEqual({
      audio_feedback_volume: 0.75,
      paste_delay_ms: 60,
    });
  });
});

describe("settings hook subscription", () => {
  test("subscribes to updating state and handles ignored rejected writes", () => {
    const source = readFileSync(
      new URL("../hooks/useSettings.ts", import.meta.url),
      "utf8",
    );

    expect(source).toContain("isUpdating: state.isUpdating");
    expect(source).toContain("void operation.catch");
  });

  test("routes each overlay write through the shared position queue", () => {
    const source = readFileSync(
      new URL("./settingsStore.ts", import.meta.url),
      "utf8",
    );

    expect(source).toContain("const updateKey = String(key);");
    expect(source.match(/const updateKey = "overlay_position";/g)?.length).toBe(
      2,
    );
  });

  test("starts the app with bounded retries and one final error log", () => {
    const source = readFileSync(
      new URL("../main.tsx", import.meta.url),
      "utf8",
    );

    expect(source).toContain("initializeSettingsWithRetry");
    expect(source.match(/Failed to initialize settings/g)?.length).toBe(1);
  });

  test("serializes backend settings refreshes", () => {
    const source = readFileSync(
      new URL("./settingsStore.ts", import.meta.url),
      "utf8",
    );

    expect(source).toContain("settingsRefreshQueue.run");
  });
});
