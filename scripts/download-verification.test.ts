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
    expect(text).toContain("says nothing about what the file does");
    expect(text).toContain("digital signatures");
  });

  it("names the exact publisher a genuine installer must report", () => {
    expect(compactHome).toMatch(/<dt>Publisher<\/dt> <dd>Joseph Amditis<\/dd>/);
    expect(read("README.md")).toContain(
      "CN=Joseph Amditis, O=Joseph Amditis, L=Bloomfield, S=nj, C=US",
    );
  });

  it("quotes the numbered issuer the shipped certificate actually carries", () => {
    // v0.4.0 chains to "Microsoft ID Verified CS AOC CA 04". Publishing the
    // unnumbered base name would tell someone holding a genuine installer that
    // their issuer does not match.
    expect(compactHome).toMatch(
      /<dt>Issuer<\/dt> <dd>Microsoft ID Verified CS AOC CA 04<\/dd>/,
    );
    expect(read("README.md")).toContain("Microsoft ID Verified CS AOC CA 04");

    // Compact first: the sentence wraps across lines in the HTML source.
    for (const source of [home, read("README.md")]) {
      expect(compact(source)).toMatch(
        /rotates the number ending (the|that) issuer/,
      );
    }
  });

  it("never sells the signature as a byte-for-byte check", () => {
    // Authenticode skips the CheckSum field, the certificate table, and any
    // trailing data, so a patched file can still verify as Valid. Claiming
    // otherwise invites users to skip the hash comparison that does cover it.
    for (const source of [home, read("README.md")]) {
      expect(source).not.toMatch(/bytes are untouched/);
      expect(source).not.toMatch(/[Cc]hange one byte[\s\S]{0,80}invalid/);
    }

    expect(compact(home)).toContain(
      "covers the signed parts of the installer rather than every byte",
    );
    expect(read("README.md")).toContain(
      "the signature alone does not cover the whole file",
    );
  });

  it("hard-codes no digest, so the page can never publish a stale one", () => {
    // A hash in the markup is correct for exactly one release and wrong
    // afterwards, and a wrong published hash tells someone holding a good
    // installer that theirs is bad. Digests come from the release API.
    expect(home).not.toMatch(/\b[0-9a-f]{64}\b/);

    // Nothing on the page is allowed to name a version either (#152): the
    // last time this page carried one, it went stale on the next release.
    // SVG path data holds decimals like 4.6, so match version shapes only.
    expect(home).not.toMatch(/\b\d+\.\d+\.\d+\b/);
    expect(home).not.toMatch(/\bv\d+\.\d+/);
    expect(home).not.toMatch(/AudioBud_\d/);
  });

  it("falls back to the release's SHA256SUMS.txt rather than to nothing", () => {
    // Before the API answers -- or if it never does -- the row has to lead
    // somewhere that still lets a user check their file.
    const fallback =
      /<li class="checksum-row checksum-row-pending">[\s\S]*?<\/li>/.exec(
        home,
      )?.[0];
    expect(fallback).toBeDefined();
    expect(compact(fallback!)).toContain(
      'href="https://github.com/jamditis/audiobud/releases/latest/download/SHA256SUMS.txt"',
    );

    // That row holds a link, not a digest, so it must not inherit the
    // select-all monospace styling that makes a hash look copyable.
    const css = compact(read("docs/styles.css"));
    expect(css).toMatch(
      /\.checksum-row-pending \.checksum-value \{[^}]*user-select: auto;/,
    );
  });

  it("ships a verification command users can copy", () => {
    // <version> rather than a number: the command outlives the release.
    expect(compactHome).toContain(
      "Get-FileHash -Algorithm SHA256 .\\AudioBud_&lt;version&gt;_x64-setup.exe",
    );
    expect(read("README.md")).toMatch(
      /Get-AuthenticodeSignature \.\\AudioBud_<version>_x64-setup\.exe/,
    );

    // Once the release is known the placeholder is replaced with the real
    // file name, which is the form a user can paste unedited.
    expect(compact(read("docs/site.js"))).toContain(
      "commandLine.textContent = `Get-FileHash -Algorithm SHA256 .\\\\${installer.name}`",
    );
  });

  it("documents provenance verification against the release workflow", () => {
    const readme = read("README.md");
    expect(readme).toContain(
      "gh attestation verify .\\AudioBud_<version>_x64-setup.exe",
    );
    expect(readme).toContain("--repo jamditis/audiobud");
    expect(readme).toContain(
      "--signer-workflow jamditis/audiobud/.github/workflows/release.yml",
    );
  });

  it("refreshes the checksums from the release API without trusting it", () => {
    const script = compact(read("docs/site.js"));

    expect(script).toContain(
      "https://api.github.com/repos/jamditis/audiobud/releases/latest",
    );
    // A blocked or failing request must leave the published fallback in place.
    expect(script).toMatch(/\.catch\(\(\) => \{/);
    expect(script).toMatch(/if \(installers\.length === 0\) return;/);
    // SHA256SUMS.txt is a release asset and carries its own digest. Rendering
    // it as a row would publish the hash of the hash file next to the two
    // that matter, which reads as a third installer.
    expect(script).toMatch(
      /const installers = assets\.filter\(\(asset\) => \/\\\.\(exe\|msi\)\$\/i\.test\(asset\.name\)/,
    );
    // Digests are written as text so a hostile response cannot inject markup.
    expect(script).toMatch(/value\.textContent = asset\.digest/);
    expect(script).not.toMatch(/innerHTML/);
  });

  it("hands the download button the installer from that same release", () => {
    // The href in the markup stays on the releases page: it can never 404 and
    // it names no version, so a release does not drag a site edit behind it.
    expect(compactHome).toMatch(
      /<a class="button primary" data-download="\.exe" href="https:\/\/github\.com\/jamditis\/audiobud\/releases\/latest"/,
    );

    // Direct-downloading the .exe hides the MSI and the notes, so the card has
    // to keep a way through to the release itself.
    expect(compactHome).toMatch(
      /href="https:\/\/github\.com\/jamditis\/audiobud\/releases\/latest" >All downloads/,
    );

    const script = compact(read("docs/site.js"));

    expect(script).toContain("link.href = asset.browser_download_url");
    // One request feeds both the checksums and the button, so the file the
    // button serves is the file the page publishes a digest for.
    expect(script.match(/fetch\(/g)).toHaveLength(1);
    // A hostile or mistaken response must not be able to retarget the button
    // at a javascript: URL, another host, or another repository.
    expect(script).toContain('url.protocol === "https:"');
    expect(script).toContain('url.hostname === "github.com"');
    expect(script).toContain(
      'url.pathname.startsWith("/jamditis/audiobud/releases/download/")',
    );
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
