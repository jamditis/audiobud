import { describe, it, expect } from "bun:test";
import { findExtraKeys, findMissingKeys } from "./check-translations";

// en carries only the two plural categories English has.
const reference = {
  customWords: {
    import: {
      added_one: "Added {{count}} word",
      added_other: "Added {{count}} words",
    },
    title: "Custom words",
  },
};

describe("findExtraKeys", () => {
  it("does not flag plural categories the reference language cannot have", () => {
    // Russian needs few/many; English has no such categories, so en can never
    // carry these keys. They are the same plural group, not extra keys.
    const ru = {
      customWords: {
        import: {
          added_one: "Добавлено {{count}} слово",
          added_few: "Добавлено {{count}} слова",
          added_many: "Добавлено {{count}} слов",
          added_other: "Добавлено {{count}} слова",
        },
        title: "Пользовательские слова",
      },
    };
    expect(findExtraKeys(reference, ru)).toEqual([]);
  });

  it("does not flag the six Arabic plural categories", () => {
    const ar = {
      customWords: {
        import: {
          added_zero: "z",
          added_one: "o",
          added_two: "t",
          added_few: "f",
          added_many: "m",
          added_other: "x",
        },
        title: "t",
      },
    };
    expect(findExtraKeys(reference, ar)).toEqual([]);
  });

  it("still flags a genuinely extra key", () => {
    const bad = {
      customWords: {
        import: { added_one: "a", added_other: "b" },
        title: "t",
        inventedKey: "should be reported",
      },
    };
    expect(findExtraKeys(reference, bad)).toEqual([
      ["customWords", "inventedKey"],
    ]);
  });

  it("flags a plural-looking suffix that is not a CLDR category", () => {
    const bad = {
      customWords: {
        import: { added_one: "a", added_other: "b", added_plural: "nope" },
        title: "t",
      },
    };
    expect(findExtraKeys(reference, bad)).toEqual([
      ["customWords", "import", "added_plural"],
    ]);
  });

  it("flags a plural category whose base group is absent from the reference", () => {
    // "removed" is not a plural group in en at all, so removed_few is extra --
    // the exemption must be scoped to groups the reference actually defines.
    const bad = {
      customWords: {
        import: { added_one: "a", added_other: "b", removed_few: "nope" },
        title: "t",
      },
    };
    expect(findExtraKeys(reference, bad)).toEqual([
      ["customWords", "import", "removed_few"],
    ]);
  });
});

describe("findMissingKeys", () => {
  it("reports a key the locale never translated", () => {
    const partial = {
      customWords: { import: { added_one: "a", added_other: "b" } },
    };
    expect(findMissingKeys(reference, partial)).toEqual([
      ["customWords", "title"],
    ]);
  });

  it("accepts a locale that supplies extra plural categories", () => {
    const ru = {
      customWords: {
        import: {
          added_one: "a",
          added_few: "b",
          added_many: "c",
          added_other: "d",
        },
        title: "t",
      },
    };
    expect(findMissingKeys(reference, ru)).toEqual([]);
  });
});
