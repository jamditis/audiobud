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
  // Keep this as a source-level guard so future release-link changes cannot
  // silently route an installed AudioBud build back to the upstream fork.
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
  it("honors the user setting once the AudioBud feed is ready", () => {
    expect(UPDATER_FEED_READY).toBe(true);
    expect(updateChecksActive(true)).toBe(true);
    expect(updateChecksActive(false)).toBe(false);
    expect(updateChecksActive(undefined)).toBe(false);
  });

  it("pins AudioBud's public key and published-release endpoint", () => {
    const config = JSON.parse(
      readFileSync("src-tauri/tauri.conf.json", "utf8"),
    );
    const updater = config.plugins.updater;
    expect(updater.endpoints).toEqual([
      "https://github.com/jamditis/audiobud/releases/latest/download/latest.json",
    ]);
    const decodedPublicKey = Buffer.from(updater.pubkey, "base64").toString(
      "utf8",
    );
    expect(decodedPublicKey).toContain("minisign public key");
    expect(decodedPublicKey).toContain("A6C4E33D84B3A55F");
    expect(decodedPublicKey).not.toContain("PRIVATE KEY");
  });
});
