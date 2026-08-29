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

  test("selects the highest eligible release from standard input", () => {
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

  test("does not select a lower version that was published later", () => {
    const result = runSelector([
      [
        {
          tag_name: "v0.6.0",
          draft: false,
          prerelease: false,
          published_at: "2026-08-28T08:00:00Z",
        },
        {
          tag_name: "v0.5.1",
          draft: false,
          prerelease: false,
          published_at: "2026-08-29T08:00:00Z",
        },
      ],
    ]);

    expect(result).toEqual({ exitCode: 0, stderr: "", stdout: "v0.6.0\n" });
  });

  test("prefers a canonical tag regardless of release order", () => {
    const canonical = {
      tag_name: "v1.2.3",
      draft: false,
      prerelease: false,
      published_at: "2026-08-28T08:00:00Z",
    };
    const leadingZero = {
      tag_name: "v01.2.3",
      draft: false,
      prerelease: false,
      published_at: "2026-08-29T08:00:00Z",
    };

    for (const releases of [
      [canonical, leadingZero],
      [leadingZero, canonical],
    ]) {
      expect(runSelector([releases])).toEqual({
        exitCode: 0,
        stderr: "",
        stdout: "v1.2.3\n",
      });
    }
  });

  test("compares version components above the safe number limit", () => {
    const higher = {
      tag_name: "v9007199254740993.0.0",
      draft: false,
      prerelease: false,
      published_at: "2026-08-28T08:00:00Z",
    };
    const lower = {
      tag_name: "v9007199254740992.999999999999999999.999999999999999999",
      draft: false,
      prerelease: false,
      published_at: "2026-08-29T08:00:00Z",
    };

    for (const releases of [
      [higher, lower],
      [lower, higher],
    ]) {
      expect(runSelector([releases]).stdout).toBe("v9007199254740993.0.0\n");
    }
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
