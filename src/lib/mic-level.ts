// The backend emits a `mic-level` event carrying 16 spectrum bands in 0..1
// (src-tauri/src/audio_toolkit/audio/visualizer.rs). It fires during the real
// recording stream and while the settings mic monitor is open, so anything that
// wants to animate with the live voice listens to this one event.
//
// Three surfaces read it: the settings meter (InputLevelMeter), the recording
// overlay's bars and critter, and the menu wordmark's critter (useMicLevel).
// The reduction to a single amplitude and the smoothing curve live here so the
// critter feels the same in the overlay and in the menu window instead of
// drifting apart in two copies. Pure functions, so the feel is unit-testable
// without a webview, a mic, or Tauri.

export const MIC_LEVEL_EVENT = "mic-level";

/** Bands are 0..1; the peak is lifted by this much so ordinary speech reads. */
const AMPLITUDE_GAIN = 1.4;

/** Smoothing weight applied to the previous value when the level is rising. */
const ATTACK_WEIGHT = 0.4;

/** Smoothing weight applied to the previous value when the level is falling. */
const RELEASE_WEIGHT = 0.75;

function clamp01(value: number): number {
  if (!Number.isFinite(value)) return 0;
  return Math.min(1, Math.max(0, value));
}

/**
 * Reduce a spectrum frame to one 0..1 amplitude, using the loudest band so a
 * voice concentrated in a few bands still reads as loud. A missing, empty, or
 * non-numeric frame is silence rather than an error: the event arrives from a
 * separate process, and a critter that throws on a malformed frame would take
 * the overlay's webview down mid-dictation.
 */
export function bandsToAmplitude(
  bands: readonly number[] | null | undefined,
): number {
  if (!bands || bands.length === 0) return 0;

  let peak = 0;
  for (const band of bands) {
    if (Number.isFinite(band) && band > peak) peak = band;
  }

  return clamp01(peak * AMPLITUDE_GAIN);
}

/**
 * Fast attack, slow release — the curve the settings meter already uses, which
 * is what makes a level read as a voice rather than as flicker. Rising jumps
 * most of the way to the target so a syllable lands on time; falling eases back
 * so the critter does not snap shut between words.
 */
export function smoothAmplitude(previous: number, target: number): number {
  const to = clamp01(target);
  const from = clamp01(previous);
  const weight = to > from ? ATTACK_WEIGHT : RELEASE_WEIGHT;
  return from * weight + to * (1 - weight);
}
