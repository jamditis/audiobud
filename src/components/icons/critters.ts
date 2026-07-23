import FrogMascot from "./FrogMascot";
import type { CritterEntry } from "./mascot";

// The critter registry (#8). One entry per selectable character; the picker and
// the persisted `active_critter` setting come later, so today this holds the frog
// alone and everything that used to hardcode <FrogMascot /> resolves through it.
//
// Adding a critter is: drop an SVG component honoring MascotProps, add an entry
// here, answer `micLevel`. Nothing else changes.
export const CRITTERS: readonly CritterEntry[] = [
  {
    id: "frog",
    labelKey: "critters.frog",
    Component: FrogMascot,
    micLevel: "vocal-sac",
  },
];

/** The critter shown when nothing has been chosen, and the fallback below. */
export const DEFAULT_CRITTER_ID = "frog";

/**
 * Resolve a critter id to its entry, falling back to the default.
 *
 * The fallback is the point: the id will arrive from persisted settings, so it
 * can name a critter that was removed or was never in this build. A missing
 * mascot should never be a blank space where the frog was, and it should never
 * throw inside a render.
 */
export function getCritter(id: string | null | undefined): CritterEntry {
  const found = CRITTERS.find((c) => c.id === id);
  if (found) return found;
  const fallback = CRITTERS.find((c) => c.id === DEFAULT_CRITTER_ID);
  if (!fallback) {
    // Only reachable if DEFAULT_CRITTER_ID is edited out of sync with CRITTERS.
    throw new Error(
      `critter registry has no "${DEFAULT_CRITTER_ID}" entry to fall back to; ` +
        `CRITTERS has: ${CRITTERS.map((c) => c.id).join(", ") || "(empty)"}`,
    );
  }
  return fallback;
}
