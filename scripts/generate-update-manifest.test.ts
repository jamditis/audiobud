import { describe, expect, test } from "bun:test";
import { mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
  buildUpdateManifest,
  writeUpdateManifest,
  type GitHubRelease,
} from "./generate-update-manifest";

const repository = "jamditis/audiobud";
const tag = "v0.4.2";
const archiveName = "AudioBud_0.4.2_x64-setup.nsis.zip";
const archiveUrl = `https://github.com/${repository}/releases/download/${tag}/${archiveName}`;

function publishedRelease(
  overrides: Partial<GitHubRelease> = {},
): GitHubRelease {
  return {
    tag_name: tag,
    draft: false,
    prerelease: false,
    published_at: "2026-07-31T20:00:00Z",
    body: "Update notes",
    assets: [
      {
        name: archiveName,
        browser_download_url: archiveUrl,
        state: "uploaded",
        size: 123,
      },
      {
        name: `${archiveName}.sig`,
        browser_download_url: `${archiveUrl}.sig`,
        state: "uploaded",
        size: 64,
      },
    ],
    ...overrides,
  };
}

describe("update feed manifest", () => {
  test("describes the published signed Windows updater payload", () => {
    const manifest = buildUpdateManifest(
      publishedRelease(),
      repository,
      "signed-payload",
    );

    expect(manifest).toEqual({
      version: "0.4.2",
      notes: "Update notes",
      pub_date: "2026-07-31T20:00:00.000Z",
      platforms: {
        "windows-x86_64": {
          signature: "signed-payload",
          url: archiveUrl,
        },
      },
    });
  });

  test("rejects drafts, prereleases, missing publication dates, and bad tags", () => {
    for (const release of [
      publishedRelease({ draft: true }),
      publishedRelease({ prerelease: true }),
      publishedRelease({ published_at: null }),
      publishedRelease({ tag_name: "nightly" }),
      publishedRelease({ tag_name: "v0.4.3-rc.1" }),
    ]) {
      expect(() =>
        buildUpdateManifest(release, repository, "signed-payload"),
      ).toThrow();
    }
  });

  test("rejects missing, ambiguous, incomplete, or foreign updater assets", () => {
    const archive = publishedRelease().assets[0];
    const signature = publishedRelease().assets[1];

    for (const assets of [
      [],
      [archive],
      [archive, signature, { ...archive, name: `copy-${archive.name}` }],
      [archive, { ...signature, state: "new" }],
      [
        {
          ...archive,
          browser_download_url: `https://example.com/${archive.name}`,
        },
        signature,
      ],
    ]) {
      expect(() =>
        buildUpdateManifest(
          publishedRelease({ assets }),
          repository,
          "signed-payload",
        ),
      ).toThrow();
    }
  });

  test("rejects an empty or multiline signature", () => {
    expect(() =>
      buildUpdateManifest(publishedRelease(), repository, " "),
    ).toThrow(/signature/i);
    expect(() =>
      buildUpdateManifest(
        publishedRelease(),
        repository,
        "first line\nsecond line",
      ),
    ).toThrow(/signature/i);
  });

  test("writes deterministic JSON with a trailing newline", () => {
    const directory = mkdtempSync(join(tmpdir(), "audiobud-update-feed-"));
    const output = join(directory, "latest.json");
    const releasePath = join(directory, "release.json");
    const signaturePath = join(directory, `${archiveName}.sig`);
    writeFileSync(releasePath, JSON.stringify(publishedRelease()));
    writeFileSync(signaturePath, "signed-payload\n");

    writeUpdateManifest({
      releasePath,
      repository,
      signaturePath,
      outputPath: output,
    });

    const rendered = readFileSync(output, "utf8");
    expect(rendered.endsWith("\n")).toBe(true);
    expect(JSON.parse(rendered)).toEqual(
      buildUpdateManifest(publishedRelease(), repository, "signed-payload"),
    );
  });
});
