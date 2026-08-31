import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";

const root = join(import.meta.dir, "..");
const readCompact = (path: string) =>
  readFileSync(join(root, path), "utf8")
    .replace(/\n>\s?/g, "\n")
    .replace(/\s+/g, " ");

describe("experimental Windows output targeting release text", () => {
  test("states one consistent release boundary on every public surface", () => {
    const documents = [
      "README.md",
      "CHANGELOG.md",
      "RELEASE_NOTES.md",
      "STORE_SUBMISSION.md",
    ];

    for (const document of documents) {
      const text = readCompact(document);
      expect(text, document).toContain("Experimental output targeting");
      expect(text, document).toContain("off by default");
      expect(text, document).toContain(
        "Windows can refuse to activate the selected window",
      );
      expect(text, document).toContain(
        "AudioBud does not send input to a different window",
      );
      expect(text, document).toContain("history and clipboard");
    }
  });
});
