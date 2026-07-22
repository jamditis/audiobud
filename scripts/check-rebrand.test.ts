import { describe, expect, it } from "bun:test";
import { readFileSync } from "node:fs";

const packageJson = JSON.parse(readFileSync("package.json", "utf8")) as {
  scripts?: Record<string, string>;
};
const ci = readFileSync(".github/workflows/ci.yml", "utf8");
const ciDirectives = ci
  .split("\n")
  .filter((line) => !/^\s*#/.test(line))
  .join("\n");

describe("rebrand CI gate", () => {
  it("defines the rebrand check as a package command", () => {
    expect(packageJson.scripts?.["check:rebrand"]).toBe(
      "bun scripts/check-rebrand.ts",
    );
  });

  it("runs the package command in CI", () => {
    expect(ciDirectives).toMatch(/^\s*run: bun run check:rebrand\s*$/m);
  });
});
