import { describe, expect, it } from "bun:test";
import fs from "fs";
import path from "path";
import { fileURLToPath } from "url";
import i18next from "i18next";

// i18next picks a plural form by asking Intl.PluralRules for the CLDR category of
// `count` *in the target language*, then looking up `key_<category>`. The categories
// are language-specific, so a locale can need forms English does not have: en has
// one/other, ru adds few/many, ar has all six.
//
// Two failure modes follow, and these tests cover both:
//
//   1. en itself omits a form it needs -- `{{count}} learned words` renders
//      "1 learned words" (#96).
//   2. A locale omits a form *it* needs, so resolution falls through
//      `fallbackLng: "en"` and the user sees English mid-UI (#96 again, for the
//      import toast, fixed in #98).
//
// Mode 2 is the subtle one. A key that exists as a *bare* key (no _one/_other) is
// accidentally immune: i18next tries `count_few`, misses, then finds the bare
// `count` in the same locale and stops before reaching en. Converting such a key to
// plural form removes that safety net, so every locale must gain the categories it
// can actually reach or the conversion *causes* mode 2. That is what the leak test
// below pins down.

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const LOCALES_DIR = path.join(__dirname, "..", "src", "i18n", "locales");
const REFERENCE_LANG = "en";

// Every CLDR category is reachable within this range for every language we ship
// (ar `other` starts at 100; ar `many` at 11; pl/ru `many` at 5). Categories that
// only fractional counts reach -- ru/uk/pl `other` -- are unreachable by design:
// every call site passes an array length.
const MAX_COUNT = 200;

type TranslationData = Record<string, unknown>;

const load = (lang: string): TranslationData =>
  JSON.parse(
    fs.readFileSync(path.join(LOCALES_DIR, lang, "translation.json"), "utf8"),
  ) as TranslationData;

const languages = fs
  .readdirSync(LOCALES_DIR, { withFileTypes: true })
  .filter((entry) => entry.isDirectory())
  .map((entry) => entry.name);

const reference = load(REFERENCE_LANG);

const PLURAL_CATEGORIES = ["zero", "one", "two", "few", "many", "other"];
const PLURAL_SUFFIX = new RegExp(`_(${PLURAL_CATEGORIES.join("|")})$`);

// Collect the keys a call site would pass `count` to. Discovering these from the
// resource file rather than hardcoding a list means a new {{count}} string added to
// en is guarded automatically, instead of silently escaping these tests.
function countKeys(data: TranslationData): string[] {
  const found: string[] = [];
  const walk = (node: TranslationData, prefix: string[]): void => {
    for (const key of Object.keys(node)) {
      const value = node[key];
      const keyPath = prefix.concat([key]);
      if (value && typeof value === "object") {
        walk(value as TranslationData, keyPath);
      } else if (typeof value === "string" && value.includes("{{count}}")) {
        // Strip the plural suffix: `added_one` is looked up as `added`.
        found.push(keyPath.join(".").replace(PLURAL_SUFFIX, ""));
      }
    }
  };
  walk(data, []);
  return [...new Set(found)].sort();
}

const KEYS = countKeys(reference);

// Replace every en *value* with a sentinel naming its own key. Key sets are
// untouched, so plural resolution behaves exactly as in production -- only the
// payload changes. Any locale render containing a sentinel has provably resolved
// against en. This tests the real resolver instead of re-implementing its lookup
// order, which is where reasoning about i18next tends to go wrong.
const SENTINEL_MARK = "@@EN_FALLBACK@@";
function sentinelise(node: TranslationData, prefix: string[]): TranslationData {
  const out: TranslationData = {};
  for (const key of Object.keys(node)) {
    const value = node[key];
    const keyPath = prefix.concat([key]);
    out[key] =
      value && typeof value === "object"
        ? sentinelise(value as TranslationData, keyPath)
        : `${SENTINEL_MARK}${keyPath.join(".")}`;
  }
  return out;
}

async function makeInstance(resources: Record<string, unknown>) {
  const instance = i18next.createInstance();
  await instance.init({
    resources: resources as never,
    lng: REFERENCE_LANG,
    fallbackLng: REFERENCE_LANG,
    interpolation: { escapeValue: false },
  });
  return instance;
}

// Interpolation values for the non-count variables these strings also take. Their
// content is irrelevant here; they only need to be present so a render never fails
// for an unrelated reason.
const VARS = { cap: 500, max: 40, word: "word", modelName: "model" };

describe("en plural coverage", () => {
  it("renders the singular form of learned.count at count 1", async () => {
    const i18n = await makeInstance({
      [REFERENCE_LANG]: { translation: reference },
    });
    const key = "settings.advanced.personalization.learned.count";
    // The call site renders this whenever `learnedWords.length > 0`, so count 1 is
    // reachable rather than theoretical.
    expect(i18n.t(key, { count: 1, ...VARS })).toBe("1 learned word");
    expect(i18n.t(key, { count: 2, ...VARS })).toBe("2 learned words");
  });
});

describe("locale plural coverage", () => {
  it("never falls back to English for a key that interpolates count", async () => {
    const resources: Record<string, unknown> = {
      [REFERENCE_LANG]: { translation: sentinelise(reference, []) },
    };
    for (const lang of languages) {
      if (lang !== REFERENCE_LANG)
        resources[lang] = { translation: load(lang) };
    }
    const i18n = await makeInstance(resources);

    const leaks: string[] = [];
    for (const lang of languages) {
      if (lang === REFERENCE_LANG) continue;
      for (const key of KEYS) {
        const broken: number[] = [];
        for (let count = 0; count <= MAX_COUNT; count++) {
          const out = i18n.t(key, { lng: lang, count, ...VARS });
          if (typeof out === "string" && out.includes(SENTINEL_MARK)) {
            broken.push(count);
          }
        }
        if (broken.length > 0) {
          const categories = [
            ...new Set(broken.map((n) => new Intl.PluralRules(lang).select(n))),
          ];
          leaks.push(
            `${lang} "${key}" is missing [${categories.join(", ")}] ` +
              `(${broken.length} of ${MAX_COUNT + 1} counts render English)`,
          );
        }
      }
    }
    expect(leaks).toEqual([]);
  });
});
