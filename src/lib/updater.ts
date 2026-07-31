// Keep this in step with the backend gate and the `plugins.updater` block in
// tauri.conf.json. The feed manifest is attached only after a stable GitHub
// release is published, and its payload signature verifies against AudioBud's
// own public key.
export const UPDATER_FEED_READY = true;

/**
 * Where a portable install sends the user to fetch a new build by hand.
 *
 * This lived inline in UpdateChecker.tsx still pointing at cjpais/Handy, the
 * repo AudioBud forked from. Nobody reaches it today because the dialog is
 * behind UPDATER_FEED_READY, but the moment that flag flips it would hand
 * users a different application signed by a different publisher. Keeping the
 * URL here means the rebrand guard in updater.test.ts can see it.
 */
export const RELEASES_URL =
  "https://github.com/jamditis/audiobud/releases/latest";

/**
 * Whether an update check may run, given the user's stored or optimistic
 * setting. Gated by UPDATER_FEED_READY so no setting value - including an
 * optimistic UI toggle that bypasses the backend load gate - can trigger a
 * check while no AudioBud feed is configured.
 */
export function updateChecksActive(enabled: boolean | undefined): boolean {
  return UPDATER_FEED_READY && Boolean(enabled);
}
