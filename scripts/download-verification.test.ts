import { describe, expect, it } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";

const root = join(import.meta.dir, "..");
const read = (path: string) => readFileSync(join(root, path), "utf8");
const compact = (value: string) => value.replace(/\s+/g, " ");

const home = read("docs/index.html");
const compactHome = compact(home);

describe("download verification guidance", () => {
  it("gives the SmartScreen warning its own anchored section", () => {
    expect(compactHome).toMatch(/<section id="verify"/);
    expect(compactHome).toMatch(
      /class="install-note"[\s\S]*?<a href="#verify">/,
    );
    // Every page footer reaches the section, so the anchor has to be absolute
    // rather than the same-page "#verify" the install note can use.
    for (const page of ["index", "roadmap", "privacy", "terms"]) {
      expect(compact(read(`docs/${page}.html`))).toMatch(
        /<nav class="footer-links"[^>]*> <a href="\.\/index\.html#verify">Verify a download<\/a>/,
      );
    }
  });

  it("explains that the warning measures downloads rather than risk", () => {
    const section = /<section id="verify"[\s\S]*?<\/section>/.exec(home)?.[0];
    expect(section).toBeDefined();

    const text = compact(section!.replace(/<[^>]+>/g, " ")).toLowerCase();
    expect(text).toContain("isn't commonly downloaded");
    expect(text).toContain("popularity score, not a security verdict");
    expect(text).toContain("digital signatures");
  });

  it("names the exact publisher a genuine installer must report", () => {
    expect(compactHome).toMatch(/<dt>Publisher<\/dt> <dd>Joseph Amditis<\/dd>/);
    expect(read("README.md")).toContain(
      "CN=Joseph Amditis, O=Joseph Amditis, L=Bloomfield, S=nj, C=US",
    );
  });

  it("publishes a SHA-256 digest for every shipped installer", () => {
    const rows = [...home.matchAll(/<li class="checksum-row">[\s\S]*?<\/li>/g)];
    expect(rows).toHaveLength(2);

    const names = rows.map(
      ([row]) => /<span class="checksum-name">([^<]+)</.exec(row)?.[1],
    );
    expect(names).toEqual([
      "AudioBud_0.4.0_x64-setup.exe",
      "AudioBud_0.4.0_x64_en-US.msi",
    ]);

    for (const [row] of rows) {
      const digest = /<code class="checksum-value"\s*>([^<]+)</.exec(row)?.[1];
      expect(digest?.trim()).toMatch(/^[0-9a-f]{64}$/);
    }
  });

  it("ships a verification command users can copy", () => {
    expect(compactHome).toMatch(
      /Get-FileHash -Algorithm SHA256 \.\\AudioBud_[\d.]+_x64-setup\.exe/,
    );
    expect(read("README.md")).toMatch(
      /Get-AuthenticodeSignature \.\\AudioBud_<version>_x64-setup\.exe/,
    );
  });

  it("refreshes the checksums from the release API without trusting it", () => {
    const script = compact(read("docs/site.js"));

    expect(script).toContain(
      "https://api.github.com/repos/jamditis/audiobud/releases/latest",
    );
    // A blocked or failing request must leave the published fallback in place.
    expect(script).toMatch(/\.catch\(\(\) => \{/);
    expect(script).toMatch(/if \(assets\.length === 0\) return;/);
    // Digests are written as text so a hostile response cannot inject markup.
    expect(script).toMatch(/value\.textContent = asset\.digest/);
    expect(script).not.toMatch(/innerHTML/);
  });

  it("keeps the verification cards readable on narrow screens", () => {
    const css = compact(read("docs/styles.css"));

    expect(css).toMatch(
      /@media \(max-width: 980px\) \{[\s\S]*?\.verify-grid \{[^}]*grid-template-columns: 1fr;/,
    );
    expect(css).toMatch(/\.checksum-value \{[^}]*overflow-wrap: anywhere;/);
    // The verification command has to stay fully readable rather than clipping
    // its tail into a scroll container users will not notice.
    expect(css).toMatch(
      /@media \(max-width: 560px\) \{[\s\S]*?\.checksum-command pre \{[^}]*white-space: pre-wrap;/,
    );
  });
});
