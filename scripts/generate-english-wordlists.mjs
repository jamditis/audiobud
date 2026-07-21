import { createHash } from "node:crypto";
import { readFileSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";

// Debian wamerican 2020.12.07-2. Refuse silent regeneration from a different SCOWL release.
const EXPECTED_SOURCE_SHA256 =
  "9f513f1ceadb6a01c5485b7dbdfd5118dc66cd70b59cae2851292112d4066a32";

const source = resolve(process.argv[2] || "/usr/share/dict/american-english");
const wordsOutput = resolve(
  process.argv[3] || "src-tauri/src/audio_toolkit/english_words_en.txt",
);
const namedEntitiesOutput = resolve(
  process.argv[4] ||
    "src-tauri/src/audio_toolkit/english_named_entities_en.txt",
);

const sourceBytes = readFileSync(source);
const sourceSha256 = createHash("sha256").update(sourceBytes).digest("hex");
if (sourceSha256 !== EXPECTED_SOURCE_SHA256) {
  throw new Error(
    `dictionary checksum mismatch: expected ${EXPECTED_SOURCE_SHA256}, got ${sourceSha256}`,
  );
}

const entries = sourceBytes.toString("utf8").split(/\r?\n/);

function uniqueFilteredEntries(pattern, normalize = (entry) => entry) {
  const seen = new Set();
  const output = [];
  for (const entry of entries) {
    if (!pattern.test(entry)) continue;

    const normalized = normalize(entry);
    if (
      normalized.length < 2 ||
      normalized.length > 30 ||
      seen.has(normalized)
    ) {
      continue;
    }

    seen.add(normalized);
    output.push(normalized);
  }
  return output;
}

const words = uniqueFilteredEntries(/^[a-z][a-z']+$/);
const namedEntities = uniqueFilteredEntries(/^[A-Z][A-Za-z']+$/, (entry) =>
  entry.toLowerCase(),
);

writeFileSync(wordsOutput, `${words.join("\n")}\n`);
writeFileSync(namedEntitiesOutput, `${namedEntities.join("\n")}\n`);
console.log(
  `wrote ${words.length} words and ${namedEntities.length} normalized named entities`,
);
