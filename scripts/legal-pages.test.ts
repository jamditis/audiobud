import { describe, expect, it } from "bun:test";
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";

const root = join(import.meta.dir, "..");
const docs = join(root, "docs");
const read = (name: string) => readFileSync(join(docs, name), "utf8");
const readRoot = (name: string) => readFileSync(join(root, name), "utf8");
const readText = (name: string) =>
  read(name)
    .replace(/<[^>]+>/g, " ")
    .replace(/\s+/g, " ")
    .trim();
const compactCss = (value: string) => value.replace(/\s+/g, "");
const escapeRegExp = (value: string) =>
  value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
const readCssRule = (css: string, selector: string) => {
  const selectorPattern = selector
    .trim()
    .split(/\s+/)
    .map(escapeRegExp)
    .join("\\s+");
  return new RegExp(`${selectorPattern}\\s*\\{([^{}]*)\\}`).exec(css)?.[1];
};
const readCssBlock = (css: string, marker: string) => {
  const markerIndex = css.indexOf(marker);
  if (markerIndex === -1) return undefined;

  const blockStart = css.indexOf("{", markerIndex + marker.length);
  if (blockStart === -1) return undefined;

  let depth = 1;
  for (let cursor = blockStart + 1; cursor < css.length; cursor++) {
    if (css[cursor] === "{") depth++;
    if (css[cursor] === "}") depth--;
    if (depth === 0) return css.slice(blockStart + 1, cursor);
  }

  return undefined;
};
const expectCssRule = (
  css: string,
  selector: string,
  declarations: string[],
) => {
  const rule = readCssRule(css, selector);
  expect(rule, `Missing CSS rule for ${selector}`).toBeDefined();
  const compactRule = compactCss(rule ?? "");
  for (const declaration of declarations) {
    expect(compactRule).toContain(compactCss(declaration));
  }
};

const sitePages = [
  {
    name: "index.html",
    url: "https://audiobud.amditis.tech/",
    headingId: "hero-title",
    skipLabel: "Skip to main content",
  },
  {
    name: "roadmap.html",
    url: "https://audiobud.amditis.tech/roadmap.html",
    headingId: "roadmap-title",
    skipLabel: "Skip to roadmap",
    currentFooterLabel: "Roadmap",
  },
  {
    name: "privacy.html",
    url: "https://audiobud.amditis.tech/privacy.html",
    headingId: "privacy-title",
    skipLabel: "Skip to privacy policy",
    currentFooterLabel: "Privacy",
  },
  {
    name: "terms.html",
    url: "https://audiobud.amditis.tech/terms.html",
    headingId: "terms-title",
    skipLabel: "Skip to terms of use",
    currentFooterLabel: "Terms",
  },
];
const footerLinks = [
  { label: "Roadmap", href: "./roadmap.html" },
  { label: "Privacy", href: "./privacy.html" },
  { label: "Terms", href: "./terms.html" },
  {
    label: "Changelog",
    href: "https://github.com/jamditis/audiobud/blob/main/CHANGELOG.md",
  },
  { label: "GitHub", href: "https://github.com/jamditis/audiobud" },
];
const socialImage = "https://audiobud.amditis.tech/assets/og-image.png";
const socialImageAlt = "AudioBud local dictation for Windows app interface";
const browserFavicon =
  "data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 64 64'%3E%3Crect width='64' height='64' rx='14' fill='%23101b13'/%3E%3Ccircle cx='32' cy='34' r='20' fill='%2384d150'/%3E%3Ccircle cx='22' cy='22' r='8' fill='%23f3f7ee'/%3E%3Ccircle cx='42' cy='22' r='8' fill='%23f3f7ee'/%3E%3Ccircle cx='22' cy='22' r='4' fill='%23ff5147'/%3E%3Ccircle cx='42' cy='22' r='4' fill='%23ff5147'/%3E%3Cpath d='M22 39c5 5 15 5 20 0' fill='none' stroke='%23101b13' stroke-width='4' stroke-linecap='round'/%3E%3Cpath d='M17 52h30' stroke='%23ffb23e' stroke-width='5' stroke-linecap='round'/%3E%3C/svg%3E";
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
type RealDocumentTag = {
  name: string;
  attributes: Map<string, string>;
  isClosing: boolean;
  isInFirstHead: boolean;
  start: number;
  end: number;
};
const rawTextElementNames = new Set(["script", "style", "title", "textarea"]);
const scanRealDocumentTags = (html: string) => {
  const tags: RealDocumentTag[] = [];
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
    const isClosing = html[nameStart] === "/";
    if (isClosing) nameStart++;
    if (!isAsciiLetter(html[nameStart])) {
      cursor = tagStart + 1;
      continue;
    }

    let nameEnd = nameStart;
    while (nameEnd < html.length && isTagNameCharacter(html[nameEnd]))
      nameEnd++;

    const name = html.slice(nameStart, nameEnd).toLowerCase();
    const tagEnd = findOpeningTagEnd(html, nameEnd);
    if (tagEnd === null) return null;
    if (!isTagNameDelimiter(html[nameEnd])) {
      cursor = tagEnd + 1;
      continue;
    }

    const tag: RealDocumentTag = {
      name,
      attributes: isClosing
        ? new Map()
        : parseQuotedAttributes(html.slice(nameEnd, tagEnd)),
      isClosing,
      isInFirstHead: isInsideHead,
      start: tagStart,
      end: tagEnd + 1,
    };

    if (isClosing) {
      if (name === "template" && templateDepth > 0) {
        templateDepth--;
        if (templateDepth === 0) tags.push(tag);
      } else if (templateDepth === 0) {
        tags.push(tag);
        if (name === "head") isInsideHead = false;
      }
      cursor = tagEnd + 1;
      continue;
    }

    if (templateDepth > 0) {
      if (name === "template") templateDepth++;
      cursor = tagEnd + 1;
      continue;
    }

    if (name === "head" && !hasSeenHead && !hasSeenBody) {
      hasSeenHead = true;
      isInsideHead = true;
      tag.isInFirstHead = true;
    } else if (name === "body") {
      hasSeenBody = true;
      isInsideHead = false;
      tag.isInFirstHead = false;
    }

    tags.push(tag);
    if (name === "template") {
      templateDepth = 1;
      cursor = tagEnd + 1;
      continue;
    }

    if (rawTextElementNames.has(name)) {
      const closingTag = new RegExp(`</${name}[ \\t\\n\\r\\f]*>`, "gi");
      closingTag.lastIndex = tagEnd + 1;
      const closingMatch = closingTag.exec(html);
      if (!closingMatch) return null;
      tags.push({
        name,
        attributes: new Map(),
        isClosing: true,
        isInFirstHead: isInsideHead,
        start: closingMatch.index,
        end: closingTag.lastIndex,
      });
      cursor = closingTag.lastIndex;
      continue;
    }

    cursor = tagEnd + 1;
  }

  return templateDepth === 0 ? tags : null;
};
const extractRealElements = (html: string, elementName: string) => {
  const tags = scanRealDocumentTags(html);
  if (!tags) return null;

  const normalizedName = elementName.toLowerCase();
  const elements: Array<{
    attributes: Map<string, string>;
    innerHtml: string;
    start: number;
  }> = [];
  for (let index = 0; index < tags.length; index++) {
    const openingTag = tags[index];
    if (openingTag.isClosing || openingTag.name !== normalizedName) continue;

    let depth = 1;
    let closingTag: RealDocumentTag | undefined;
    for (
      let candidateIndex = index + 1;
      candidateIndex < tags.length;
      candidateIndex++
    ) {
      const candidate = tags[candidateIndex];
      if (candidate.name !== normalizedName) continue;
      depth += candidate.isClosing ? -1 : 1;
      if (depth === 0) {
        closingTag = candidate;
        break;
      }
    }
    if (!closingTag) return null;

    elements.push({
      attributes: openingTag.attributes,
      innerHtml: html.slice(openingTag.end, closingTag.start),
      start: openingTag.start,
    });
  }

  return elements;
};
const hasClassToken = (attributes: Map<string, string>, expected: string) =>
  attributes
    .get("class")
    ?.split(/[ \t\n\r\f]+/)
    .includes(expected) === true;
const readElementText = (html: string) =>
  html
    .replace(/<[^>]+>/g, " ")
    .replace(/\s+/g, " ")
    .trim();
const getFooterNavigation = (html: string) => {
  const footers = extractRealElements(html, "footer");
  if (!footers || footers.length !== 1) return null;

  const navigations = extractRealElements(footers[0].innerHtml, "nav")?.filter(
    ({ attributes }) => hasClassToken(attributes, "footer-links"),
  );
  if (!navigations || navigations.length !== 1) return null;

  const anchors = extractRealElements(navigations[0].innerHtml, "a");
  return anchors?.map(({ attributes, innerHtml }) => ({
    label: readElementText(innerHtml),
    href: attributes.get("href"),
    ariaCurrent: attributes.get("aria-current"),
  }));
};
const getFirstRealBodyElement = (html: string) => {
  const bodies = extractRealElements(html, "body");
  if (!bodies || bodies.length !== 1) return null;
  const firstTag = scanRealDocumentTags(bodies[0].innerHtml)?.find(
    ({ isClosing }) => !isClosing,
  );
  if (!firstTag) return null;
  const element = extractRealElements(bodies[0].innerHtml, firstTag.name)?.find(
    ({ start }) => start === firstTag.start,
  );
  return element ? { ...firstTag, innerHtml: element.innerHtml } : null;
};
const countRealIdAttributes = (html: string, id: string) =>
  scanRealDocumentTags(html)?.filter(
    ({ attributes, isClosing }) => !isClosing && attributes.get("id") === id,
  ).length;
const scanMetadataOpeningTags = (html: string) =>
  scanRealDocumentTags(html)?.flatMap(
    ({ attributes, isClosing, isInFirstHead, name }) =>
      !isClosing && (name === "link" || name === "meta")
        ? [{ name, attributes, isInFirstHead }]
        : [],
  );
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

describe("Real document tag scanner", () => {
  it("extracts one semantic footer and ignores footer lookalikes", () => {
    const html = `<body>
      <div data-copy="<footer><nav class='footer-links'></nav></footer>"></div>
      <!-- <footer><nav class="footer-links"><a href="./wrong.html">Wrong</a></nav></footer> -->
      <footer><nav class="footer-links">
        <a href="./roadmap.html">Roadmap</a>
        <a href="./privacy.html">Privacy</a>
        <a href="./terms.html">Terms</a>
        <a href="https://github.com/jamditis/audiobud/blob/main/CHANGELOG.md">Changelog</a>
        <a href="https://github.com/jamditis/audiobud">GitHub</a>
      </nav></footer>
    </body>`;

    expect(
      getFooterNavigation(html)?.map(({ label, href }) => ({ label, href })),
    ).toEqual(footerLinks);
  });

  it("rejects documents with multiple semantic footers", () => {
    expect(
      getFooterNavigation(
        '<body><footer></footer><footer><nav class="footer-links"></nav></footer></body>',
      ),
    ).toBeNull();
  });

  it("finds the first real body child and counts only real ID attributes", () => {
    const html = `<body>
      <!-- <div id="main-title"></div> -->
      <a class="skip-link" href="#main-title">Skip to main content</a>
      <div data-copy='<h1 id="main-title">Wrong</h1>'></div>
      <h1 id="main-title">Main title</h1>
    </body>`;
    const firstElement = getFirstRealBodyElement(html);

    expect(firstElement?.name).toBe("a");
    expect(
      firstElement && hasClassToken(firstElement.attributes, "skip-link"),
    ).toBe(true);
    expect(firstElement?.attributes.get("href")).toBe("#main-title");
    expect(firstElement && readElementText(firstElement.innerHtml)).toBe(
      "Skip to main content",
    );
    expect(countRealIdAttributes(html, "main-title")).toBe(1);
  });
});

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
  it("publishes the Microsoft publisher-domain association", () => {
    expect(existsSync(join(docs, ".nojekyll"))).toBe(true);
    expect(
      JSON.parse(read(".well-known/microsoft-identity-association.json")),
    ).toEqual({
      associatedApplications: [
        {
          applicationId: "2fc67628-088e-4eb5-aeab-ffdbd246a42b",
        },
      ],
    });
  });

  it("provides visible focus and fixed-header-clearing skip behavior", () => {
    const css = read("styles.css");

    expectCssRule(css, ":focus-visible", [
      "outline: 3px solid var(--amber);",
      "outline-offset: 4px;",
    ]);
    expectCssRule(css, ".skip-link", [
      "position: fixed;",
      "z-index: 30;",
      "top: 12px;",
      "left: 12px;",
      "padding: 10px 14px;",
      "transform: translateY(-200%);",
      "border-radius: 6px;",
      "background: var(--amber);",
      "color: var(--bg);",
      "font-weight: 800;",
    ]);
    expectCssRule(css, ".skip-link:focus, .skip-link:focus-visible", [
      "transform: translateY(0);",
    ]);
    expectCssRule(
      css,
      "#hero-title, #roadmap-title, #privacy-title, #terms-title",
      ["scroll-margin-top: 80px;"],
    );
    expect(css.match(/(^|})\s*:focus-visible\s*{/g)).toHaveLength(1);
    expect(css.match(/(^|})\s*\.skip-link\s*{/g)).toHaveLength(1);
  });

  it("keeps public-page typography local", () => {
    for (const page of sitePages) {
      const html = read(page.name);
      expect(html).not.toContain("fonts.googleapis.com");
      expect(html).not.toContain("fonts.gstatic.com");
    }
  });

  it("marks current navigation destinations without extra decoration", () => {
    const css = read("styles.css");

    expectCssRule(
      css,
      '.nav-links a[aria-current="page"], .footer-links a[aria-current="page"]',
      ["color: var(--amber);", "font-weight: 800;"],
    );
  });

  it("styles the legal hero and privacy data summary", () => {
    const css = read("styles.css");

    expectCssRule(css, ".legal-hero", [
      "padding: 132px 0 48px;",
      "border-bottom: 1px solid var(--line);",
    ]);
    expectCssRule(css, ".legal-hero h1", ["max-width: 850px;"]);
    expectCssRule(css, ".legal-meta", [
      "margin-bottom: 0;",
      "color: var(--muted);",
      "font-size: 14px;",
    ]);
    expectCssRule(css, ".data-grid", [
      "display: grid;",
      "grid-template-columns: repeat(3, minmax(0, 1fr));",
      "gap: 14px;",
      "margin-top: 34px;",
    ]);
    expectCssRule(css, ".data-card", [
      "padding: 20px;",
      "border: 1px solid rgba(243, 247, 238, 0.12);",
      "border-radius: 8px;",
      "background: rgba(16, 27, 19, 0.78);",
    ]);
    expectCssRule(css, ".data-card h2", [
      "margin-bottom: 10px;",
      "font-size: 20px;",
      "line-height: 1.2;",
    ]);
    expectCssRule(css, ".data-card p", [
      "margin-bottom: 0;",
      "color: var(--muted);",
      "line-height: 1.55;",
    ]);
    expectCssRule(css, ".data-label", [
      "display: block;",
      "margin-bottom: 12px;",
      "color: var(--green);",
      "font-size: 12px;",
      "font-weight: 900;",
      "letter-spacing: 0.08em;",
      "text-transform: uppercase;",
    ]);
  });

  it("styles the legal layout and semantic contents navigation", () => {
    const css = read("styles.css");

    expectCssRule(css, ".legal-layout", [
      "display: grid;",
      "grid-template-columns: 210px minmax(0, 760px);",
      "gap: 52px;",
      "align-items: start;",
      "justify-content: center;",
    ]);
    expectCssRule(css, ".legal-toc", [
      "position: sticky;",
      "top: 92px;",
      "padding: 18px;",
      "border: 1px solid rgba(243, 247, 238, 0.1);",
      "border-radius: 8px;",
      "background: rgba(16, 27, 19, 0.72);",
    ]);
    expectCssRule(css, ".legal-toc h2", [
      "display: block;",
      "margin-bottom: 10px;",
      "color: var(--text);",
      "font-size: 13px;",
    ]);
    expectCssRule(css, ".legal-toc ul", [
      "margin: 0;",
      "padding: 0;",
      "list-style: none;",
    ]);
    expectCssRule(css, ".legal-toc li", ["list-style: none;"]);
    expectCssRule(css, ".legal-toc a", [
      "display: block;",
      "padding: 5px 0;",
      "color: var(--muted);",
      "font-size: 13px;",
      "line-height: 1.35;",
      "text-decoration: none;",
    ]);
    expectCssRule(css, ".legal-toc a:hover, .legal-toc a:focus-visible", [
      "color: var(--green);",
    ]);
  });

  it("styles legal document sections, links, and callouts", () => {
    const css = read("styles.css");

    expectCssRule(css, ".legal-document", ["min-width: 0;"]);
    expectCssRule(css, ".legal-section", [
      "scroll-margin-top: 90px;",
      "padding: 0 0 34px;",
    ]);
    expectCssRule(css, ".legal-section + .legal-section", [
      "padding-top: 34px;",
      "border-top: 1px solid rgba(243, 247, 238, 0.1);",
    ]);
    expectCssRule(css, ".legal-section h2", [
      "margin-bottom: 14px;",
      "font-size: clamp(24px, 3vw, 34px);",
      "line-height: 1.12;",
    ]);
    expectCssRule(css, ".legal-section h3", [
      "margin: 24px 0 10px;",
      "font-size: 18px;",
    ]);
    expectCssRule(css, ".legal-section p, .legal-section li", [
      "color: var(--muted);",
      "font-size: 16px;",
      "line-height: 1.72;",
    ]);
    expectCssRule(css, ".legal-section li + li", ["margin-top: 8px;"]);
    expectCssRule(css, ".legal-section a", [
      "color: var(--green);",
      "text-underline-offset: 3px;",
    ]);
    expectCssRule(css, ".legal-callout", [
      "margin: 22px 0;",
      "padding: 18px 20px;",
      "border-left: 3px solid var(--amber);",
      "background: rgba(255, 178, 62, 0.08);",
    ]);
    expectCssRule(css, ".legal-callout p:last-child", ["margin-bottom: 0;"]);
  });

  it("keeps roadmap heading dimensions and five footer links usable", () => {
    const css = read("styles.css");

    expectCssRule(css, ".section-head h1, .section-head h2", [
      "max-width: 680px;",
      "margin-bottom: 0;",
      "font-size: clamp(30px, 4vw, 48px);",
      "line-height: 1.04;",
      "letter-spacing: 0;",
    ]);
    expectCssRule(css, ".footer-links", ["flex-wrap: wrap;"]);
  });

  it("collapses the legal layout at the approved mobile breakpoint", () => {
    const css = read("styles.css");
    const mobile = readCssBlock(css, "@media (max-width: 860px)");

    expect(mobile).toBeDefined();
    expectCssRule(mobile ?? "", ".roadmap-grid, .data-grid", [
      "grid-template-columns: 1fr;",
    ]);
    expectCssRule(mobile ?? "", ".legal-hero", ["padding: 108px 0 34px;"]);
    expectCssRule(mobile ?? "", ".legal-layout", [
      "grid-template-columns: 1fr;",
      "gap: 30px;",
    ]);
    expectCssRule(mobile ?? "", ".legal-toc", ["position: static;"]);
  });

  it("collapses the redesigned homepage at its tablet and phone breakpoints", () => {
    const css = read("styles.css");
    const tablet = readCssBlock(css, "@media (max-width: 980px)");
    const phone = readCssBlock(css, "@media (max-width: 560px)");

    expect(tablet).toBeDefined();
    expectCssRule(tablet ?? "", ".hero-grid", ["grid-template-columns: 1fr;"]);
    expectCssRule(tablet ?? "", ".feature-wide, .feature-screen", [
      "grid-template-columns: 1fr;",
    ]);
    expectCssRule(tablet ?? "", ".model-row", [
      "grid-template-columns: 48px 1fr auto;",
    ]);
    expectCssRule(tablet ?? "", ".cta-card", [
      "grid-template-columns: 86px 1fr;",
    ]);

    expect(phone).toBeDefined();
    expectCssRule(phone ?? "", ".bento-grid", ["grid-template-columns: 1fr;"]);
    expectCssRule(phone ?? "", ".model-row", [
      "grid-template-columns: 42px 1fr;",
    ]);
    expectCssRule(phone ?? "", ".cta-card", ["display: block;"]);
  });

  it("disables smooth scrolling when reduced motion is requested", () => {
    const css = read("styles.css");
    const reducedMotion = readCssBlock(
      css,
      "@media (prefers-reduced-motion: reduce)",
    );

    expect(reducedMotion).toBeDefined();
    expectCssRule(reducedMotion ?? "", "html", ["scroll-behavior: auto;"]);
  });

  it("keeps legal contents reachable in short desktop viewports", () => {
    const css = read("styles.css");
    const shortDesktop = readCssBlock(
      css,
      "@media (min-width: 861px) and (max-height: 760px)",
    );

    expect(shortDesktop).toBeDefined();
    expectCssRule(shortDesktop ?? "", ".legal-toc", ["position: static;"]);
  });

  for (const page of ["privacy.html", "terms.html"]) {
    it(`uses a semantic contents heading and list in ${page}`, () => {
      const navigations = extractRealElements(read(page), "nav")?.filter(
        ({ attributes }) => hasClassToken(attributes, "legal-toc"),
      );
      expect(navigations).toHaveLength(1);

      const contents = navigations?.[0].innerHtml ?? "";
      const headings = extractRealElements(contents, "h2");
      const lists = extractRealElements(contents, "ul");
      const allLinks = extractRealElements(contents, "a");
      const listLinks = extractRealElements(lists?.[0]?.innerHtml ?? "", "a");
      const listItems = extractRealElements(lists?.[0]?.innerHtml ?? "", "li");

      expect(headings).toHaveLength(1);
      expect(readElementText(headings?.[0].innerHtml ?? "")).toBe("Contents");
      expect(lists).toHaveLength(1);
      expect(listItems?.length).toBe(allLinks?.length);
      expect(
        listLinks?.map(({ innerHtml }) => readElementText(innerHtml)),
      ).toEqual(allLinks?.map(({ innerHtml }) => readElementText(innerHtml)));
    });
  }

  it("uses divs for the unlabelled privacy layout wrappers", () => {
    const privacy = read("privacy.html");
    const divSections = extractRealElements(privacy, "div")?.filter(
      ({ attributes }) => hasClassToken(attributes, "section"),
    );
    const unlabelledSections = extractRealElements(privacy, "section")?.filter(
      ({ attributes }) => hasClassToken(attributes, "section"),
    );

    expect(divSections).toHaveLength(1);
    expect(unlabelledSections).toHaveLength(0);
  });

  it("publishes the custom domain from docs", () => {
    expect(read("CNAME").trim()).toBe("audiobud.amditis.tech");
  });

  for (const page of ["privacy.html", "terms.html"]) {
    it(`publishes ${page}`, () => {
      expect(existsSync(join(docs, page))).toBe(true);
    });
  }

  it("keeps the signed-installer status beside the download CTA", () => {
    const home = read("index.html");

    expect(home).toContain('class="install-note"');
    expect(home).toContain("<strong>Code-signed release:</strong>");
    expect(home).toContain("signed and timestamped through Microsoft");
    expect(home).toContain("SmartScreen can still show a reputation warning");
    expect(home).not.toContain("<strong>Unsigned release:</strong>");
  });

  it("marks the signed 0.4.0 milestone as shipped", () => {
    const roadmap = read("roadmap.html").replace(/\s+/g, " ");

    expect(roadmap).toMatch(
      /<h3>v0\.4\.0<\/h3> <span class="status-pill status-shipped">shipped<\/span>/,
    );
    expect(roadmap).toContain(
      'href="https://github.com/jamditis/audiobud/releases/tag/v0.4.0"',
    );
    expect(roadmap).not.toContain("v0.4.0 &mdash; signed &amp; distributable");
  });

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
      expectTagWithAttributes(html, "meta", {
        property: "og:image:alt",
        content: socialImageAlt,
      });
      expectTagWithAttributes(html, "meta", {
        name: "twitter:image:alt",
        content: socialImageAlt,
      });
      const faviconTags = scanMetadataOpeningTags(html)?.filter(
        ({ name, attributes }) =>
          name === "link" &&
          attributes
            .get("rel")
            ?.split(/[ \t\n\r\f]+/)
            .some((token) => token.toLowerCase() === "icon"),
      );
      expect(faviconTags).toHaveLength(1);
      expect(faviconTags?.[0].isInFirstHead).toBe(true);
      expect(faviconTags?.[0].attributes.get("type")).toBe("image/svg+xml");
      expect(faviconTags?.[0].attributes.get("href")).toBe(browserFavicon);
      expect(html).not.toContain("https://jamditis.github.io/audiobud");
    });
  }

  for (const page of sitePages) {
    it(`uses the complete footer navigation in ${page.name}`, () => {
      const navigation = getFooterNavigation(read(page.name));
      expect(navigation?.map(({ label, href }) => ({ label, href }))).toEqual(
        footerLinks,
      );

      const currentLinks = navigation?.filter(
        ({ ariaCurrent }) => ariaCurrent === "page",
      );
      if (page.currentFooterLabel) {
        expect(currentLinks?.map(({ label }) => label)).toEqual([
          page.currentFooterLabel,
        ]);
      } else {
        expect(currentLinks).toHaveLength(0);
      }
    });
  }

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

  for (const page of sitePages) {
    it(`starts ${page.name} with the skip link as the first body element`, () => {
      const html = read(page.name);
      const firstElement = getFirstRealBodyElement(html);
      const targetHeadings = scanRealDocumentTags(html)?.filter(
        ({ attributes, isClosing, name }) =>
          !isClosing &&
          name === "h1" &&
          attributes.get("id") === page.headingId,
      );

      expect(firstElement?.name).toBe("a");
      expect(
        firstElement && hasClassToken(firstElement.attributes, "skip-link"),
      ).toBe(true);
      expect(firstElement?.attributes.get("href")).toBe(`#${page.headingId}`);
      expect(firstElement && readElementText(firstElement.innerHtml)).toBe(
        page.skipLabel,
      );
      expect(countRealIdAttributes(html, page.headingId)).toBe(1);
      expect(targetHeadings).toHaveLength(1);
      expect(targetHeadings?.[0].attributes.get("tabindex")).toBe("-1");
    });
  }

  it("uses one page-level roadmap heading", () => {
    const headings = scanRealDocumentTags(read("roadmap.html"))?.filter(
      ({ isClosing, name }) => !isClosing && name === "h1",
    );
    expect(headings).toHaveLength(1);
    expect(headings?.[0].attributes.get("id")).toBe("roadmap-title");
  });

  it("lists the exact public URLs in README", () => {
    const readme = readRoot("README.md");
    expect(readme).toContain("- **Website:** <https://audiobud.amditis.tech/>");
    expect(readme).toContain(
      "- **Privacy:** <https://audiobud.amditis.tech/privacy.html>",
    );
    expect(readme).toContain(
      "- **Terms:** <https://audiobud.amditis.tech/terms.html>",
    );
    expect(readme).toContain(
      "- **Support:** <https://github.com/jamditis/audiobud/issues>",
    );
    expect(readme).not.toContain("https://jamditis.github.io/audiobud");
  });

  it("describes signed Windows releases accurately in README", () => {
    const readme = readRoot("README.md");

    expect(readme).toContain("Beginning with v0.4.0");
    expect(readme).toContain("signed and timestamped through Microsoft");
    expect(readme).toContain("SmartScreen can still show a reputation warning");
    expect(readme).not.toContain("The build is not code-signed yet");
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
