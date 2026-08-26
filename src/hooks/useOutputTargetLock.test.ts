import { describe, expect, it } from "bun:test";
import { toSnapshot } from "./useOutputTargetLock";

// The backend sends Option<String> fields, which specta maps to `T | null`;
// the indicator core (output-target-indicator.ts) uses the optional-property
// convention (`T | undefined`) instead. toSnapshot is the one place that
// reconciles the two, so it is the one place this needs proving (#255).

describe("toSnapshot", () => {
  it("maps an unlocked event with no app/title fields", () => {
    expect(toSnapshot({ kind: "unlocked" })).toEqual({ kind: "unlocked" });
  });

  it("maps null app/title to undefined for a locked snapshot", () => {
    expect(toSnapshot({ kind: "locked", app: null, title: null })).toEqual({
      kind: "locked",
      app: undefined,
      title: undefined,
    });
  });

  it("passes real app/title strings through unchanged", () => {
    expect(
      toSnapshot({ kind: "locked", app: "Terminal", title: "zsh" }),
    ).toEqual({ kind: "locked", app: "Terminal", title: "zsh" });
  });

  it("maps a lost snapshot the same way as locked", () => {
    expect(toSnapshot({ kind: "lost", app: "Terminal", title: null })).toEqual({
      kind: "lost",
      app: "Terminal",
      title: undefined,
    });
  });
});
