import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";

const read = (path: string) => readFileSync(path, "utf8");
const compact = (value: string) => value.replace(/\s+/g, " ");

describe("public release state before v0.6.0 publication", () => {
  test("labels macOS as a candidate and keeps current downloads on Windows", () => {
    const home = compact(read("docs/index.html"));
    const readme = compact(read("README.md"));

    expect(home).toContain("macOS release candidate");
    expect(home).not.toContain('data-download-macos="_macos_aarch64.dmg"');
    expect(home).not.toContain("releases/latest/download/SHA256SUMS-macos.txt");
    expect(readme).toContain("v0.6.0 release candidate");
  });

  test("does not mark the release shipped or link to a missing tag", () => {
    const roadmap = compact(read("docs/roadmap.html"));

    expect(roadmap).toMatch(
      /<h3>v0\.6\.0<\/h3> <span class="status-pill status-in-progress">candidate<\/span>/,
    );
    expect(roadmap).not.toContain("releases/tag/v0.6.0");
  });

  test("keeps patch notes and the changelog in draft state", () => {
    expect(read("CHANGELOG.md")).toContain("## 0.6.0 - Unreleased");
    expect(read("RELEASE_NOTES.md")).toContain(
      "Status: draft. AudioBud v0.6.0 is not published yet.",
    );
  });
});
