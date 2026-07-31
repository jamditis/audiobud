import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";

const runbook = readFileSync("docs/updater-signing.md", "utf8");
const workflow = readFileSync(".github/workflows/release.yml", "utf8");

describe("updater signing key operations", () => {
  test("documents the secret names and encrypted backup pointers", () => {
    for (const name of [
      "TAURI_SIGNING_PRIVATE_KEY",
      "TAURI_SIGNING_PRIVATE_KEY_PASSWORD",
    ]) {
      expect(runbook).toContain(name);
      expect(workflow).toContain(`secrets.${name}`);
    }
    expect(runbook).toContain("claude/audiobud/updater-private-key");
    expect(runbook).toContain("claude/audiobud/updater-private-key-password");
  });

  test("covers planned rotation and confirmed compromise separately", () => {
    expect(runbook).toContain("## Planned rotation");
    expect(runbook).toContain("## Suspected or confirmed compromise");
    expect(runbook).toContain("old key");
    expect(runbook).toContain("new public key");
    expect(runbook).toContain("latest.json");
    expect(runbook).toContain("Authenticode");
  });

  test("never suggests printing secret material", () => {
    expect(runbook).not.toMatch(/echo\s+\$?TAURI_SIGNING_PRIVATE_KEY/);
    expect(runbook).not.toMatch(/gh secret set[^\n]+--body/);
  });
});
