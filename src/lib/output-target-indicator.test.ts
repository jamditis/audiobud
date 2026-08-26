import { describe, it, expect } from "bun:test";
import {
  deriveIndicator,
  formatDeliveredWindowName,
  resolveTargetName,
  truncateMiddle,
  truncateName,
  MAX_TARGET_NAME_LENGTH,
  type LockSnapshot,
} from "./output-target-indicator";

// The indicator core maps a lock snapshot to the single view-model the recording
// overlay, the tray, and settings all render. What matters is not line coverage
// but the contract that lets those surfaces never disagree: the same precedence,
// the same truncation, and a tone that stays quiet for a live lock the user
// chose and only rises when the lock has gone stale.

describe("resolveTargetName", () => {
  it("prefers the app name over the window title", () => {
    expect(resolveTargetName("Terminal", "zsh - 80x24")).toBe("Terminal");
  });

  it("falls back to the title when no app name is present", () => {
    expect(resolveTargetName(undefined, "Untitled document")).toBe(
      "Untitled document",
    );
  });

  it("falls back to the title when the app name is blank", () => {
    expect(resolveTargetName("   ", "Notepad")).toBe("Notepad");
  });

  it("resolves to empty when neither name is known", () => {
    expect(resolveTargetName(undefined, undefined)).toBe("");
  });

  it("collapses whitespace so a multi-line title is one clean line", () => {
    expect(resolveTargetName(undefined, "Draft\n  second line\treview")).toBe(
      "Draft second line review",
    );
  });
});

describe("formatDeliveredWindowName", () => {
  it("combines the title and the app when both are known", () => {
    expect(formatDeliveredWindowName("Chrome", "Inbox - Gmail")).toBe(
      "Inbox - Gmail — Chrome",
    );
  });

  it("distinguishes two windows of the same app", () => {
    const first = formatDeliveredWindowName("Chrome", "Inbox - Gmail");
    const second = formatDeliveredWindowName("Chrome", "Docs - Sheet1");
    expect(first).not.toBe(second);
  });

  it("falls back to the app alone when there is no title", () => {
    expect(formatDeliveredWindowName("Terminal", undefined)).toBe("Terminal");
  });

  it("falls back to the title alone when there is no app", () => {
    expect(formatDeliveredWindowName(undefined, "Untitled document")).toBe(
      "Untitled document",
    );
  });

  it("does not repeat an identical app and title", () => {
    expect(formatDeliveredWindowName("Notepad", "Notepad")).toBe("Notepad");
  });

  it("resolves to empty when neither name is known", () => {
    expect(formatDeliveredWindowName(undefined, undefined)).toBe("");
  });
});

describe("truncateName", () => {
  it("leaves a name within the ceiling unchanged", () => {
    const name = "Terminal";
    expect(truncateName(name, 32)).toBe(name);
  });

  it("leaves a name exactly at the ceiling unchanged", () => {
    const name = "x".repeat(10);
    expect(truncateName(name, 10)).toBe(name);
  });

  it("truncates one past the ceiling and appends an ASCII marker", () => {
    const result = truncateName("x".repeat(11), 10);
    expect(result).toBe("xxxxxxx...");
    expect(Array.from(result).length).toBe(10);
  });

  it("never splits an astral character when truncating", () => {
    // "grinning face" is one code point but two UTF-16 units. Counting code
    // points keeps the truncation from leaving a broken surrogate half.
    const result = truncateName("\u{1F600}".repeat(40), 10);
    expect(Array.from(result).length).toBe(10);
    expect(result.endsWith("...")).toBe(true);
    expect(result.slice(0, result.length - 3)).toBe("\u{1F600}".repeat(7));
  });

  it("hard-slices with no marker when the ceiling has no room for one", () => {
    expect(truncateName("abcdef", 2)).toBe("ab");
  });
});

describe("truncateMiddle", () => {
  it("leaves a name within the budget unchanged", () => {
    expect(truncateMiddle("Terminal")).toBe("Terminal");
  });

  it("keeps two same-prefix names distinguishable", () => {
    // The scenario the confirmation chip exists to avoid (#279 review round
    // 4): two windows sharing everything but their tail must not truncate to
    // the same compact string the way a head-only truncation would.
    const a = truncateMiddle("Google Docs - A", 6, 5);
    const b = truncateMiddle("Google Docs - B", 6, 5);
    expect(a).not.toBe(b);
  });

  it("keeps two same-prefix delivered-window names distinguishable with the overlay's own budget", () => {
    // The overlay's chip uses a wider tail than the generic default so a
    // formatDeliveredWindowName "title — app" string's distinguishing suffix
    // survives even with a short app name appended after it.
    const a = truncateMiddle(
      formatDeliveredWindowName("Chrome", "Google Docs - A"),
      6,
      10,
    );
    const b = truncateMiddle(
      formatDeliveredWindowName("Chrome", "Google Docs - B"),
      6,
      10,
    );
    expect(a).not.toBe(b);
  });

  it("keeps the requested number of code points on each end", () => {
    const result = truncateMiddle("abcdefghijklmnopqrstuvwxyz", 6, 5);
    expect(result).toBe("abcdef...vwxyz");
  });

  it("never splits an astral character when truncating", () => {
    const result = truncateMiddle("\u{1F600}".repeat(40), 2, 2);
    expect(result.startsWith("\u{1F600}\u{1F600}")).toBe(true);
    expect(result.endsWith("\u{1F600}\u{1F600}")).toBe(true);
    expect(result).toBe("\u{1F600}\u{1F600}...\u{1F600}\u{1F600}");
  });

  it("supports a zero-length tail for a head-only compact form", () => {
    expect(truncateMiddle("abcdefghij", 4, 0)).toBe("abcd...");
  });
});

describe("deriveIndicator", () => {
  it("hides the indicator when the target is unlocked", () => {
    const view = deriveIndicator({ kind: "unlocked" });
    expect(view.visible).toBe(false);
    expect(view.status).toBe("hidden");
    expect(view.targetName).toBe("");
    expect(view.tone).toBe("quiet");
    expect(view.showUnlock).toBe(false);
  });

  it("shows a quiet, unlockable indicator while locked", () => {
    const view = deriveIndicator({ kind: "locked", app: "Terminal" });
    expect(view.visible).toBe(true);
    expect(view.status).toBe("locked");
    expect(view.targetName).toBe("Terminal");
    expect(view.tone).toBe("quiet");
    expect(view.showUnlock).toBe(true);
  });

  it("still offers an unlock when a live lock has no known name", () => {
    // A visible-but-nameless lock leaves targetName empty for the surface to
    // fill with its own localized fallback; the unlock affordance stays.
    const view = deriveIndicator({ kind: "locked" });
    expect(view.visible).toBe(true);
    expect(view.status).toBe("locked");
    expect(view.targetName).toBe("");
    expect(view.showUnlock).toBe(true);
  });

  it("raises the tone and keeps the last name when the lock goes stale", () => {
    const view = deriveIndicator({ kind: "lost", app: "Terminal" });
    expect(view.visible).toBe(true);
    expect(view.status).toBe("stale");
    expect(view.targetName).toBe("Terminal");
    expect(view.tone).toBe("attention");
    expect(view.showUnlock).toBe(true);
  });

  it("truncates a long target name by default", () => {
    const long = "A very long window title that will not fit on one short line";
    const view = deriveIndicator({ kind: "locked", title: long });
    expect(Array.from(view.targetName).length).toBe(MAX_TARGET_NAME_LENGTH);
    expect(view.targetName.endsWith("...")).toBe(true);
  });

  it("honors a roomier truncation ceiling for a surface that has space", () => {
    const long = "A very long window title that will not fit on one short line";
    const view = deriveIndicator(
      { kind: "locked", title: long },
      { maxNameLength: 80 },
    );
    expect(view.targetName).toBe(long);
  });

  it("applies app-over-title precedence through the full derivation", () => {
    const snapshot: LockSnapshot = {
      kind: "locked",
      app: "Terminal",
      title: "zsh - 80x24",
    };
    expect(deriveIndicator(snapshot).targetName).toBe("Terminal");
  });
});
