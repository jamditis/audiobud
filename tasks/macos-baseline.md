# macOS v0.5.0 release baseline

Recorded on August 27, 2026, before the v0.6.0 release changes.

## Test system

- Hardware architecture: Apple Silicon (`aarch64`).
- macOS: 26.6.2, build 25G83.
- Xcode: 26.6, build 17F113.
- macOS SDK: 26.5.
- Rust: 1.98.0 stable for `aarch64-apple-darwin`.
- Bun dependencies: installed from the lockfile with `bun install --frozen-lockfile`.

The required native engine assets were already present. The optimized build did not load a Homebrew library at run time.

## Checks

| Check                     | Result | Notes                                                                                                   |
| ------------------------- | ------ | ------------------------------------------------------------------------------------------------------- |
| Frontend tests            | Pass   | 394 passed, 0 failed.                                                                                   |
| Frontend lint             | Pass   | No lint errors.                                                                                         |
| Frontend format           | Pass   | Prettier check passed.                                                                                  |
| Translation validation    | Pass   | All 19 translated locales passed.                                                                       |
| Rebrand validation        | Pass   | No blocked product names found.                                                                         |
| Production frontend build | Pass   | 1.62 MiB raw and 596.5 KiB gzip.                                                                        |
| Rust format               | Pass   | `cargo fmt --all -- --check`.                                                                           |
| Rust tests                | Pass   | 467 library tests and the dictionary integration test passed.                                           |
| Rust clippy               | Pass   | `cargo clippy --all-targets --all-features -- -D warnings`.                                             |
| Native app and DMG build  | Pass   | `bun run tauri build --bundles app,dmg`.                                                                |
| DMG checksum              | Pass   | `hdiutil verify` accepted the image.                                                                    |
| Packaged app launch       | Pass   | The app stayed open for an eight-second smoke test with `--no-tray` and stopped cleanly with `SIGTERM`. |

The first Rust test run found generated binding drift. The non-macOS clamshell documentation was made equal to the macOS implementation, and the generated binding test then passed. Clippy also found small cross-platform warning defects. These were fixed without changing run-time behavior.

Cargo reports a future-compatibility warning for the third-party `block` 0.1.6 crate. It does not fail the current build. Track it as dependency maintenance; do not patch vendored code for this release.

## Package measurements

| Item                       |                                 Baseline |
| -------------------------- | ---------------------------------------: |
| Main frontend graph        |            638.6 KiB raw; 185.9 KiB gzip |
| Full frontend output       |             1.62 MiB raw; 596.5 KiB gzip |
| Packaged native executable |                         36,763,504 bytes |
| `.app` bundle              |                           36 MiB by `du` |
| DMG                        |         16,948,931 bytes; 16 MiB by `du` |
| Packaged app idle memory   | 105,840 KiB resident after eight seconds |

The smoke harness detected the process at once, so it did not produce a useful time-to-interactive value. A clean first-launch measurement is also pending because this Mac already has an AudioBud history database in Application Support. Do not delete that user data for a benchmark.

## Native package inspection

- The executable is a thin arm64 Mach-O file.
- Strong dynamic dependencies are macOS system frameworks and libraries.
- Foundation Models and Swift libraries use weak links.
- The executable has `/usr/lib/swift` as its Swift run-time search path.
- The app has no bundled `Frameworks` directory.
- Bundle resources contain the icon, settings, sounds, tray images, and third-party notices.
- The bundle identifier is `tech.amditis.audiobud`.
- The packaged version is 0.5.0.
- The packaged minimum system version is 10.15. The v0.6.0 work must set this to 11.0 to match the Swift bridge target.
- `NSMicrophoneUsageDescription` is present.
- The signed entitlements contain the direct-distribution audio-input and microphone permissions.

## Signature and notarization baseline

- The local bundle has an ad hoc signature and hardened run time.
- `codesign` accepts the internal ad hoc signature.
- Gatekeeper rejects the ad hoc build, as expected.
- The app has no stapled notarization ticket, as expected.
- The login Keychain contains a valid Developer ID Application identity with a private key.
- A selective PKCS#12 export contains one Developer ID certificate bag and one encrypted private-key bag.
- The exported certificate and its generated password are outside the repository with owner-only file permissions.
- A dedicated App Store Connect key now exists for AudioBud notarization.
- Issuer and key identifiers are stored outside the repository and in the protected GitHub environment.
- The `.p8` key is outside the repository with owner-only file permissions. Its content was not printed.

The selective export used the exact Developer ID certificate name. It did not export the unrelated iPhone distribution or Apple development identities.

## Baseline limits

- Time to interactive is not measured yet.
- A clean first-launch permission test is not complete.
- The certificate and notarization key still need a CI import and signed-build test.
- The local baseline is not a release candidate because it is not Developer ID signed or notarized.

## v0.6.0 pre-final local candidate evidence

Recorded on August 27, 2026, before the last model, mute, shortcut, workflow,
and documentation changes. These values describe an earlier signed candidate,
not the current tree.

| Item                       | v0.5.0 baseline                 | v0.6.0 local candidate          | Change                                      |
| -------------------------- | ------------------------------- | ------------------------------- | ------------------------------------------- |
| Main frontend graph        | 638.6 KiB raw; 185.9 KiB gzip   | 647.2 KiB raw; 188.7 KiB gzip   | +8.6 KiB raw; +2.8 KiB gzip                 |
| Full frontend output       | 1.62 MiB raw; 596.5 KiB gzip    | 1.63 MiB raw; 599.8 KiB gzip    | +0.01 MiB raw; +3.3 KiB gzip                |
| Packaged native executable | 36,763,504 bytes                | 35,051,360 bytes                | -1,712,144 bytes (-4.66%)                   |
| `.app` bundle              | 36 MiB by `du`                  | 35,296 KiB by `du -sk`          | Smaller; baseline precision prevents a rate |
| DMG                        | 16,948,931 bytes                | 16,043,256 bytes                | -905,675 bytes (-5.34%)                     |
| Packaged app idle memory   | 105,840 KiB after eight seconds | 101,200 KiB after eight seconds | -4,640 KiB (-4.38%)                         |

The small frontend increase comes from the settings, permission, shortcut, and
audio-lifecycle coordination added for correctness. Native packaging still
became smaller because unused native paths and dependencies were removed.

That pre-final local candidate passed these checks:

- 469 frontend tests and 2,077 assertions across 44 files.
- 481 Rust library tests and the dictionary integration test.
- Frontend lint, formatting, translation, rebrand, build, and size checks.
- Rust formatting and clippy with warnings denied.
- Workflow lint and release-contract tests.
- Developer ID signing, hardened run time, Apple notarization, and stapling for both the app and DMG.
- Gatekeeper assessment, `codesign`, `stapler`, and `hdiutil verify` for the final app and DMG.
- Thin arm64 executable, macOS 11.0 minimum, and no Homebrew or `/usr/local` runtime dependency.
- An eight-second launch smoke test followed by a clean controlled shutdown.

The pre-final local DMG is
`src-tauri/target/release/bundle/dmg/AudioBud_0.6.0_aarch64.dmg`. Its SHA-256
value was
`1e050d955fcf8b7766688ba5e08bde996f54e45077ec2d254831c013da004ba8`.
This hash is local evidence only. The public release must use the checksum from
the exact remote artifact that passes the draft-release tests.

## v0.6.0 current checkpoint evidence

The package measurements were recorded on August 27, 2026, from the signed
candidate before pull request review follow-up. Source validation was updated
on August 28 from an isolated worktree based on pull request commit `db89c23`
plus the current uncommitted fixes. Do not treat the package measurements as a
build of the current source checkpoint.

| Item                       | v0.5.0 baseline                 | v0.6.0 signed candidate        | Change                                      |
| -------------------------- | ------------------------------- | ------------------------------ | ------------------------------------------- |
| Main frontend graph        | 638.6 KiB raw; 185.9 KiB gzip   | 647.2 KiB raw; 188.7 KiB gzip  | +8.6 KiB raw; +2.8 KiB gzip                 |
| Full frontend output       | 1.62 MiB raw; 596.5 KiB gzip    | 1.63 MiB raw; 599.8 KiB gzip   | +0.01 MiB raw; +3.3 KiB gzip                |
| Packaged native executable | 36,763,504 bytes                | 35,051,360 bytes               | -1,712,144 bytes (-4.66%)                   |
| `.app` bundle              | 36 MiB by `du`                  | 35,296 KiB by `du -sk`         | Smaller; baseline precision prevents a rate |
| DMG                        | 16,948,931 bytes                | 16,044,070 bytes               | -904,861 bytes (-5.34%)                     |
| Packaged app idle memory   | 105,840 KiB after eight seconds | 92,640 KiB after eight seconds | -13,200 KiB (-12.47%)                       |

The current isolated source checkpoint passed these checks:

- 481 frontend tests and 2,180 assertions across 45 files.
- 503 Rust library tests and the dictionary integration test.
- The focused optional-shortcut cleanup regression test and all 34
  shortcut-related tests.
- The macOS release workflow contract and `actionlint`.
- The full lint, format, translation, rebrand, type, build, and Clippy checks.
- The size report, with a 647.3 KiB raw and 188.6 KiB gzip main initial
  payload.

The earlier signed candidate passed these package checks:

- Developer ID signing and hardened run time for the app and DMG.
- Separate accepted Apple notarization and stapled tickets for the app and DMG.
- Gatekeeper assessment, `codesign`, `stapler`, and `hdiutil verify` for the
  exact app and DMG.
- Thin arm64 executable, version 0.6.0, macOS 11.0 minimum, and no Homebrew or
  `/usr/local` run-time dependency.
- Audio-input and microphone signing entitlements.
- An eight-second launch smoke test followed by a controlled `SIGTERM` stop.

The earlier local DMG is
`src-tauri/target/release/bundle/dmg/AudioBud_0.6.0_aarch64.dmg`. Its SHA-256
value is
`43db27f9ab0ddeb54da64f5fc664eecd43d86f22952fd24c828ac856d1629a6f`.
This hash is local evidence only. The public checksum must come from the exact
tag-built draft asset after that asset passes the release tests.

## v0.6.0 remaining limits

- Time to interactive is not measured because the smoke harness observes only process state.
- Clean first-launch, permission-state, transcription, paste, display, sleep, and device-change tests are pending on a clean Apple Silicon Mac.
- The current isolated source checkpoint has not been signed, notarized, or
  packaged. The signed evidence above applies to the August 27 candidate.
- Pull request 309 is open at `db89c23`. Five remote checks passed. The real
  Windows engine check failed on a redundant closure that is fixed locally.
- An independent review of `db89c23` plus the Clippy fix found an optional
  shortcut regression and two macOS workflow proof gaps. Their fixes pass the
  full local validation suite. Fresh local and Claude Code reviews found no
  unresolved P1, P2, or P3 issue in the complete follow-up checkpoint.
- The protected signed candidate workflow has not run from reviewed `main`.
- The Windows installers, signed updater path, and Microsoft Store package URL are not ready for submission.
