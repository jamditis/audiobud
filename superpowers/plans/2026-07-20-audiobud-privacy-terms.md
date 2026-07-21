# AudioBud privacy and terms implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superjawn:subagent-driven-development (recommended) or superjawn:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Publish implementation-aligned privacy and terms pages at stable AudioBud URLs and link them from every public project surface.

**Architecture:** Keep the site static under `docs/`. Add two independent HTML documents that reuse the existing header, footer, favicon, assets, and stylesheet. Add a Bun contract test that prevents the legal pages, custom-domain metadata, and cross-links from drifting away from the shipped app.

**Tech stack:** Static HTML and CSS, GitHub Pages, Bun tests, Prettier, Playwright browser checks, GitHub CLI.

---

## File map

- Create `scripts/legal-pages.test.ts`: executable contract for required pages, metadata, policy claims, cross-links, and CNAME.
- Create `docs/privacy.html`: public privacy policy.
- Create `docs/terms.html`: public terms of use.
- Modify `docs/index.html`: custom-domain metadata and footer policy links.
- Modify `docs/roadmap.html`: custom-domain metadata and footer policy links.
- Modify `docs/styles.css`: legal-document layout, summary cards, focus states, and responsive rules.
- Modify `README.md`: publish the home, privacy, terms, and support URLs.
- Preserve `docs/CNAME`: existing custom-domain mapping created in commit `061ca71`.

### Task 1: Pin the public policy contract with a failing test

**Files:**

- Create: `scripts/legal-pages.test.ts`

- [ ] **Step 1: Add the contract test**

Create `scripts/legal-pages.test.ts` with:

```ts
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
    const termsImageAlt = "AudioBud local dictation for Windows app interface";
    expect(termsText).toContain(
      "The MIT License's warranty terms govern AudioBud software.",
    );
    expect(termsText).toContain(
      'The official project website and other maintainer-operated surfaces are provided "as is" and "as available," without warranties to the extent permitted by law.',
    );
    expect(termsText).toContain(
      "The MIT License's liability terms govern AudioBud software.",
    );
    expect(termsText).toContain(
      "indirect, incidental, special, consequential, lost-data, or lost-profit damages arising from the official project website or other maintainer-operated surfaces",
    );
    expect(termsText).not.toContain(
      'The app and project website are provided "as is"',
    );
    expect(termsText).not.toContain(
      "damages arising from AudioBud or the project website",
    );
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
```

- [ ] **Step 2: Run the new test and verify it fails for the missing pages**

Run:

```powershell
bun test scripts/legal-pages.test.ts
```

Expected at this checkpoint: FAIL with 30 helper and contract checks passing and 13 contract checks failing. The failures cover missing policy pages and content, including MIT-controlled software warranty and liability terms, website-scoped disclaimers, and accurate social image alt metadata, the missing privacy skip link, mode-by-mode transcript-delivery disclosure, and personalization-choice disclosure, the old origin in `docs/index.html` and `docs/roadmap.html` metadata, and pending privacy and terms links across the public pages.

- [ ] **Step 3: Commit the failing contract**

```powershell
git add -- scripts/legal-pages.test.ts
git commit -m "test(docs): pin public policy contract"
```

### Task 2: Publish the privacy policy

**Files:**

- Create: `docs/privacy.html`
- Test: `scripts/legal-pages.test.ts`

- [ ] **Step 1: Create the static page shell**

Use the same `doctype`, viewport, favicon, stylesheet, fixed header, `.swamp` layer, brand link, and footer structure as `docs/roadmap.html`. Set these metadata values:

```html
<title>AudioBud privacy policy</title>
<meta
  name="description"
  content="How AudioBud processes audio, transcripts, settings, optional AI requests, downloads, and website visits."
/>
<link rel="canonical" href="https://audiobud.amditis.tech/privacy.html" />
<meta property="og:title" content="AudioBud privacy policy" />
<meta
  property="og:description"
  content="AudioBud is local-first. Learn what stays on your device and when a feature contacts another service."
/>
<meta property="og:type" content="website" />
<meta property="og:url" content="https://audiobud.amditis.tech/privacy.html" />
<meta
  property="og:image"
  content="https://audiobud.amditis.tech/assets/og-image.png"
/>
<meta property="og:image:width" content="1200" />
<meta property="og:image:height" content="630" />
<meta
  property="og:image:alt"
  content="AudioBud local dictation for Windows app interface"
/>
<meta name="twitter:card" content="summary_large_image" />
<meta name="twitter:title" content="AudioBud privacy policy" />
<meta
  name="twitter:description"
  content="What stays on your device and when AudioBud contacts another service."
/>
<meta
  name="twitter:image"
  content="https://audiobud.amditis.tech/assets/og-image.png"
/>
<meta
  name="twitter:image:alt"
  content="AudioBud local dictation for Windows app interface"
/>
```

The header navigation must link to Home, Roadmap, Privacy, and Terms, with `aria-current="page"` on Privacy. The footer must link to Privacy, Terms, Changelog, and GitHub.

Add `<a class="skip-link" href="#privacy-title">Skip to privacy policy</a>` as the first focusable element in the body.

- [ ] **Step 2: Add the approved privacy content**

Use an `.legal-hero` followed by `.legal-layout`. The hero must contain this summary:

```html
<p class="eyebrow">Privacy</p>
<h1 id="privacy-title">Your voice stays local unless you choose otherwise.</h1>
<p class="lede">
  AudioBud records and transcribes on your device. The project does not run an
  AudioBud account service or receive your audio, transcripts, history, or
  settings. A few optional actions contact services you select.
</p>
<p class="legal-meta">Effective July 20, 2026</p>
```

Add three `.data-card` summaries with these headings and claims:

```html
<article class="data-card">
  <span class="data-label">Local by default</span>
  <h2>Audio, transcripts, and history stay on your device.</h2>
  <p>You control retention, saved entries, learned words, and deletion.</p>
</article>
<article class="data-card">
  <span class="data-label">Only when enabled</span>
  <h2>Cloud post-processing sends text, not audio.</h2>
  <p>
    Your selected provider receives the transcript and prompt you ask it to
    process.
  </p>
</article>
<article class="data-card">
  <span class="data-label">Public website</span>
  <h2>GitHub Pages hosts this site.</h2>
  <p>
    There are no AudioBud analytics, forms, ads, or first-party tracking
    cookies.
  </p>
</article>
```

Add a contents navigation and the following sections with these exact facts:

1. `Who operates AudioBud`: Joe Amditis maintains the open-source AudioBud project; privacy contact is `jamditis@gmail.com`; public issues must not contain private information.
2. `What this policy covers`: the desktop app, official GitHub repository/releases, and `audiobud.amditis.tech`; third-party sites and providers follow their own policies.
3. `What stays on your device`: microphone audio, WAV recordings, raw/formatted/post-processed transcripts, history titles/timestamps, settings, device selections, shortcuts, custom words, word replacements, optional learned personalization, model files, logs, and provider keys; no claim of encryption.
4. `When information leaves your device`: the following exact lead paragraph must appear:

```html
<p>
  AudioBud does not send your audio to an AudioBud server. Network activity is
  limited to actions such as downloading a model, opening a project or release
  link, contacting a provider you configured, or using an update check if a
  future release enables that feature.
</p>
```

Keep that lead paragraph unchanged. Then distinguish every delivery path in user-facing terms. The Ctrl+V, Ctrl+Shift+V, and Shift+Insert clipboard paste modes temporarily place the transcript in the system clipboard, paste it into the focused application, and then try to restore the previous clipboard contents. Direct types the transcript into the focused application without using that clipboard paste path. External Script sends the transcript as one command-line argument to the configured script. None skips paste or script delivery. The `Copy to clipboard` setting runs after every paste method, including Direct, External Script, and None, and leaves the transcript in the system clipboard. State that these actions do not send text off-device by themselves, while receiving applications and scripts, and applications that read copied text, control later transmission and retention.

5. `Optional AI post-processing`: include the exact boundary sentences:

```html
<p>
  Post-processing is off by default. If you enable it, AudioBud sends transcript
  text and the selected prompt to the provider or compatible endpoint you chose.
  AudioBud does not send your audio to that provider. The provider can receive
  normal connection data such as your IP address and the request headers needed
  to identify AudioBud.
</p>
<p>
  Provider API keys are stored in AudioBud's local settings file. Provider
  privacy terms, retention, security, account rules, and charges apply to those
  requests. A custom local endpoint can keep this step on your network,
  depending on how you configured it.
</p>
```

6. `Downloads and external links`: models currently come from listed download hosts including `blob.handy.computer`; GitHub handles releases and project links; hosts receive normal network metadata; current automatic update checks are disabled.
7. `Website hosting and cookies`: GitHub Pages hosts the site; no AudioBud analytics, forms, advertising scripts, or first-party cookies; link GitHub's privacy statement.
8. `Sharing, sale, and tracking`: include the exact statement:

```html
<p>
  AudioBud does not sell personal information, share it for cross-context
  behavioral advertising, or run advertising profiles. Because AudioBud does not
  perform first-party cross-site tracking, Do Not Track and Global Privacy
  Control signals do not change AudioBud's behavior. GitHub and linked providers
  handle their own signals under their policies.
</p>
```

9. `Retention and deletion`: local unsaved history defaults to five entries; saved entries can remain until deletion; users can delete entries and recordings, change retention, reset personalization, remove keys/settings, and remove app data; provider/GitHub retention is separate; support email is kept only as needed to respond and maintain project records.
10. `Security`: local-first reduces transfers but no system is perfectly secure; OS/account protection matters; secrets in local settings should not be used on an untrusted shared device.
11. `Your choices and rights`: users control optional features and local deletion; users can export or reset learned personalization on their device; applicable-law access/correction/deletion requests can be emailed; third-party data requests go to the third party; users may complain to their local authority where applicable.
12. `Children`: general-purpose tool, not directed to children under 13; contact the maintainer if the project received a child's information.
13. `Changes`: material changes get a revised effective date and publication at the same URL.
14. `Contact`: Joe Amditis, AudioBud project maintainer, `mailto:jamditis@gmail.com`.

- [ ] **Step 3: Run the contract test**

```powershell
bun test scripts/legal-pages.test.ts
```

Expected: FAIL with 37 helper and contract checks passing and 6 contract checks failing. The failures cover the missing terms page and terms content, the old origin in `docs/index.html` and `docs/roadmap.html` metadata, and pending privacy and terms links across the existing public pages.

- [ ] **Step 4: Commit the privacy page**

```powershell
git add -- docs/privacy.html
git commit -m "docs: publish AudioBud privacy policy"
```

### Task 3: Publish the terms of use

**Files:**

- Create: `docs/terms.html`
- Test: `scripts/legal-pages.test.ts`

- [ ] **Step 1: Create the static page shell**

Reuse the privacy-page shell. Set the canonical URL to `https://audiobud.amditis.tech/terms.html`, set the page title to `AudioBud terms of use`, and use this description in standard, Open Graph, and Twitter metadata:

```text
Terms for AudioBud's official project website, release pages, support channels, and other maintainer-operated surfaces.
```

Include exactly one `og:image:alt` tag and one `twitter:image:alt` tag in the
first real document head. Use this exact content for both:

```text
AudioBud local dictation for Windows app interface
```

Set `aria-current="page"` on the Terms navigation and footer link.

- [ ] **Step 2: Add the approved terms content**

Use this hero:

```html
<p class="eyebrow">Terms</p>
<h1 id="terms-title">Use AudioBud carefully, lawfully, and on your terms.</h1>
<p class="lede">
  AudioBud is free, open-source software for local dictation. These terms govern
  the official project website and other maintainer-operated surfaces. The MIT
  License governs downloading, using, copying, modifying, and distributing
  AudioBud software.
</p>
<p class="legal-meta">Effective July 20, 2026</p>
```

Use a non-section layout wrapper around the contents navigation and legal
document because that wrapper has no heading. Make `Contents` a heading and put
the stable section links in a list while preserving `.legal-toc`,
`.legal-document`, `.legal-section`, and the existing anchor targets.

Add these sections:

1. `Agreement and scope`: the MIT License governs downloading, using, copying, modifying, and distributing AudioBud software. These terms govern the official project website, release and support pages, and other maintainer-operated surfaces, not software-license permissions. Acceptance applies to use of those maintainer-operated surfaces, not use of the app. Official surfaces include the repository, releases, support pages, and custom domain.
2. `Open-source license`: include this exact statement:

```html
<p>
  AudioBud's source code is released under the MIT License. The MIT License, not
  these website terms, governs copying, modifying, or distributing the source
  code. If these terms and the MIT License address the same source-code right,
  the MIT License controls.
</p>
```

Keep that paragraph unchanged. Follow it with a clear statement that the MIT
License governs downloading and using copies of AudioBud software and that
nothing in these terms limits permissions granted by the MIT License.

3. `Using AudioBud`: state legal compliance, microphone permissions, device security, credentials, storage, backups, and content as user responsibilities rather than software-license conditions. Apply no-interference and unlawful or harmful submission rules only to project infrastructure, the issue tracker, release pages, and support channels. State that these responsibilities do not narrow MIT permissions.
4. `Your content`: users keep ownership; the project receives no license to local audio/transcripts because it does not receive them; third-party provider requests follow provider terms.
5. `Optional providers and costs`: user-selected post-processing provider or custom endpoint; user provides credentials and accepts provider terms/privacy/charges; no AudioBud promise about provider availability or output handling.
6. `Models, downloads, and third-party components`: models/downloads can be hosted by third parties; third-party licenses/notices apply; verify official release source; no promise every model remains available.
7. `Transcription and AI output`: output can be incomplete or wrong; review before use; do not rely without qualified human review for medical, legal, financial, emergency, accessibility, or other safety-related decisions.
8. `Privacy`: link `./privacy.html`; explain local-first boundary and third-party policy responsibility.
9. `Availability, updates, and support`: free project with no uptime, update schedule, compatibility, or individual support promise; features can change; current release facts control over roadmap statements.
10. `No warranty`: state that the MIT License's warranty terms govern AudioBud software. Apply the “as is” and “as available” disclaimer only to the official project website and other maintainer-operated surfaces, to the extent permitted by law, and preserve non-waivable rights.
11. `Limits on liability`: state that the MIT License's liability terms govern AudioBud software. Apply the indirect, incidental, special, consequential, lost-data, and lost-profit limit only to the official project website and other maintainer-operated surfaces, to the extent permitted by law, and preserve liability that cannot be excluded.
12. `Stopping use and changes`: users can stop using maintainer-operated surfaces at any time; new terms apply prospectively from the revised effective date; continued use of those surfaces after publication means acceptance.
13. `Contact`: Joe Amditis, AudioBud project maintainer, `mailto:jamditis@gmail.com`.

Do not add arbitration, class-action waiver, indemnity, governing-law, venue, or
jurisdiction clauses, or any new restriction on MIT-licensed software.

- [ ] **Step 3: Run the contract test**

```powershell
bun test scripts/legal-pages.test.ts
```

Expected: FAIL with 40 helper and contract checks passing and 3 contract checks failing. The failures cover the old origin in `docs/index.html` and `docs/roadmap.html` metadata and pending privacy and terms links across the existing public pages.

- [ ] **Step 4: Commit the terms page**

```powershell
git add -- docs/terms.html
git commit -m "docs: publish AudioBud terms of use"
```

### Task 4: Connect the policies to the existing public site

**Files:**

- Modify: `docs/index.html`
- Modify: `docs/roadmap.html`
- Modify: `README.md`
- Test: `scripts/legal-pages.test.ts`

- [ ] **Step 1: Update custom-domain metadata**

In `docs/index.html`, replace every `https://jamditis.github.io/audiobud/` metadata origin with `https://audiobud.amditis.tech/`.

In `docs/roadmap.html`, use:

```html
<link rel="canonical" href="https://audiobud.amditis.tech/roadmap.html" />
<meta property="og:url" content="https://audiobud.amditis.tech/roadmap.html" />
<meta
  property="og:image"
  content="https://audiobud.amditis.tech/assets/og-image.png"
/>
<meta
  name="twitter:image"
  content="https://audiobud.amditis.tech/assets/og-image.png"
/>
```

- [ ] **Step 2: Add footer links to both existing pages**

Insert before Changelog in both footers:

```html
<a href="./privacy.html">Privacy</a> <a href="./terms.html">Terms</a>
```

- [ ] **Step 3: Add public links to README**

Replace the current website line near the top with:

```markdown
- **Website:** <https://audiobud.amditis.tech/>
- **Privacy:** <https://audiobud.amditis.tech/privacy.html>
- **Terms:** <https://audiobud.amditis.tech/terms.html>
- **Support:** <https://github.com/jamditis/audiobud/issues>
```

- [ ] **Step 4: Run the contract test**

```powershell
bun test scripts/legal-pages.test.ts
```

Expected: PASS with all 43 helper and contract checks green.

- [ ] **Step 5: Commit the links and metadata**

```powershell
git add -- README.md docs/index.html docs/roadmap.html
git commit -m "docs: connect policies to public site"
```

### Task 5: Add the legal-document layout

**Files:**

- Modify: `docs/styles.css`

- [ ] **Step 1: Add visible focus behavior**

Add near the base link rules:

```css
:focus-visible {
  outline: 3px solid var(--amber);
  outline-offset: 4px;
}

.skip-link {
  position: fixed;
  z-index: 30;
  top: 12px;
  left: 12px;
  padding: 10px 14px;
  transform: translateY(-200%);
  border-radius: 6px;
  background: var(--amber);
  color: var(--bg);
  font-weight: 800;
}

.skip-link:focus,
.skip-link:focus-visible {
  transform: translateY(0);
}

#privacy-title,
#terms-title {
  scroll-margin-top: 80px;
}
```

The skip link must remain off-canvas until focused, then become fully visible
above the fixed header (`z-index: 20`) with readable contrast and the global
focus outline. The title offset must clear the 64px fixed header plus spacing
when a skip or fragment link targets either heading.

- [ ] **Step 2: Add the legal layout styles**

Add before the existing responsive block:

```css
.legal-hero {
  padding: 132px 0 48px;
  border-bottom: 1px solid var(--line);
}

.legal-hero h1 {
  max-width: 850px;
}

.legal-meta {
  margin-bottom: 0;
  color: var(--muted);
  font-size: 14px;
}

.data-grid {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 14px;
  margin-top: 34px;
}

.data-card {
  padding: 20px;
  border: 1px solid rgba(243, 247, 238, 0.12);
  border-radius: 8px;
  background: rgba(16, 27, 19, 0.78);
}

.data-card h2 {
  margin-bottom: 10px;
  font-size: 20px;
  line-height: 1.2;
}

.data-card p {
  margin-bottom: 0;
  color: var(--muted);
  line-height: 1.55;
}

.data-label {
  display: block;
  margin-bottom: 12px;
  color: var(--green);
  font-size: 12px;
  font-weight: 900;
  letter-spacing: 0.08em;
  text-transform: uppercase;
}

.legal-layout {
  display: grid;
  grid-template-columns: 210px minmax(0, 760px);
  gap: 52px;
  align-items: start;
  justify-content: center;
}

.legal-toc {
  position: sticky;
  top: 92px;
  padding: 18px;
  border: 1px solid rgba(243, 247, 238, 0.1);
  border-radius: 8px;
  background: rgba(16, 27, 19, 0.72);
}

.legal-toc strong,
.legal-toc h2 {
  display: block;
  margin-bottom: 10px;
  color: var(--text);
  font-size: 13px;
}

.legal-toc ul {
  margin: 0;
  padding: 0;
  list-style: none;
}

.legal-toc a {
  display: block;
  padding: 5px 0;
  color: var(--muted);
  font-size: 13px;
  line-height: 1.35;
  text-decoration: none;
}

.legal-toc a:hover {
  color: var(--green);
}

.legal-document {
  min-width: 0;
}

.legal-section {
  scroll-margin-top: 90px;
  padding: 0 0 34px;
}

.legal-section + .legal-section {
  padding-top: 34px;
  border-top: 1px solid rgba(243, 247, 238, 0.1);
}

.legal-section h2 {
  margin-bottom: 14px;
  font-size: clamp(24px, 3vw, 34px);
  line-height: 1.12;
}

.legal-section h3 {
  margin: 24px 0 10px;
  font-size: 18px;
}

.legal-section p,
.legal-section li {
  color: var(--muted);
  font-size: 16px;
  line-height: 1.72;
}

.legal-section li + li {
  margin-top: 8px;
}

.legal-section a {
  color: var(--green);
  text-underline-offset: 3px;
}

.legal-callout {
  margin: 22px 0;
  padding: 18px 20px;
  border-left: 3px solid var(--amber);
  background: rgba(255, 178, 62, 0.08);
}

.legal-callout p:last-child {
  margin-bottom: 0;
}
```

- [ ] **Step 3: Extend the mobile rules**

Inside `@media (max-width: 860px)`, include `.data-grid` in the existing one-column selector and add:

```css
.legal-hero {
  padding: 108px 0 34px;
}

.legal-layout {
  grid-template-columns: 1fr;
  gap: 30px;
}

.legal-toc {
  position: static;
}
```

- [ ] **Step 4: Format the touched files**

Run:

```powershell
bunx prettier --write README.md docs/index.html docs/roadmap.html docs/privacy.html docs/terms.html docs/styles.css scripts/legal-pages.test.ts superpowers/specs/2026-07-20-audiobud-privacy-terms-design.md superpowers/plans/2026-07-20-audiobud-privacy-terms.md
```

Expected: command exits 0 and only the listed files are formatted.

- [ ] **Step 5: Commit the legal-page presentation**

```powershell
git add -- docs/styles.css docs/privacy.html docs/terms.html
git commit -m "docs: style policy pages for clear reading"
```

### Task 6: Verify the pages and publish the review PR

**Files:**

- Verify all changed files.
- Use `.github/PULL_REQUEST_TEMPLATE.md` for the PR body.

- [ ] **Step 1: Run focused and repository checks**

```powershell
bun test scripts/legal-pages.test.ts
bun test scripts src
bun run build
bunx prettier --check README.md docs/index.html docs/roadmap.html docs/privacy.html docs/terms.html docs/styles.css scripts/legal-pages.test.ts superpowers/specs/2026-07-20-audiobud-privacy-terms-design.md superpowers/plans/2026-07-20-audiobud-privacy-terms.md
git diff --check origin/main...HEAD
```

Expected: all commands exit 0; Bun reports 0 failed tests; Vite produces `dist/`; Prettier reports all matched files use its style; Git reports no whitespace errors.

- [ ] **Step 2: Run a local Pages server**

```powershell
$server = Start-Process python -ArgumentList '-m','http.server','4173','--directory','docs' -WindowStyle Hidden -PassThru
```

Open and inspect:

- `http://127.0.0.1:4173/`
- `http://127.0.0.1:4173/roadmap.html`
- `http://127.0.0.1:4173/privacy.html`
- `http://127.0.0.1:4173/terms.html`

Check each page at 1440×1000 and 390×844. Verify readable line length, no horizontal overflow, visible focus, working contents links, correct `aria-current`, working footer links, and reduced-motion behavior. Stop the server after inspection:

```powershell
Stop-Process -Id $server.Id
```

- [ ] **Step 3: Verify custom-domain readiness**

```powershell
Resolve-DnsName audiobud.amditis.tech -Type CNAME
gh api repos/jamditis/audiobud/pages
curl.exe -I https://audiobud.amditis.tech/
curl.exe -I https://jamditis.github.io/audiobud/
```

Expected before merge: DNS points to `jamditis.github.io`; GitHub reports the custom domain; the custom domain serves or is awaiting only GitHub certificate issuance; the old URL redirects to the matching custom path. Enable `Enforce HTTPS` only after GitHub reports the certificate is ready.

- [ ] **Step 4: Inspect the exact publication scope**

```powershell
git status --short
git diff --stat origin/main...HEAD
git diff --name-status origin/main...HEAD
```

Expected tracked scope: the approved spec and plan, `scripts/legal-pages.test.ts`, `docs/privacy.html`, `docs/terms.html`, `docs/index.html`, `docs/roadmap.html`, `docs/styles.css`, and `README.md`. No screenshots, `.firecrawl`, credentials, or Azure values are staged.

- [ ] **Step 5: Push the branch**

```powershell
git push -u origin agent/privacy-terms-pages
```

- [ ] **Step 6: Open a ready-for-review PR**

Write a PR body that completes every section of `.github/PULL_REQUEST_TEMPLATE.md`:

```markdown
## Before submitting

- [x] I searched existing issues and pull requests (including closed ones) so this isn't a duplicate
- [x] I tested this change locally

Skipping any of the above? Explain why here:

Nothing skipped.

## Description

Publishes implementation-aligned privacy and terms pages for AudioBud, links them from the GitHub Pages site and README, and moves public metadata to `audiobud.amditis.tech`.

The privacy policy distinguishes local audio and history from optional text-only provider requests, model downloads, GitHub hosting, and support email. The terms keep source-code rights under the MIT License and avoid unreviewed dispute clauses.

## Related issues

No existing issue. This work supplies stable privacy and terms URLs for the AudioBud Microsoft Entra application profile and release-signing setup.

## Testing

- `bun test scripts/legal-pages.test.ts`
- `bun test scripts src`
- `bun run build`
- focused Prettier check
- local desktop and mobile browser review of all four Pages documents
- DNS, GitHub Pages, redirect, and HTTPS readiness checks

## Screenshots / videos (optional)

Desktop and mobile policy-page screenshots are attached if they materially help review the layout.
```

Use `apply_patch` to create
`C:\Users\amdit\AppData\Local\Temp\audiobud-privacy-terms-pr.md` with the
approved body above. Then run:

```powershell
$prBodyFile = 'C:\Users\amdit\AppData\Local\Temp\audiobud-privacy-terms-pr.md'
gh pr create --base main --head agent/privacy-terms-pages --title "docs: publish AudioBud privacy and terms pages" --body-file $prBodyFile
```

Do not pass `--draft`; the user requested a ready-for-review PR.

- [ ] **Step 7: Confirm the PR state and checks**

```powershell
gh pr view --json url,title,isDraft,headRefName,baseRefName,state,statusCheckRollup
```

Expected: `isDraft` is false, base is `main`, head is `agent/privacy-terms-pages`, state is `OPEN`, and required checks have started or passed.
