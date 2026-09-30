export interface TrimCandidate {
  id: number;
  /** Unix seconds, as stored in history.db. */
  timestamp: number;
  saved: boolean;
}

const DAY_SECONDS = 24 * 60 * 60;

// Must match cleanup_by_time_with_conn in src-tauri/src/managers/history.rs.
// These are the serde values of RecordingRetentionPeriod ("days3", not the
// "days_3" that bindings.ts declares), the same strings the retention
// dropdown sends.
const RETENTION_SECONDS = new Map<string, number>([
  ["days3", 3 * DAY_SECONDS],
  ["weeks2", 2 * 7 * DAY_SECONDS],
  ["months3", 3 * 30 * DAY_SECONDS],
]);

/**
 * Ids of the entries that the lazy cleanup will delete when the next recording
 * is saved (#55, #76). Settings changes never delete anything; save_entry runs
 * the cleanup after it inserts the new, unsaved recording. Saved entries are
 * never deleted.
 *
 * `entries` must run without gaps from the newest entry, which is how the
 * History page loads its pages. Count mode ranks the unsaved entries, so a gap
 * would shift every rank after it.
 */
export function idsTrimmedByNextRecording(
  entries: readonly TrimCandidate[],
  retentionPeriod: string,
  historyLimit: number,
  nowSeconds: number,
): Set<number> {
  // The page loads by id; the cleanup ranks by timestamp, so rank by timestamp.
  const unsaved = entries
    .filter((entry) => !entry.saved)
    .sort((a, b) => b.timestamp - a.timestamp || b.id - a.id);

  if (retentionPeriod === "preserve_limit") {
    // The new recording takes one of the kept slots, so an existing unsaved
    // entry survives only if it is among the newest (limit - 1).
    const kept = Math.max(historyLimit - 1, 0);
    return new Set(unsaved.slice(kept).map((entry) => entry.id));
  }

  const retentionSeconds = RETENTION_SECONDS.get(retentionPeriod);
  if (retentionSeconds === undefined) {
    // "never", or a value this code does not know: promise nothing.
    return new Set();
  }
  const cutoff = nowSeconds - retentionSeconds;
  return new Set(
    unsaved
      .filter((entry) => entry.timestamp < cutoff)
      .map((entry) => entry.id),
  );
}
