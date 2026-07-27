import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import {
  MIC_LEVEL_EVENT,
  bandsToAmplitude,
  settleToRest,
  smoothAmplitude,
} from "@/lib/mic-level";

/** No frame for this long means the stream stopped, so start releasing to rest. */
const SILENCE_AFTER_MS = 120;

/** How often to apply the release once frames stop arriving. */
const RELEASE_TICK_MS = 80;

const REDUCE_MOTION_QUERY = "(prefers-reduced-motion: reduce)";

/**
 * The reduce-motion query, or null where there is no DOM to ask (a unit test).
 * A missing matchMedia is "no preference" rather than a crash.
 */
function reduceMotionQuery(): MediaQueryList | null {
  if (typeof window === "undefined") return null;
  if (typeof window.matchMedia !== "function") return null;
  return window.matchMedia(REDUCE_MOTION_QUERY);
}

/**
 * The OS reduce-motion preference, kept current if it changes mid-session. Read
 * once it would go stale for the life of the window, which for the wordmark is
 * the life of the app.
 *
 * Exported because a caller that drives a critter from its own `mic-level`
 * subscription rather than from useMicLevel (the recording overlay does, since
 * it needs the per-band values for its bars) has to honor the preference itself.
 * A CSS `prefers-reduced-motion` rule cannot cover that case: the amplitude
 * reaches the critter as inline style, which beats a stylesheet rule.
 */
export function usePrefersReducedMotion(): boolean {
  const [reduced, setReduced] = useState(
    () => reduceMotionQuery()?.matches ?? false,
  );

  useEffect(() => {
    const query = reduceMotionQuery();
    if (!query) return;

    const onChange = (event: MediaQueryListEvent) => setReduced(event.matches);
    query.addEventListener("change", onChange);
    // Re-read on mount in case it changed between the initial state and here.
    setReduced(query.matches);

    return () => query.removeEventListener("change", onChange);
  }, []);

  return reduced;
}

/**
 * Live input amplitude in 0..1, smoothed, for driving a critter's mic-level
 * visual (see MascotProps.sacScale). Returns exactly 0 at rest.
 *
 * `mic-level` only fires while the recording stream or the settings mic monitor
 * is open, and it stops without a final zero frame. A hook that only updated on
 * events would therefore freeze at whatever the last syllable measured and leave
 * the critter half-inflated for the rest of the session, so this releases to rest
 * on its own once frames stop. That is why a caller does not have to track
 * recording state to know when to stop animating.
 *
 * Stays at 0 under `prefers-reduced-motion`, so a caller can wire it
 * unconditionally and still honor the setting.
 *
 * @param enabled pass false to stay at rest and skip the subscription entirely.
 */
export function useMicLevel(enabled = true): number {
  const [amplitude, setAmplitude] = useState(0);
  const smoothedRef = useRef(0);
  const lastFrameAtRef = useRef(0);
  const reduceMotion = usePrefersReducedMotion();

  useEffect(() => {
    if (!enabled || reduceMotion) {
      smoothedRef.current = 0;
      setAmplitude(0);
      return;
    }

    let unlisten: (() => void) | undefined;
    let cancelled = false;

    // The rest snap lives here so both writers get it rather than one remembering.
    // "Exactly 0 at rest" is load-bearing: LiveFrog reads `amp > 0` to decide the
    // frog may croak again, and exponential smoothing approaches 0 without ever
    // arriving. A stream of zero-valued frames (the settings mic monitor left
    // open) also keeps refreshing lastFrameAtRef, so the release tick's silence
    // check never fires and cannot do the snapping on the event path's behalf.
    const apply = (next: number) => {
      const settled = settleToRest(next);
      smoothedRef.current = settled;
      setAmplitude(settled);
    };

    // Release to rest when frames stop. Without this the critter keeps the last
    // frame's pose indefinitely once dictation ends.
    const release = setInterval(() => {
      if (smoothedRef.current === 0) return;
      if (Date.now() - lastFrameAtRef.current < SILENCE_AFTER_MS) return;

      apply(smoothAmplitude(smoothedRef.current, 0));
    }, RELEASE_TICK_MS);

    const start = async () => {
      try {
        const stop = await listen<number[]>(MIC_LEVEL_EVENT, (event) => {
          lastFrameAtRef.current = Date.now();
          apply(
            smoothAmplitude(
              smoothedRef.current,
              bandsToAmplitude(event.payload),
            ),
          );
        });
        if (cancelled) {
          // Unmounted while awaiting the subscription: drop it immediately
          // rather than leaking a listener that outlives the component.
          stop();
          return;
        }
        unlisten = stop;
      } catch {
        // No event bridge available (non-Tauri context): stay at rest.
      }
    };

    start();

    return () => {
      cancelled = true;
      clearInterval(release);
      unlisten?.();
      smoothedRef.current = 0;
    };
  }, [enabled, reduceMotion]);

  return amplitude;
}
