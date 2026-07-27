import { describe, expect, it } from "bun:test";
import { bandsToAmplitude, smoothAmplitude } from "./mic-level";

describe("bandsToAmplitude", () => {
  it("is silent for a missing or empty frame", () => {
    expect(bandsToAmplitude(undefined)).toBe(0);
    expect(bandsToAmplitude(null)).toBe(0);
    expect(bandsToAmplitude([])).toBe(0);
  });

  it("follows the loudest band, not the average", () => {
    // A voice concentrated in one band still has to read as loud, which is the
    // whole reason this reduces by peak.
    const oneLoudBand = [0, 0, 0, 0, 0, 0, 0, 0.5, 0, 0, 0, 0, 0, 0, 0, 0];
    expect(bandsToAmplitude(oneLoudBand)).toBeCloseTo(0.7, 5);
  });

  it("clamps to 1 rather than overshooting past full", () => {
    // The 1.4 gain means anything above ~0.72 saturates; a critter scaled past
    // 1 would render outside its own body.
    expect(bandsToAmplitude([0.9])).toBe(1);
    expect(bandsToAmplitude([1])).toBe(1);
  });

  it("treats a malformed frame as silence instead of throwing", () => {
    // The frame crosses a process boundary. A NaN that propagated into a
    // transform would blank the overlay webview mid-dictation.
    expect(bandsToAmplitude([Number.NaN, Number.NaN])).toBe(0);
    expect(bandsToAmplitude([Number.POSITIVE_INFINITY])).toBe(0);
    expect(bandsToAmplitude([-0.5, -1])).toBe(0);
  });

  it("ignores unusable entries but still reads the usable ones", () => {
    expect(bandsToAmplitude([Number.NaN, 0.5, Number.NaN])).toBeCloseTo(0.7, 5);
  });
});

describe("smoothAmplitude", () => {
  it("rises faster than it falls", () => {
    // Fast attack, slow release is what makes a level read as a voice. Compare
    // equal-size steps in each direction from the same midpoint.
    const rise = smoothAmplitude(0.5, 0.9) - 0.5;
    const fall = 0.5 - smoothAmplitude(0.5, 0.1);
    expect(rise).toBeGreaterThan(fall);
  });

  it("moves toward the target from either direction", () => {
    expect(smoothAmplitude(0, 1)).toBeGreaterThan(0);
    expect(smoothAmplitude(0, 1)).toBeLessThan(1);
    expect(smoothAmplitude(1, 0)).toBeLessThan(1);
    expect(smoothAmplitude(1, 0)).toBeGreaterThan(0);
  });

  it("holds still when it is already at the target", () => {
    expect(smoothAmplitude(0, 0)).toBe(0);
    expect(smoothAmplitude(1, 1)).toBe(1);
  });

  it("releases to visual rest in a bounded number of ticks", () => {
    // useMicLevel drives this with target 0 once frames stop, and snaps to exact
    // 0 below 0.01. If the release were too slow the critter would sit visibly
    // half-inflated after dictation; this pins the decay so a coefficient change
    // cannot quietly regress that.
    let value = 1;
    let ticks = 0;
    while (value >= 0.01 && ticks < 100) {
      value = smoothAmplitude(value, 0);
      ticks += 1;
    }
    expect(value).toBeLessThan(0.01);
    expect(ticks).toBeLessThanOrEqual(17); // ~1.4s at the 80ms release tick
  });

  it("never returns a value outside 0..1, even fed garbage", () => {
    for (const [prev, target] of [
      [Number.NaN, 0.5],
      [0.5, Number.NaN],
      [-1, 2],
      [2, -1],
    ] as const) {
      const result = smoothAmplitude(prev, target);
      expect(result).toBeGreaterThanOrEqual(0);
      expect(result).toBeLessThanOrEqual(1);
    }
  });
});
