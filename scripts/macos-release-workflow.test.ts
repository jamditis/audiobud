import { describe, expect, test } from "bun:test";
import { existsSync, readFileSync } from "node:fs";

const workflow = readFileSync(".github/workflows/release.yml", "utf8");

function jobBlock(name: string): string {
  const marker = `  ${name}:`;
  const position = workflow.indexOf(marker);
  expect(position, `Missing workflow job: ${name}`).toBeGreaterThan(-1);
  const remainder = workflow.slice(position + marker.length);
  const next = /^  [a-zA-Z0-9_-]+:\s*$/m.exec(remainder)?.index;
  return workflow.slice(
    position,
    next === undefined ? undefined : position + marker.length + next,
  );
}

function stepPosition(job: string, name: string): number {
  const position = job.indexOf(`- name: ${name}`);
  expect(position, `Missing workflow step: ${name}`).toBeGreaterThan(-1);
  return position;
}

describe("signed macOS release workflow", () => {
  test("uses an isolated Apple Silicon release job", () => {
    const macOS = jobBlock("build-macos");

    expect(macOS).toContain("runs-on: macos-15");
    expect(macOS).toContain("environment: artifact-signing");
    expect(macOS).toContain("group: release-macos");
    expect(macOS).toContain("attestations: write");
    expect(macOS).toContain("contents: read");
    expect(macOS).toContain("inputs.store_candidate != true");
    expect(macOS).toContain(
      "DEVELOPER_DIR: /Applications/Xcode_26.0.1.app/Contents/Developer",
    );

    const windows = jobBlock("build-windows");
    expect(windows).toContain("group: release-windows");
    expect(workflow).not.toMatch(/^concurrency:/m);
  });

  test("tests the full frontend and Rust application before signing", () => {
    const macOS = jobBlock("build-macos");
    const orderedSteps = [
      "Check out repository",
      "Resolve macOS version and paths",
      "Set up Bun",
      "Install Rust stable",
      "Restore Rust cache",
      "Install macOS build tools",
      "Require Apple Intelligence SDK",
      "Install frontend dependencies",
      "Download Silero VAD model",
      "Run frontend checks",
      "Run Rust checks",
      "Build application without bundling",
      "Prepare Apple signing credentials",
      "Build signed and notarized macOS bundles",
    ].map((name) => stepPosition(macOS, name));

    for (let index = 1; index < orderedSteps.length; index += 1) {
      expect(orderedSteps[index]).toBeGreaterThan(orderedSteps[index - 1]);
    }

    expect(macOS).toContain("bun install --frozen-lockfile");
    expect(macOS).toContain("FoundationModels.framework");
    expect(macOS).toContain("Select Xcode 26 or newer before releasing");
    expect(macOS).toContain("bun run lint");
    expect(macOS).toContain("bun run format:check");
    expect(macOS).toContain("bun run test");
    expect(macOS).toContain("bun run check:translations");
    expect(macOS).toContain("bun run check:rebrand");
    expect(macOS).toContain("cargo test --all-targets --locked");
    expect(macOS).toContain(
      "cargo clippy --all-targets --all-features --locked -- -D warnings",
    );
    expect(macOS).toContain("bun run tauri build --no-bundle --ci -- --locked");
  });

  test("uses only the protected Apple credentials during bundle creation", () => {
    const macOS = jobBlock("build-macos");

    expect(macOS).toContain(
      "APPLE_CERTIFICATE: ${{ secrets.APPLE_CERTIFICATE }}",
    );
    expect(macOS).toContain(
      "APPLE_CERTIFICATE_PASSWORD: ${{ secrets.APPLE_CERTIFICATE_PASSWORD }}",
    );
    expect(macOS).toContain(
      "APPLE_API_PRIVATE_KEY: ${{ secrets.APPLE_API_PRIVATE_KEY }}",
    );
    expect(macOS).toContain("APPLE_API_KEY: ${{ vars.APPLE_API_KEY }}");
    expect(macOS).toContain("APPLE_API_ISSUER: ${{ vars.APPLE_API_ISSUER }}");
    expect(macOS).toContain("APPLE_API_KEY_PATH: ${{ env.API_KEY_PATH }}");
    expect(macOS).toContain("chmod 600");
    expect(macOS).not.toContain("set -x");

    expect(existsSync("src-tauri/tauri.macos-signing.conf.json")).toBe(true);
    const signingConfig = JSON.parse(
      readFileSync("src-tauri/tauri.macos-signing.conf.json", "utf8"),
    );
    expect(signingConfig.bundle.macOS.signingIdentity).toBe(
      "Developer ID Application: Joe Amditis (5624SD289G)",
    );
  });

  test("signs, notarizes, staples, and verifies the app and DMG", () => {
    const macOS = jobBlock("build-macos");
    const bundle = stepPosition(
      macOS,
      "Build signed and notarized macOS bundles",
    );
    const resolve = stepPosition(macOS, "Resolve macOS release artifacts");
    const notarizeDMG = stepPosition(macOS, "Notarize and staple macOS DMG");
    const removeKey = stepPosition(macOS, "Remove Apple API private key");
    const verify = stepPosition(
      macOS,
      "Verify macOS signatures and notarization",
    );

    expect(macOS).toContain("bun run tauri bundle --bundles app,dmg");
    expect(macOS).not.toContain("tauri bundle --verbose");
    expect(macOS).toContain("src-tauri/tauri.macos-signing.conf.json");
    expect(macOS).toContain('xcrun notarytool submit "$DMG_PATH"');
    expect(macOS).toContain("--wait --output-format json");
    expect(macOS).toContain('xcrun stapler staple "$DMG_PATH"');
    expect(macOS).toContain("codesign --verify --deep --strict");
    expect(macOS).toContain('codesign --verify --verbose=2 "$DMG_PATH"');
    expect(macOS).toContain("codesign --display --entitlements");
    expect(macOS).toContain("spctl --assess --type execute");
    expect(macOS).toContain("spctl --assess --type open");
    expect(macOS).toContain("xcrun stapler validate");
    expect(macOS).toContain("hdiutil verify");
    expect(macOS).toContain("otool -L");
    expect(macOS).toContain("/opt/homebrew|/usr/local");
    expect(macOS).toContain("APP_CANDIDATE_COUNT");
    expect(macOS).toContain("DMG_CANDIDATE_COUNT");
    expect(macOS).toContain('[[ "$APP_CANDIDATE_COUNT" != "1" ]]');
    expect(macOS).toContain('[[ "$DMG_CANDIDATE_COUNT" != "1" ]]');
    expect(macOS).toContain('LIPO_ARCHS=$(lipo -archs "$MAIN_BINARY")');
    expect(macOS).toContain('[[ "$LIPO_ARCHS" != "arm64" ]]');
    expect(resolve).toBeGreaterThan(bundle);
    expect(notarizeDMG).toBeGreaterThan(resolve);
    expect(removeKey).toBeGreaterThan(notarizeDMG);
    expect(verify).toBeGreaterThan(removeKey);
  });

  test("names, checksums, inventories, attests, and uploads the macOS artifact", () => {
    const macOS = jobBlock("build-macos");
    const publish = jobBlock("publish-macos-release");

    expect(macOS).toContain("AudioBud_${VERSION}_macos_aarch64.dmg");
    expect(macOS).toContain("SHA256SUMS-macos.txt");
    expect(macOS).toContain("_macos_aarch64_sbom.spdx.json");
    expect(macOS).toContain(
      "uses: anchore/sbom-action@e22c389904149dbc22b58101806040fa8d37a610",
    );
    expect(macOS).toContain(
      "uses: actions/attest@f7c74d28b9d84cb8768d0b8ca14a4bac6ef463e6",
    );
    expect(macOS).toContain(
      "uses: actions/upload-artifact@ea165f8d65b6e75b540449e92b4886f43607fa02",
    );

    expect(publish).toContain("needs: [build-windows, build-macos]");
    expect(publish).toContain(
      "uses: actions/download-artifact@d3f86a106a0bac45b974a628896c90dbdf5c8093",
    );
    expect(publish).toContain("gh release upload");
    expect(publish).toContain("--clobber");
    expect(publish).toContain("isDraft");
    expect(publish).toContain("targetCommitish");
  });
});
