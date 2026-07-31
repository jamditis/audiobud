import { readFileSync, writeFileSync } from "node:fs";

export interface GitHubReleaseAsset {
  name: string;
  browser_download_url: string;
  state: string;
  size: number;
}

export interface GitHubRelease {
  tag_name: string;
  draft: boolean;
  prerelease: boolean;
  published_at: string | null;
  body: string | null;
  assets: GitHubReleaseAsset[];
}

export interface UpdateManifest {
  version: string;
  notes: string;
  pub_date: string;
  platforms: {
    "windows-x86_64": {
      signature: string;
      url: string;
    };
  };
}

const RELEASE_TAG = /^v?(\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?)$/;

function expectedAssetUrl(
  repository: string,
  tag: string,
  assetName: string,
): string {
  return `https://github.com/${repository}/releases/download/${tag}/${assetName}`;
}

export function buildUpdateManifest(
  release: GitHubRelease,
  repository: string,
  rawSignature: string,
): UpdateManifest {
  if (release.draft) {
    throw new Error("Refusing to publish an update feed for a draft release");
  }
  if (release.prerelease) {
    throw new Error(
      "Refusing to publish a stable update feed for a prerelease",
    );
  }
  if (!release.published_at) {
    throw new Error("Published release is missing published_at");
  }

  const versionMatch = RELEASE_TAG.exec(release.tag_name);
  if (!versionMatch) {
    throw new Error(
      `Release tag is not a supported semver: ${release.tag_name}`,
    );
  }
  const version = versionMatch[1];

  const archives = release.assets.filter((asset) =>
    asset.name.endsWith(".nsis.zip"),
  );
  if (archives.length !== 1) {
    throw new Error(
      `Expected exactly one NSIS updater archive, found ${archives.length}`,
    );
  }
  const archive = archives[0];
  const expectedArchiveName = `AudioBud_${version}_x64-setup.nsis.zip`;
  if (archive.name !== expectedArchiveName) {
    throw new Error(
      `Unexpected NSIS updater archive: ${archive.name}; expected ${expectedArchiveName}`,
    );
  }

  const signatures = release.assets.filter(
    (asset) => asset.name === `${archive.name}.sig`,
  );
  if (signatures.length !== 1) {
    throw new Error(
      `Expected exactly one signature for ${archive.name}, found ${signatures.length}`,
    );
  }

  for (const asset of [archive, signatures[0]]) {
    if (asset.state !== "uploaded" || asset.size <= 0) {
      throw new Error(`Updater asset is not fully uploaded: ${asset.name}`);
    }
    const expectedUrl = expectedAssetUrl(
      repository,
      release.tag_name,
      asset.name,
    );
    if (asset.browser_download_url !== expectedUrl) {
      throw new Error(
        `Updater asset URL is outside the published release: ${asset.browser_download_url}`,
      );
    }
  }

  const signature = rawSignature.trim();
  if (!signature || /[\r\n]/.test(signature)) {
    throw new Error("Updater signature must be one non-empty line");
  }

  const publishedAt = new Date(release.published_at);
  if (Number.isNaN(publishedAt.valueOf())) {
    throw new Error(
      `Invalid release publication date: ${release.published_at}`,
    );
  }

  return {
    version,
    notes: release.body ?? "",
    pub_date: publishedAt.toISOString(),
    platforms: {
      "windows-x86_64": {
        signature,
        url: archive.browser_download_url,
      },
    },
  };
}

export interface WriteUpdateManifestOptions {
  releasePath: string;
  repository: string;
  signaturePath: string;
  outputPath: string;
}

export function writeUpdateManifest({
  releasePath,
  repository,
  signaturePath,
  outputPath,
}: WriteUpdateManifestOptions): void {
  const release = JSON.parse(
    readFileSync(releasePath, "utf8"),
  ) as GitHubRelease;
  const signature = readFileSync(signaturePath, "utf8");
  const manifest = buildUpdateManifest(release, repository, signature);
  writeFileSync(outputPath, `${JSON.stringify(manifest, null, 2)}\n`, "utf8");
}

function option(name: string): string {
  const index = process.argv.indexOf(name);
  const value = process.argv[index + 1];
  if (index === -1 || !value || value.startsWith("--")) {
    throw new Error(`Missing required option: ${name}`);
  }
  return value;
}

if (import.meta.main) {
  writeUpdateManifest({
    releasePath: option("--release"),
    repository: option("--repository"),
    signaturePath: option("--signature"),
    outputPath: option("--output"),
  });
}
