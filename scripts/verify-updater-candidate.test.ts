import { expect, test } from "bun:test";
import { spawnSync } from "node:child_process";
import { existsSync, mkdtempSync, readdirSync, rmdirSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

const powershell = Bun.which("pwsh");
const guardTest = powershell ? test : test.skip;

guardTest.each([
  ["local", "", "", "Windows"],
  ["self-hosted", "true", "self-hosted", "Windows"],
  ["non-Windows", "true", "github-hosted", "Linux"],
])(
  "rejects a %s environment before certificate setup",
  (_, actions, runner, os) => {
    const directory = mkdtempSync(join(tmpdir(), "audiobud-updater-guard-"));
    const evidence = join(directory, "evidence");
    try {
      const result = spawnSync(
        powershell!,
        [
          "-NoProfile",
          "-File",
          resolve("scripts/verify-updater-candidate.ps1"),
          "-TargetCommit",
          "149caae5df9a1ac998e2080af3cfb93a74e5bf98",
          "-TargetTag",
          "v0.6.3",
          "-TargetVersion",
          "0.6.3",
          "-ArchivePath",
          join(directory, "missing.nsis.zip"),
          "-SignaturePath",
          join(directory, "missing.sig"),
          "-PriorInstallerPath",
          join(directory, "missing.exe"),
          "-EvidenceDirectory",
          evidence,
          "-PriorTag",
          "v0.6.0",
          "-PriorVersion",
          "0.6.0",
        ],
        {
          encoding: "utf8",
          timeout: 15_000,
          env: {
            ...process.env,
            GITHUB_ACTIONS: actions,
            RUNNER_ENVIRONMENT: runner,
            RUNNER_OS: os,
            NO_COLOR: "1",
          },
        },
      );
      expect(result.error).toBeUndefined();
      expect(result.status).toBe(1);
      expect(result.stderr.replace(/\s+/g, " ")).toContain(
        "requires a disposable GitHub-hosted Windows runner",
      );
      expect(existsSync(evidence)).toBe(false);
    } finally {
      if (existsSync(directory) && readdirSync(directory).length === 0) {
        rmdirSync(directory);
      }
    }
  },
);
