// AudioBud has no release feed of its own yet. The `plugins.updater` block was
// removed from tauri.conf.json (it pointed at upstream Handy's feed and key), so
// an update check has nothing to query, and the backend no longer registers the
// updater plugin at all -- it gates on its own UPDATER_FEED_READY in
// src-tauri/src/lib.rs (registering the plugin without that config panics at
// startup; issue #32). Keep this false until AudioBud ships its own signed feed
// (milestone B), then flip both flags and restore the config block together.
// updater.test.ts asserts this stays false until then.
export const UPDATER_FEED_READY = false;

/**
 * Whether an update check may run, given the user's stored or optimistic
 * setting. Gated by UPDATER_FEED_READY so no setting value - including an
 * optimistic UI toggle that bypasses the backend load gate - can trigger a
 * check while no AudioBud feed is configured.
 */
export function updateChecksActive(enabled: boolean | undefined): boolean {
  return UPDATER_FEED_READY && Boolean(enabled);
}
