import { describe, it, expect } from "bun:test";
import { readFileSync } from "node:fs";
import { RELEASES_URL, updateChecksActive, updaterFeedReady } from "./updater";

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
  it("enables the published feed only on Windows", () => {
    expect(updaterFeedReady("windows")).toBe(true);
    expect(updaterFeedReady("macos")).toBe(false);
    expect(updaterFeedReady("linux")).toBe(false);
  });

  it("honors the user setting only on a supported platform", () => {
    expect(updateChecksActive(true, "windows", true)).toBe(true);
    expect(updateChecksActive(false, "windows", true)).toBe(false);
    expect(updateChecksActive(undefined, "windows", true)).toBe(false);
    expect(updateChecksActive(true, "windows", false)).toBe(false);
    expect(updateChecksActive(true, "macos", true)).toBe(false);
    expect(updateChecksActive(true, "linux", true)).toBe(false);
  });

  it("queries the installed package type before enabling updates", () => {
    const hook = readFileSync("src/hooks/useUpdateChannelAvailable.ts", "utf8");
    expect(hook).toMatch(/commands\s*\.isUpdateChannelAvailable\(\)/);

    for (const path of [
      "src/components/settings/UpdateChecksToggle.tsx",
      "src/components/update-checker/UpdateChecker.tsx",
    ]) {
      expect(readFileSync(path, "utf8")).toContain(
        "useUpdateChannelAvailable()",
      );
    }
  });

  it("refreshes the tray when the one-time updater migration runs", () => {
    const settings = readFileSync("src-tauri/src/settings.rs", "utf8");
    expect(settings).toMatch(/app\.emit\(\s*"settings-changed"/);
  });

  it("pins AudioBud's public key and published-release endpoint", () => {
    const config = JSON.parse(
      readFileSync("src-tauri/tauri.conf.json", "utf8"),
    );
    const updater = config.plugins.updater;
    expect(updater.endpoints).toEqual([
      "https://github.com/jamditis/audiobud/releases/download/update-feed/latest.json",
    ]);
    const decodedPublicKey = Buffer.from(updater.pubkey, "base64").toString(
      "utf8",
    );
    expect(decodedPublicKey).toContain("minisign public key");
    expect(decodedPublicKey).toContain("A6C4E33D84B3A55F");
    expect(decodedPublicKey).not.toContain("PRIVATE KEY");
  });

  it("exposes the Windows update opt-out in normal Advanced settings", () => {
    const advancedSettings = readFileSync(
      "src/components/settings/advanced/AdvancedSettings.tsx",
      "utf8",
    );
    expect(advancedSettings).toContain(
      'import { UpdateChecksToggle } from "../UpdateChecksToggle";',
    );
    expect(advancedSettings).toContain(
      '<UpdateChecksToggle descriptionMode="tooltip" grouped={true} />',
    );

    const toggle = readFileSync(
      "src/components/settings/UpdateChecksToggle.tsx",
      "utf8",
    );
    expect(toggle).toContain("updaterFeedReady(platform())");
    expect(toggle).toContain("if (!feedReady) return null;");

    const readme = readFileSync("README.md", "utf8");
    expect(readme).not.toContain("Automatic update checks remain disabled");
  });
});
