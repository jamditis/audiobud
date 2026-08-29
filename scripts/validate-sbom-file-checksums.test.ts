import { createHash } from "node:crypto";
import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  symlinkSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { describe, expect, test } from "bun:test";
import {
  collectPayloadInventory,
  completeWindowsSyftPlaceholderChecksums,
  validateSbomFileChecksums,
  writeJsonAtomically,
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

function runWindowsPlaceholderCompletion(
  document: unknown,
  serializedDocument = JSON.stringify(document),
) {
  return withPayload({ "audiobud.exe": fixtureContent }, (payloadRoot) => {
    const fixturePath = join(dirname(payloadRoot), "fixture.spdx.json");
    writeFileSync(fixturePath, serializedDocument);

    const result = Bun.spawnSync({
      cmd: [
        process.execPath,
        "run",
        "scripts/validate-sbom-file-checksums.ts",
        "--complete-windows-placeholders",
        fixturePath,
        payloadRoot,
      ],
      cwd: join(import.meta.dir, ".."),
      stdout: "pipe",
      stderr: "pipe",
    });

    const completedBytes = readFileSync(fixturePath, "utf8");
    const completedDocument: unknown = JSON.parse(completedBytes);
    return { completedBytes, completedDocument, result };
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

  test.skipIf(process.platform === "win32")(
    "rejects special filesystem entries instead of treating them as files",
    () => {
      withPayload({}, (payloadRoot) => {
        const fifoPath = join(payloadRoot, "named-pipe");
        const result = Bun.spawnSync({
          cmd: ["mkfifo", fifoPath],
          stdout: "pipe",
          stderr: "pipe",
        });
        expect(result.exitCode).toBe(0);
        expect(() => collectPayloadInventory(payloadRoot)).toThrow(
          "Unsupported payload entry type: named-pipe",
        );
      });
    },
  );
});

describe("SBOM file checksum validator command", () => {
  test("completes the exact Syft Windows placeholder form before validation", () => {
    const { completedDocument, result } = runWindowsPlaceholderCompletion({
      creationInfo: {
        creators: ["Organization: Anchore, Inc", "Tool: syft-1.49.0"],
      },
      documentNamespace: "https://example.test/syft/windows-candidate",
      files: [
        directoryRecord(""),
        fileRecord("audiobud.exe", [
          { algorithm: "SHA1", checksumValue: zeroSha1 },
        ]),
      ],
    });

    expect(result.exitCode).toBe(0);
    expect(result.stdout.toString()).toContain(
      "Completed 1 Syft Windows file checksum placeholder.",
    );
    expect(completedDocument).toEqual({
      creationInfo: {
        creators: [
          "Organization: Anchore, Inc",
          "Tool: syft-1.49.0",
          "Tool: audiobud-sbom-checksum-completer-1",
        ],
      },
      documentNamespace:
        "https://example.test/syft/windows-candidate?audiobud-sbom-checksum-completer=1",
      files: [
        directoryRecord(""),
        fileRecord("audiobud.exe", [
          {
            algorithm: "SHA1",
            checksumValue: "51cff3c1f0bc59f6187e7040cc12a4e9b1eca7aa",
          },
          {
            algorithm: "SHA256",
            checksumValue:
              "f16d05ec6b29248d2c61adb1e9263f78e4f7bace1b955014a2d17872cfe4064d",
          },
        ]),
      ],
    });
  });

  test("leaves a valid document byte-for-byte unchanged", () => {
    const document = {
      creationInfo: {
        creators: ["Organization: Anchore, Inc", "Tool: syft-1.49.0"],
      },
      documentNamespace: "https://example.test/syft/valid-candidate",
      files: [directoryRecord(""), fileRecord("audiobud.exe")],
    };
    const source = `${JSON.stringify(document, null, 2)}\n`;
    const { completedBytes, completedDocument, result } =
      runWindowsPlaceholderCompletion(document, source);

    expect(result.exitCode).toBe(0);
    expect(result.stdout.toString()).toContain(
      "Completed 0 Syft Windows file checksum placeholders.",
    );
    expect(completedBytes).toBe(source);
    expect(completedDocument).toEqual(document);
  });

  test("does not complete placeholders for symlink inventory entries", () => {
    const document = {
      files: [directoryRecord(""), directoryRecord("audiobud-link")],
    };
    const inventory = new Map([
      ["", { kind: "directory" as const, path: "unused-root" }],
      ["audiobud-link", { kind: "symlink" as const, path: "unread-target" }],
    ]);

    expect(
      completeWindowsSyftPlaceholderChecksums(document, inventory),
    ).toEqual({
      completedFileCount: 0,
      directoryCount: 1,
      fileCount: 0,
      symlinkCount: 1,
    });
    expect(document).toEqual({
      files: [directoryRecord(""), directoryRecord("audiobud-link")],
    });
  });

  test("rejects every form outside the exact single zero-SHA-1 placeholder", () => {
    const invalidChecksums: unknown[] = [
      undefined,
      [{ algorithm: "SHA256", checksumValue: "0".repeat(64) }],
      [
        { algorithm: "SHA1", checksumValue: zeroSha1 },
        { algorithm: "SHA1", checksumValue: zeroSha1 },
      ],
      [{ algorithm: "sha1", checksumValue: zeroSha1 }],
      [{ algorithm: "SHA1", checksumValue: "0".repeat(39) }],
      [{ algorithm: "SHA1", checksumValue: 0 }],
    ];

    for (const checksums of invalidChecksums) {
      const file: Record<string, unknown> = { fileName: "audiobud.exe" };
      if (checksums !== undefined) file.checksums = checksums;
      const document = { files: [directoryRecord(""), file] };
      const { completedDocument, result } =
        runWindowsPlaceholderCompletion(document);

      expect(result.exitCode).toBe(1);
      expect(result.stderr.toString()).toContain(
        "missing, malformed, unsupported, or placeholder checksums",
      );
      expect(completedDocument).toEqual(document);
    }
  });

  test("does not repair an inventory mismatch", () => {
    const document = {
      files: [
        directoryRecord(""),
        fileRecord("unexpected.exe", [
          { algorithm: "SHA1", checksumValue: zeroSha1 },
        ]),
      ],
    };
    const { completedDocument, result } =
      runWindowsPlaceholderCompletion(document);

    expect(result.exitCode).toBe(1);
    expect(result.stderr.toString()).toContain(
      "SBOM file inventory does not match the staged payload",
    );
    expect(completedDocument).toEqual(document);
  });

  test("does not repair missing or duplicate inventory records", () => {
    const documents = [
      { files: [directoryRecord("")] },
      {
        creationInfo: { creators: ["Tool: syft-1.49.0"] },
        documentNamespace: "https://example.test/syft/windows-candidate",
        files: [
          directoryRecord(""),
          fileRecord("audiobud.exe", [
            { algorithm: "SHA1", checksumValue: zeroSha1 },
          ]),
          fileRecord("audiobud.exe", [
            { algorithm: "SHA1", checksumValue: zeroSha1 },
          ]),
        ],
      },
    ];

    for (const document of documents) {
      const { completedDocument, result } =
        runWindowsPlaceholderCompletion(document);
      expect(result.exitCode).toBe(1);
      expect(result.stderr.toString()).toContain(
        "SBOM file inventory does not match the staged payload",
      );
      expect(completedDocument).toEqual(document);
    }
  });

  test("requires valid SPDX creation metadata before a repair", () => {
    const missingCreationInfo = {
      documentNamespace: "https://example.test/syft/windows-candidate",
      files: [
        directoryRecord(""),
        fileRecord("audiobud.exe", [
          { algorithm: "SHA1", checksumValue: zeroSha1 },
        ]),
      ],
    };
    const malformedNamespace = {
      creationInfo: { creators: ["Tool: syft-1.49.0"] },
      documentNamespace: "not an absolute URI",
      files: missingCreationInfo.files,
    };

    for (const document of [missingCreationInfo, malformedNamespace]) {
      const { completedDocument, result } =
        runWindowsPlaceholderCompletion(document);
      expect(result.exitCode).toBe(1);
      expect(result.stderr.toString()).toContain(
        "valid SPDX creation metadata",
      );
      expect(completedDocument).toEqual(document);
    }
  });

  test("preserves the original and removes the temporary file when replacement fails", () => {
    withPayload({}, (payloadRoot) => {
      const fixturePath = join(dirname(payloadRoot), "fixture.spdx.json");
      const temporaryPath = `${fixturePath}.tmp-${process.pid}`;
      const original = "original SBOM bytes\n";
      writeFileSync(fixturePath, original);

      expect(() =>
        writeJsonAtomically(fixturePath, { files: [] }, () => {
          throw new Error("simulated replacement failure");
        }),
      ).toThrow("simulated replacement failure");
      expect(readFileSync(fixturePath, "utf8")).toBe(original);
      expect(existsSync(temporaryPath)).toBe(false);
    });
  });

  test("does not replace a mixed placeholder record", () => {
    const document = {
      files: [
        directoryRecord(""),
        fileRecord("audiobud.exe", [
          { algorithm: "SHA1", checksumValue: zeroSha1 },
          { algorithm: "SHA256", checksumValue: validSha256 },
        ]),
      ],
    };
    const { completedDocument, result } =
      runWindowsPlaceholderCompletion(document);

    expect(result.exitCode).toBe(1);
    expect(result.stderr.toString()).toContain("placeholder checksums");
    expect(completedDocument).toEqual(document);
  });

  test("does not replace a real but stale checksum", () => {
    const staleSha256 =
      "d6fd3a1e3b3b585ed085bb8d58efc64c54672b9e7e924c22003e1f604a0cbe0e";
    const document = {
      files: [
        directoryRecord(""),
        fileRecord("audiobud.exe", [
          { algorithm: "SHA256", checksumValue: staleSha256 },
        ]),
      ],
    };
    const { completedDocument, result } =
      runWindowsPlaceholderCompletion(document);

    expect(result.exitCode).toBe(1);
    expect(result.stderr.toString()).toContain(
      "checksums do not match the staged payload bytes",
    );
    expect(completedDocument).toEqual(document);
  });

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
