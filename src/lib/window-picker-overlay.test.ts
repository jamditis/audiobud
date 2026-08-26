import { describe, it, expect } from "bun:test";
import {
  createPickerState,
  moveHighlight,
  highlightedWindow,
  foregroundGesture,
  chooseAt,
  handleKey,
  pickWasRefused,
  targetOwnsKey,
  type PickerWindow,
  type PickerState,
} from "./window-picker-overlay";

// The overlay reducer is the frontend twin of window_picker.rs resolve_gesture:
// it owns which row is highlighted and which keystroke ends the pick. What
// matters is not line coverage but the two contracts that keep a one-shot pick
// safe: navigation never leaves an out-of-range highlight, and no keystroke ends
// the pick in a way that could misfire a paste. In particular Esc must dismiss,
// never send to the foreground (#124's decided design question).

const rows: readonly PickerWindow[] = [
  { handle: "10", label: "Slack: general" },
  { handle: "20", label: "Chrome: inbox" },
  { handle: "30", label: "Code: window-picker-overlay.ts" },
];

const at = (highlighted: number): PickerState => ({
  windows: rows,
  highlighted,
});

describe("createPickerState", () => {
  it("highlights the first row so a promoted repeat-pick is one confirm", () => {
    // The backend already moved the remembered pick to the front, so first-row
    // highlight is the single confirm #124 asks for.
    expect(createPickerState(rows)).toEqual({ windows: rows, highlighted: 0 });
  });
  it("highlights nothing when the list is empty", () => {
    expect(createPickerState([])).toEqual({ windows: [], highlighted: -1 });
  });
});

describe("moveHighlight", () => {
  it("moves down and up within range", () => {
    expect(moveHighlight(at(0), 1).highlighted).toBe(1);
    expect(moveHighlight(at(2), -1).highlighted).toBe(1);
  });
  it("wraps past the last row to the first and before the first to the last", () => {
    expect(moveHighlight(at(2), 1).highlighted).toBe(0);
    expect(moveHighlight(at(0), -1).highlighted).toBe(2);
  });
  it("is a no-op on an empty list", () => {
    const empty = createPickerState([]);
    expect(moveHighlight(empty, 1)).toEqual(empty);
  });
});

describe("highlightedWindow", () => {
  it("returns the highlighted row", () => {
    expect(highlightedWindow(at(1))).toEqual(rows[1]);
  });
  it("returns null when nothing is highlighted", () => {
    expect(highlightedWindow(createPickerState([]))).toBeNull();
  });
});

describe("chooseAt", () => {
  it("chooses the row at a valid index", () => {
    expect(chooseAt(at(0), 2)).toEqual({
      gesture: { kind: "chose", handle: "30" },
    });
  });
  it("stays open on an out-of-range index, so a stray click cannot fire a pick", () => {
    expect(chooseAt(at(0), 9)).toEqual({ next: at(0) });
  });
});

describe("handleKey navigation", () => {
  it("ArrowDown and ArrowUp move the highlight and wrap", () => {
    expect(handleKey(at(0), "ArrowDown")).toEqual({ next: at(1) });
    expect(handleKey(at(0), "ArrowUp")).toEqual({ next: at(2) });
  });
  it("Home and End jump to the first and last rows", () => {
    expect(handleKey(at(2), "Home")).toEqual({ next: at(0) });
    expect(handleKey(at(0), "End")).toEqual({ next: at(2) });
  });
  it("Home and End keep nothing highlighted on an empty list", () => {
    // End sets length - 1, which is -1 on an empty list only by arithmetic; pin
    // it so a later clamp cannot turn it into an invalid index 0.
    const empty = createPickerState([]);
    expect(handleKey(empty, "Home")).toEqual({ next: empty });
    expect(handleKey(empty, "End")).toEqual({ next: empty });
  });
  it("ignores an unknown key and returns the state unchanged", () => {
    expect(handleKey(at(1), "x")).toEqual({ next: at(1) });
  });
});

describe("handleKey terminal gestures", () => {
  it("Enter chooses the highlighted row", () => {
    expect(handleKey(at(1), "Enter")).toEqual({
      gesture: { kind: "chose", handle: "20" },
    });
  });
  it("a 1-9 digit quick-picks that row by position", () => {
    expect(handleKey(at(0), "3")).toEqual({
      gesture: { kind: "chose", handle: "30" },
    });
  });
  it("ignores a digit past the last row instead of ending the pick", () => {
    expect(handleKey(at(0), "4")).toEqual({ next: at(0) });
  });
  it("f and F deliver to the foreground window", () => {
    expect(handleKey(at(0), "f")).toEqual({ gesture: foregroundGesture() });
    expect(handleKey(at(0), "F")).toEqual({ gesture: { kind: "foreground" } });
  });
});

describe("Esc is a dismissal, never a foreground send (#124 decision)", () => {
  it("Escape dismisses, suppressing the paste", () => {
    expect(handleKey(at(1), "Escape")).toEqual({
      gesture: { kind: "dismiss" },
    });
  });
  it("Enter on an empty list dismisses rather than delivering anywhere", () => {
    expect(handleKey(createPickerState([]), "Enter")).toEqual({
      gesture: { kind: "dismiss" },
    });
  });
});

describe("handles cross the boundary as opaque lossless strings", () => {
  it("echoes a 64-bit-range handle back untouched (no JS number rounding)", () => {
    // A Win64 HWND can exceed 2^53; a string handle round-trips exactly where a
    // number would round. 9_007_199_254_740_993 == 2^53 + 1 loses its low bit
    // as a number.
    const big = "9007199254740993";
    const state = createPickerState([{ handle: big, label: "Editor: big.rs" }]);
    expect(handleKey(state, "Enter")).toEqual({
      gesture: { kind: "chose", handle: big },
    });
    expect(String(Number(big))).not.toBe(big);
  });
});

describe("keys on the picker's own controls belong to the control", () => {
  it("leaves Enter to a focused button instead of arming the highlighted row", () => {
    // The footer's "Use current window" and "Cancel" buttons are activated with
    // Enter. The picker listens on the window, so without this the reducer would
    // pick a row AND swallow the button press.
    for (const tagName of ["BUTTON", "INPUT", "SELECT", "TEXTAREA", "A"]) {
      expect(targetOwnsKey({ tagName }, "Enter")).toBe(true);
    }
    expect(targetOwnsKey({ isContentEditable: true }, "Enter")).toBe(true);
  });

  it("keeps Escape for the picker, even from a focused button", () => {
    expect(targetOwnsKey({ tagName: "BUTTON" }, "Escape")).toBe(false);
  });

  it("leaves keys on the surface itself to the picker", () => {
    expect(targetOwnsKey({ tagName: "DIV" }, "Enter")).toBe(false);
    expect(targetOwnsKey({ tagName: "LI" }, "ArrowDown")).toBe(false);
    expect(targetOwnsKey(null, "Enter")).toBe(false);
  });

  it("matches tag names case-insensitively, as a lowercase DOM would report", () => {
    expect(targetOwnsKey({ tagName: "button" }, "1")).toBe(true);
  });
});

describe("a chosen row the backend could not honor", () => {
  it("is a refusal the user is owed an explanation for", () => {
    // The window closed, or its handle was recycled, between being offered and
    // being clicked. Closing the picker as if the pick worked would leave the
    // user believing their next transcript is routed.
    expect(
      pickWasRefused({ kind: "chose", handle: "7" }, { kind: "cancelled" }),
    ).toBe(true);
  });

  it("is not raised for a pick that landed", () => {
    expect(
      pickWasRefused({ kind: "chose", handle: "7" }, { kind: "window" }),
    ).toBe(false);
  });

  it("is not raised for a dismissal, which is what the user asked for", () => {
    expect(pickWasRefused({ kind: "dismiss" }, { kind: "cancelled" })).toBe(
      false,
    );
    expect(pickWasRefused({ kind: "foreground" }, { kind: "foreground" })).toBe(
      false,
    );
  });
});
