import { describe, it, expect } from "bun:test";
import { idsTrimmedByNextRecording, type TrimCandidate } from "./history-trim";

// The History page marks the entries that the next recording will delete
// (#76). A wrong mark either scares a user about an entry that stays, or, the
// case that matters, stays quiet about one that goes. These cases mirror the
// cleanup in src-tauri/src/managers/history.rs.

const NOW = 1_800_000_000;
const DAY = 24 * 60 * 60;

// Newest first, one minute apart, as the History page holds them.
const entries = (savedIds: number[] = []): TrimCandidate[] =>
  [5, 4, 3, 2, 1].map((id) => ({
    id,
    timestamp: NOW - (6 - id) * 60,
    saved: savedIds.includes(id),
  }));

const trimmed = (
  list: TrimCandidate[],
  period: string,
  limit = 5,
  now = NOW,
): number[] =>
  [...idsTrimmedByNextRecording(list, period, limit, now)].sort(
    (a, b) => a - b,
  );

describe("count mode (preserve_limit)", () => {
  it("counts the new recording against the limit", () => {
    // Limit 3: the new recording plus the two newest unsaved entries stay.
    expect(trimmed(entries(), "preserve_limit", 3)).toEqual([1, 2, 3]);
  });

  it("marks nothing while there is room for the new recording", () => {
    expect(trimmed(entries(), "preserve_limit", 6)).toEqual([]);
  });

  it("marks the oldest entry when history is exactly at the limit", () => {
    expect(trimmed(entries(), "preserve_limit", 5)).toEqual([1]);
  });

  it("never marks a saved entry and does not count it against the limit", () => {
    // 4 and 2 are saved, so the unsaved ranking is 5, 3, 1.
    expect(trimmed(entries([4, 2]), "preserve_limit", 3)).toEqual([1]);
  });

  it("marks every unsaved entry at a limit of zero", () => {
    expect(trimmed(entries([3]), "preserve_limit", 0)).toEqual([1, 2, 4, 5]);
  });

  it("marks a loaded page exactly as the full history would", () => {
    // The History page holds only the pages loaded so far, newest first. Every
    // entry newer than a loaded one is also loaded, so its rank is the same as
    // in the database. Older, unloaded entries get their mark when they load.
    const full: TrimCandidate[] = Array.from({ length: 70 }, (_, i) => ({
      id: 70 - i,
      timestamp: NOW - i * 60,
      saved: i % 4 === 0,
    }));
    const fullMarks = idsTrimmedByNextRecording(
      full,
      "preserve_limit",
      40,
      NOW,
    );
    for (const loaded of [30, 60, 70]) {
      const page = full.slice(0, loaded);
      const expected = page
        .filter((e) => fullMarks.has(e.id))
        .map((e) => e.id)
        .sort((a, b) => a - b);
      expect(trimmed(page, "preserve_limit", 40)).toEqual(expected);
    }
  });

  it("ranks by timestamp, not by list order", () => {
    // Entry 5 has the highest id but the oldest timestamp, so it goes first.
    const list = entries().map((e) =>
      e.id === 5 ? { ...e, timestamp: NOW - DAY } : e,
    );
    expect(trimmed(list, "preserve_limit", 5)).toEqual([5]);
  });
});

describe("time modes", () => {
  const aged = (ageDays: number[], savedIds: number[] = []): TrimCandidate[] =>
    ageDays.map((days, i) => ({
      id: i + 1,
      timestamp: NOW - days * DAY,
      saved: savedIds.includes(i + 1),
    }));

  it("marks unsaved entries older than 3 days", () => {
    expect(trimmed(aged([1, 2.9, 3.1, 10]), "days3")).toEqual([3, 4]);
  });

  it("uses 14 days for weeks2 and 90 days for months3", () => {
    const list = aged([13, 15, 89, 91]);
    expect(trimmed(list, "weeks2")).toEqual([2, 3, 4]);
    expect(trimmed(list, "months3")).toEqual([4]);
  });

  it("never marks a saved entry, however old", () => {
    expect(trimmed(aged([10, 20], [2]), "days3")).toEqual([1]);
  });

  it("ignores the history limit", () => {
    expect(trimmed(aged([1, 1, 1, 1]), "days3", 1)).toEqual([]);
  });
});

describe("modes that delete nothing", () => {
  it("marks nothing with never", () => {
    expect(trimmed(entries(), "never", 0)).toEqual([]);
  });

  it("marks nothing for a value it does not know", () => {
    // bindings.ts spells the time modes "days_3", but the stored value is
    // "days3". An unknown value marks nothing rather than guess.
    expect(trimmed(entries(), "days_3")).toEqual([]);
  });
});
