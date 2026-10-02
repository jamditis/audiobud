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

// This range covers the common integer categories. Sparse categories such as the
// Romance `many` at 1,000,000 are checked for the affected keys below. Categories
// that only fractional counts reach are unreachable because call sites pass integers.
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
const AFFECTED_COUNT_KEYS = [
  "settings.advanced.customWords.import.skippedInvalid",
  "settings.advanced.personalization.suggestions.count",
];

const COUNT_TOKEN_EXEMPTIONS = new Set([
  // These established forms spell out zero or two, or use a dual noun instead of a digit.
  "ar|settings.advanced.customWords.import.added|0",
  "ar|settings.advanced.customWords.import.added|2",
  "ar|settings.advanced.personalization.learned.count|0",
  "ar|settings.advanced.personalization.learned.count|2",
  "he|settings.advanced.customWords.import.added|2",
]);

function containsExactCount(value: string, count: number): boolean {
  return new RegExp(`(?<!\\p{N})${count}(?!\\p{N})`, "u").test(value);
}

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

describe("count agreement", () => {
  it("renders the affected import and suggestion forms", async () => {
    const resources = Object.fromEntries(
      languages.map((lang) => [lang, { translation: load(lang) }]),
    );
    const i18n = await makeInstance(resources);
    const importKey = "settings.advanced.customWords.import.skippedInvalid";
    const suggestionKey = "settings.advanced.personalization.suggestions.count";
    const cases: Array<[string, string, number, string]> = [
      ["fr", importKey, 1, "1 trop long"],
      ["fr", importKey, 2, "2 trop longs"],
      ["fr", importKey, 1_000_000, "1000000 trop longs"],
      ["es", importKey, 1, "1 demasiado larga"],
      ["es", importKey, 1_000_000, "1000000 demasiado largas"],
      ["it", importKey, 1, "1 troppo lunga"],
      ["it", importKey, 1_000_000, "1000000 troppo lunghe"],
      ["pt", importKey, 1, "1 longa demais"],
      ["pt", importKey, 1_000_000, "1000000 longas demais"],
      ["bg", importKey, 1, "1 твърде дълга"],
      ["sv", importKey, 1, "1 för långt"],
      ["he", importKey, 1, "1 ארוכה מדי"],
      ["ru", importKey, 1, "1 слишком длинная запись"],
      ["ru", importKey, 2, "2 слишком длинные записи"],
      ["ru", importKey, 5, "5 слишком длинных записей"],
      ["ru", importKey, 21, "21 слишком длинная запись"],
      ["uk", importKey, 1, "1 задовгий запис"],
      ["uk", importKey, 2, "2 задовгі записи"],
      ["uk", importKey, 5, "5 задовгих записів"],
      ["uk", importKey, 21, "21 задовгий запис"],
      ["pl", importKey, 1, "1 zbyt długi wpis"],
      ["pl", importKey, 2, "2 zbyt długie wpisy"],
      ["pl", importKey, 5, "5 zbyt długich wpisów"],
      ["cs", importKey, 1, "1 příliš dlouhá položka"],
      ["cs", importKey, 2, "2 příliš dlouhé položky"],
      ["cs", importKey, 5, "5 příliš dlouhých položek"],
      ["en", suggestionKey, 1, "seen 1 time"],
      ["es", suggestionKey, 1, "visto 1 vez"],
      ["it", suggestionKey, 1, "vista 1 volta"],
      ["pt", suggestionKey, 1, "vista 1 vez"],
      ["bg", suggestionKey, 1, "среща се 1 път"],
      ["sv", suggestionKey, 1, "förekommer 1 gång"],
      ["he", suggestionKey, 1, "נראתה 1 פעם"],
      ["ru", suggestionKey, 1, "встречается 1 раз"],
      ["ru", suggestionKey, 2, "встречается 2 раза"],
      ["ru", suggestionKey, 3, "встречается 3 раза"],
      ["ru", suggestionKey, 5, "встречается 5 раз"],
      ["ru", suggestionKey, 11, "встречается 11 раз"],
      ["ru", suggestionKey, 21, "встречается 21 раз"],
      ["ru", suggestionKey, 22, "встречается 22 раза"],
      ["ru", suggestionKey, 100, "встречается 100 раз"],
      ["ru", suggestionKey, 101, "встречается 101 раз"],
      ["uk", suggestionKey, 1, "зустрічається 1 раз"],
      ["uk", suggestionKey, 2, "зустрічається 2 рази"],
      ["uk", suggestionKey, 3, "зустрічається 3 рази"],
      ["uk", suggestionKey, 5, "зустрічається 5 разів"],
      ["uk", suggestionKey, 11, "зустрічається 11 разів"],
      ["uk", suggestionKey, 21, "зустрічається 21 раз"],
      ["uk", suggestionKey, 22, "зустрічається 22 рази"],
      ["uk", suggestionKey, 100, "зустрічається 100 разів"],
      ["uk", suggestionKey, 101, "зустрічається 101 раз"],
      ["ar", suggestionKey, 3, "عدد مرات الظهور: 3"],
      ["ar", suggestionKey, 11, "عدد مرات الظهور: 11"],
      ["ar", suggestionKey, 100, "عدد مرات الظهور: 100"],
      ["fr", suggestionKey, 1_000_000, "vu 1000000 fois"],
      ["es", suggestionKey, 1_000_000, "visto 1000000 veces"],
      ["it", suggestionKey, 1_000_000, "vista 1000000 volte"],
      ["pt", suggestionKey, 1_000_000, "vista 1000000 vezes"],
    ];

    for (const [lang, key, count, expected] of cases) {
      expect(i18n.t(key, { lng: lang, count, ...VARS })).toBe(expected);
    }
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
        const counts = Array.from(
          { length: MAX_COUNT + 1 },
          (_, count) => count,
        );
        if (AFFECTED_COUNT_KEYS.includes(key)) counts.push(1_000_000);
        for (const count of counts) {
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
              `(${broken.length} of ${counts.length} counts render English)`,
          );
        }
      }
    }
    expect(leaks).toEqual([]);
  });

  it("preserves the exact count numeral in rendered count strings", async () => {
    const resources = Object.fromEntries(
      languages.map((lang) => [lang, { translation: load(lang) }]),
    );
    const i18n = await makeInstance(resources);
    const missingCounts: string[] = [];

    for (const lang of languages) {
      for (const key of KEYS) {
        const counts = Array.from(
          { length: MAX_COUNT + 1 },
          (_, count) => count,
        );
        if (AFFECTED_COUNT_KEYS.includes(key)) counts.push(1_000_000);
        for (const count of counts) {
          if (COUNT_TOKEN_EXEMPTIONS.has(`${lang}|${key}|${count}`)) continue;

          const out = i18n.t(key, {
            lng: lang,
            count,
            ...VARS,
            max: 400,
          });
          if (typeof out === "string" && !containsExactCount(out, count)) {
            missingCounts.push(`${lang} "${key}" at count ${count}: ${out}`);
          }
        }
      }
    }

    expect(missingCounts).toEqual([]);
  });
});
