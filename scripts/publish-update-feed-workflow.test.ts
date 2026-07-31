import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";

const workflow = readFileSync(
  ".github/workflows/publish-update-feed.yml",
  "utf8",
);
const releaseWorkflow = readFileSync(".github/workflows/release.yml", "utf8");

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
    expect(workflow).toContain("contents: write");
    expect(releaseWorkflow).not.toContain("latest.json");
  });

  test("validates the release before uploading the manifest", () => {
    expect(stepPosition("Resolve published release")).toBeLessThan(
      stepPosition("Download signed updater assets"),
    );
    expect(stepPosition("Download signed updater assets")).toBeLessThan(
      stepPosition("Generate latest.json"),
    );
    expect(stepPosition("Generate latest.json")).toBeLessThan(
      stepPosition("Upload latest.json"),
    );
    expect(workflow).toContain(
      ".draft or .prerelease or (.published_at == null)",
    );
    expect(workflow).toContain('gh release download "$TAG"');
    expect(workflow).toContain('--pattern "*.nsis.zip"');
    expect(workflow).toContain('--pattern "*.nsis.zip.sig"');
    expect(workflow).toContain("scripts/generate-update-manifest.ts");
    expect(workflow).toContain(
      'gh release upload "$TAG" "$MANIFEST" --clobber',
    );
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
    expect(workflow).toContain("needs: publish");
    expect(workflow).toContain("runs-on: windows-2025");
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
});
