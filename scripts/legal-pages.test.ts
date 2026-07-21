import { describe, expect, it } from "bun:test";
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";

const root = join(import.meta.dir, "..");
const docs = join(root, "docs");
const read = (name: string) => readFileSync(join(docs, name), "utf8");
const readText = (name: string) =>
  read(name)
    .replace(/<[^>]+>/g, " ")
    .replace(/\s+/g, " ")
    .trim();

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
const hasCanonicalRelToken = (value: string | undefined) =>
  value !== undefined &&
  value
    .split(/[ \t\n\r\f]+/)
    .some((token) => token.toLowerCase() === "canonical");
type MetadataIdentity = readonly [
  attribute: "rel" | "name" | "property",
  value: string,
];
const metadataIdentityRules = [
  { attribute: "property", value: "og:url" },
  { attribute: "property", value: "og:image" },
  { attribute: "property", value: "og:description" },
  { attribute: "property", value: "og:image:alt" },
  { attribute: "name", value: "twitter:image" },
  { attribute: "name", value: "description" },
  { attribute: "name", value: "twitter:description" },
  { attribute: "name", value: "twitter:image:alt" },
] as const;
const resolveMetadataIdentity = (
  tag: MetadataTagName,
  attributes: Record<string, string>,
): MetadataIdentity | null => {
  if (tag === "link" && hasCanonicalRelToken(attributes.rel)) {
    return ["rel", "canonical"];
  }
  if (tag !== "meta") return null;

  const rule = metadataIdentityRules.find(
    ({ attribute, value }) => attributes[attribute] === value,
  );
  return rule ? [rule.attribute, rule.value] : null;
};
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
    isInFirstHead: boolean;
  }> = [];
  let cursor = 0;
  let hasSeenHead = false;
  let hasSeenBody = false;
  let isInsideHead = false;
  let templateDepth = 0;

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
      if (hasValidNameDelimiter && name === "template" && templateDepth > 0) {
        templateDepth--;
      } else if (
        templateDepth === 0 &&
        hasValidNameDelimiter &&
        name === "head"
      ) {
        isInsideHead = false;
      }
      cursor = tagEnd + 1;
      continue;
    }

    if (hasValidNameDelimiter && name === "template") {
      templateDepth++;
      cursor = tagEnd + 1;
      continue;
    }

    if (
      hasValidNameDelimiter &&
      ["script", "style", "title", "textarea"].includes(name)
    ) {
      const closingTag = new RegExp(`</${name}[ \\t\\n\\r\\f]*>`, "gi");
      closingTag.lastIndex = tagEnd + 1;
      const closingMatch = closingTag.exec(html);
      if (!closingMatch) return null;
      cursor = closingTag.lastIndex;
      continue;
    }

    if (templateDepth > 0) {
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

    if (hasValidNameDelimiter && (name === "link" || name === "meta")) {
      tags.push({
        name,
        attributes: parseQuotedAttributes(html.slice(nameEnd, tagEnd)),
        isInFirstHead: isInsideHead,
      });
    }
    cursor = tagEnd + 1;
  }

  return templateDepth === 0 ? tags : null;
};
const hasTagWithAttributes = (
  html: string,
  tag: MetadataTagName,
  attributes: Record<string, string>,
) => {
  const metadataTags = scanMetadataOpeningTags(html);
  if (!metadataTags) return false;

  const identity = resolveMetadataIdentity(tag, attributes);
  if (!identity) return false;

  const candidates = metadataTags.filter(
    (candidate) =>
      candidate.name === tag &&
      (identity[0] === "rel"
        ? hasCanonicalRelToken(candidate.attributes.get("rel"))
        : candidate.attributes.get(identity[0]) === identity[1]),
  );
  return (
    candidates.length === 1 &&
    candidates[0].isInFirstHead &&
    Object.entries(attributes).every(([name, value]) =>
      tag === "link" && name.toLowerCase() === "rel"
        ? hasCanonicalRelToken(candidates[0].attributes.get("rel"))
        : candidates[0].attributes.get(name.toLowerCase()) === value,
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

  it("rejects metadata after a nested template", () => {
    expect(
      hasTagWithAttributes(
        '<head><template><template></template><meta property="og:url" content="https://example.com/page" /></template></head>',
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

  it("rejects an uppercase canonical conflict", () => {
    expect(
      hasTagWithAttributes(
        '<head><link rel="canonical" href="https://example.com/page" /><link rel="CANONICAL" href="https://example.com/wrong" /></head>',
        "link",
        { rel: "canonical", href: "https://example.com/page" },
      ),
    ).toBe(false);
  });

  it("rejects a multi-token canonical conflict", () => {
    expect(
      hasTagWithAttributes(
        '<head><link rel="canonical" href="https://example.com/page" /><link rel="alternate canonical" href="https://example.com/wrong" /></head>',
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

  it("rejects a canonical duplicate in the body", () => {
    expect(
      hasTagWithAttributes(
        '<html><head><link rel="canonical" href="https://example.com/page" /></head><body><link rel="canonical" href="https://example.com/wrong" /></body></html>',
        "link",
        { rel: "canonical", href: "https://example.com/page" },
      ),
    ).toBe(false);
  });

  it("rejects a canonical duplicate in a second head", () => {
    expect(
      hasTagWithAttributes(
        '<html><head><link rel="canonical" href="https://example.com/page" /></head><head><link rel="canonical" href="https://example.com/wrong" /></head></html>',
        "link",
        { rel: "canonical", href: "https://example.com/page" },
      ),
    ).toBe(false);
  });

  it("rejects an Open Graph URL duplicate in the body", () => {
    expect(
      hasTagWithAttributes(
        '<html><head><meta property="og:url" content="https://example.com/page" /></head><body><meta property="og:url" content="https://example.com/wrong" /></body></html>',
        "meta",
        { property: "og:url", content: "https://example.com/page" },
      ),
    ).toBe(false);
  });

  it("rejects an Open Graph URL duplicate in a second head", () => {
    expect(
      hasTagWithAttributes(
        '<html><head><meta property="og:url" content="https://example.com/page" /></head><head><meta property="og:url" content="https://example.com/wrong" /></head></html>',
        "meta",
        { property: "og:url", content: "https://example.com/page" },
      ),
    ).toBe(false);
  });

  it("rejects duplicate attributes on one element", () => {
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

  it("starts the privacy page with a focusable skip link", () => {
    const privacy = read("privacy.html");
    const body = privacy.slice(privacy.indexOf("<body>") + "<body>".length);
    const firstFocusable = body.match(
      /<(?:a|button|input|select|textarea)\b[^>]*>/i,
    );

    expect(firstFocusable?.[0]).toBe(
      '<a class="skip-link" href="#privacy-title">',
    );
    expect(body).toContain(
      '<a class="skip-link" href="#privacy-title">Skip to privacy policy</a>',
    );
  });

  it("distinguishes transcript delivery modes and later handling", () => {
    const privacy = readText("privacy.html");

    expect(privacy).toContain(
      "Clipboard paste modes (Ctrl+V, Ctrl+Shift+V, and Shift+Insert)",
    );
    expect(privacy).toContain(
      "paste the transcript into the focused application",
    );
    expect(privacy).toContain("try to restore the previous clipboard contents");
    expect(privacy).toContain(
      "Direct types the transcript into the focused application",
    );
    expect(privacy).toContain("without using the clipboard paste path");
    expect(privacy).toContain(
      "External Script sends the transcript as one command-line argument to the configured script",
    );
    expect(privacy).toContain("None skips transcript delivery");
    expect(privacy).toContain(
      "The Copy to clipboard setting runs after every paste method",
    );
    expect(privacy).toContain("including Direct, External Script, and None");
    expect(privacy).toContain("leaves the transcript in the system clipboard");
    expect(privacy).toContain("do not send text off-device by themselves");
    expect(privacy).toContain("applications that read copied text");
    expect(privacy).toContain("control any later transmission and retention");
  });

  it("offers local personalization export and reset choices", () => {
    const privacy = readText("privacy.html");

    expect(privacy).toContain(
      "You can export or reset learned personalization on your device",
    );
  });

  it("states the website tracking and sale practices", () => {
    const privacy = read("privacy.html");
    expect(privacy).toContain("no AudioBud analytics");
    expect(privacy).toContain("does not sell personal information");
    expect(privacy).toContain("Global Privacy Control");
  });

  it("keeps source-code rights under the MIT license", () => {
    const terms = read("terms.html");
    const termsText = readText("terms.html");
    const normalizedTerms = terms.toLowerCase();
    const termsDescription =
      "Terms for AudioBud's official project website, release pages, support channels, and other maintainer-operated surfaces.";
    const termsImageAlt =
      "AudioBud terms for the official project website and maintainer-operated surfaces.";
    expectTagWithAttributes(terms, "meta", {
      name: "description",
      content: termsDescription,
    });
    expectTagWithAttributes(terms, "meta", {
      property: "og:description",
      content: termsDescription,
    });
    expectTagWithAttributes(terms, "meta", {
      name: "twitter:description",
      content: termsDescription,
    });
    expectTagWithAttributes(terms, "meta", {
      property: "og:image:alt",
      content: termsImageAlt,
    });
    expectTagWithAttributes(terms, "meta", {
      name: "twitter:image:alt",
      content: termsImageAlt,
    });
    expect(terms).toContain("MIT License");
    expect(terms).toContain(
      "copying, modifying, or distributing the source code",
    );
    expect(termsText).toContain(
      "The MIT License governs downloading and using copies of AudioBud software",
    );
    expect(termsText).toContain(
      "Nothing in these terms limits permissions granted by the MIT License",
    );
    expect(termsText).toContain(
      "These terms govern the official project website, release and support pages, and other maintainer-operated surfaces",
    );
    expect(termsText).toContain(
      "They do not govern software-license permissions",
    );
    expect(normalizedTerms).not.toContain("class-action waiver");
    expect(normalizedTerms).not.toContain("binding arbitration");
  });
});
