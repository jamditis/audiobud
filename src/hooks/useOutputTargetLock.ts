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
 * Subscribe to the output-target lock snapshot (#255: a command for the
 * state on mount, an event for every change after) and derive the single
 * indicator view-model every surface -- the recording overlay, the tray via
 * the backend, and settings -- renders through `deriveIndicator`. Because
 * they all derive from the same snapshot through the same pure function, they
 * cannot disagree about what is locked.
 *
 * A `lost` snapshot only ever arrives over the event, never the initial
 * command read (see `output_target.rs`'s `OutputTargetLockEvent` doc): the
 * lock is already cleared by the time the backend reports it, so a mount
 * that happens after a loss reads `unlocked`. Within one mounted session the
 * "stale" state is a latch this hook holds until `unlock` is called or a new
 * `locked`/`unlocked` event replaces it.
 */
export function useOutputTargetLock(
  options?: DeriveOptions,
): UseOutputTargetLockResult {
  const [snapshot, setSnapshot] = useState<LockSnapshot>({
    kind: "unlocked",
  });

  useEffect(() => {
    let cancelled = false;

    commands
      .getOutputTargetLock()
      .then((event) => {
        if (!cancelled) setSnapshot(toSnapshot(event));
      })
      .catch(() => {
        // No Tauri bridge available (e.g. a browser-only test render): stay
        // unlocked rather than throw.
      });

    const unlistenPromise = events.outputTargetLockEvent.listen((event) => {
      setSnapshot(toSnapshot(event.payload));
    });

    return () => {
      cancelled = true;
      unlistenPromise.then((unlisten) => unlisten()).catch(() => {});
    };
  }, []);

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
