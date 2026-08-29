import { describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const selectorPath = "scripts/select-latest-stable-app-release.mjs";

function runSelector(releasePages: unknown) {
  const directory = mkdtempSync(join(tmpdir(), "audiobud-release-selector-"));
  const fixturePath = join(directory, "releases.json");
  writeFileSync(fixturePath, JSON.stringify(releasePages));
  const result = Bun.spawnSync({
    cmd: ["node", selectorPath, fixturePath],
    stderr: "pipe",
    stdout: "pipe",
  });
  const response = {
    exitCode: result.exitCode,
    stderr: result.stderr.toString(),
    stdout: result.stdout.toString(),
  };
  rmSync(directory, { force: true, recursive: true });
  return response;
}

function runSelectorFromStdin(releasePages: unknown) {
  const result = Bun.spawnSync({
    cmd: ["node", selectorPath],
    stdin: new TextEncoder().encode(JSON.stringify(releasePages)),
    stderr: "pipe",
    stdout: "pipe",
  });
  return {
    exitCode: result.exitCode,
    stderr: result.stderr.toString(),
    stdout: result.stdout.toString(),
  };
}

describe("latest stable app release selection", () => {
  test("ignores newer auxiliary, uppercase, draft, and prerelease tags", () => {
    const result = runSelector([
      [
        {
          tag_name: "model-assets-v2",
          draft: false,
          prerelease: false,
          published_at: "2026-08-29T05:00:00Z",
        },
        {
          tag_name: "V9.9.9",
          draft: false,
          prerelease: false,
          published_at: "2026-08-29T04:00:00Z",
        },
        {
          tag_name: "v8.0.0",
          draft: true,
          prerelease: false,
          published_at: null,
        },
        {
          tag_name: "v0.4.0",
          draft: false,
          prerelease: false,
          published_at: "2026-08-26T07:14:30Z",
        },
      ],
      [
        {
          tag_name: "v7.0.0",
          draft: false,
          prerelease: true,
          published_at: "2026-08-29T03:00:00Z",
        },
        {
          tag_name: "v0.5.0",
          draft: false,
          prerelease: false,
          published_at: "2026-08-27T07:14:30Z",
        },
      ],
    ]);

    expect(result).toEqual({ exitCode: 0, stderr: "", stdout: "v0.5.0\n" });
  });

  test("selects the newest eligible release from standard input", () => {
    const result = runSelectorFromStdin([
      [
        {
          tag_name: "v0.5.0",
          draft: false,
          prerelease: false,
          published_at: "2026-08-27T07:14:30Z",
        },
      ],
      [
        {
          tag_name: "v0.4.0",
          draft: false,
          prerelease: false,
          published_at: "2026-08-26T07:14:30Z",
        },
        {
          tag_name: "v0.3.0",
          draft: false,
          prerelease: false,
          published_at: "2026-08-25T07:14:30Z",
        },
      ],
    ]);

    expect(result).toEqual({ exitCode: 0, stderr: "", stdout: "v0.5.0\n" });
  });

  test("fails closed when no stable app release exists", () => {
    const result = runSelector([
      [
        {
          tag_name: "update-feed",
          draft: false,
          prerelease: false,
          published_at: "2026-08-29T05:00:00Z",
        },
      ],
    ]);

    expect(result.exitCode).not.toBe(0);
    expect(result.stdout).toBe("");
  });
});
