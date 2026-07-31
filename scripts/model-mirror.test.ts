import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import {
  type ModelAssetManifest,
  validateModelAssetRelease,
} from "./verify-model-assets";

const manifest = JSON.parse(
  readFileSync("model-assets.json", "utf8"),
) as ModelAssetManifest;
const modelSource = readFileSync("src-tauri/src/managers/model.rs", "utf8");
const workflowPaths = [
  ".github/workflows/ci.yml",
  ".github/workflows/engine.yml",
  ".github/workflows/release.yml",
];

function releaseFixture() {
  return {
    tag_name: manifest.release_tag,
    draft: false,
    prerelease: false,
    assets: [
      ...manifest.assets.map((asset) => ({
        name: asset.name,
        size: asset.bytes,
        state: "uploaded",
        browser_download_url: `${manifest.base_url}/${asset.name}`,
      })),
      {
        name: "model-assets.json",
        size: 100,
        state: "uploaded",
        browser_download_url: `${manifest.base_url}/model-assets.json`,
      },
      {
        name: "SHA256SUMS-models.txt",
        size: 100,
        state: "uploaded",
        browser_download_url: `${manifest.base_url}/SHA256SUMS-models.txt`,
      },
    ],
  };
}

describe("AudioBud model mirror", () => {
  test("routes every app model through one pinned base URL", () => {
    expect(modelSource.match(/const MODEL_BASE_URL:/g)).toHaveLength(1);
    expect(modelSource).toContain(`"${manifest.base_url}";`);
    expect(modelSource).not.toContain("blob.handy.computer");

    const appAssets = manifest.assets.filter(
      (asset) => asset.name !== "silero_vad_v4.onnx",
    );
    expect(
      [...modelSource.matchAll(/model_url\("([^"]+)"\)/g)].map(
        (match) => match[1],
      ),
    ).toEqual(appAssets.map((asset) => asset.name));
    for (const asset of appAssets) {
      expect(modelSource).toContain(asset.sha256);
    }
  });

  test("keeps CI and release builds off upstream model hosts", () => {
    for (const path of workflowPaths) {
      const workflow = readFileSync(path, "utf8");
      expect(workflow, path).not.toMatch(
        /blob\.handy\.computer|raw\.githubusercontent\.com\/cjpais\/Handy/i,
      );
      expect(workflow, path).toContain(manifest.base_url);
    }
  });

  test("accepts only the complete published release with exact byte counts", () => {
    expect(() =>
      validateModelAssetRelease(manifest, releaseFixture()),
    ).not.toThrow();

    const missing = releaseFixture();
    missing.assets = missing.assets.filter(
      (asset) => asset.name !== manifest.assets[0].name,
    );
    expect(() => validateModelAssetRelease(manifest, missing)).toThrow(
      "Published model asset is missing",
    );

    const wrongSize = releaseFixture();
    wrongSize.assets[0].size += 1;
    expect(() => validateModelAssetRelease(manifest, wrongSize)).toThrow(
      "unexpected state or size",
    );

    const draft = releaseFixture();
    draft.draft = true;
    expect(() => validateModelAssetRelease(manifest, draft)).toThrow(
      "published pinned release",
    );
  });
});
