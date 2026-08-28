import { describe, expect, it } from "bun:test";
import { pasteMethodsForOs, profilePasteMethodsForOs } from "./paste-methods";

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

describe("profile paste methods by platform", () => {
  it("uses only persistence-safe macOS profile methods", () => {
    expect(profilePasteMethodsForOs("macos")).toEqual(["ctrl_v", "none"]);
  });

  it("never offers the confirmation-gated external script", () => {
    expect(profilePasteMethodsForOs("windows")).toEqual([
      "ctrl_v",
      "direct",
      "none",
      "ctrl_shift_v",
      "shift_insert",
    ]);
    expect(profilePasteMethodsForOs("linux")).toEqual([
      "ctrl_v",
      "direct",
      "none",
      "ctrl_shift_v",
      "shift_insert",
    ]);
  });
});
