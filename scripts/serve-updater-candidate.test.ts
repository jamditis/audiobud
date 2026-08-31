import { describe, expect, test } from "bun:test";
import {
  existsSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const serverPath = "./serve-updater-candidate.mjs";
const serverExists = existsSync(new URL(serverPath, import.meta.url));
const serverModule = serverExists
  ? await import(serverPath)
  : (null as {
      buildCandidateManifest?: (input: Record<string, unknown>) => unknown;
      candidateRoute?: (pathname: string, archiveName: string) => unknown;
      publishReadyFile?: (
        readyPath: string,
        readiness: Record<string, unknown>,
      ) => void;
    } | null);

describe("private updater candidate server", () => {
  test("has a dedicated localhost-only implementation", () => {
    expect(serverExists).toBe(true);
    expect(serverModule?.buildCandidateManifest).toBeFunction();
    expect(serverModule?.candidateRoute).toBeFunction();
    expect(serverModule?.publishReadyFile).toBeFunction();
  });

  test("builds the exact signed v0.6.0 manifest", () => {
    expect(serverModule).not.toBeNull();
    if (!serverModule?.buildCandidateManifest) return;

    expect(
      serverModule.buildCandidateManifest({
        version: "0.6.0",
        signature: "c2lnbmVkLXBheWxvYWQ=",
        archiveName: "AudioBud_0.6.0_x64-setup.nsis.zip",
        port: 44321,
        pubDate: "2026-08-29T04:00:00Z",
      }),
    ).toEqual({
      version: "0.6.0",
      notes: "Private prepublication updater verification",
      pub_date: "2026-08-29T04:00:00.000Z",
      platforms: {
        "windows-x86_64": {
          signature: "c2lnbmVkLXBheWxvYWQ=",
          url: "https://localhost:44321/AudioBud_0.6.0_x64-setup.nsis.zip",
        },
      },
    });
  });

  test("rejects inputs that could select another payload", () => {
    expect(serverModule).not.toBeNull();
    if (!serverModule?.buildCandidateManifest) return;

    for (const input of [
      {
        version: "0.6.0-rc.1",
        signature: "c2lnbmVkLXBheWxvYWQ=",
        archiveName: "AudioBud_0.6.0-rc.1_x64-setup.nsis.zip",
        port: 44321,
        pubDate: "2026-08-29T04:00:00Z",
      },
      {
        version: "0.6.0",
        signature: "first\nsecond",
        archiveName: "AudioBud_0.6.0_x64-setup.nsis.zip",
        port: 44321,
        pubDate: "2026-08-29T04:00:00Z",
      },
      {
        version: "0.6.0",
        signature: "c2lnbmVkLXBheWxvYWQ=",
        archiveName: "different.nsis.zip",
        port: 44321,
        pubDate: "2026-08-29T04:00:00Z",
      },
    ]) {
      expect(() => serverModule.buildCandidateManifest?.(input)).toThrow();
    }
  });

  test("serves only the candidate manifest and exact updater archive", () => {
    expect(serverModule).not.toBeNull();
    if (!serverModule?.candidateRoute) return;

    const archive = "AudioBud_0.6.0_x64-setup.nsis.zip";
    expect(serverModule.candidateRoute("/latest-candidate.json", archive)).toBe(
      "manifest",
    );
    expect(serverModule.candidateRoute(`/${archive}`, archive)).toBe("archive");
    expect(serverModule.candidateRoute("/latest.json", archive)).toBeNull();
    expect(
      serverModule.candidateRoute(
        "/AudioBud_0.5.0_x64-setup.nsis.zip",
        archive,
      ),
    ).toBeNull();
  });

  test("publishes complete readiness JSON atomically and exclusively", () => {
    expect(serverModule?.publishReadyFile).toBeFunction();
    if (!serverModule?.publishReadyFile) return;

    const directory = mkdtempSync(join(tmpdir(), "audiobud-ready-"));
    const readyPath = join(directory, "ready.json");
    const readiness = {
      archive_sha256: "a".repeat(64),
      manifest_url: "https://localhost:44321/latest-candidate.json",
      port: 44321,
    };
    serverModule.publishReadyFile(readyPath, readiness);

    expect(JSON.parse(readFileSync(readyPath, "utf8"))).toEqual(readiness);
    expect(readdirSync(directory)).toEqual(["ready.json"]);

    writeFileSync(readyPath, "existing\n", "utf8");
    expect(() =>
      serverModule.publishReadyFile?.(readyPath, readiness),
    ).toThrow();
    expect(readFileSync(readyPath, "utf8")).toBe("existing\n");
    expect(readdirSync(directory)).toEqual(["ready.json"]);
  });
});
