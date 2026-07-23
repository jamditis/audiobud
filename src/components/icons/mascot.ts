import type { ComponentType } from "react";

// The contract every pond critter honors, so the "alive" behavior layer
// (LiveFrog) and the recording overlay can drive any of them without knowing
// which one they got. Extracted from FrogMascot for #8: today the frog is the
// only critter, but the shape is what a second one has to fit.
//
// Animatable parts stay props, so callers drive state:
//  - blink/wink/croak/lick toggle facial expressions (CSS classes in App.css)
//  - sacScale (0..1) drives the live mic-level visual, see MicLevelVisual below
//  - irisDX/irisDY translate the pupils to follow the cursor
export interface MascotProps {
  size?: number | string;
  className?: string;
  blink?: boolean;
  wink?: boolean;
  croak?: boolean;
  lick?: boolean;
  /** 0 = deflated/hidden, 1 = fully inflated. Overrides the croak class when set. */
  sacScale?: number;
  irisDX?: number;
  irisDY?: number;
}

/**
 * What a critter inflates, beats, or lights up when `sacScale` rises with the
 * live mic level. The frog has a throat sac; a turtle, dragonfly, or heron does
 * not, so this is part of the contract rather than something a second critter
 * discovers it broke.
 *
 * `"none"` is a real answer, not a gap. Declaring it is what stops a second
 * critter from silently breaking the recording meter: nothing else in the overlay
 * draws the mic level, so the first `"none"` critter is also the moment someone
 * has to decide what it shows instead.
 */
export type MicLevelVisual = "vocal-sac" | "none";

export interface CritterEntry {
  /** Stable id. Persisted as a settings value once the picker lands (#8 slice 3). */
  id: string;
  /** Human-facing name for the picker. */
  label: string;
  Component: ComponentType<MascotProps>;
  micLevel: MicLevelVisual;
}
