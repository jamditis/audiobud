import { describe, it, expect } from "bun:test";
import { readFileSync } from "node:fs";
import {
  RELEASES_URL,
  UPDATER_FEED_READY,
  updateChecksActive,
} from "./updater";

describe("release links", () => {
  it("sends portable users to AudioBud's releases, not the fork's", () => {
    expect(RELEASES_URL).toBe(
      "https://github.com/jamditis/audiobud/releases/latest",
    );
  });

  // Attribution to cjpais belongs on the About page and in the locale files --
  // the MIT license requires it. What must never appear is a cjpais URL the
  // app *navigates to*, because the only such link was an installer download.
  // The dialog it lived in is dark until UPDATER_FEED_READY flips, so nothing
  // catches this at runtime today; it has to be caught here.
  it("routes no updater code path to the upstream repository", () => {
    const sources = [
      "src/lib/updater.ts",
      "src/components/update-checker/UpdateChecker.tsx",
    ];

    for (const path of sources) {
      const source = readFileSync(path, "utf8");
      const links = [...source.matchAll(/https?:\/\/[^\s"'`)]+/g)].map(
        ([url]) => url,
      );

      for (const url of links) {
        expect(url, `${path} links at the upstream fork`).not.toMatch(
          /cjpais|handy\.computer|\/Handy\b/i,
        );
      }
    }
  });
});

describe("updateChecksActive", () => {
  // Milestone A: the updater endpoint still points at upstream Handy (see
  // superpowers/DEFERRED-issues.md "Provenance"). Update checks must never
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
