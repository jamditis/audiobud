// One-shot window picker: overlay interaction core (#124).
//
// #258 landed the platform-independent core in src-tauri/src/window_picker.rs:
// visible_candidates() filters and labels the OS window list, resolve_gesture()
// turns the user's terminal action into a PickOutcome a dismissal cannot turn
// into a stray paste, and LastPick remembers the last pick so a repeat route is
// one confirm. What that core does NOT own is the overlay the user drives: which
// row is highlighted, how a keystroke moves it, and which keystroke ends the
// pick. This module is that piece, kept as a pure reducer so the React overlay is
// a thin renderer and the interaction is testable without a window system.
//
// It is the frontend twin of resolve_gesture: every terminal key maps to a
// PickerGesture the wiring child hands to resolve_gesture over a Tauri command.
// The two must agree on the vocabulary -- chose a window, send to the foreground,
// or dismiss -- so PickerGesture's three cases correspond one-to-one to the Rust
// PickGesture (Chose, FocusForeground, Dismiss). The kind strings differ from the
// Rust variant names, so the wiring child maps each kind to its variant.
//
// THE OPEN DESIGN QUESTION #124 left ("Esc = deliver to foreground, or cancel")
// is decided here: Esc DISMISSES and suppresses the paste; it never delivers to
// the foreground. Binding a send to Esc is the exact stray-paste misfire the
// one-shot flow exists to avoid, and acceptance criterion 2 asks for a dismissal
// that is "predictable and non-destructive". Foreground delivery stays reachable,
// but only through a deliberate key (f) or the overlay's own button, never
// through the key a user reaches for to back out. This mirrors the Rust core
// keeping FocusForeground and Dismiss as separate gestures for the same reason.
//
// Like window_picker.rs before its Windows caller landed, this core is exercised
// only by the unit tests below until the wiring child lands: the Tauri command
// that enumerates windows and maps a PickerGesture to resolve_gesture, and the
// React overlay that renders these rows and forwards keys. That wiring is tracked
// in #259.
//
// One boundary contract that child must hold: a WindowHandle is an isize
// (src-tauri/src/output_target.rs), which is 64-bit on Win64 and can exceed a JS
// number's 2^53 safe range. So the command must serialize each handle as a
// STRING, and this module carries it opaquely as one -- it only compares and
// echoes handles, never interprets them -- so a large HWND is never rounded on
// the way to the paste path.

/**
 * An opaque window handle as the backend serializes it. A string, never a
 * number: see the module header on 64-bit HWND precision. This module treats it
 * as an identifier to compare and echo, nothing more.
 */
export type WindowHandleId = string;

/**
 * One offered row. `label` is produced by the backend's WindowCandidate::label
 * (app and title together) and rendered as-is; the frontend never recomputes it,
 * so the row text and its source cannot drift -- the same one-place discipline
 * the lock indicator uses for its display name.
 */
export interface PickerWindow {
  readonly handle: WindowHandleId;
  readonly label: string;
}

/**
 * The live overlay state: the rows on offer and which one is highlighted.
 * `highlighted` indexes `windows`, and is -1 only when there are no windows.
 */
export interface PickerState {
  readonly windows: readonly PickerWindow[];
  readonly highlighted: number;
}

/**
 * The user's terminal action, mirroring window_picker.rs PickGesture:
 * - `chose`: deliver this one transcript to the window with `handle`.
 * - `foreground`: deliver to the current foreground window instead of a pick.
 * - `dismiss`: back out; the paste is suppressed, never a foreground fallback.
 */
export type PickerGesture =
  | { readonly kind: "chose"; readonly handle: WindowHandleId }
  | { readonly kind: "foreground" }
  | { readonly kind: "dismiss" };

/**
 * The result of feeding one key to the reducer: either the overlay stays open
 * with a new state (navigation, or an ignored key that returns the state
 * unchanged), or the pick ends with a gesture. Exactly one field is set.
 */
export type PickerStep =
  | { readonly next: PickerState; readonly gesture?: undefined }
  | { readonly gesture: PickerGesture; readonly next?: undefined };

/**
 * The initial overlay state for a freshly enumerated window list. The highlight
 * starts on the first row because the backend has already moved the remembered
 * pick there (LastPick::promote_to_front), so a repeat route is the single
 * confirm #124 asks for. An empty list highlights nothing (-1).
 */
export function createPickerState(
  windows: readonly PickerWindow[],
): PickerState {
  return { windows, highlighted: windows.length > 0 ? 0 : -1 };
}

/**
 * Move the highlight by `delta` rows, wrapping at both ends. A no-op on an empty
 * list. Wrapping is deliberate: the list is short, and a user pressing ArrowUp on
 * the first row means the last one, not a dead end.
 */
export function moveHighlight(state: PickerState, delta: number): PickerState {
  const n = state.windows.length;
  if (n === 0) return state;
  const next = (((state.highlighted + delta) % n) + n) % n;
  return { ...state, highlighted: next };
}

/** The highlighted row, or null when the list is empty. */
export function highlightedWindow(state: PickerState): PickerWindow | null {
  const { windows, highlighted } = state;
  return highlighted >= 0 && highlighted < windows.length
    ? windows[highlighted]
    : null;
}

/**
 * The gesture for the explicit "send to the current window" affordance, shared by
 * the overlay's foreground button and the f key so both name the same action.
 */
export function foregroundGesture(): PickerGesture {
  return { kind: "foreground" };
}

/**
 * Choose the row at `index`, for a pointer click on a row. An out-of-range index
 * is ignored and the overlay stays open, so a click on empty chrome around the
 * rows never fires a pick.
 */
export function chooseAt(state: PickerState, index: number): PickerStep {
  const win = state.windows[index];
  return win
    ? { gesture: { kind: "chose", handle: win.handle } }
    : { next: state };
}

/**
 * The parts of a keydown target this module needs. Structural, not a DOM type,
 * so the rule below is testable without a document.
 */
export interface KeyTarget {
  readonly tagName?: string;
  readonly isContentEditable?: boolean;
}

/** Controls that own their own keyboard behaviour. */
const INTERACTIVE_TAGS = new Set([
  "BUTTON",
  "INPUT",
  "SELECT",
  "TEXTAREA",
  "A",
]);

/**
 * Whether a keydown on `target` belongs to the control it landed on rather than
 * to the picker.
 *
 * The picker listens for keys on the window, so a keydown on its own footer
 * buttons reaches the reducer too. Without this, Enter while "Use current
 * window" or "Cancel" is focused would arm the HIGHLIGHTED ROW and swallow the
 * button's activation -- the picker would do something the user did not ask
 * for. Escape is the one exception: backing out has to work from anywhere,
 * including a focused button, and no control here does anything else with it.
 */
export function targetOwnsKey(target: KeyTarget | null, key: string): boolean {
  if (key === "Escape" || !target) return false;
  return (
    INTERACTIVE_TAGS.has((target.tagName ?? "").toUpperCase()) ||
    target.isContentEditable === true
  );
}

/**
 * Feed one keydown to the overlay. `key` is a KeyboardEvent.key. Returns either
 * the next state (the overlay stays open) or the gesture that ends the pick:
 * - ArrowDown / ArrowUp: move the highlight, wrapping.
 * - Home / End: jump to the first / last row.
 * - 1-9: quick-pick that row by 1-based position; an out-of-range digit is inert.
 * - Enter: choose the highlighted row; on an empty list it dismisses, since there
 *   is nothing to deliver to and a suppressed paste is the safe end.
 * - Escape: dismiss (see the module header: never a foreground paste).
 * - f / F: deliver to the foreground window, the one deliberate non-pick send.
 * - anything else: ignored, the state is returned unchanged, so a stray keystroke
 *   never ends the pick.
 */
export function handleKey(state: PickerState, key: string): PickerStep {
  switch (key) {
    case "ArrowDown":
      return { next: moveHighlight(state, 1) };
    case "ArrowUp":
      return { next: moveHighlight(state, -1) };
    case "Home":
      return {
        next: { ...state, highlighted: state.windows.length > 0 ? 0 : -1 },
      };
    case "End":
      return { next: { ...state, highlighted: state.windows.length - 1 } };
    case "Enter": {
      const win = highlightedWindow(state);
      return win
        ? { gesture: { kind: "chose", handle: win.handle } }
        : { gesture: { kind: "dismiss" } };
    }
    case "Escape":
      return { gesture: { kind: "dismiss" } };
    case "f":
    case "F":
      return { gesture: foregroundGesture() };
    default: {
      // A 1-9 digit is a quick-pick by 1-based position, routed through chooseAt
      // so the "a bad index never fires a pick" guard lives in one place.
      if (/^[1-9]$/.test(key)) {
        return chooseAt(state, Number(key) - 1);
      }
      return { next: state };
    }
  }
}
