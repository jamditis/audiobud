import { useCallback, useEffect, useMemo, useState } from "react";
import { commands, events, type OutputTargetLockEvent } from "@/bindings";
import {
  deriveIndicator,
  type DeriveOptions,
  type LockSnapshot,
  type TargetIndicator,
} from "@/lib/output-target-indicator";

/**
 * Translate the backend's event/command payload (#255) into the frontend
 * indicator core's snapshot type. The two shapes carry the same fields; only
 * `null` (specta's mapping for `Option<String>`) vs. `undefined` (the core's
 * optional-property convention) differ, so this is the one place that
 * reconciles them rather than every caller re-deriving it.
 */
export function toSnapshot(event: OutputTargetLockEvent): LockSnapshot {
  if (event.kind === "unlocked") {
    return { kind: "unlocked" };
  }
  return {
    kind: event.kind,
    app: event.app ?? undefined,
    title: event.title ?? undefined,
  };
}

export interface UseOutputTargetLockResult {
  /** The view-model to render -- identical across every surface that calls this. */
  readonly indicator: TargetIndicator;
  /**
   * Release the lock. Safe to call for a `stale` indicator too: the backend
   * has already dropped that lock by the time the indicator can show it, so
   * this dismisses the local latch and the backend call is a no-op.
   */
  readonly unlock: () => void;
}

/**
 * Wires the initial snapshot read against the live event stream without
 * pulling in React, so the ordering guarantee is unit-testable on its own
 * (#266 review): `query()` and `subscribe()` both cross the Tauri IPC bridge
 * asynchronously, so firing them at the same time races -- a slow initial
 * read can resolve after a newer event and clobber it with a stale snapshot.
 * `subscribe` is awaited before `query` fires, closing most of that gap, and
 * a `receivedEvent` flag discards the initial read outright once any event
 * has arrived, closing the rest: a snapshot from `query` is only ever applied
 * if no event beat it there.
 *
 * `subscribe` mirrors `events.outputTargetLockEvent.listen`'s shape --
 * `(onEvent) => Promise<UnlistenFn>` -- so the hook can pass that function
 * directly.
 *
 * Returns a cleanup function; nothing here is React-specific.
 */
export function subscribeToOutputTargetLock(
  query: () => Promise<OutputTargetLockEvent>,
  subscribe: (
    onEvent: (event: OutputTargetLockEvent) => void,
  ) => Promise<() => void>,
  onSnapshot: (snapshot: LockSnapshot) => void,
): () => void {
  let cancelled = false;
  let receivedEvent = false;

  const unlistenPromise = subscribe((event) => {
    receivedEvent = true;
    if (!cancelled) onSnapshot(toSnapshot(event));
  });

  unlistenPromise
    .then(() =>
      query().then((event) => {
        if (!cancelled && !receivedEvent) onSnapshot(toSnapshot(event));
      }),
    )
    .catch(() => {
      // No Tauri bridge available (e.g. a browser-only test render), or the
      // listener itself failed to register: stay on the default snapshot
      // rather than throw.
    });

  return () => {
    cancelled = true;
    unlistenPromise.then((unlisten) => unlisten()).catch(() => {});
  };
}

/**
 * Subscribe to the output-target lock snapshot (#255: a command for the
 * state on mount, an event for every change after -- ordered by
 * `subscribeToOutputTargetLock` so a slow initial read can never overwrite a
 * newer event, #266 review) and derive the single indicator view-model every
 * surface -- the recording overlay, the tray via the backend, and settings --
 * renders through `deriveIndicator`. Because they all derive from the same
 * snapshot through the same pure function, they cannot disagree about what is
 * locked.
 *
 * A `lost` snapshot can also arrive from the initial command read, not only
 * the event (see `output_target.rs`'s `OutputTargetLockEvent` doc, #266
 * review finding 1): the backend keeps its own memory of the last loss
 * (`LostLockNotice`) for exactly this reason -- a webview that mounts after
 * the one-shot event already fired (settings opened after the overlay showed
 * the stale target, say) still needs to see it, or it would silently
 * disagree with a surface that mounted earlier. Within one mounted session
 * the "stale" state is a latch this hook holds until `unlock` is called or a
 * new `locked`/`unlocked` event replaces it.
 */
export function useOutputTargetLock(
  options?: DeriveOptions,
): UseOutputTargetLockResult {
  const [snapshot, setSnapshot] = useState<LockSnapshot>({
    kind: "unlocked",
  });

  useEffect(
    () =>
      subscribeToOutputTargetLock(
        commands.getOutputTargetLock,
        (onEvent) =>
          events.outputTargetLockEvent.listen((event) =>
            onEvent(event.payload),
          ),
        setSnapshot,
      ),
    [],
  );

  const unlock = useCallback(() => {
    // Dismiss a stale latch immediately rather than wait on the round trip:
    // the backend already unlocked when the loss happened, so
    // releaseOutputTargetLock is a no-op for it and only confirms via the
    // authoritative "unlocked" event.
    setSnapshot((current) =>
      current.kind === "lost" ? { kind: "unlocked" } : current,
    );
    commands.releaseOutputTargetLock().catch(() => {
      // Best-effort: a failed round trip leaves the next snapshot event (or
      // the next mount's read) as the source of truth.
    });
  }, []);

  const indicator = useMemo(
    () => deriveIndicator(snapshot, options),
    [snapshot, options],
  );

  return { indicator, unlock };
}
