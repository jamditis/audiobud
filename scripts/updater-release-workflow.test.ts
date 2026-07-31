import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";

const workflow = readFileSync(".github/workflows/release.yml", "utf8");
const config = JSON.parse(readFileSync("src-tauri/tauri.conf.json", "utf8"));
const updaterConfig = JSON.parse(
  readFileSync("src-tauri/tauri.updater.conf.json", "utf8"),
);
const storeConfig = JSON.parse(
  readFileSync("src-tauri/tauri.microsoftstore.conf.json", "utf8"),
);

function stepBlock(name: string): string {
  const position = workflow.indexOf(`- name: ${name}`);
  expect(position, `Missing workflow step: ${name}`).toBeGreaterThan(-1);
  const next = workflow.indexOf("\n      - name:", position + 1);
  return workflow.slice(position, next === -1 ? undefined : next);
}

describe("signed updater release artifacts", () => {
  test("creates updater artifacts only for the normal GitHub package", () => {
    expect(config.bundle.createUpdaterArtifacts).toBe(false);
    expect(updaterConfig.bundle.createUpdaterArtifacts).toBe(true);
    expect(storeConfig.bundle.createUpdaterArtifacts).toBe(false);
    expect(stepBlock("Bundle installers")).toContain(
      "--config src-tauri/tauri.updater.conf.json",
    );
    expect(workflow).toContain(
      "TAURI_SIGNING_PRIVATE_KEY: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY }}",
    );
    expect(workflow).toContain(
      "TAURI_SIGNING_PRIVATE_KEY_PASSWORD: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY_PASSWORD }}",
    );
  });

  test("resolves and validates the signed updater archive", () => {
    const paths = stepBlock("Resolve updater artifact paths");
    expect(paths).toContain("AudioBud_$($env:APP_VERSION)_x64-setup.nsis.zip");
    expect(paths).toContain('"archive=$archive"');
    expect(paths).toContain('"signature=$signature"');
    expect(paths).toContain("Get-AuthenticodeSignature");
    expect(paths).toContain("CN=Joseph Amditis");
    expect(paths).toContain("Expand-Archive");
  });

  test("publishes and attests updater files but never latest.json", () => {
    for (const stepName of [
      "Attest release provenance",
      "Upload release artifacts to GitHub release",
      "Upload release artifacts as CI artifact",
    ]) {
      const step = stepBlock(stepName);
      expect(step).toContain("steps.updater-paths.outputs.archive");
      expect(step).toContain("steps.updater-paths.outputs.signature");
    }
    expect(workflow).not.toContain("latest.json");
  });
});
