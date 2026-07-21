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
type MetadataTagName = "link" | "meta";
const htmlAsciiWhitespace = new Set([" ", "\t", "\n", "\r", "\f"]);
const isHtmlAsciiWhitespace = (character: string | undefined) =>
  character !== undefined && htmlAsciiWhitespace.has(character);
const isAsciiLetter = (character: string | undefined) =>
  character !== undefined && /[A-Za-z]/.test(character);
const isTagNameCharacter = (character: string | undefined) =>
  isAsciiLetter(character) ||
  (character !== undefined && /[0-9:-]/.test(character));
const isTagNameDelimiter = (character: string | undefined) =>
  isHtmlAsciiWhitespace(character) || character === "/" || character === ">";
const parseQuotedAttributes = (source: string) => {
  const attributes = new Map<string, string>();
  let cursor = 0;

  while (cursor < source.length) {
    while (
      cursor < source.length &&
      (isHtmlAsciiWhitespace(source[cursor]) || source[cursor] === "/")
    )
      cursor++;

    const nameStart = cursor;
    while (
      cursor < source.length &&
      !isHtmlAsciiWhitespace(source[cursor]) &&
      !"=/'\"<>".includes(source[cursor])
    )
      cursor++;
    const name = source.slice(nameStart, cursor).toLowerCase();

    while (cursor < source.length && isHtmlAsciiWhitespace(source[cursor]))
      cursor++;
    if (!name) {
      cursor++;
      continue;
    }
    if (source[cursor] !== "=") {
      if (!attributes.has(name)) attributes.set(name, "");
      continue;
    }

    cursor++;
    while (cursor < source.length && isHtmlAsciiWhitespace(source[cursor]))
      cursor++;

    const quote = source[cursor];
    if (quote !== '"' && quote !== "'") {
      const valueStart = cursor;
      while (cursor < source.length && !isHtmlAsciiWhitespace(source[cursor]))
        cursor++;
      if (!attributes.has(name)) {
        attributes.set(name, source.slice(valueStart, cursor));
      }
      continue;
    }

    cursor++;
    const valueStart = cursor;
    while (cursor < source.length && source[cursor] !== quote) cursor++;
    if (cursor === source.length) break;

    if (!attributes.has(name)) {
      attributes.set(name, source.slice(valueStart, cursor));
    }
    cursor++;
  }

  return attributes;
};
const findOpeningTagEnd = (html: string, start: number) => {
  let quote: string | null = null;

  for (let cursor = start; cursor < html.length; cursor++) {
    const character = html[cursor];
    if (quote) {
      if (character === quote) quote = null;
    } else if (character === '"' || character === "'") {
      quote = character;
    } else if (character === ">") {
      return cursor;
    }
  }

  return null;
};
const scanMetadataOpeningTags = (html: string) => {
  const tags: Array<{
    name: MetadataTagName;
    attributes: Map<string, string>;
  }> = [];
  let cursor = 0;
  let hasSeenHead = false;
  let hasSeenBody = false;
  let isInsideHead = false;

  while (cursor < html.length) {
    const tagStart = html.indexOf("<", cursor);
    if (tagStart === -1) break;

    if (html.startsWith("<!--", tagStart)) {
      const commentEnd = html.indexOf("-->", tagStart + 4);
      if (commentEnd === -1) return null;
      cursor = commentEnd + 3;
      continue;
    }

    let nameStart = tagStart + 1;
    const isClosingTag = html[nameStart] === "/";
    if (isClosingTag) nameStart++;

    if (!isAsciiLetter(html[nameStart])) {
      cursor = tagStart + 1;
      continue;
    }

    let nameEnd = nameStart;
    while (nameEnd < html.length && isTagNameCharacter(html[nameEnd]))
      nameEnd++;

    const name = html.slice(nameStart, nameEnd).toLowerCase();
    const hasValidNameDelimiter = isTagNameDelimiter(html[nameEnd]);
    const tagEnd = findOpeningTagEnd(html, nameEnd);
    if (tagEnd === null) return null;

    if (isClosingTag) {
      if (hasValidNameDelimiter && name === "head") isInsideHead = false;
      cursor = tagEnd + 1;
      continue;
    }

    if (hasValidNameDelimiter && name === "head") {
      if (!hasSeenHead && !hasSeenBody) {
        hasSeenHead = true;
        isInsideHead = true;
      }
      cursor = tagEnd + 1;
      continue;
    }

    if (hasValidNameDelimiter && name === "body") {
      hasSeenBody = true;
      isInsideHead = false;
      cursor = tagEnd + 1;
      continue;
    }

    if (
      hasValidNameDelimiter &&
      ["script", "style", "template", "title", "textarea"].includes(name)
    ) {
      const closingTag = new RegExp(`</${name}[ \\t\\n\\r\\f]*>`, "gi");
      closingTag.lastIndex = tagEnd + 1;
      const closingMatch = closingTag.exec(html);
      if (!closingMatch) return null;
      cursor = closingTag.lastIndex;
      continue;
    }

    if (
      isInsideHead &&
      hasValidNameDelimiter &&
      (name === "link" || name === "meta")
    ) {
      tags.push({
        name,
        attributes: parseQuotedAttributes(html.slice(nameEnd, tagEnd)),
      });
    }
    cursor = tagEnd + 1;
  }

  return tags;
};
const hasTagWithAttributes = (
  html: string,
  tag: MetadataTagName,
  attributes: Record<string, string>,
) => {
  const metadataTags = scanMetadataOpeningTags(html);
  if (!metadataTags) return false;

  const identity =
    tag === "link" && attributes.rel === "canonical"
      ? (["rel", "canonical"] as const)
      : tag === "meta" &&
          (attributes.property === "og:url" ||
            attributes.property === "og:image")
        ? (["property", attributes.property] as const)
        : tag === "meta" && attributes.name === "twitter:image"
          ? (["name", "twitter:image"] as const)
          : null;
  if (!identity) return false;

  const candidates = metadataTags.filter(
    (candidate) =>
      candidate.name === tag &&
      candidate.attributes.get(identity[0]) === identity[1],
  );
  return (
    candidates.length === 1 &&
    Object.entries(attributes).every(
      ([name, value]) =>
        candidates[0].attributes.get(name.toLowerCase()) === value,
    )
  );
};
const expectTagWithAttributes = (
  html: string,
  tag: MetadataTagName,
  attributes: Record<string, string>,
) => {
  expect(hasTagWithAttributes(html, tag, attributes)).toBe(true);
};

describe("Metadata tag matcher", () => {
  it("accepts reordered real attributes", () => {
    expectTagWithAttributes(
      '<head><meta content="https://example.com/page" property="og:url" /></head>',
      "meta",
      { property: "og:url", content: "https://example.com/page" },
    );
  });

  it("accepts single or double quotes", () => {
    expectTagWithAttributes(
      "<head><link rel='canonical' href='https://example.com/page' /></head>",
      "link",
      { rel: "canonical", href: "https://example.com/page" },
    );
    expectTagWithAttributes(
      '<head><link rel="canonical" href="https://example.com/page" /></head>',
      "link",
      { rel: "canonical", href: "https://example.com/page" },
    );
  });

  it("accepts HTML ASCII whitespace around attributes and equals signs", () => {
    for (const whitespace of [" ", "\t", "\n", "\r", "\f"]) {
      expectTagWithAttributes(
        `<head><meta${whitespace}property${whitespace}=${whitespace}"og:url"${whitespace}content${whitespace}=${whitespace}"https://example.com/page" /></head>`,
        "meta",
        { property: "og:url", content: "https://example.com/page" },
      );
    }
  });

  it("rejects data attribute suffixes", () => {
    expect(
      hasTagWithAttributes(
        '<head><link data-rel="canonical" data-href="https://example.com/page" /></head>',
        "link",
        { rel: "canonical", href: "https://example.com/page" },
      ),
    ).toBe(false);
  });

  it("rejects attributes embedded inside another quoted value", () => {
    expect(
      hasTagWithAttributes(
        `<head><meta data-copy='property="og:url" content="https://example.com/page"' /></head>`,
        "meta",
        { property: "og:url", content: "https://example.com/page" },
      ),
    ).toBe(false);
  });

  it("rejects a complete tag inside an HTML comment", () => {
    expect(
      hasTagWithAttributes(
        '<head><!-- <meta property="og:url" content="https://example.com/page" /> --></head>',
        "meta",
        { property: "og:url", content: "https://example.com/page" },
      ),
    ).toBe(false);
  });

  it("rejects a complete tag inside another element attribute", () => {
    expect(
      hasTagWithAttributes(
        `<head><div data-copy='<meta property="og:url" content="https://example.com/page" />'></div></head>`,
        "meta",
        { property: "og:url", content: "https://example.com/page" },
      ),
    ).toBe(false);
  });

  it("rejects a complete tag inside script text", () => {
    expect(
      hasTagWithAttributes(
        `<head><script>const tag = '<meta property="og:url" content="https://example.com/page" />';</script></head>`,
        "meta",
        { property: "og:url", content: "https://example.com/page" },
      ),
    ).toBe(false);
  });

  it("rejects a punctuation-suffixed meta tag name", () => {
    expect(
      hasTagWithAttributes(
        '<head><meta! property="og:url" content="https://example.com/page" /></head>',
        "meta",
        { property: "og:url", content: "https://example.com/page" },
      ),
    ).toBe(false);
  });

  it("rejects an underscore-suffixed link tag name", () => {
    expect(
      hasTagWithAttributes(
        '<head><link_ rel="canonical" href="https://example.com/page" /></head>',
        "link",
        { rel: "canonical", href: "https://example.com/page" },
      ),
    ).toBe(false);
  });

  it("rejects a complete tag inside title text", () => {
    expect(
      hasTagWithAttributes(
        '<head><title><meta property="og:url" content="https://example.com/page" /></title></head>',
        "meta",
        { property: "og:url", content: "https://example.com/page" },
      ),
    ).toBe(false);
  });

  it("rejects a complete tag inside textarea text", () => {
    expect(
      hasTagWithAttributes(
        '<head><textarea><meta property="og:url" content="https://example.com/page" /></textarea></head>',
        "meta",
        { property: "og:url", content: "https://example.com/page" },
      ),
    ).toBe(false);
  });

  it("rejects metadata in the document body", () => {
    expect(
      hasTagWithAttributes(
        '<html><head></head><body><meta property="og:url" content="https://example.com/page" /></body></html>',
        "meta",
        { property: "og:url", content: "https://example.com/page" },
      ),
    ).toBe(false);
  });

  it("rejects metadata inside template content", () => {
    expect(
      hasTagWithAttributes(
        '<head><template><meta property="og:url" content="https://example.com/page" /></template></head>',
        "meta",
        { property: "og:url", content: "https://example.com/page" },
      ),
    ).toBe(false);
  });

  it("rejects metadata inside style text", () => {
    expect(
      hasTagWithAttributes(
        `<head><style>.example { content: '<meta property="og:url" content="https://example.com/page" />'; }</style></head>`,
        "meta",
        { property: "og:url", content: "https://example.com/page" },
      ),
    ).toBe(false);
  });

  it("rejects a conflicting canonical before the expected tag", () => {
    expect(
      hasTagWithAttributes(
        '<head><link rel="canonical" href="https://example.com/wrong" /><link rel="canonical" href="https://example.com/page" /></head>',
        "link",
        { rel: "canonical", href: "https://example.com/page" },
      ),
    ).toBe(false);
  });

  it("rejects a conflicting canonical after the expected tag", () => {
    expect(
      hasTagWithAttributes(
        '<head><link rel="canonical" href="https://example.com/page" /><link rel="canonical" href="https://example.com/wrong" /></head>',
        "link",
        { rel: "canonical", href: "https://example.com/page" },
      ),
    ).toBe(false);
  });

  it("rejects a conflicting Open Graph URL before the expected tag", () => {
    expect(
      hasTagWithAttributes(
        '<head><meta property="og:url" content="https://example.com/wrong" /><meta property="og:url" content="https://example.com/page" /></head>',
        "meta",
        { property: "og:url", content: "https://example.com/page" },
      ),
    ).toBe(false);
  });

  it("rejects a conflicting Open Graph URL after the expected tag", () => {
    expect(
      hasTagWithAttributes(
        '<head><meta property="og:url" content="https://example.com/page" /><meta property="og:url" content="https://example.com/wrong" /></head>',
        "meta",
        { property: "og:url", content: "https://example.com/page" },
      ),
    ).toBe(false);
  });

  it("rejects a later duplicate with the expected value", () => {
    expect(
      hasTagWithAttributes(
        '<head><meta property="wrong" property="og:url" content="https://example.com/page" /></head>',
        "meta",
        { property: "og:url", content: "https://example.com/page" },
      ),
    ).toBe(false);
  });

  it("fails closed for an unmatched quote", () => {
    expect(
      hasTagWithAttributes(
        '<head><meta property="og:url content="https://example.com/page"></head>',
        "meta",
        { property: "og:url", content: "https://example.com/page" },
      ),
    ).toBe(false);
  });

  it("fails closed for a truncated tag", () => {
    expect(
      hasTagWithAttributes(
        '<head><meta property="og:url" content="https://example.com/page"',
        "meta",
        { property: "og:url", content: "https://example.com/page" },
      ),
    ).toBe(false);
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
