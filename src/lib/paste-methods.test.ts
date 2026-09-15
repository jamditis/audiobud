import { describe, expect, it } from "bun:test";
import {
  pasteMethodModifierForOs,
  pasteMethodsForOs,
  profilePasteMethodsForOs,
} from "./paste-methods";

describe("paste methods by platform", () => {
  it("uses the platform modifier in clipboard labels", () => {
    expect(pasteMethodModifierForOs("macos")).toBe("Cmd");
    expect(pasteMethodModifierForOs("windows")).toBe("Ctrl");
    expect(pasteMethodModifierForOs("unknown")).toBe("Ctrl");
  });

  it("does not offer direct typing on macOS", () => {
    expect(pasteMethodsForOs("macos")).toEqual(["ctrl_v", "none"]);
  });

  it("keeps the platform-specific methods on Windows", () => {
    expect(pasteMethodsForOs("windows")).toEqual([
      "ctrl_v",
      "direct",
      "none",
      "ctrl_shift_v",
      "shift_insert",
    ]);
    expect(pasteMethodsForOs("unknown")).toEqual(["ctrl_v", "none"]);
  });
});

describe("profile paste methods by platform", () => {
  it("uses only persistence-safe macOS profile methods", () => {
    expect(profilePasteMethodsForOs("macos")).toEqual(["ctrl_v", "none"]);
  });

  it("does not offer external scripts on retained platforms", () => {
    expect(profilePasteMethodsForOs("windows")).toEqual([
      "ctrl_v",
      "direct",
      "none",
      "ctrl_shift_v",
      "shift_insert",
    ]);
  });
});
