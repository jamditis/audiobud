import { readFileSync } from "node:fs";

export interface ModelAssetManifest {
  schema_version: number;
  release_tag: string;
  base_url: string;
  assets: Array<{
    name: string;
    bytes: number;
    sha256: string;
    source_url: string;
  }>;
}

interface GitHubAsset {
  name: string;
  size: number;
  state: string;
  browser_download_url: string;
}

interface GitHubRelease {
  tag_name: string;
  draft: boolean;
  prerelease: boolean;
  assets: GitHubAsset[];
}

export function validateModelAssetRelease(
  manifest: ModelAssetManifest,
  release: GitHubRelease,
): void {
  if (manifest.schema_version !== 1) {
    throw new Error(
      `Unsupported model asset schema: ${manifest.schema_version}`,
    );
  }
  if (
    release.tag_name !== manifest.release_tag ||
    release.draft ||
    release.prerelease
  ) {
    throw new Error("Model assets must come from the published pinned release");
  }

  const releaseAssets = new Map(
    release.assets.map((asset) => [asset.name, asset]),
  );
  for (const expected of manifest.assets) {
    const actual = releaseAssets.get(expected.name);
    if (!actual) {
      throw new Error(`Published model asset is missing: ${expected.name}`);
    }
    if (actual.state !== "uploaded" || actual.size !== expected.bytes) {
      throw new Error(
        `Published model asset has unexpected state or size: ${expected.name}`,
      );
    }
    const expectedUrl = `${manifest.base_url}/${expected.name}`;
    if (actual.browser_download_url !== expectedUrl) {
      throw new Error(
        `Published model asset has an unexpected URL: ${expected.name}`,
      );
    }
  }

  for (const metadataName of ["model-assets.json", "SHA256SUMS-models.txt"]) {
    const metadata = releaseAssets.get(metadataName);
    if (!metadata || metadata.state !== "uploaded" || metadata.size <= 0) {
      throw new Error(`Published model metadata is missing: ${metadataName}`);
    }
  }
}

async function main(): Promise<void> {
  const manifest = JSON.parse(
    readFileSync("model-assets.json", "utf8"),
  ) as ModelAssetManifest;
  const headers: Record<string, string> = {
    Accept: "application/vnd.github+json",
    "User-Agent": "AudioBud-model-asset-verifier",
    "X-GitHub-Api-Version": "2022-11-28",
  };
  if (process.env.GITHUB_TOKEN) {
    headers.Authorization = `Bearer ${process.env.GITHUB_TOKEN}`;
  }

  const response = await fetch(
    `https://api.github.com/repos/jamditis/audiobud/releases/tags/${manifest.release_tag}`,
    { headers },
  );
  if (!response.ok) {
    throw new Error(
      `Failed to read model asset release: ${response.status} ${response.statusText}`,
    );
  }
  validateModelAssetRelease(manifest, (await response.json()) as GitHubRelease);
  console.log(`Verified ${manifest.assets.length} published model assets`);
}

if (import.meta.main) {
  await main();
}
