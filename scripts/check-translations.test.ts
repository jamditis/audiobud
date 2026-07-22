import { describe, it, expect } from "bun:test";
import {
  findExtraKeys,
  findMissingKeys,
  findUntranslatedKeys,
} from "./check-translations";

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

describe("findUntranslatedKeys", () => {
  const copied = {
    customWords: {
      import: {
        added_one: "Ajout de {{count}} mot",
        added_other: "Ajout de {{count}} mots",
      },
      title: "Custom words",
    },
  };

  it("reports a string copied unchanged from the reference", () => {
    expect(findUntranslatedKeys(reference, copied, "fr", {})).toEqual([
      ["customWords", "title"],
    ]);
  });

  it("reports copied English despite case and whitespace drift", () => {
    const stale = structuredClone(copied);
    stale.customWords.title = "  CUSTOM   WORDS  ";
    expect(findUntranslatedKeys(reference, stale, "fr", {})).toEqual([
      ["customWords", "title"],
    ]);
  });

  it("reports copied English written with compatibility characters", () => {
    expect(
      findUntranslatedKeys(
        { status: "Model 1" },
        { status: "Ｍｏｄｅｌ １" },
        "ja",
        {},
      ),
    ).toEqual([["status"]]);
  });

  it("accepts a key that is invariant in every language", () => {
    const allowlist = {
      "customWords.title": { source: "Custom words", locales: "*" as const },
    };
    expect(findUntranslatedKeys(reference, copied, "fr", allowlist)).toEqual(
      [],
    );
  });

  it("keeps language-specific exceptions scoped to one locale", () => {
    const allowlist = {
      "customWords.title": {
        source: "Custom words",
        locales: ["fr"],
      },
    };
    expect(findUntranslatedKeys(reference, copied, "fr", allowlist)).toEqual(
      [],
    );
    expect(findUntranslatedKeys(reference, copied, "de", allowlist)).toEqual([
      ["customWords", "title"],
    ]);
  });

  it("keeps literal dots in key segments distinct from nested paths", () => {
    const dottedReference = {
      "a.b": { label: "Same" },
      a: { b: { label: "Same" } },
    };
    const allowlist = {
      "a\\.b.label": { source: "Same", locales: "*" as const },
    };

    expect(
      findUntranslatedKeys(dottedReference, dottedReference, "fr", allowlist),
    ).toEqual([["a", "b", "label"]]);
  });

  it("expires an exception when the English source changes", () => {
    const changedReference = structuredClone(reference);
    changedReference.customWords.title = "Personal vocabulary";
    const stale = structuredClone(copied);
    stale.customWords.title = "Personal vocabulary";
    const allowlist = {
      "customWords.title": { source: "Custom words", locales: "*" as const },
    };
    expect(
      findUntranslatedKeys(changedReference, stale, "fr", allowlist),
    ).toEqual([["customWords", "title"]]);
  });

  it("accepts translated prose that preserves an interpolation token", () => {
    expect(
      findUntranslatedKeys(
        { status: "Added {{count}} words" },
        { status: "{{count}} mots ajoutés" },
        "fr",
        {},
      ),
    ).toEqual([]);
  });

  it("does not double-report a missing key as untranslated", () => {
    const partial = {
      customWords: {
        import: {
          added_one: "Ajout de {{count}} mot",
          added_other: "Ajout de {{count}} mots",
        },
      },
    };
    expect(findUntranslatedKeys(reference, partial, "fr", {})).toEqual([]);
  });

  it("ignores identical non-string metadata", () => {
    expect(findUntranslatedKeys({ count: 1 }, { count: 1 }, "fr", {})).toEqual(
      [],
    );
  });

  it("flags a copied reference plural without rejecting locale-only variants", () => {
    const ru = {
      customWords: {
        import: {
          added_one: "Added {{count}} word",
          added_few: "Добавлено {{count}} слова",
          added_many: "Добавлено {{count}} слов",
          added_other: "Добавлено {{count}} слова",
        },
        title: "Пользовательские слова",
      },
    };
    expect(findExtraKeys(reference, ru)).toEqual([]);
    expect(findUntranslatedKeys(reference, ru, "ru", {})).toEqual([
      ["customWords", "import", "added_one"],
    ]);
  });

  it("reports copied English in a locale-only plural category", () => {
    const ru = {
      customWords: {
        import: {
          added_one: "Добавлено {{count}} слово",
          added_few: "Added {{count}} words",
          added_many: "Добавлено {{count}} слов",
          added_other: "Добавлено {{count}} слова",
        },
        title: "Пользовательские слова",
      },
    };

    expect(findExtraKeys(reference, ru)).toEqual([]);
    expect(findUntranslatedKeys(reference, ru, "ru", {})).toEqual([
      ["customWords", "import", "added_few"],
    ]);
  });

  it("accepts an allowlisted locale-only plural category", () => {
    const ru = {
      customWords: {
        import: {
          added_one: "Добавлено {{count}} слово",
          added_few: "Added {{count}} words",
          added_other: "Добавлено {{count}} слова",
        },
        title: "Пользовательские слова",
      },
    };
    const allowlist = {
      "customWords.import.added_few": {
        source: "Added {{count}} words",
        locales: ["ru"],
      },
    };

    expect(findUntranslatedKeys(reference, ru, "ru", allowlist)).toEqual([]);
  });

  it("expires a locale-only plural exception when its English source changes", () => {
    const changedReference = structuredClone(reference);
    changedReference.customWords.import.added_other =
      "Imported {{count}} words";
    const ru = {
      customWords: {
        import: {
          added_one: "Добавлено {{count}} слово",
          added_few: "Imported {{count}} words",
          added_other: "Добавлено {{count}} слова",
        },
        title: "Пользовательские слова",
      },
    };
    const allowlist = {
      "customWords.import.added_few": {
        source: "Added {{count}} words",
        locales: ["ru"],
      },
    };

    expect(findUntranslatedKeys(changedReference, ru, "ru", allowlist)).toEqual(
      [["customWords", "import", "added_few"]],
    );
  });

  it("does not scan locale-only plurals from unrelated groups", () => {
    const ru = {
      customWords: {
        import: {
          added_one: "Добавлено {{count}} слово",
          added_other: "Добавлено {{count}} слова",
          removed_few: "Added {{count}} words",
        },
        title: "Пользовательские слова",
      },
    };

    expect(findExtraKeys(reference, ru)).toEqual([
      ["customWords", "import", "removed_few"],
    ]);
    expect(findUntranslatedKeys(reference, ru, "ru", {})).toEqual([]);
  });

  it("keeps reference plural comparisons scoped to the exact path", () => {
    const ru = {
      customWords: {
        import: {
          added_one: "Added {{count}} words",
          added_other: "Добавлено {{count}} слова",
        },
        title: "Пользовательские слова",
      },
    };

    expect(findUntranslatedKeys(reference, ru, "ru", {})).toEqual([]);
  });
});
