import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";

const read = (path: string) => readFileSync(path, "utf8");
const compact = (value: string) => value.replace(/\s+/g, " ");

describe("public release state for v0.6.0", () => {
  test("offers the published macOS download and checksum manifest", () => {
    const home = compact(read("docs/index.html"));
    const readme = compact(read("README.md"));

    expect(home).toContain("Apple Silicon macOS");
    expect(home).toContain('data-download-macos="_macos_aarch64.dmg"');
    expect(home).toContain("releases/latest/download/SHA256SUMS-macos.txt");
    expect(home).not.toContain("macOS release candidate");
    expect(readme).toContain("Apple Silicon macOS release");
    expect(readme).not.toContain("v0.6.0 release candidate");
  });

  test("marks v0.6.0 shipped and links to the release", () => {
    const roadmap = compact(read("docs/roadmap.html"));

    expect(roadmap).toMatch(
      /<h3>v0\.6\.0<\/h3> <span class="status-pill status-shipped">shipped<\/span>/,
    );
    expect(roadmap).toContain("releases/tag/v0.6.0");
  });

  test("dates the changelog and uses public release notes", () => {
    expect(read("CHANGELOG.md")).toContain("## 0.6.0 - 2026-08-31");
    expect(read("RELEASE_NOTES.md")).toContain(
      "# AudioBud v0.6.0 release notes",
    );
  });
});
