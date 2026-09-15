import { describe, expect, test } from "bun:test";
import { existsSync, readFileSync } from "node:fs";
import { resolve } from "node:path";

const root = resolve(import.meta.dir, "..");

describe("retired Nix packaging", () => {
  test("does not leave an installable or generated Nix path", () => {
    for (const path of [
      "flake.nix",
      "flake.lock",
      "nix",
      ".nix",
      "scripts/check-nix-deps.ts",
    ]) {
      expect(existsSync(resolve(root, path))).toBe(false);
    }
  });

  test("dependency installation does not invoke Nix tooling", () => {
    const pkg = JSON.parse(readFileSync(resolve(root, "package.json"), "utf8"));
    for (const command of Object.values(pkg.scripts)) {
      expect(String(command)).not.toMatch(/bun2nix|check-nix-deps|flake\.nix/);
    }
  });
});
