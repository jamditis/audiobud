import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";

const runbook = readFileSync("docs/updater-signing.md", "utf8");
const workflow = readFileSync(".github/workflows/release.yml", "utf8");
const keyTransitionValidator = readFileSync(
  "scripts/validate-updater-key-transition.ts",
  "utf8",
);

describe("updater signing key operations", () => {
  test("documents the secret names and encrypted backup pointers", () => {
    for (const name of [
      "TAURI_SIGNING_PRIVATE_KEY",
      "TAURI_SIGNING_PRIVATE_KEY_PASSWORD",
      "TAURI_SIGNING_PUBLIC_KEY",
    ]) {
      expect(runbook).toContain(name);
    }
    expect(workflow).toContain("secrets.TAURI_SIGNING_PRIVATE_KEY");
    expect(workflow).toContain("secrets.TAURI_SIGNING_PRIVATE_KEY_PASSWORD");
    expect(workflow).toContain("vars.TAURI_SIGNING_PUBLIC_KEY");
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

  test("documents private candidate validation before live feed publication", () => {
    expect(runbook).toContain("before the draft becomes public");
    expect(runbook).toContain("localhost HTTPS");
    expect(runbook).toContain("installs v0.5.0");
    expect(runbook).toMatch(/private Actions\s+artifact/);
    expect(runbook).toContain("attached to the joint draft");
    expect(runbook).toMatch(/same\s+tag, commit, archive hash/);
    expect(runbook).toMatch(/restores\s+and verifies the saved manifest/);
    expect(runbook).toMatch(/stops and checks the\s+server process/);
    expect(runbook).toMatch(/disposable\s+GitHub-hosted runner/);
    expect(runbook).toContain("Re-run all jobs");
    expect(runbook).toContain("durable backup asset");
    expect(runbook).toContain("manifest_sha256");
    expect(runbook).toContain("expected_live_sha256");
    expect(runbook).toContain("confirm_rollback");
    expect(runbook).not.toContain("latest-candidate.json");
    expect(runbook).toContain("`update-feed`");
    expect(runbook).toContain("gh release create update-feed");
    expect(runbook).toContain("--latest=false");
    expect(runbook).toContain("vMAJOR.MINOR.PATCH");
  });

  test("rejects accidental key drift and requires an exact bridge declaration", () => {
    expect(workflow).toContain("scripts/validate-updater-key-transition.ts");
    expect(keyTransitionValidator).toContain("src-tauri/tauri.conf.json");
    expect(keyTransitionValidator).toContain("updater-key-bridge.json");
    expect(keyTransitionValidator).toContain(
      "TAURI_SIGNING_PUBLIC_KEY does not match the updater key pinned in tauri.conf.json",
    );
    expect(runbook).toContain("updater-key-bridge.json");
  });

  test("never suggests printing secret material", () => {
    expect(runbook).not.toMatch(/echo\s+\$?TAURI_SIGNING_PRIVATE_KEY/);
    expect(runbook).not.toMatch(/gh secret set[^\n]+--body/);
  });
});
