import { describe, expect, it } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";

const root = join(import.meta.dir, "..");
const read = (path: string) => readFileSync(join(root, path), "utf8");
const compact = (value: string) => value.replace(/\s+/g, " ");

describe("visual polish regression contracts", () => {
  it("routes the prominent website downloads through the installer warning", () => {
    const home = compact(read("docs/index.html"));
    const roadmap = compact(read("docs/roadmap.html"));

    expect(home).toMatch(/class="nav-cta" href="#install"/);
    expect(home).toMatch(/class="button primary" href="#install"/);
    expect(roadmap).toMatch(/class="nav-cta" href="\.\/index\.html#install"/);
    expect(home).toMatch(
      /id="install"[\s\S]*class="install-note"[\s\S]*Unsigned release:[\s\S]*github\.com\/jamditis\/audiobud\/releases\/latest/,
    );
  });

  it("stacks the website hero before its preview can overlap the copy", () => {
    const css = compact(read("docs/styles.css"));

    expect(css).toMatch(
      /@media \(max-width: 980px\) \{[\s\S]*?\.hero-grid \{[^}]*grid-template-columns: 1fr;/,
    );
  });

  it("does not paint a second app screenshot behind the mobile hero", () => {
    const css = compact(read("docs/styles.css"));

    expect(css).not.toMatch(
      /@media \(max-width: 860px\) \{[\s\S]*?\.hero::before \{[^}]*url\("\.\/assets\/app-general\.png"\)/,
    );
  });

  it("keeps the fixed header readable without JavaScript or scrolling", () => {
    const css = compact(read("docs/styles.css"));

    expect(css).toMatch(
      /\.site-header \{[^}]*background: rgba\(7, 16, 11, 0\.82\);[^}]*backdrop-filter: blur\(20px\) saturate\(1\.18\);/,
    );
  });

  it("prints all progressively revealed website content", () => {
    const css = compact(read("docs/styles.css"));

    expect(css).toMatch(
      /@media print \{[\s\S]*?\.js \[data-reveal\] \{[^}]*opacity: 1;[^}]*transform: none;/,
    );
  });

  it("hides decorative model initials from assistive technology", () => {
    const home = read("docs/index.html");
    const modelRanks = [...home.matchAll(/<div class="model-rank"([^>]*)>/g)];

    expect(modelRanks).toHaveLength(3);
    for (const [, attributes] of modelRanks) {
      expect(attributes).toContain('aria-hidden="true"');
    }
  });

  it("reserves animated state dots for overlays without the RAW badge", () => {
    const source = compact(read("src/overlay/RecordingOverlay.tsx"));
    const guardedDots = source.match(
      /\{!isRaw && \( <span className="state-dots" aria-hidden="true">/g,
    );

    expect(guardedDots).toHaveLength(2);
  });

  it("lets translated overlay state labels shrink safely", () => {
    const css = compact(read("src/overlay/RecordingOverlay.css"));

    expect(css).toMatch(
      /\.state-label \{[^}]*min-width: 0;[^}]*overflow: hidden;/,
    );
    expect(css).toMatch(
      /\.state-label > span:first-child \{[^}]*overflow: hidden;[^}]*text-overflow: ellipsis;/,
    );
  });

  it("uses the full theme text color for the small toolbar kicker", () => {
    const css = compact(read("src/App.css"));

    expect(css).toMatch(
      /\.content-toolbar-kicker \{[^}]*color: var\(--color-text\);/,
    );
  });
});
