import { describe, expect, test } from "bun:test";
import { existsSync, readFileSync, statSync } from "node:fs";
import path from "node:path";

const root = path.resolve(import.meta.dir, "..");
const read = (relativePath: string) =>
  readFileSync(path.join(root, relativePath), "utf8");

describe("offline app fonts", () => {
  test("does not allow a Google Fonts request from the app shell or CSP", () => {
    const appShell = read("index.html");
    const tauriConfig = read("src-tauri/tauri.conf.json");

    for (const content of [appShell, tauriConfig]) {
      expect(content).not.toContain("fonts.googleapis.com");
      expect(content).not.toContain("fonts.gstatic.com");
    }
  });

  test("vendors every declared font file and both upstream licenses", () => {
    const stylesheet = read("src/App.css");
    const fontFiles = [
      "bungee-latin.woff2",
      "bungee-latin-ext.woff2",
      "bungee-vietnamese.woff2",
      "fredoka-hebrew.woff2",
      "fredoka-latin.woff2",
      "fredoka-latin-ext.woff2",
    ];

    for (const filename of fontFiles) {
      const relativePath = `src/assets/fonts/${filename}`;
      expect(stylesheet).toContain(`./assets/fonts/${filename}`);
      expect(existsSync(path.join(root, relativePath))).toBe(true);
      expect(statSync(path.join(root, relativePath)).size).toBeGreaterThan(
        1_000,
      );
    }

    for (const filename of ["OFL-Bungee.txt", "OFL-Fredoka.txt"]) {
      expect(read(`src/assets/fonts/${filename}`)).toContain(
        "SIL OPEN FONT LICENSE Version 1.1",
      );
    }

    const bundledNotices = read("THIRD_PARTY_NOTICES.md");
    expect(bundledNotices).toContain(
      "Copyright 2023 The Bungee Project Authors",
    );
    expect(bundledNotices).toContain(
      "Copyright 2016 The Fredoka Project Authors",
    );
    expect(bundledNotices).toContain("SIL OPEN FONT LICENSE Version 1.1");
    expect(read("src-tauri/tauri.conf.json")).toContain(
      '"../THIRD_PARTY_NOTICES.md": "THIRD_PARTY_NOTICES.md"',
    );
  });
});
