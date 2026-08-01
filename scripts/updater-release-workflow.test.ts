import { describe, expect, test } from "bun:test";
import { existsSync, readFileSync } from "node:fs";

const workflow = readFileSync(".github/workflows/release.yml", "utf8");
const config = JSON.parse(readFileSync("src-tauri/tauri.conf.json", "utf8"));
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
  test("creates and signs updater artifacts after unrelated bundle hooks", () => {
    expect(config.bundle.createUpdaterArtifacts).toBe(false);
    expect(storeConfig.bundle.createUpdaterArtifacts).toBe(false);
    expect(existsSync("src-tauri/tauri.updater.conf.json")).toBe(false);

    const bundle = stepBlock("Bundle GitHub installers");
    expect(bundle).not.toContain("TAURI_SIGNING_PRIVATE_KEY");
    expect(bundle).not.toContain("createUpdaterArtifacts");

    const archive = stepBlock("Create updater archive");
    expect(archive).toContain("Compress-Archive");
    expect(archive).toContain("-CompressionLevel NoCompression");
    expect(archive).not.toContain("TAURI_SIGNING_PRIVATE_KEY");

    const signer = stepBlock("Sign updater archive");
    expect(signer).toContain("bun run tauri signer sign");
    expect(signer).not.toContain("tauri bundle");
    expect(signer).toContain(
      "TAURI_SIGNING_PRIVATE_KEY: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY }}",
    );
    expect(signer).toContain(
      "TAURI_SIGNING_PRIVATE_KEY_PASSWORD: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY_PASSWORD }}",
    );
    expect(
      workflow.match(/secrets\.TAURI_SIGNING_PRIVATE_KEY\s*}}/g),
    ).toHaveLength(1);
    expect(
      workflow.match(/secrets\.TAURI_SIGNING_PRIVATE_KEY_PASSWORD\s*}}/g),
    ).toHaveLength(1);
    expect(workflow.indexOf("- name: Bundle GitHub installers")).toBeLessThan(
      workflow.indexOf("- name: Sign updater archive"),
    );
    const jobEnvironment = workflow.slice(0, workflow.indexOf("    steps:"));
    expect(jobEnvironment).not.toContain("TAURI_SIGNING_PRIVATE_KEY");
    expect(stepBlock("Bundle Store installers")).not.toContain(
      "TAURI_SIGNING_PRIVATE_KEY",
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

  test("verifies the updater signature against the release signing key", () => {
    const verifierPath = "scripts/verify-updater-signature/src/main.rs";
    expect(existsSync(verifierPath)).toBe(true);
    const verifier = readFileSync(verifierPath, "utf8");
    expect(verifier).toContain("minisign_verify");
    expect(verifier).toContain("verify_stream");
    expect(verifier).toContain("finalize()");

    const paths = stepBlock("Resolve updater artifact paths");
    expect(paths).toContain(
      "TAURI_SIGNING_PUBLIC_KEY: ${{ vars.TAURI_SIGNING_PUBLIC_KEY }}",
    );
    expect(paths).toContain("updater-signing-public-key.pub");
    expect(paths).toContain("scripts/validate-updater-key-transition.ts");
    expect(paths).toContain("Updater signing key transition validation failed");
    expect(paths).toContain("[Convert]::FromBase64String");
    expect(paths).toContain("scripts/verify-updater-signature/Cargo.toml");
    expect(paths).toContain("cargo run --locked --release");
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
      expect(step).toContain("steps.updater-paths.outputs.public_key");
    }
    expect(workflow).not.toContain("latest.json");
  });
});
