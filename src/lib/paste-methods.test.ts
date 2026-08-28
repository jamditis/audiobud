import { describe, expect, it } from "bun:test";
import { pasteMethodsForOs } from "./paste-methods";

describe("paste methods by platform", () => {
  it("does not offer direct typing on macOS", () => {
    expect(pasteMethodsForOs("macos")).toEqual(["ctrl_v", "none"]);
  });

  it("keeps the platform-specific methods on Windows and Linux", () => {
    expect(pasteMethodsForOs("windows")).toEqual([
      "ctrl_v",
      "direct",
      "none",
      "ctrl_shift_v",
      "shift_insert",
    ]);
    expect(pasteMethodsForOs("linux")).toEqual([
      "ctrl_v",
      "direct",
      "none",
      "ctrl_shift_v",
      "shift_insert",
      "external_script",
    ]);
  });
});
