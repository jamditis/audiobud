// Milestone A: the Tauri updater endpoint in tauri.conf.json still points at
// upstream Handy's release feed and verifies against upstream's key (see
// docs/superpowers/DEFERRED-issues.md "Provenance"). Until that feed is
// repointed to AudioBud and its builds are signed (milestone B), AudioBud must
// never run an update check - a check would query, and could offer to install,
// an upstream Handy release. Flip this to true in milestone B once the feed is
// AudioBud's own; updater.test.ts asserts it stays false until then.
export const UPDATER_FEED_READY = false;

/**
 * Whether an update check may run, given the user's stored or optimistic
 * setting. Gated by UPDATER_FEED_READY so no setting value - including an
 * optimistic UI toggle that bypasses the backend load gate - can trigger a
 * check while the feed is still upstream.
 */
export function updateChecksActive(enabled: boolean | undefined): boolean {
  return UPDATER_FEED_READY && Boolean(enabled);
}
