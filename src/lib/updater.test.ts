import { describe, it, expect } from "bun:test";
import { UPDATER_FEED_READY, updateChecksActive } from "./updater";

describe("updateChecksActive", () => {
  // Milestone A: the updater endpoint still points at upstream Handy (see
  // docs/superpowers/DEFERRED-issues.md "Provenance"). Update checks must never
  // run until the feed is repointed in milestone B, even if a stored or
  // optimistic setting says they are enabled. This guards the optimistic-toggle
  // path that bypasses the backend load gate.
  it("never reports active while the feed is upstream", () => {
    expect(UPDATER_FEED_READY).toBe(false);
    expect(updateChecksActive(true)).toBe(false);
    expect(updateChecksActive(false)).toBe(false);
    expect(updateChecksActive(undefined)).toBe(false);
  });
});
