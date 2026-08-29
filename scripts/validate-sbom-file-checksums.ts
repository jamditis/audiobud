import { createHash } from "node:crypto";
import {
  closeSync,
  openSync,
  readFileSync,
  readSync,
  readdirSync,
} from "node:fs";
import { join } from "node:path";

type PayloadEntryKind = "directory" | "file" | "symlink";
type PayloadEntry = {
  kind: PayloadEntryKind;
  path: string;
};
type ParsedChecksum = {
  algorithm: string;
  checksum: string;
  nodeAlgorithm: string;
};

const digestAlgorithms: Record<
  string,
  { length: number; nodeAlgorithm: string }
> = {
  MD5: { length: 32, nodeAlgorithm: "md5" },
  SHA1: { length: 40, nodeAlgorithm: "sha1" },
  SHA224: { length: 56, nodeAlgorithm: "sha224" },
  SHA256: { length: 64, nodeAlgorithm: "sha256" },
  SHA384: { length: 96, nodeAlgorithm: "sha384" },
  SHA512: { length: 128, nodeAlgorithm: "sha512" },
};

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function parseRealChecksum(value: unknown): ParsedChecksum | undefined {
  if (!isRecord(value)) return undefined;

  const algorithm =
    typeof value.algorithm === "string"
      ? value.algorithm.toUpperCase().replaceAll("-", "")
      : "";
  const checksum = value.checksumValue;
  const digestAlgorithm = digestAlgorithms[algorithm];

  if (
    typeof checksum !== "string" ||
    digestAlgorithm === undefined ||
    checksum.length !== digestAlgorithm.length ||
    !/^[0-9a-f]+$/i.test(checksum) ||
    /^0+$/.test(checksum)
  ) {
    return undefined;
  }

  return {
    algorithm,
    checksum: checksum.toLowerCase(),
    nodeAlgorithm: digestAlgorithm.nodeAlgorithm,
  };
}

function normalizeFileName(fileName: string): string {
  if (fileName === "." || fileName === "./" || fileName === "/") return "";
  const normalizedSeparators = fileName.startsWith("\\")
    ? fileName.replaceAll("\\", "/")
    : fileName;
  return normalizedSeparators.replace(/^\.?\//, "").replace(/\/+$/, "");
}

function displayFileName(fileName: string): string {
  return fileName === "" ? "<root>" : fileName;
}

function limitedList(values: string[]): string {
  const displayed = values.slice(0, 20);
  const omittedCount = values.length - displayed.length;
  const omittedSummary = omittedCount > 0 ? `, and ${omittedCount} more` : "";
  return `${displayed.join(", ")}${omittedSummary}`;
}

export function collectPayloadInventory(
  payloadRoot: string,
): Map<string, PayloadEntry> {
  const inventory = new Map<string, PayloadEntry>([
    ["", { kind: "directory", path: payloadRoot }],
  ]);

  function visit(directoryPath: string, relativeDirectory: string): void {
    for (const entry of readdirSync(directoryPath, { withFileTypes: true })) {
      const relativePath = relativeDirectory
        ? `${relativeDirectory}/${entry.name}`
        : entry.name;
      const entryPath = join(directoryPath, entry.name);

      if (entry.isDirectory()) {
        inventory.set(relativePath, { kind: "directory", path: entryPath });
        visit(entryPath, relativePath);
      } else if (entry.isSymbolicLink()) {
        inventory.set(relativePath, { kind: "symlink", path: entryPath });
      } else {
        inventory.set(relativePath, { kind: "file", path: entryPath });
      }
    }
  }

  visit(payloadRoot, "");
  return inventory;
}

function calculateChecksums(
  filePath: string,
  checksums: ParsedChecksum[],
): Map<string, string> {
  const hashers = new Map(
    checksums.map(({ algorithm, nodeAlgorithm }) => [
      algorithm,
      createHash(nodeAlgorithm),
    ]),
  );
  const descriptor = openSync(filePath, "r");
  const buffer = Buffer.allocUnsafe(1024 * 1024);

  try {
    let bytesRead = 0;
    do {
      bytesRead = readSync(descriptor, buffer, 0, buffer.length, null);
      if (bytesRead > 0) {
        const chunk = buffer.subarray(0, bytesRead);
        for (const hasher of hashers.values()) hasher.update(chunk);
      }
    } while (bytesRead > 0);
  } finally {
    closeSync(descriptor);
  }

  return new Map(
    [...hashers].map(([algorithm, hasher]) => [
      algorithm,
      hasher.digest("hex"),
    ]),
  );
}

export function validateSbomFileChecksums(
  document: unknown,
  payloadInventory: ReadonlyMap<string, PayloadEntry>,
): { directoryCount: number; fileCount: number; symlinkCount: number } {
  if (!isRecord(document) || !Array.isArray(document.files)) {
    throw new Error("SBOM contains no file records.");
  }

  if (document.files.length === 0) {
    throw new Error("SBOM contains no file records.");
  }

  const seenFiles = new Set<string>();
  const duplicateFiles: string[] = [];
  const unexpectedFiles: string[] = [];
  const invalidFiles: string[] = [];
  const missingSha256Files: string[] = [];
  const mismatchedFiles: string[] = [];

  document.files.forEach((file, index) => {
    const rawFileName =
      isRecord(file) && typeof file.fileName === "string"
        ? file.fileName
        : `file record ${index + 1}`;
    const fileName = normalizeFileName(rawFileName);
    const displayName = displayFileName(fileName);
    const payloadEntry = payloadInventory.get(fileName);

    if (payloadEntry === undefined) {
      unexpectedFiles.push(displayName);
      return;
    }
    if (seenFiles.has(fileName)) {
      duplicateFiles.push(displayName);
      return;
    }
    seenFiles.add(fileName);

    if (payloadEntry.kind !== "file") return;

    const checksums =
      isRecord(file) && Array.isArray(file.checksums) ? file.checksums : [];
    const parsedChecksums = checksums.map(parseRealChecksum);

    if (
      parsedChecksums.length === 0 ||
      parsedChecksums.some((checksum) => checksum === undefined)
    ) {
      invalidFiles.push(displayName);
      return;
    }

    const realChecksums = parsedChecksums as ParsedChecksum[];
    if (!realChecksums.some(({ algorithm }) => algorithm === "SHA256")) {
      missingSha256Files.push(displayName);
      return;
    }
    const calculatedChecksums = calculateChecksums(
      payloadEntry.path,
      realChecksums,
    );
    if (
      realChecksums.some(
        ({ algorithm, checksum }) =>
          calculatedChecksums.get(algorithm) !== checksum,
      )
    ) {
      mismatchedFiles.push(displayName);
    }
  });

  const missingFiles = [...payloadInventory.keys()]
    .filter((fileName) => !seenFiles.has(fileName))
    .map(displayFileName);

  if (
    missingFiles.length > 0 ||
    unexpectedFiles.length > 0 ||
    duplicateFiles.length > 0
  ) {
    const details = [
      missingFiles.length > 0 ? `missing ${limitedList(missingFiles)}` : "",
      unexpectedFiles.length > 0
        ? `unexpected ${limitedList(unexpectedFiles)}`
        : "",
      duplicateFiles.length > 0
        ? `duplicate ${limitedList(duplicateFiles)}`
        : "",
    ].filter(Boolean);
    throw new Error(
      `SBOM file inventory does not match the staged payload: ${details.join("; ")}.`,
    );
  }

  if (invalidFiles.length > 0) {
    throw new Error(
      `SBOM file records have missing, malformed, unsupported, or placeholder checksums: ${limitedList(invalidFiles)}`,
    );
  }

  if (missingSha256Files.length > 0) {
    throw new Error(
      `SBOM file records are missing a required SHA-256 checksum: ${limitedList(missingSha256Files)}`,
    );
  }

  if (mismatchedFiles.length > 0) {
    throw new Error(
      `SBOM file checksums do not match the staged payload bytes: ${limitedList(mismatchedFiles)}`,
    );
  }

  const entries = [...payloadInventory.values()];
  return {
    directoryCount: entries.filter(({ kind }) => kind === "directory").length,
    fileCount: entries.filter(({ kind }) => kind === "file").length,
    symlinkCount: entries.filter(({ kind }) => kind === "symlink").length,
  };
}

function main(): void {
  const sbomPath = process.argv[2];
  const payloadRoot = process.argv[3];
  if (!sbomPath || !payloadRoot) {
    throw new Error(
      "Usage: bun run validate-sbom-file-checksums.ts <sbom> <payload-root>",
    );
  }

  const document: unknown = JSON.parse(readFileSync(sbomPath, "utf8"));
  const payloadInventory = collectPayloadInventory(payloadRoot);
  const { directoryCount, fileCount, symlinkCount } = validateSbomFileChecksums(
    document,
    payloadInventory,
  );
  console.log(
    `Verified ${fileCount} file checksums and ${directoryCount} directory plus ${symlinkCount} symlink records.`,
  );
}

if (import.meta.main) {
  try {
    main();
  } catch (error: unknown) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exitCode = 1;
  }
}
