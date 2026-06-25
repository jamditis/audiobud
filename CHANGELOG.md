# Changelog

All notable changes to AudioBud are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

AudioBud is a detached fork of [Handy](https://github.com/cjpais/Handy) by CJ Pais
(MIT). This log records what AudioBud changed from its fork point; it does not
restate Handy's own history. AudioBud versions independently of Handy, starting at
`0.1.0`. Releases are not yet code-signed; signing arrives in milestone B.

## [Unreleased] - 0.3.0

Work in progress. See the [roadmap](https://jamditis.github.io/audiobud/roadmap.html).

## 0.2.0 - 2026-06-24

The first AudioBud release with installers on the
[releases page](https://github.com/jamditis/audiobud/releases/latest): a themed
Windows setup wizard and a portable build, plus overlay placement, tray
quick-toggles, and a dictionary that stops correcting words that were already
right. Windows (x64) only; the macOS and Linux code remains inherited and
untested.

### Added

- A 3x3 overlay placement grid: pin the recording bug to any corner, edge, or
  center of the active monitor, with a reset to the centered default. Placement
  is clamped on-screen so a saved position can never strand the overlay off a
  smaller or disconnected display (#9).
- Tray quick-toggles for the most-used settings -- push-to-talk, mute, trailing
  space, auto-submit, and overlay show/hide -- so they can be flipped from the
  tray without opening the settings window. The tray and the settings window
  stay in sync when either one changes a setting (#12).
- Raw transcript mode: a global toggle, a per-dictation `transcribe_raw`
  shortcut, and a `--toggle-raw` CLI flag. Raw output drops sentence casing and
  clause punctuation while preserving acronyms, versions, paths, emails, and the
  standalone "I". The active mode is saved per history entry, so retrying an
  entry reproduces its original output (#19).
- A RAW badge on the recording overlay so the active output mode is visible at a
  glance while recording (#24).
- A guidance panel on the dictionary settings explaining which tool fixes which
  problem -- the phonetic bias list versus deterministic word replacements --
  with collapsible worked recipes for a hard name, a domain or email, and a
  handle (#10).
- A portable install mode, restored with AudioBud's own NSIS template: a
  Normal-versus-Portable choice in the installer and a self-contained `Data/`
  directory beside the executable (#3).
- A frog and pond themed Windows setup wizard -- header banner, welcome and
  finish sidebar art, and the frog installer icon -- matching the in-app
  identity (#13).
- A Windows release pipeline (NSIS and MSI installers) in continuous
  integration, inert until a `v*` tag is pushed or it is dispatched by hand.

### Fixed

- The custom dictionary no longer corrupts words that were already correct. The
  old matcher accepted any phonetic hit at up to ~60% character difference, so
  "cloud" became "Claude" and "region" became "Legion"; a multi-signal gate
  (length, first-character anchor, edit-distance floor, common-word veto, and
  two-of-N agreement) now blocks wrong corrections, with a deterministic
  word-replacement map for the corrections fuzzy matching cannot make safely
  (#19).
- The app no longer fails to launch on a clean Windows machine. The installer
  now bundles the VC++ runtime (MSVCP140 / VCRUNTIME140) and the Vulkan loader
  (`vulkan-1.dll`) -- neither is present on a fresh install -- for both the NSIS
  and MSI bundlers (#36).
- A startup panic when the updater configuration was absent: the updater plugin
  is now registered only when a release feed is configured, matching the
  frontend gate (#32).
- The Word replacements "Add" button no longer overflows its container on the
  Advanced tab (#47).
- Initialization errors from the keyboard and shortcut setup are surfaced and
  logged instead of being silently swallowed.

### Changed

- Inherited cjpais/Handy references in the contributor docs, the issue and PR
  templates, and the updater configuration were repointed to AudioBud, and the
  public site and screenshots were refreshed for the release (#5, #29).

## 0.1.0 (milestone A) - 2026-06-21

AudioBud's first working local prototype, forked from Handy 0.8.3. Validated on
Windows 11 (RTX 4080 Super); the macOS and Linux code is inherited from Handy and
has not been tested here. A Windows installer is on the
[releases page](https://github.com/jamditis/audiobud/releases/latest); you can also
build from source.

### Added

- Frog and swamp visual identity: a full-bleed wetland background (pond-depth
  gradient, canopy light, drifting mist, night fireflies, lily pads at the
  waterline), an animated red-eyed tree frog whose vocal sac scales with the live
  microphone level, and a heavy "Bungee" wordmark over "Fredoka" body text.
- A "pond" save toast and a ribbit sound on the frog, with a synthesized fallback
  croak when the bundled sound cannot play.
- A Konami-code easter egg (frog rain).
- A live input-level meter and an output-device test in audio settings, backed by
  a new microphone-monitor command.
- Custom-words `.txt` import with a documented format and a tested parser.
- An engine benchmark with a word-error-rate scorer (`scripts/wer.ts`) and
  recorded results (`bench/RESULTS.md`).
- A README, a LICENSE, and an in-app acknowledgment crediting Handy and CJ Pais
  under MIT.
- Continuous-integration, lint, and secret-scanning workflows owned by AudioBud,
  replacing the inherited cjpais workflows.
- Accessibility work: native buttons for navigation and the overlay cancel
  control, focus-visible rings, and a live region on the recording overlay.

### Changed

- The default transcribe hotkey on Windows is `Ctrl+Alt+Space`. Handy shipped a
  different default.
- The default engine on Windows is `parakeet-tdt-0.6b-v3`, chosen from a local
  benchmark as the smallest model that transcribes reliably on this build's
  DirectML path. Non-Windows platforms keep upstream's empty default, which opens
  the model picker on first run.
- The application identity is rebranded to AudioBud: bundle identifier
  `tech.amditis.audiobud`, product name, crate name, and window title.
- UI strings were swept to sentence case and de-jargoned.
- The in-app source-code link points to the AudioBud repository, and the sponsor
  control reads "Feed the frog / Buy me a fly".

### Security

- A path-traversal guard on `get_audio_file_path`, so the asset handler cannot be
  steered to read outside the recordings directory.
- A native confirmation gate before the external-script paste method can run an
  external program -- a prompt a compromised webview cannot satisfy on its own.
- A strict production Content-Security-Policy and an asset-protocol scope narrowed
  to the recordings directory.

### Disabled

- Automatic update checks are gated off. The inherited updater still points at
  Handy's release feed; checks return once that feed is repointed to AudioBud and
  its builds are signed, in milestone B.

### Known limitations

- Only Windows (x64) is validated. The macOS and Linux code is inherited from
  Handy and may work, but has not been tested.
- The Windows installer is not code-signed yet, so SmartScreen warns on first
  launch (choose **More info -> Run anyway**). Signing arrives in milestone B.
- Injection into a window running as administrator fails unless AudioBud also runs
  elevated. This is inherited behavior, out of scope for milestone A.
