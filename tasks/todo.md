# macOS release plan

## Goal

Publish AudioBud v0.6.0 for Apple Silicon as a signed and notarized DMG. Publish verified Windows artifacts in the same product release without changing Windows behavior. Reduce release risk and measured complexity before publication.

This plan uses these release decisions:

- Distribution: direct download from GitHub Releases.
- Architecture: Apple Silicon (`aarch64-apple-darwin`) only for the first release.
- Minimum system: macOS 11.0, aligned with the current Swift bridge target.
- Updates: manual download for v0.6.0. A signed macOS updater is a separate follow-up.
- Store scope: no Mac App Store package in this release.
- Version scope: create one joint Windows and macOS v0.6.0 release. Do not replace or change v0.5.0.
- Windows scope: rebuild and test the existing Windows installers and signed updater. Do not add or remove Windows features.

## Rules

- Do not read, copy, print, or commit secrets.
- Keep certificates and notarization credentials in Keychain or GitHub secrets.
- Do not commit, push, publish, or change a live release without user approval.
- Write a failing test before each bug fix.
- Preserve Windows behavior and public command names.
- Use the complexity and readability guide as a decision tool. Do not rewrite the repository.
- Measure native startup and package size before and after debloat work.

## Starting evidence

- [x] The repository is clean on `main` and matches `origin/main` at v0.5.0.
- [x] The current release workflow builds Windows only.
- [x] The current GitHub release has no macOS artifact.
- [x] The public download page selects Windows installers only.
- [x] Frontend tests pass: 394 passed and 0 failed.
- [x] Frontend lint and production build pass.
- [x] Frontend output is 1.62 MiB raw and 596.5 KiB gzip.
- [x] The main initial frontend graph is 638.6 KiB raw and 185.9 KiB gzip.
- [x] Rust 1.98.0, native checks, and a local Apple Silicon Tauri build now pass.
- [x] A valid Developer ID Application identity is available in the local Keychain.
- [x] The signing identity was selectively exported with its private key without exposing it.
- [x] A dedicated AudioBud App Store Connect key, issuer id, and key id are available without exposing the private key.
- [x] The current Tauri configuration uses ad hoc signing and does not create updater artifacts.
- [x] The configured minimum macOS version is 10.15, but the Swift bridge targets macOS 11.0.
- [x] Upstream Handy has newer macOS fixes that need a selective review.

## Phase 0: establish a native baseline

- [x] Install the current stable Rust toolchain with the Apple Silicon target.
- [x] Confirm Xcode command-line tools, the Tauri prerequisites, and the WebKit toolchain.
- [x] Fetch or generate only the test assets required by the documented build process.
- [x] Run `bun install --frozen-lockfile`.
- [x] Run `bun run lint`.
- [x] Run `bun run format:check`.
- [x] Run `bun run test`.
- [x] Run `bun run check:translations`.
- [x] Run `bun run check:rebrand`.
- [x] Run `cargo fmt --all -- --check`.
- [x] Run `cargo test --all-targets`.
- [x] Run `cargo clippy --all-targets --all-features -- -D warnings`.
- [x] Build an unsigned local Apple Silicon `.app` and DMG.
- [ ] Record app launch time, time to interactive, idle memory, binary size, `.app` size, DMG size, and bundled frameworks.
- [x] Inspect native dependencies with `otool -L` and confirm that all non-system libraries are bundled.
- [x] Dump the built app's `Info.plist` and entitlements without printing credentials.
- [x] Confirm the microphone usage description and required direct-distribution audio entitlement are present in the built app.
- [x] Confirm that the Developer ID identity is exportable with its private key.
- [x] Confirm that an App Store Connect API key, issuer ID, and key ID are available for notarization.

Acceptance criteria:

- All baseline checks have recorded results.
- The app starts on the development Mac without Homebrew libraries in its runtime path.
- Signing and notarization prerequisites are confirmed before release workflow work starts.
- Any failing test or build error has a separate test-first fix before later release work.

## Phase 1: fix shared state and macOS correctness

### Settings startup

- [x] Add a failing test that calls settings initialization several times before the first call resolves.
- [x] Make settings initialization single-owner and idempotent with one shared in-flight operation.
- [x] Start initialization once from the application entry point.
- [x] Register each backend listener once and retain its cleanup function.
- [x] Change `useSettings()` to use focused selectors and remove initialization from the hook.
- [x] Prove that one backend event causes one settings refresh.

### Ordered settings writes

- [x] Add a failing test that resolves two writes in reverse order.
- [x] Keep temporary slider state in the component and persist on pointer release, keyboard commit, or blur.
- [x] Add a per-setting revision guard or serialized writer.
- [x] Keep `isUpdating` true until the newest write finishes.
- [x] Prove that the final UI and stored value are the newest choice.

### macOS permissions

- [x] Add tests for all accessibility and microphone permission combinations.
- [x] Add tests for denial, retry, query failure, slow polling, duplicate completion, and permission revocation.
- [x] Give the application startup flow sole ownership of permission state.
- [x] Pass permission state and actions to onboarding and the settings warning.
- [x] Permit one permission query at a time and one onboarding completion.
- [x] Replace the unrestricted asynchronous interval with a guarded recursive timeout or an equivalent single-flight mechanism.
- [x] Keep native bridge calls in `src/lib/macos-permissions.ts`.
- [x] Make the settings action reopen System Settings after a denial or closed window.

### Native input and startup safety

- [x] Add fake-input tests that fail after every modifier-key event.
- [x] Add a pressed-key guard that releases Command and Shift in reverse order on every error path.
- [x] Add a failing model-load panic test and prove that a later load can retry.
- [x] Use the existing loading guard in the background model-load task.
- [x] Add shortcut backend tests for registration and manager-start failures.
- [x] Return a shortcut initialization result and set the initialization marker only after the required dictation shortcut works.
- [x] Permit cleanup and retry after a shortcut initialization failure.
- [x] Remove the direct paste option from macOS and migrate an existing direct-paste setting to the safe macOS method.
- [x] Review upstream macOS fixes and keep only changes verified for AudioBud.

### Audio lifecycle

- [x] Add concurrent microphone open, close, mute, and unmute tests.
- [x] Use one documented lock order or one lifecycle mutex.
- [x] Move blocking `osascript` work outside state locks.
- [x] Prove that the tests do not deadlock and state remains correct after a native command error.

Acceptance criteria:

- Each fix starts with a failing test and ends with a passing test.
- Settings and permission listeners have one owner.
- A native permission denial can be retried without restarting the app unless macOS itself requires a restart.
- Input failures do not leave a modifier key pressed.
- Shortcut and model-load failures can retry in the same process.
- Windows tests remain green.

## Phase 2: complete release-focused cleanup

### Safe removals and platform gates

- [x] Remove the unused direct `rdev` dependency after `cargo tree` confirms it is not a required direct dependency.
- [x] Remove duplicate `autostart` and `updater` capability entries.
- [x] Compile and initialize the updater only on Windows for v0.6.0.
- [x] Gate or remove the unused voice-command parser if no command, test, or documented feature uses it.
- [x] Pin mutable patched Tauri branches to reviewed revisions, or remove each patch if the released dependency contains the needed fix.

### Release UI

- [x] Fix the macOS footer separator and remove the stale hard-coded version fallback.
- [x] Add a clear manual update link for macOS.
- [x] Move the hard-coded `Use \"...\"` label to localization files and test it.
- [x] Keep the settings updater table, `modelStore.ts`, generated bindings, localization loader, microphone level hook, and macOS permission bridge unless a test proves a defect.

### Measure again

- [ ] Repeat the native baseline measurements.
- [ ] Compare startup time, time to interactive, memory, native binary size, `.app` size, DMG size, main frontend graph, and full frontend output.
- [x] Keep a refactor only when it removes a defect, a clear ownership problem, an unused path, or measured cost without reducing readability.

Acceptance criteria:

- No duplicate listener owner, capability entry, direct unused dependency, or unsupported updater path remains.
- Cleanup removes a proven defect, duplicate, unused path, unsupported platform path, or mutable dependency reference.
- The before-and-after report states measured gains and any regressions.
- The full test and static-check suite passes.

## Phase 3: add the macOS release path

- [x] Set one minimum macOS version of 11.0 in Tauri, Rust build settings, and Swift build settings.
- [x] Add a macOS CI job that compiles and tests the real application and engine on Apple Silicon.
- [x] Add an Apple Silicon release job that creates an `.app` and DMG.
- [x] Import the Developer ID certificate from GitHub secrets during the job.
- [x] Add a macOS signing configuration overlay that replaces ad hoc signing only in the release job.
- [x] Sign the app and all nested native code with hardened runtime enabled.
- [x] Submit the signed app for Apple notarization.
- [x] Staple the notarization ticket to the distributable.
- [x] Verify the signature with `codesign`.
- [x] Verify Gatekeeper acceptance with `spctl`.
- [x] Verify the stapled ticket without network access where practical.
- [x] Verify every bundled dynamic library with `otool -L` and `codesign`.
- [x] Generate SHA-256 checksums, an SBOM, and build provenance for the macOS artifact.
- [x] Use an explicit artifact name that contains `macos`, `aarch64`, and the version.
- [x] Add an explicit rename step instead of relying on Tauri's `bundle.targets: \"all\"` output name.
- [x] Use separate workflow concurrency groups for Windows and macOS build jobs.
- [x] Keep signing secrets out of logs and uploaded artifacts.

Acceptance criteria:

- A clean release job produces one signed, notarized, and stapled Apple Silicon DMG.
- `codesign`, `spctl`, dependency inspection, checksum generation, SBOM generation, and provenance verification pass in automation.
- CI runs real macOS backend tests before the release job can publish artifacts.

## Phase 4: test the release candidate on a clean Mac

- [ ] Install the DMG on an Apple Silicon Mac that has no project toolchain or Homebrew dependency.
- [ ] Confirm first launch through Gatekeeper.
- [ ] Test first launch with no permissions.
- [ ] Test each single-permission state and both permissions granted.
- [ ] Deny each permission, close System Settings, and retry.
- [ ] Revoke each permission after startup and confirm the app recovers or gives a clear action.
- [ ] Test microphone recording and local transcription.
- [ ] Test the default Whisper path and each supported Apple Intelligence path or fallback.
- [ ] Test safe paste in TextEdit, a browser field, and Terminal.
- [ ] Confirm clipboard restoration after success and simulated failure.
- [ ] Test overlay placement on multiple displays, Spaces, and full-screen applications.
- [ ] Test tray, Dock, start hidden, autostart, reopen, and quit behavior.
- [ ] Test sleep, wake, microphone device changes, and an interrupted model download.
- [ ] Confirm the manual update link opens the correct AudioBud release page.
- [ ] Confirm no updater error or orphan footer separator appears on macOS.
- [ ] Record crash logs, console errors, startup time, memory, and package sizes.
- [x] Accept the documented five-second HandyKeys native-startup limit for
      v0.6.0. The project maintainer owns follow-up work if field reports show
      that the blocked native constructor can remain alive.

Acceptance criteria:

- A clean Mac can install, open, authorize, transcribe, paste, quit, reopen, and find the next release without development tools.
- No critical or high-severity defect remains.
- Medium-severity defects have a written release decision and owner.

### Verify Windows for the joint release

- [ ] Install the v0.6.0 NSIS and MSI artifacts on supported Windows hardware.
- [ ] Test record, transcribe, paste, output routing, target lock, startup, tray, and quit behavior.
- [ ] Test the signed updater from v0.5.0 to the v0.6.0 draft artifact.
- [ ] Confirm `update-feed/latest.json` selects only the tested v0.6.0 Windows updater artifact.
- [ ] Confirm the Windows code signature, checksum, SBOM, and provenance.

Acceptance criteria:

- Both Windows installers pass the release smoke test.
- A real v0.5.0 installation updates to v0.6.0 and keeps its settings and models.
- The updater feed is not changed if the update test fails.

## Phase 5: prepare and publish v0.6.0

- [x] Update the version in all manifests and generated metadata.
- [x] Inventory every documentation and public web surface before editing so no inherited or stale release text is missed.
- [x] Update the README and system requirements to mark Apple Silicon macOS 11 or newer as validated.
- [x] Update the changelog and write clear v0.6.0 patch notes from the verified changes.
- [x] Update architecture, build, testing, release, signing, notarization, support, and contributor documentation.
- [x] Update all in-repository help pages and public website copy that describes platforms, downloads, installation, permissions, updates, or versions.
- [x] Update the public download page to detect macOS and select the Apple Silicon DMG.
- [x] Keep Windows downloads and behavior unchanged in the source and release configuration.
- [x] Confirm the existing favicon, Open Graph image, social text, and download metadata describe the macOS release correctly.
- [x] Add install, permission, update, signature, checksum, and support instructions for macOS.
- [x] Search the final tree and public site for stale versions, Windows-only claims, AudioBash or Handy names, old download URLs, and unsupported macOS claims.
- [x] Check every edited link and render the README, changelog, patch notes, and public pages before publication.
- [x] Run all frontend and Rust checks from a clean checkout.
- [x] Confirm every public surface labels macOS v0.6.0 as a candidate and does not link to an absent tag, DMG, or Mac checksum manifest.
- [x] Ask for explicit approval to commit and push the reviewed candidate to
      `release/v0.6.0-candidate`.
- [x] Commit and push only to `release/v0.6.0-candidate` after that approval.
      The candidate-safe `docs/` copy may publish through GitHub Pages, but it
      must not call v0.6.0 shipped.
- [x] Dispatch the signed release workflow from exact `main` commit `e417154`
      with `make_release: false` and `store_candidate: false`.
- [x] Download both CI artifacts from run `33195432598` and repeat their
      checksum, SBOM, provenance, signature, notarization, Gatekeeper,
      architecture, dependency, and controlled-launch checks where the local
      platform permits.
- [ ] Produce a replacement candidate whose macOS and Windows SPDX file records
      all contain real checksums.
- [ ] Complete first-launch, permission, transcription, paste, display, sleep,
      and device-change tests on a clean Apple Silicon Mac.
- [ ] Test the candidate Windows installers and the v0.5.0 to v0.6.0 updater path.
- [ ] Confirm the prepared `update-feed/latest.json` points to the tested Windows updater artifact.
- [ ] Review every changed line and complete the release checklist.
- [ ] After candidate validation, prepare one docs-only release-state commit on top of the tested `main` commit. Change the website, README, changelog date, patch-note status, download buttons, roadmap, and checksum fallbacks from candidate to released. Do not update `main` yet.
- [ ] Run the full docs and public-surface tests against that release-state commit and confirm it changes no application or packaging code from the tested candidate.
- [ ] Ask for separate explicit approval to commit the release-state changes, create the v0.6.0 tag on that commit, and push the tag without pushing `main`.
- [ ] Create and push the tag only after that approval. Let its workflow build fresh signed and notarized Windows and Mac files into the joint draft.
- [ ] Confirm the joint draft body exactly matches `RELEASE_NOTES.md` from the tagged commit.
- [ ] Download the exact tag-built draft assets. Repeat every signature, notarization, Gatekeeper, architecture, dependency, checksum, SBOM, provenance, clean-Mac install/transcription/paste, Windows install/uninstall, and updater test against those exact bytes.
- [ ] Ask for explicit publication approval after the exact tag-built draft passes.
- [ ] Publish the release only after that approval.
- [ ] After the release is public, fast-forward `main` to the same tagged release-state commit so GitHub Pages publishes links that now exist.

Acceptance criteria:

- The release page contains the verified DMG, checksum, SBOM, provenance, installation notes, system requirements, and known limits.
- The website serves the correct installer to macOS and Windows users.
- The README, changelog, patch notes, repository docs, help pages, website, and release notes agree on version, supported platforms, install steps, update behavior, and known limits.
- No stale platform, product, version, or download statement remains on a release-facing surface.
- The published DMG hash matches the tested draft artifact.
- The published Windows hashes and updater feed match the tested draft artifacts.
- The tag-built draft files, not the earlier `main` CI artifacts, pass the final exact-byte release checks.
- GitHub Pages and the README do not call v0.6.0 shipped or link to its assets before the release is public.
- The commit and push have explicit user approval.
- The tag and publication have separate explicit user approval.

## Work after v0.6.0

- [ ] Add a signed macOS updater artifact and feed.
- [ ] Test v0.6.0 to the next release through the updater before enabling automatic checks.
- [ ] Move Tailwind and its Vite plugin to development dependencies.
- [ ] Extract engine loading, language selection, result processing, recording persistence, history policy, and delivery handoff from the large transcription path only where tests define behavior.
- [ ] Split the recording stop path by lifecycle phase, not by line count.
- [ ] Extract one application notification hook from `App.tsx` after the startup controller is stable.
- [ ] Lazy-load non-default settings sections if native startup measurements show a useful gain.
- [ ] Keep `react-select` and Emotion outside the initial graph only if the change preserves accessible creatable input and keyboard behavior.
- [ ] Load Windows-only output controls behind a build-time platform boundary if the packaging system can prove that they leave the macOS output.
- [ ] Do not remove Lucide based on installed package size; use built output as evidence.
- [ ] Evaluate Intel support with a real Intel test machine and bundled ONNX libraries.
- [ ] Evaluate Mac App Store distribution as a separate sandboxing, entitlement, and private-API removal project.
- [ ] Migrate inherited storage names only with a tested data migration.

## Review record

- [x] Frontend complexity audit completed.
- [x] Rust complexity audit completed.
- [x] macOS release audit completed.
- [x] Independent final review of this plan completed.
- [x] Valid review findings incorporated.
- [x] User approved implementation.
- [x] User required a final docs, website, README, changelog, and patch-notes sweep across all release surfaces.
- [x] Pull request 309 opened from `release/v0.6.0-candidate`.
- [x] The three original connector findings were resolved in remote commit
      `db89c23`.
- [x] An independent review of `db89c23` plus its Clippy fix found one Windows
      shortcut regression and two macOS release proof gaps.
- [x] Test-first fixes for those findings are in the isolated worktree.
- [x] Full validation of the complete follow-up checkpoint passes.
- [x] Fresh local and Claude Code reviews have no unresolved P1, P2, or P3
      finding.
- [x] The final read-only review of the complete SBOM fix has no unresolved P1,
      P2, or P3 finding.
- [x] The GitHub connector review of pull request 319 completed with no
      finding.
- [x] Pull request 319 merged into `main` as `1dcea94` after all required checks
      passed.
- [x] The independent re-review of the Windows Syft compatibility fix found no
      actionable P1, P2, or P3 issue after its filesystem, SPDX metadata,
      no-op, inventory, and atomic-write findings were corrected.
- [x] Pull request 309 merged into `main` as commits `a6fdd06`, `688b8aa`, and
      `e417154`.
- [x] CI, the real Windows engine job, the size report, Pages, and the protected
      unsigned-release candidate workflow passed on `e417154`.

### Current release decision

Status: local and remote `main` point to `1dcea94`. Pull request 319 merged after
all required checks and the GitHub connector review passed. Candidate run
`33226723040` failed closed at the Windows SBOM checksum gate without creating
a tag, release, Windows artifact, Windows attestation, or checksum manifest.

The macOS job passed signing, app and DMG notarization, stapling, Gatekeeper,
SBOM generation, the new checksum gate, provenance, SBOM attestation, and
artifact upload. The Windows job passed its builds, signing, Authenticode,
portable-install, packaged-signature, and SBOM-generation checks. Its 339
Windows regular-file records then failed checksum validation.

The environment setting reached Syft 1.49.0. The failure is the upstream Syft
Windows directory-resolver bug: Syft catalogs the staged paths but cannot
resolve them again for digest collection. The test-first workaround adds a
`Complete Windows SBOM file checksums` step. It replaces only an actual regular
file's exact single zero-SHA-1 placeholder with SHA-1 and SHA-256 calculated
from the staged bytes. Mixed, missing, malformed, unsupported, and real but
stale checksums still fail. The completed document is written atomically only
after the existing exact-inventory and byte validation passes. A changed
document records the completion tool and a derived SPDX namespace. A document
with valid native digests stays byte-for-byte unchanged. Special filesystem
entries fail before hashing. macOS does not use this compatibility step.

The corrected workaround passes 510 frontend and release-contract tests with
2,310 assertions across 46 files, 503 Rust library tests plus the dictionary
integration test, the production frontend build, TypeScript, ESLint, all 19
translation checks, rebrand validation, Prettier, Rust formatting, Clippy with
warnings denied, workflow lint, and whitespace validation. The final
independent re-review found no actionable P1, P2, or P3 issue.

Next gate: commit and push the workaround through a dedicated bug-fix pull
request. After it merges, run a new protected candidate and repeat exact
artifact checks. Pull requests 311 through 317 stay outside v0.6.0 and must not
merge into `main` before publication. Do not create or push the v0.6.0 tag,
publish a release, change the updater feed, or use Partner Center until a
replacement candidate passes.
The user's end-to-end release approval authorizes those actions after their
technical gates pass. It does not authorize publishing a failed or unverified
artifact.

### Session handoff for August 28, 2026

Safe state:

- Local `main` and `origin/main` point to `1dcea94`. The worktree is on
  `fix/windows-syft-file-digests` with the uncommitted test-first Windows Syft
  compatibility fix and release evidence updates.
- The older local connector implementation is recoverable from the named
  `pre-reconcile local connector fixes 2026-08-28` stash. It is not part of the
  candidate.
- The old detached pull request worktree was clean and has been removed.
- Candidate run `33226723040` failed closed on the Windows SBOM gate. Its macOS
  job passed and uploaded artifact id `9707382447`; no Windows artifact exists.
- The rejected earlier candidate artifacts remain under
  `/private/tmp/audiobud-v060-candidate.taVo6P`. Do not use those bytes as
  replacement-candidate evidence.
- The replacement run's macOS artifact passed the checksum validator. No
  replacement Windows artifact exists because its job failed closed.
- The full local suite and final independent re-review pass. Commit, push, and
  the dedicated bug-fix pull request are the next gates.
- Pull requests 311 through 317 each have a release-freeze coordination comment
  for the Office agent. Do not modify or merge those pull requests here.
- No v0.6.0 tag, release, updater-feed change, or Partner Center submission has
  been made.

Separate follow-up, outside this release checkpoint:

- [ ] Add a tested load-time confirmation boundary for hand-edited global and
      profile `external_script` settings. Pull request 309 did not introduce
      this existing settings-load gap.

Resume in this order:

1. Complete full validation and independent review of the Windows Syft
   workaround. Keep pull requests 311 through 317 unmerged.
2. Commit, push, and open the dedicated bug-fix pull request under the user's
   end-to-end release approval.
3. After the fix passes review, CI, and merge, rerun the protected
   candidate workflow with release and Store publication disabled.
4. Verify both replacement artifacts, then run Windows install, delivery,
   target-lock, uninstall, and v0.5.0 updater tests.
5. Finish the clean-Mac first-launch, permission, transcription, paste, display,
   sleep, and device-change tests.
6. Confirm that the prepared Windows update manifest selects only the tested
   candidate updater. Do not publish it.
7. Review every changed line and complete the release checklist before the
   release-state commit and tag approval gates.
