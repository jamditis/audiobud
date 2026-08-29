import { createHash } from "node:crypto";
import {
  mkdirSync,
  mkdtempSync,
  rmSync,
  symlinkSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { describe, expect, test } from "bun:test";
import {
  collectPayloadInventory,
  validateSbomFileChecksums,
} from "./validate-sbom-file-checksums";

const fixtureContent = "fixture";
const validSha1 = createHash("sha1").update(fixtureContent).digest("hex");
const validSha256 = createHash("sha256").update(fixtureContent).digest("hex");
const zeroSha1 = "0".repeat(40);

function directoryRecord(fileName: string): Record<string, unknown> {
  return {
    fileName,
    checksums: [{ algorithm: "SHA1", checksumValue: zeroSha1 }],
  };
}

function fileRecord(
  fileName: string,
  checksums: Array<Record<string, string>> = [
    { algorithm: "SHA256", checksumValue: validSha256 },
  ],
): Record<string, unknown> {
  return { fileName, checksums };
}

function withPayload<T>(
  files: Record<string, string>,
  callback: (payloadRoot: string) => T,
  directories: string[] = [],
): T {
  const fixtureDirectory = mkdtempSync(
    join(tmpdir(), "audiobud-sbom-payload-"),
  );
  const payloadRoot = join(fixtureDirectory, "payload");

  try {
    mkdirSync(payloadRoot);
    for (const directory of directories) {
      mkdirSync(join(payloadRoot, directory), { recursive: true });
    }
    for (const [fileName, content] of Object.entries(files)) {
      const filePath = join(payloadRoot, fileName);
      mkdirSync(dirname(filePath), { recursive: true });
      writeFileSync(filePath, content);
    }
    return callback(payloadRoot);
  } finally {
    rmSync(fixtureDirectory, { recursive: true, force: true });
  }
}

function validatePayload(
  document: unknown,
  files: Record<string, string>,
  directories: string[] = [],
): { directoryCount: number; fileCount: number; symlinkCount: number } {
  return withPayload(
    files,
    (payloadRoot) =>
      validateSbomFileChecksums(document, collectPayloadInventory(payloadRoot)),
    directories,
  );
}

function runValidator(document: unknown, includePayloadRoot = true) {
  return withPayload({ "audiobud.exe": fixtureContent }, (payloadRoot) => {
    const fixturePath = join(dirname(payloadRoot), "fixture.spdx.json");
    writeFileSync(fixturePath, JSON.stringify(document));
    const cmd = [
      process.execPath,
      "run",
      "scripts/validate-sbom-file-checksums.ts",
      fixturePath,
    ];
    if (includePayloadRoot) cmd.push(payloadRoot);

    return Bun.spawnSync({
      cmd,
      cwd: join(import.meta.dir, ".."),
      stdout: "pipe",
      stderr: "pipe",
    });
  });
}

describe("SBOM file checksum validation", () => {
  test("accepts matching checksums for every content-bearing record", () => {
    expect(
      validatePayload(
        {
          files: [
            directoryRecord(""),
            fileRecord("audiobud.exe", [
              { algorithm: "SHA1", checksumValue: validSha1 },
              { algorithm: "SHA256", checksumValue: validSha256 },
            ]),
          ],
        },
        { "audiobud.exe": fixtureContent },
      ),
    ).toEqual({ directoryCount: 1, fileCount: 1, symlinkCount: 0 });
  });

  test("rejects an all-zero file placeholder", () => {
    expect(() =>
      validatePayload(
        {
          files: [
            directoryRecord(""),
            fileRecord("DirectML.dll", [
              { algorithm: "SHA1", checksumValue: zeroSha1 },
            ]),
          ],
        },
        { "DirectML.dll": fixtureContent },
      ),
    ).toThrow("placeholder checksums: DirectML.dll");
  });

  test("rejects a placeholder beside a matching checksum", () => {
    expect(() =>
      validatePayload(
        {
          files: [
            directoryRecord(""),
            fileRecord("audiobud.exe", [
              { algorithm: "SHA1", checksumValue: zeroSha1 },
              { algorithm: "SHA256", checksumValue: validSha256 },
            ]),
          ],
        },
        { "audiobud.exe": fixtureContent },
      ),
    ).toThrow("placeholder checksums: audiobud.exe");
  });

  test("rejects missing and malformed checksums", () => {
    expect(() =>
      validatePayload(
        {
          files: [
            directoryRecord(""),
            { fileName: "missing.exe" },
            fileRecord("short.dll", [
              { algorithm: "SHA256", checksumValue: "abc" },
            ]),
          ],
        },
        { "missing.exe": fixtureContent, "short.dll": fixtureContent },
      ),
    ).toThrow("missing.exe, short.dll");
  });

  test("rejects a checksum that does not match the staged bytes", () => {
    const staleSha256 = createHash("sha256").update("stale").digest("hex");
    expect(() =>
      validatePayload(
        {
          files: [
            directoryRecord(""),
            fileRecord("audiobud.exe", [
              { algorithm: "SHA256", checksumValue: staleSha256 },
            ]),
          ],
        },
        { "audiobud.exe": fixtureContent },
      ),
    ).toThrow("checksums do not match the staged payload bytes: audiobud.exe");
  });

  test("requires a matching SHA-256 checksum", () => {
    expect(() =>
      validatePayload(
        {
          files: [
            directoryRecord(""),
            fileRecord("audiobud.exe", [
              { algorithm: "SHA1", checksumValue: validSha1 },
            ]),
          ],
        },
        { "audiobud.exe": fixtureContent },
      ),
    ).toThrow("missing a required SHA-256 checksum: audiobud.exe");
  });

  test("rejects an SBOM with no file records", () => {
    expect(() => validatePayload({ files: [] }, {})).toThrow(
      "SBOM contains no file records",
    );
  });

  test("rejects missing, unexpected, and duplicate inventory records", () => {
    expect(() =>
      validatePayload(
        {
          files: [
            directoryRecord(""),
            fileRecord("audiobud.exe"),
            fileRecord("audiobud.exe"),
            fileRecord("unexpected.dll"),
          ],
        },
        {
          "audiobud.exe": fixtureContent,
          "DirectML.dll": fixtureContent,
        },
      ),
    ).toThrow(
      "missing DirectML.dll; unexpected unexpected.dll; duplicate audiobud.exe",
    );
  });

  test("allows placeholders for real directories", () => {
    expect(
      validatePayload(
        {
          files: [
            directoryRecord(""),
            directoryRecord("Contents"),
            directoryRecord("Contents/MacOS"),
            fileRecord("Contents/MacOS/audiobud"),
          ],
        },
        { "Contents/MacOS/audiobud": fixtureContent },
      ),
    ).toEqual({ directoryCount: 3, fileCount: 1, symlinkCount: 0 });
  });

  test("matches Windows SPDX paths to the staged inventory", () => {
    expect(
      validatePayload(
        {
          files: [
            directoryRecord(""),
            directoryRecord("\\dependency-manifests"),
            fileRecord("\\dependency-manifests\\Cargo.lock"),
          ],
        },
        { "dependency-manifests/Cargo.lock": fixtureContent },
      ),
    ).toEqual({ directoryCount: 2, fileCount: 1, symlinkCount: 0 });
  });

  test("normalizes POSIX scan-root and relative path prefixes", () => {
    expect(
      validatePayload(
        {
          files: [
            directoryRecord("."),
            directoryRecord("./Contents"),
            fileRecord("/Contents/audiobud"),
          ],
        },
        { "Contents/audiobud": fixtureContent },
      ),
    ).toEqual({ directoryCount: 2, fileCount: 1, symlinkCount: 0 });
  });

  test("limits the number of file names in an error", () => {
    const files = Object.fromEntries(
      Array.from({ length: 21 }, (_, index) => [
        `invalid-${index + 1}`,
        fixtureContent,
      ]),
    );
    const records = Object.keys(files).map((fileName) =>
      fileRecord(fileName, [{ algorithm: "SHA1", checksumValue: zeroSha1 }]),
    );

    expect(() =>
      validatePayload({ files: [directoryRecord(""), ...records] }, files),
    ).toThrow("invalid-20, and 1 more");
  });

  test.skipIf(process.platform === "win32")(
    "inventories symlinks without traversing their targets",
    () => {
      withPayload({ "real/sub/file.txt": fixtureContent }, (payloadRoot) => {
        symlinkSync("real/sub/file.txt", join(payloadRoot, "file-link"));
        symlinkSync("real", join(payloadRoot, "directory-link"));
        const inventory = collectPayloadInventory(payloadRoot);

        expect(inventory.get("file-link")?.kind).toBe("symlink");
        expect(inventory.get("directory-link")?.kind).toBe("symlink");
        expect(inventory.has("directory-link/sub/file.txt")).toBe(false);
        expect(
          validateSbomFileChecksums(
            {
              files: [
                directoryRecord(""),
                directoryRecord("directory-link"),
                directoryRecord("file-link"),
                directoryRecord("real"),
                directoryRecord("real/sub"),
                fileRecord("real/sub/file.txt"),
              ],
            },
            inventory,
          ),
        ).toEqual({ directoryCount: 3, fileCount: 1, symlinkCount: 2 });
      });
    },
  );
});

describe("SBOM file checksum validator command", () => {
  test("exits successfully for a valid document", () => {
    const result = runValidator({
      files: [directoryRecord(""), fileRecord("audiobud.exe")],
    });

    expect(result.exitCode).toBe(0);
    expect(result.stdout.toString()).toContain(
      "Verified 1 file checksums and 1 directory plus 0 symlink records.",
    );
  });

  test("requires the staged payload root", () => {
    const result = runValidator(
      { files: [directoryRecord(""), fileRecord("audiobud.exe")] },
      false,
    );

    expect(result.exitCode).toBe(1);
    expect(result.stderr.toString()).toContain(
      "Usage: bun run validate-sbom-file-checksums.ts <sbom> <payload-root>",
    );
  });

  test("exits with an error for a placeholder document", () => {
    const result = runValidator({
      files: [
        directoryRecord(""),
        fileRecord("audiobud.exe", [
          { algorithm: "SHA1", checksumValue: zeroSha1 },
        ]),
      ],
    });

    expect(result.exitCode).toBe(1);
    expect(result.stderr.toString()).toContain(
      "missing, malformed, unsupported, or placeholder checksums: audiobud.exe",
    );
  });
});
