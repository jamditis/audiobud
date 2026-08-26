// Output-target lock indicator core (#121).
//
// #120 landed the platform-independent lock primitive in
// src-tauri/src/output_target.rs: a paste is delivered to the foreground window
// or pinned to one the user locked, and a pinned window that has closed resolves
// to LockLost, which clears the lock as a fail-safe. #121 asks for a visible,
// dismissible indicator of that lock, shown the same way across the recording
// overlay, the tray, and settings so they can never disagree about what is
// locked.
//
// This module is the one place that decides what those surfaces show. It maps a
// lock snapshot (the state the backend reports over an event) to a single
// view-model: whether the indicator is visible, the target's display name, how
// loud it should be, and whether to offer an unlock. Keeping the precedence,
// truncation, and tone in one tested core, not duplicated in three components,
// is what makes "the surfaces never disagree" hold by construction rather than
// by review.
//
// Like output_target.rs before its Windows caller landed, this core is
// exercised only by the unit tests below until the wiring child lands: the
// backend snapshot command and event, the Windows GetWindowTextW/process-name
// label resolution, and the overlay/tray/settings subscriptions are tracked in
// #255. The backend sends raw, untruncated app and title strings; this module
// owns truncation and formatting so the source of the name and the source of the
// display cannot drift.
//
// One contract subtlety the wiring child must preserve: the "lost" snapshot is
// event-derived, not a state the backend can be queried for. resolve() returns
// LockLost once and clears the lock in the same step, so "stale" is a latch the
// frontend holds off that event until the user dismisses it or a new lock or
// paste replaces it. A poll of the current lock state after a loss reads
// "unlocked", never "lost".

/**
 * The longest target name the indicator shows before truncation. Window titles
 * can be long; a compact indicator stays one short line. The backend sends the
 * full string and this core truncates, so every surface truncates identically.
 */
export const MAX_TARGET_NAME_LENGTH = 32;

// ASCII marker appended to a truncated name. Kept ASCII on purpose: the ellipsis
// character is avoided in source per house style.
const TRUNCATION_MARKER = "...";

/**
 * A snapshot of the output-target lock as the backend reports it to the
 * frontend. Mirrors the states of output_target.rs:
 * - `unlocked`: the paste follows the foreground window (OutputTarget::Foreground).
 * - `locked`: a window is pinned and still alive (OutputTarget::Pinned).
 * - `lost`: a pinned window closed; resolve() returned LockLost and cleared the
 *   lock. Carried as an event, held by the frontend as a latch (see the module
 *   header). `app`/`title` are the last known name.
 *
 * `app` and `title` are the raw strings the platform probe read (an app/process
 * name and the window title). Either may be absent. They arrive untruncated.
 */
export type LockSnapshot =
  | { readonly kind: "unlocked" }
  | { readonly kind: "locked"; readonly app?: string; readonly title?: string }
  | { readonly kind: "lost"; readonly app?: string; readonly title?: string };

/** What the indicator is currently saying. `hidden` renders nothing. */
export type IndicatorStatus = "hidden" | "locked" | "stale";

/**
 * How loud the indicator should be. `quiet` for a live lock the user chose;
 * `attention` only when the lock went stale and needs a glance.
 */
export type IndicatorTone = "quiet" | "attention";

/**
 * The normalized view-model every surface renders. A surface reads these fields
 * and adds only its own chrome (an icon, a localized caption, the arrow glyph):
 * the decisions that must match across surfaces live here, not in the chrome.
 */
export interface TargetIndicator {
  /** Whether to render the indicator at all. False only when unlocked. */
  readonly visible: boolean;
  /** What the indicator is saying, for a surface that branches on it. */
  readonly status: IndicatorStatus;
  /**
   * The target's display name, resolved and truncated. Empty when the name is
   * unknown or the indicator is hidden; a surface supplies its own localized
   * fallback (for example "locked window") when this is empty but visible.
   */
  readonly targetName: string;
  /** Presentation loudness. `attention` only when stale. */
  readonly tone: IndicatorTone;
  /** Whether to offer an unlock affordance. True whenever visible. */
  readonly showUnlock: boolean;
}

/** Options for deriving an indicator, defaulted for the common case. */
export interface DeriveOptions {
  /**
   * Override the truncation ceiling (a roomier surface such as settings may pass
   * a larger value). A value at or below the marker length falls back to a hard
   * code-point slice with no marker.
   */
  readonly maxNameLength?: number;
}

/**
 * Resolve the display name from a snapshot's app and title. The app (process)
 * name wins when present because it is the stabler identifier; the window title
 * is the fallback; an unknown target resolves to an empty string. Whitespace is
 * collapsed so a multi-line title becomes one clean line.
 */
export function resolveTargetName(app?: string, title?: string): string {
  const clean = (value?: string): string =>
    (value ?? "").replace(/\s+/g, " ").trim();
  const appName = clean(app);
  if (appName.length > 0) return appName;
  return clean(title);
}

/**
 * Format a window name for the delivered-transcript confirmation (#165 review
 * round 1, finding 4). `resolveTargetName` above is built for the lock
 * indicator badge -- a narrow surface that only has room for one identifier,
 * so it picks the stabler one (the app) and drops the title. A delivery
 * confirmation exists specifically to say *which* window received the text,
 * so when a process has several open windows (two Chrome windows, two Word
 * documents) the app name alone is ambiguous; this combines both when both
 * are known, and falls back to `resolveTargetName`'s single-name precedence
 * when only one is.
 */
export function formatDeliveredWindowName(
  app?: string,
  title?: string,
): string {
  const clean = (value?: string): string =>
    (value ?? "").replace(/\s+/g, " ").trim();
  const appName = clean(app);
  const windowTitle = clean(title);
  if (appName.length > 0 && windowTitle.length > 0 && appName !== windowTitle) {
    return `${windowTitle} — ${appName}`;
  }
  return resolveTargetName(app, title);
}

/**
 * Truncate a display name to `max` Unicode code points, appending an ASCII
 * marker. Counting code points, not UTF-16 units, keeps an astral character
 * from being split into a broken half. A name already within the ceiling is
 * returned unchanged.
 */
export function truncateName(
  name: string,
  max: number = MAX_TARGET_NAME_LENGTH,
): string {
  const points = Array.from(name);
  if (points.length <= max) return name;
  if (max <= TRUNCATION_MARKER.length) {
    // No room for a name plus the marker: hard-slice to the ceiling, no marker.
    return points.slice(0, Math.max(0, max)).join("");
  }
  const keep = max - TRUNCATION_MARKER.length;
  return points.slice(0, keep).join("") + TRUNCATION_MARKER;
}

/**
 * Truncate a name for a compact display by keeping both ends and eliding the
 * middle, rather than cutting off the tail the way `truncateName` does (#279
 * review round 4). `truncateName` is right for a lock indicator badge, where
 * names rarely share a prefix; it is wrong for `formatDeliveredWindowName`'s
 * "title — app" combination, where two windows of the same app commonly do
 * share one ("Google Docs - A" vs "Google Docs - B") -- a head-only
 * truncation collapses both to the same compact string and defeats the
 * point of a confirmation that is supposed to say which window received the
 * text. Keeping a slice of both ends preserves whatever part actually
 * differs, wherever it falls.
 */
export function truncateMiddle(
  name: string,
  headLength: number = 6,
  tailLength: number = 5,
): string {
  const points = Array.from(name);
  const max = headLength + tailLength + TRUNCATION_MARKER.length;
  if (points.length <= max) return name;
  const head = points.slice(0, headLength).join("");
  const tail =
    tailLength > 0 ? points.slice(points.length - tailLength).join("") : "";
  return `${head}${TRUNCATION_MARKER}${tail}`;
}

/**
 * Map a lock snapshot to the indicator view-model. This is the whole contract:
 * hidden when unlocked, a quiet named indicator while locked, and a louder one
 * holding the last known name when the lock went stale. Every surface derives
 * from here so none can disagree.
 */
export function deriveIndicator(
  snapshot: LockSnapshot,
  options: DeriveOptions = {},
): TargetIndicator {
  const max = options.maxNameLength ?? MAX_TARGET_NAME_LENGTH;
  if (snapshot.kind === "unlocked") {
    return {
      visible: false,
      status: "hidden",
      targetName: "",
      tone: "quiet",
      showUnlock: false,
    };
  }
  const targetName = truncateName(
    resolveTargetName(snapshot.app, snapshot.title),
    max,
  );
  if (snapshot.kind === "locked") {
    return {
      visible: true,
      status: "locked",
      targetName,
      tone: "quiet",
      showUnlock: true,
    };
  }
  // kind === "lost": the pinned window closed; hold the last known name and
  // raise the tone so the user notices the paste will fall back to foreground.
  return {
    visible: true,
    status: "stale",
    targetName,
    tone: "attention",
    showUnlock: true,
  };
}
