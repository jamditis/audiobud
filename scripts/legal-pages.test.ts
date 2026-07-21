import { describe, expect, it } from "bun:test";
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";

const root = join(import.meta.dir, "..");
const docs = join(root, "docs");
const read = (name: string) => readFileSync(join(docs, name), "utf8");

const sitePages = ["index.html", "roadmap.html", "privacy.html", "terms.html"];

describe("AudioBud public policy pages", () => {
  it("publishes the custom domain from docs", () => {
    expect(read("CNAME").trim()).toBe("audiobud.amditis.tech");
  });

  it("publishes separate privacy and terms pages", () => {
    expect(existsSync(join(docs, "privacy.html"))).toBe(true);
    expect(existsSync(join(docs, "terms.html"))).toBe(true);
  });

  it("uses the custom HTTPS origin in public metadata", () => {
    for (const page of sitePages) {
      const html = read(page);
      expect(html).toContain("https://audiobud.amditis.tech");
      expect(html).not.toContain("https://jamditis.github.io/audiobud");
    }
  });

  it("links privacy and terms from every public page", () => {
    for (const page of sitePages) {
      const html = read(page);
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
    expect(terms).toContain("MIT License");
    expect(terms).toContain(
      "copying, modifying, or distributing the source code",
    );
    expect(terms).not.toContain("class-action waiver");
    expect(terms).not.toContain("binding arbitration");
  });
});
