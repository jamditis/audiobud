// Parsing + validation for bulk-importing custom dictionary entries.
// Kept as pure functions so the rules are unit-testable without the UI
// (see wordList.test.ts).

export const CUSTOM_WORDS_CAP = 500;
const MAX_ENTRY_LENGTH = 50; // chars, matches the single-word add UI
const MAX_PHRASE_WORDS = 3; // the engine matches n-grams up to 3 words

export interface WordListResult {
  toAdd: string[]; // new entries to append, in order, deduped, within the cap
  addedCount: number;
  duplicateCount: number; // already present (case-insensitive) or repeated in the batch
  invalidCount: number; // dropped for length
  overCapCount: number; // valid + unique but dropped because the cap is full
}

// Strip the same characters the single-word input rejects, then trim. Matches
// the sanitization in the single-word add field.
function sanitizeEntry(raw: string): string {
  return raw.replace(/[<>"'&]/g, "").trim();
}

// Turn one raw token into zero or more valid candidate entries. A 1-3 word
// phrase within the length limit is kept intact; anything longer (too many
// words or too many chars) degrades to individual words so nothing the user
// pasted is silently lost.
function candidatesFromToken(token: string): {
  candidates: string[];
  invalid: number;
} {
  const s = sanitizeEntry(token);
  if (!s) return { candidates: [], invalid: 0 };

  const words = s.split(/\s+/).filter(Boolean);
  if (words.length === 0) return { candidates: [], invalid: 0 };

  const phrase = words.join(" ");
  if (words.length <= MAX_PHRASE_WORDS && phrase.length <= MAX_ENTRY_LENGTH) {
    return { candidates: [phrase], invalid: 0 };
  }

  const candidates: string[] = [];
  let invalid = 0;
  for (const w of words) {
    if (w.length <= MAX_ENTRY_LENGTH) candidates.push(w);
    else invalid++;
  }
  return { candidates, invalid };
}

export function parseWordList(
  raw: string,
  existing: string[],
  cap: number = CUSTOM_WORDS_CAP,
): WordListResult {
  const seen = new Set(existing.map((w) => w.toLowerCase()));
  const toAdd: string[] = [];
  let duplicateCount = 0;
  let invalidCount = 0;
  let overCapCount = 0;

  const remaining = () => cap - existing.length - toAdd.length;

  // Entries may be separated by new lines, commas, semicolons, or tabs.
  const tokens = raw.split(/[\n\r,;\t]+/);
  for (const token of tokens) {
    const { candidates, invalid } = candidatesFromToken(token);
    invalidCount += invalid;
    for (const candidate of candidates) {
      const key = candidate.toLowerCase();
      if (seen.has(key)) {
        duplicateCount++;
        continue;
      }
      // Mark as seen either way so a unique word isn't double-counted later.
      seen.add(key);
      if (remaining() <= 0) {
        overCapCount++;
        continue;
      }
      toAdd.push(candidate);
    }
  }

  return {
    toAdd,
    addedCount: toAdd.length,
    duplicateCount,
    invalidCount,
    overCapCount,
  };
}
