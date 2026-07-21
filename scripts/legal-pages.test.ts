import { describe, expect, it } from "bun:test";
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";

const root = join(import.meta.dir, "..");
const docs = join(root, "docs");
const read = (name: string) => readFileSync(join(docs, name), "utf8");

const sitePages = [
  { name: "index.html", url: "https://audiobud.amditis.tech/" },
  {
    name: "roadmap.html",
    url: "https://audiobud.amditis.tech/roadmap.html",
  },
  {
    name: "privacy.html",
    url: "https://audiobud.amditis.tech/privacy.html",
  },
  { name: "terms.html", url: "https://audiobud.amditis.tech/terms.html" },
];
const socialImage = "https://audiobud.amditis.tech/assets/og-image.png";
const parseQuotedAttributes = (source: string) => {
  const attributes = new Map<string, string>();
  let cursor = 0;

  while (cursor < source.length) {
    while (cursor < source.length && /[\s/]/.test(source[cursor])) cursor++;

    const nameStart = cursor;
    while (cursor < source.length && !/[\s=/'"<>]/.test(source[cursor]))
      cursor++;
    const name = source.slice(nameStart, cursor);

    while (cursor < source.length && /\s/.test(source[cursor])) cursor++;
    if (!name) {
      cursor++;
      continue;
    }
    if (source[cursor] !== "=") continue;

    cursor++;
    while (cursor < source.length && /\s/.test(source[cursor])) cursor++;

    const quote = source[cursor];
    if (quote !== '"' && quote !== "'") {
      while (cursor < source.length && !/\s/.test(source[cursor])) cursor++;
      continue;
    }

    cursor++;
    const valueStart = cursor;
    while (cursor < source.length && source[cursor] !== quote) cursor++;
    if (cursor === source.length) break;

    attributes.set(name, source.slice(valueStart, cursor));
    cursor++;
  }

  return attributes;
};
const openingTagAttributes = (html: string, tag: "link" | "meta") => {
  const tags: Map<string, string>[] = [];
  const tagStart = new RegExp(`<${tag}(?=[\\s/>])`, "g");
  let match: RegExpExecArray | null;

  while ((match = tagStart.exec(html)) !== null) {
    let cursor = tagStart.lastIndex;
    let quote: string | null = null;

    for (; cursor < html.length; cursor++) {
      const character = html[cursor];
      if (quote) {
        if (character === quote) quote = null;
      } else if (character === '"' || character === "'") {
        quote = character;
      } else if (character === ">") {
        tags.push(
          parseQuotedAttributes(html.slice(tagStart.lastIndex, cursor)),
        );
        tagStart.lastIndex = cursor + 1;
        break;
      }
    }
  }

  return tags;
};
const expectTagWithAttributes = (
  html: string,
  tag: "link" | "meta",
  attributes: Record<string, string>,
) => {
  const hasMatch = openingTagAttributes(html, tag).some((candidate) =>
    Object.entries(attributes).every(
      ([name, value]) => candidate.get(name) === value,
    ),
  );
  expect(hasMatch).toBe(true);
};

describe("Metadata tag matcher", () => {
  it("accepts reordered real attributes", () => {
    expectTagWithAttributes(
      '<meta content="https://example.com/page" property="og:url" />',
      "meta",
      { property: "og:url", content: "https://example.com/page" },
    );
  });

  it("accepts single or double quotes", () => {
    expectTagWithAttributes(
      "<link rel='canonical' href='https://example.com/page' />",
      "link",
      { rel: "canonical", href: "https://example.com/page" },
    );
    expectTagWithAttributes(
      '<link rel="canonical" href="https://example.com/page" />',
      "link",
      { rel: "canonical", href: "https://example.com/page" },
    );
  });

  it("accepts whitespace around equals signs", () => {
    expectTagWithAttributes(
      '<meta property = "og:url" content= "https://example.com/page" />',
      "meta",
      { property: "og:url", content: "https://example.com/page" },
    );
  });

  it("rejects data attribute suffixes", () => {
    expect(() =>
      expectTagWithAttributes(
        '<link data-rel="canonical" data-href="https://example.com/page" />',
        "link",
        { rel: "canonical", href: "https://example.com/page" },
      ),
    ).toThrow();
  });

  it("rejects attributes embedded inside another quoted value", () => {
    expect(() =>
      expectTagWithAttributes(
        `<meta data-copy='property="og:url" content="https://example.com/page"' />`,
        "meta",
        { property: "og:url", content: "https://example.com/page" },
      ),
    ).toThrow();
  });
});

describe("AudioBud public policy pages", () => {
  it("publishes the custom domain from docs", () => {
    expect(read("CNAME").trim()).toBe("audiobud.amditis.tech");
  });

  for (const page of ["privacy.html", "terms.html"]) {
    it(`publishes ${page}`, () => {
      expect(existsSync(join(docs, page))).toBe(true);
    });
  }

  for (const page of sitePages) {
    it(`uses exact custom-domain metadata in ${page.name}`, () => {
      const html = read(page.name);
      expectTagWithAttributes(html, "link", {
        rel: "canonical",
        href: page.url,
      });
      expectTagWithAttributes(html, "meta", {
        property: "og:url",
        content: page.url,
      });
      expectTagWithAttributes(html, "meta", {
        property: "og:image",
        content: socialImage,
      });
      expectTagWithAttributes(html, "meta", {
        name: "twitter:image",
        content: socialImage,
      });
      expect(html).not.toContain("https://jamditis.github.io/audiobud");
    });
  }

  it("links privacy and terms from every public page", () => {
    for (const page of sitePages) {
      const html = read(page.name);
      expect(html).toContain('href="./privacy.html"');
      expect(html).toContain('href="./terms.html"');
    }
  });

  it("describes the local-first boundary without an encryption promise", () => {
    const privacy = read("privacy.html");
    expect(privacy).toContain(
      "AudioBud does not send your audio to an AudioBud server",
    );
    expect(privacy).toContain("transcript text and the selected prompt");
    expect(privacy).toContain("does not send your audio to that provider");
    expect(privacy).toContain("stored in AudioBud's local settings file");
    expect(privacy.toLowerCase()).not.toContain("encrypted at rest");
  });

  it("states the website tracking and sale practices", () => {
    const privacy = read("privacy.html");
    expect(privacy).toContain("no AudioBud analytics");
    expect(privacy).toContain("does not sell personal information");
    expect(privacy).toContain("Global Privacy Control");
  });

  it("keeps source-code rights under the MIT license", () => {
    const terms = read("terms.html");
    const normalizedTerms = terms.toLowerCase();
    expect(terms).toContain("MIT License");
    expect(terms).toContain(
      "copying, modifying, or distributing the source code",
    );
    expect(normalizedTerms).not.toContain("class-action waiver");
    expect(normalizedTerms).not.toContain("binding arbitration");
  });
});
