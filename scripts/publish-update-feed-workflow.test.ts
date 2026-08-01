import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";

const workflow = readFileSync(
  ".github/workflows/publish-update-feed.yml",
  "utf8",
);
const releaseWorkflow = readFileSync(".github/workflows/release.yml", "utf8");
const tauriConfig = JSON.parse(
  readFileSync("src-tauri/tauri.conf.json", "utf8"),
);

function stepPosition(name: string): number {
  const position = workflow.indexOf(`- name: ${name}`);
  expect(position, `Missing workflow step: ${name}`).toBeGreaterThan(-1);
  return position;
}

describe("published update feed workflow", () => {
  test("runs only after publication or an explicit retry", () => {
    expect(workflow).toContain("release:\n    types: [published]");
    expect(workflow).toContain("workflow_dispatch:");
    expect(workflow).toContain("tag:");
    expect(workflow).toContain("^v[0-9]+\\.[0-9]+\\.[0-9]+$");
    expect(workflow).toContain('"publish=false" >> "$GITHUB_OUTPUT"');
    expect(workflow).toContain("if: needs.gate.outputs.publish == 'true'");
    expect(workflow).toContain("attestations: read");
    expect(workflow).toContain("contents: write");
    expect(releaseWorkflow).not.toContain("latest.json");
  });

  test("validates a candidate before promoting the live manifest", () => {
    expect(stepPosition("Resolve published release")).toBeLessThan(
      stepPosition("Download signed updater assets"),
    );
    expect(stepPosition("Download signed updater assets")).toBeLessThan(
      stepPosition("Verify updater signature"),
    );
    expect(stepPosition("Verify updater signature")).toBeLessThan(
      stepPosition("Generate latest.json"),
    );
    expect(stepPosition("Generate latest.json")).toBeLessThan(
      stepPosition("Upload candidate manifest"),
    );
    expect(stepPosition("Upload candidate manifest")).toBeLessThan(
      stepPosition("Apply signed update"),
    );
    expect(stepPosition("Apply signed update")).toBeLessThan(
      stepPosition("Publish latest.json"),
    );
    expect(workflow).toContain(
      ".draft or .prerelease or (.published_at == null)",
    );
    expect(
      workflow.match(/gh api "repos\/\$GITHUB_REPOSITORY\/releases\/latest"/g),
    ).toHaveLength(2);
    expect(workflow).toContain(
      'LATEST_TAG=$(jq -r ".tag_name" "$LATEST_JSON")',
    );
    expect(workflow).toContain('if [[ "$LATEST_TAG" != "$TAG" ]]');
    expect(workflow).toContain('gh release download "$TAG"');
    expect(workflow).toContain('--pattern "*.nsis.zip"');
    expect(workflow).toContain('--pattern "*.nsis.zip.sig"');
    expect(workflow).toContain('--pattern "updater-signing-public-key.pub"');
    expect(workflow).toContain("gh attestation verify");
    expect(workflow).toContain(
      'gh api "repos/$GITHUB_REPOSITORY/commits/$TAG"',
    );
    expect(workflow).toContain("commit: ${{ steps.release.outputs.commit }}");
    expect(workflow).toContain('--source-digest "$RELEASE_COMMIT"');
    expect(workflow).toContain("steps.updater.outputs.public_key");
    expect(workflow).not.toContain("src-tauri/tauri.conf.json");
    expect(workflow).toContain("scripts/verify-updater-signature/Cargo.toml");
    expect(workflow).toContain("cargo run --locked --release");
    expect(workflow).toContain("steps.updater.outputs.archive");
    expect(workflow).toContain("scripts/generate-update-manifest.ts");
    expect(workflow).toContain('MANIFEST="$RUNNER_TEMP/latest-candidate.json"');
    expect(workflow).toContain(
      'gh release upload "$TAG" "$MANIFEST" --clobber',
    );
    expect(workflow).toContain("--install-update-endpoint");
    expect(workflow).toContain("latest-candidate.json");
    expect(workflow).toContain("FEED_TAG: update-feed");
    expect(workflow).toContain(
      'gh release upload "$FEED_TAG" "$LIVE_MANIFEST" --clobber',
    );
    expect(tauriConfig.plugins.updater.endpoints).toEqual([
      "https://github.com/jamditis/audiobud/releases/download/update-feed/latest.json",
    ]);
    expect(workflow).not.toContain("#latest-candidate.json");
    expect(workflow).not.toContain("#latest.json");
    expect(workflow).toContain("Remove candidate manifest");
  });

  test("pins all third-party actions", () => {
    const actions = [
      ...workflow.matchAll(/^\s*uses:\s+([^@\s]+)@([^\s#]+)(?:\s+#.*)?$/gm),
    ];
    expect(actions.length).toBeGreaterThan(0);
    for (const [, name, reference] of actions) {
      expect(reference, `${name} must use a full commit SHA`).toMatch(
        /^[0-9a-f]{40}$/,
      );
    }
  });

  test("applies an update from a real prior install on clean Windows", () => {
    expect(workflow).toContain("verify_update:");
    expect(workflow).toContain("needs: prepare");
    expect(workflow).toContain("runs-on: windows-2025");
    expect(workflow).toMatch(
      /verify_update:[\s\S]*?runs-on: windows-2025[\s\S]*?timeout-minutes: 25/,
    );
    expect(stepPosition("Resolve prior updater-capable release")).toBeLessThan(
      stepPosition("Install prior release"),
    );
    expect(stepPosition("Install prior release")).toBeLessThan(
      stepPosition("Apply signed update"),
    );
    expect(stepPosition("Apply signed update")).toBeLessThan(
      stepPosition("Assert installed version and signature"),
    );
    expect(workflow).toContain('"--install-update"');
    expect(workflow).toContain("Get-AuthenticodeSignature");
    expect(workflow).toContain("VersionInfo.ProductVersion");
  });

  test("captures updater bootstrap failures and logs before version polling", () => {
    expect(workflow).toContain("-RedirectStandardOutput $stdoutLog");
    expect(workflow).toContain("-RedirectStandardError $stderrLog");
    expect(workflow).toContain("$process.WaitForExit(480000)");
    expect(workflow).toContain(
      "Updater bootstrap process failed with exit code",
    );
    expect(workflow).toContain("AudioBud updater stdout");
    expect(workflow).toContain("AudioBud updater stderr");
    expect(workflow).toContain("AudioBud file logs");
    expect(workflow).toContain("AudioBud updater temp directories");
  });
});
