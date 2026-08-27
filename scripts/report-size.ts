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
  initialFiles: FileSize[];
  initialBytes: number;
  initialGzipBytes: number;
  deferredAssets: FileSize[];
  deferredBytes: number;
  deferredGzipBytes: number;
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

// The initial payload is what a webview actually fetches to reach an
// interactive first paint: the entry chunk, its CSS, and everything reached
// by walking the statically-imported JS/CSS graph. `entry.assets` (images,
// fonts, audio) are resources a chunk *references* by URL, not resources the
// browser is guaranteed to fetch on load -- e.g. src/lib/ribbit.ts imports
// ribbit.wav to get its bundled URL, but the file itself is only requested
// when playRibbit() constructs an Audio element after a click. Counting
// those against the initial payload overstates what startup costs.
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
  for (const importedEntry of entry.imports ?? []) {
    collectInitialFiles(manifest, importedEntry, visitedEntries, files);
  }

  return files;
}

// Referenced-but-deferred assets: everything `collectInitialFiles` skips,
// gathered from the same reachable chunk graph so they stay visible without
// inflating the initial-payload number.
function collectDeferredAssets(
  manifest: Record<string, ManifestEntry>,
  entryKey: string,
  visitedEntries = new Set<string>(),
  assets = new Set<string>(),
): Set<string> {
  if (visitedEntries.has(entryKey)) return assets;
  visitedEntries.add(entryKey);

  const entry = manifest[entryKey];
  if (!entry) {
    throw new Error(`Vite manifest import is missing: ${entryKey}`);
  }

  for (const file of entry.assets ?? []) assets.add(file);
  for (const importedEntry of entry.imports ?? []) {
    collectDeferredAssets(manifest, importedEntry, visitedEntries, assets);
  }

  return assets;
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
    const initialFiles = await Promise.all(
      [...collectInitialFiles(manifest, entryKey)].map(getFileSize),
    );
    initialFiles.sort((left, right) => right.bytes - left.bytes);

    const deferredAssets = await Promise.all(
      [...collectDeferredAssets(manifest, entryKey)].map(getFileSize),
    );
    deferredAssets.sort((left, right) => right.bytes - left.bytes);

    entrySizes.push({
      entry,
      source,
      initialFiles,
      initialBytes: initialFiles.reduce((sum, file) => sum + file.bytes, 0),
      initialGzipBytes: initialFiles.reduce(
        (sum, file) => sum + file.gzipBytes,
        0,
      ),
      deferredAssets,
      deferredBytes: deferredAssets.reduce((sum, file) => sum + file.bytes, 0),
      deferredGzipBytes: deferredAssets.reduce(
        (sum, file) => sum + file.gzipBytes,
        0,
      ),
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
        `| ${entry.entry} | ${entry.initialFiles.length} | ${formatBytes(entry.initialBytes)} | ${formatBytes(entry.initialGzipBytes)} |`,
    )
    .join("\n");
  const deferredRows = entrySizes
    .map(
      (entry) =>
        `| ${entry.entry} | ${entry.deferredAssets.length} | ${formatBytes(entry.deferredBytes)} | ${formatBytes(entry.deferredGzipBytes)} |`,
    )
    .join("\n");
  const deferredFileRows = entrySizes
    .flatMap((entry) =>
      entry.deferredAssets.map(
        (file) =>
          `| ${entry.entry} | \`${file.file}\` | ${formatBytes(file.bytes)} | ${formatBytes(file.gzipBytes)} |`,
      ),
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

Resources fetched to reach an interactive first paint: each entry's chunk,
its CSS, and the statically-imported JS/CSS graph reachable from it.

| Entry | Files | Raw size | Gzip size |
| --- | ---: | ---: | ---: |
${entryRows}

## Deferred assets (referenced, not loaded at startup)

Images, fonts, and audio a chunk references by URL but does not fetch until
something at runtime asks for them (e.g. a click that plays a sound). Listed
separately so they stay visible without inflating the initial payload above.

| Entry | Files | Raw size | Gzip size |
| --- | ---: | ---: | ---: |
${deferredRows}

${deferredFileRows ? `| Entry | File | Raw size | Gzip size |\n| --- | --- | ---: | ---: |\n${deferredFileRows}\n` : ""}
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
