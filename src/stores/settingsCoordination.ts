type Unlisten = () => void;
type RegisterListener = (
  eventName: string,
  handler: () => void,
) => Promise<Unlisten>;

interface SettingsLifecycle {
  initialize: (
    load: () => Promise<void>,
    refresh: () => Promise<void>,
  ) => Promise<void>;
}

const SETTINGS_EVENTS = ["model-state-changed", "settings-changed"] as const;
const INITIALIZATION_ATTEMPTS = 3;
const INITIALIZATION_RETRY_DELAY_MS = 1_000;

export type RetryScheduler = (retry: () => void, delayMs: number) => void;

const scheduleRetry: RetryScheduler = (retry, delayMs) => {
  setTimeout(retry, delayMs);
};

export function initializeSettingsWithRetry(
  initialize: () => Promise<void>,
  schedule: RetryScheduler = scheduleRetry,
): Promise<void> {
  return new Promise((resolve, reject) => {
    let attempts = 0;
    let finished = false;

    const run = () => {
      if (finished || attempts >= INITIALIZATION_ATTEMPTS) {
        return;
      }
      attempts += 1;

      void Promise.resolve()
        .then(initialize)
        .then(
          () => {
            finished = true;
            resolve();
          },
          (error) => {
            if (attempts >= INITIALIZATION_ATTEMPTS) {
              finished = true;
              reject(error);
              return;
            }
            try {
              schedule(run, INITIALIZATION_RETRY_DELAY_MS);
            } catch (scheduleError) {
              finished = true;
              reject(scheduleError);
            }
          },
        );
    };

    run();
  });
}

export function createSettingsLifecycle(
  registerListener: RegisterListener,
): SettingsLifecycle {
  let initializationPromise: Promise<void> | null = null;

  const registerBackendListeners = async (refresh: () => Promise<void>) => {
    const registeredCleanups: Unlisten[] = [];

    try {
      for (const eventName of SETTINGS_EVENTS) {
        const unlisten = await registerListener(eventName, () => {
          void refresh().catch(() => {});
        });
        registeredCleanups.push(unlisten);
      }
    } catch (error) {
      registeredCleanups.reverse().forEach((unlisten) => unlisten());
      throw error;
    }
  };

  return {
    initialize(load, refresh) {
      if (initializationPromise) {
        return initializationPromise;
      }

      initializationPromise = (async () => {
        await load();
        await registerBackendListeners(refresh);
      })().catch((error) => {
        initializationPromise = null;
        throw error;
      });

      return initializationPromise;
    },
  };
}

export function mergePendingValues<T extends object>(
  current: T,
  pending: Partial<T>,
): T {
  return { ...current, ...pending };
}

interface KeyedSerialQueue {
  run: <T>(key: string, operation: () => Promise<T>) => Promise<T>;
  hasPending: (key: string) => boolean;
}

export function createKeyedSerialQueue(): KeyedSerialQueue {
  const tails = new Map<string, Promise<void>>();

  return {
    run<T>(key: string, operation: () => Promise<T>) {
      const previous = tails.get(key) ?? Promise.resolve();
      const result = previous.then(operation);
      const tail = result.then(
        () => undefined,
        () => undefined,
      );
      tails.set(key, tail);

      return result.finally(() => {
        if (tails.get(key) === tail) {
          tails.delete(key);
        }
      });
    },

    hasPending(key) {
      return tails.has(key);
    },
  };
}

interface PendingOptimisticWrite<T extends object> {
  latestRevision: number;
  hasConfirmedValues: boolean;
  confirmedValues: Partial<T>;
  optimisticValues: Partial<T>;
}

interface OptimisticWrite<T extends object> {
  key: string;
  hasConfirmedValues: boolean;
  confirmedValues: Partial<T>;
  optimisticValues: Partial<T>;
  persist: () => Promise<void>;
  apply: (values: Partial<T>) => void;
  setUpdating: (key: string, updating: boolean) => void;
  afterSuccess?: () => Promise<void>;
  onError?: (error: unknown, isLatest: boolean) => void;
  onSuccess?: () => void;
}

interface OptimisticWriteCoordinator<T extends object> {
  run: (write: OptimisticWrite<T>) => Promise<void>;
  pendingValues: () => Partial<T>;
}

export function createOptimisticWriteCoordinator<
  T extends object,
>(): OptimisticWriteCoordinator<T> {
  const queue = createKeyedSerialQueue();
  const pendingWrites = new Map<string, PendingOptimisticWrite<T>>();

  return {
    async run({
      key,
      hasConfirmedValues,
      confirmedValues,
      optimisticValues,
      persist,
      apply,
      setUpdating,
      afterSuccess,
      onError,
      onSuccess,
    }) {
      const writeState = pendingWrites.get(key) ?? {
        latestRevision: 0,
        hasConfirmedValues,
        confirmedValues,
        optimisticValues,
      };
      const revision = writeState.latestRevision + 1;
      writeState.latestRevision = revision;
      writeState.optimisticValues = optimisticValues;
      pendingWrites.set(key, writeState);

      setUpdating(key, true);

      try {
        apply(optimisticValues);
        await queue.run(key, async () => {
          try {
            await persist();
          } catch (error) {
            const isLatest = writeState.latestRevision === revision;
            if (isLatest && writeState.hasConfirmedValues) {
              apply(writeState.confirmedValues);
            }
            onError?.(error, isLatest);
            throw error;
          }

          writeState.confirmedValues = optimisticValues;
          if (writeState.latestRevision === revision) {
            apply(optimisticValues);
            await afterSuccess?.();
          }
          onSuccess?.();
        });
      } finally {
        if (writeState.latestRevision === revision && !queue.hasPending(key)) {
          setUpdating(key, false);
          if (pendingWrites.get(key) === writeState) {
            pendingWrites.delete(key);
          }
        }
      }
    },

    pendingValues() {
      let values: Partial<T> = {};
      for (const writeState of pendingWrites.values()) {
        values = mergePendingValues(values, writeState.optimisticValues);
      }
      return values;
    },
  };
}
