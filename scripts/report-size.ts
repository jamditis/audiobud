import { gzipSync } from "node:zlib";
import {
  mkdir,
  readFile,
  readdir,
  rm,
  stat,
  writeFile,
} from "node:fs/promises";
import path from "node:path";

interface ManifestEntry {
  file?: string;
  src?: string;
  isEntry?: boolean;
  imports?: string[];
  css?: string[];
  assets?: string[];
}

interface FileSize {
  file: string;
  bytes: number;
  gzipBytes: number;
}

interface EntrySize {
  entry: string;
  source: string;
  files: FileSize[];
  bytes: number;
  gzipBytes: number;
}

const rootDir = process.cwd();
const distDir = path.join(rootDir, "dist");
const manifestPath = path.join(distDir, ".vite", "manifest.json");
const reportDir = path.join(rootDir, "artifacts", "size-report");

const entrySources = {
  main: "index.html",
  overlay: "src/overlay/index.html",
  "window-picker": "src/window-picker/index.html",
} as const;

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KiB`;
  return `${(bytes / (1024 * 1024)).toFixed(2)} MiB`;
}

function findEntryKey(
  manifest: Record<string, ManifestEntry>,
  source: string,
): string {
  const match = Object.entries(manifest).find(
    ([key, value]) => key === source || value.src === source,
  );

  if (!match) {
    throw new Error(`Vite manifest does not contain entry: ${source}`);
  }

  return match[0];
}

function collectInitialFiles(
  manifest: Record<string, ManifestEntry>,
  entryKey: string,
  visitedEntries = new Set<string>(),
  files = new Set<string>(),
): Set<string> {
  if (visitedEntries.has(entryKey)) return files;
  visitedEntries.add(entryKey);

  const entry = manifest[entryKey];
  if (!entry) {
    throw new Error(`Vite manifest import is missing: ${entryKey}`);
  }

  if (entry.file) files.add(entry.file);
  for (const file of entry.css ?? []) files.add(file);
  for (const file of entry.assets ?? []) files.add(file);
  for (const importedEntry of entry.imports ?? []) {
    collectInitialFiles(manifest, importedEntry, visitedEntries, files);
  }

  return files;
}

async function getFileSize(file: string): Promise<FileSize> {
  const absolutePath = path.join(distDir, file);
  const contents = await readFile(absolutePath);
  const fileStats = await stat(absolutePath);

  return {
    file,
    bytes: fileStats.size,
    gzipBytes: gzipSync(contents, { level: 9 }).length,
  };
}

async function listFiles(directory: string): Promise<string[]> {
  const entries = await readdir(directory, { withFileTypes: true });
  const files: string[] = [];

  for (const entry of entries) {
    const absolutePath = path.join(directory, entry.name);
    if (entry.isDirectory()) {
      files.push(...(await listFiles(absolutePath)));
      continue;
    }

    files.push(path.relative(distDir, absolutePath).split(path.sep).join("/"));
  }

  return files;
}

function countCargoDependencies(cargoToml: string): Record<string, number> {
  const counts: Record<string, number> = {};
  let section = "";

  for (const line of cargoToml.split("\n")) {
    const sectionMatch = line.trim().match(/^\[(.+)]$/);
    if (sectionMatch) {
      section = sectionMatch[1];
      continue;
    }

    const isDependencySection =
      section === "dependencies" ||
      section === "build-dependencies" ||
      section === "dev-dependencies" ||
      section.endsWith(".dependencies");

    if (!isDependencySection) continue;
    if (!/^[A-Za-z0-9_-]+\s*=/.test(line.trim())) continue;

    counts[section] = (counts[section] ?? 0) + 1;
  }

  return counts;
}

async function main(): Promise<void> {
  const manifest = JSON.parse(await readFile(manifestPath, "utf8")) as Record<
    string,
    ManifestEntry
  >;

  const entrySizes: EntrySize[] = [];
  for (const [entry, source] of Object.entries(entrySources)) {
    const entryKey = findEntryKey(manifest, source);
    const files = await Promise.all(
      [...collectInitialFiles(manifest, entryKey)].map(getFileSize),
    );
    files.sort((left, right) => right.bytes - left.bytes);

    entrySizes.push({
      entry,
      source,
      files,
      bytes: files.reduce((sum, file) => sum + file.bytes, 0),
      gzipBytes: files.reduce((sum, file) => sum + file.gzipBytes, 0),
    });
  }

  const allFiles = (
    await Promise.all(
      (await listFiles(distDir))
        .filter((file) => file !== ".vite/manifest.json")
        .map(getFileSize),
    )
  ).sort((left, right) => right.bytes - left.bytes);

  const packageJson = JSON.parse(
    await readFile(path.join(rootDir, "package.json"), "utf8"),
  ) as {
    dependencies?: Record<string, string>;
    devDependencies?: Record<string, string>;
  };
  const cargoToml = await readFile(
    path.join(rootDir, "src-tauri", "Cargo.toml"),
    "utf8",
  );

  const report = {
    generatedAt: new Date().toISOString(),
    entries: entrySizes,
    dist: {
      files: allFiles.length,
      bytes: allFiles.reduce((sum, file) => sum + file.bytes, 0),
      gzipBytes: allFiles.reduce((sum, file) => sum + file.gzipBytes, 0),
      largestFiles: allFiles.slice(0, 20),
    },
    dependencies: {
      javascript: {
        runtime: Object.keys(packageJson.dependencies ?? {}).length,
        development: Object.keys(packageJson.devDependencies ?? {}).length,
      },
      rust: countCargoDependencies(cargoToml),
    },
  };

  const entryRows = entrySizes
    .map(
      (entry) =>
        `| ${entry.entry} | ${entry.files.length} | ${formatBytes(entry.bytes)} | ${formatBytes(entry.gzipBytes)} |`,
    )
    .join("\n");
  const fileRows = allFiles
    .slice(0, 20)
    .map(
      (file) =>
        `| \`${file.file}\` | ${formatBytes(file.bytes)} | ${formatBytes(file.gzipBytes)} |`,
    )
    .join("\n");
  const cargoRows = Object.entries(report.dependencies.rust)
    .map(([section, count]) => `| \`${section}\` | ${count} |`)
    .join("\n");

  const markdown = `# AudioBud size report

## Initial frontend payloads

| Entry | Files | Raw size | Gzip size |
| --- | ---: | ---: | ---: |
${entryRows}

## Frontend distribution

- Files: ${report.dist.files}
- Raw size: ${formatBytes(report.dist.bytes)}
- Gzip size: ${formatBytes(report.dist.gzipBytes)}

### Largest files

| File | Raw size | Gzip size |
| --- | ---: | ---: |
${fileRows}

## Direct dependency counts

| Group | Count |
| --- | ---: |
| JavaScript runtime | ${report.dependencies.javascript.runtime} |
| JavaScript development | ${report.dependencies.javascript.development} |
${cargoRows}
`;

  await mkdir(reportDir, { recursive: true });
  await writeFile(
    path.join(reportDir, "size-report.json"),
    `${JSON.stringify(report, null, 2)}\n`,
  );
  await writeFile(path.join(reportDir, "size-report.md"), markdown);

  await rm(path.dirname(manifestPath), { recursive: true, force: true });

  process.stdout.write(markdown);
}

main().catch((error: unknown) => {
  const message = error instanceof Error ? error.message : String(error);
  console.error(`Size report failed: ${message}`);
  process.exitCode = 1;
});
