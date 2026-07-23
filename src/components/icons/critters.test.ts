import { describe, expect, it } from "bun:test";
import { CRITTERS, DEFAULT_CRITTER_ID, getCritter } from "./critters";
import en from "../../i18n/locales/en/translation.json";

// The registry is what a second critter has to fit (#8 slice 1). These pin the
// two things that would silently break as critters are added: the default has to
// exist, and an id that is not in this build has to resolve to something.

describe("critter registry", () => {
  it("has an entry matching DEFAULT_CRITTER_ID", () => {
    expect(CRITTERS.some((c) => c.id === DEFAULT_CRITTER_ID)).toBe(true);
  });

  it("gives every critter a unique id", () => {
    const ids = CRITTERS.map((c) => c.id);
    expect(new Set(ids).size).toBe(ids.length);
  });

  it("gives every critter a labelKey that exists in the English locale", () => {
    // The registry stores a key, not a name, so a critter added without its
    // translation entry would render the raw key in the picker. Cheap to catch
    // here; invisible in a screenshot until someone reads it.
    const lookup = (key: string): unknown =>
      key.split(".").reduce<unknown>(
        (node, part) =>
          typeof node === "object" && node !== null
            ? (node as Record<string, unknown>)[part]
            : undefined,
        en,
      );
    for (const critter of CRITTERS) {
      expect(typeof lookup(critter.labelKey)).toBe("string");
    }
  });

  it("returns the same entry object for the same id", () => {
    // Identity, not equality. LiveFrog and the overlay use the resolved component
    // as an element type, so a registry that rebuilt entries per call would give
    // React a fresh type every render: the mascot remounts, useId regenerates its
    // gradient ids, and the vocal sac resets mid-recording. Silent and visual-only.
    expect(getCritter("frog")).toBe(getCritter("frog"));
  });

  it("resolves a known id to its own entry", () => {
    expect(getCritter(DEFAULT_CRITTER_ID).id).toBe(DEFAULT_CRITTER_ID);
  });

  it("falls back to the default for an id this build does not have", () => {
    // The id arrives from persisted settings, so it can name a critter that was
    // removed or never shipped here. That must not blank the mascot or throw
    // mid-render.
    for (const id of ["heron", "", null, undefined]) {
      expect(getCritter(id).id).toBe(DEFAULT_CRITTER_ID);
    }
  });
});
