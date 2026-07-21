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
const escapeRegExp = (value: string) =>
  value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
const expectTagWithAttributes = (
  html: string,
  tag: string,
  attributes: Record<string, string>,
) => {
  const attributeChecks = Object.entries(attributes)
    .map(
      ([name, value]) => `(?=[^>]*\\b${name}=["']${escapeRegExp(value)}["'])`,
    )
    .join("");
  expect(html).toMatch(new RegExp(`<${tag}\\b${attributeChecks}[^>]*>`));
};

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
