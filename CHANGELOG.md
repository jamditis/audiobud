# Changelog

All notable changes to AudioBud are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

AudioBud is a detached fork of [Handy](https://github.com/cjpais/Handy) by CJ Pais
(MIT). This log records what AudioBud changed from its fork point; it does not
restate Handy's own history. AudioBud versions independently of Handy, starting at
`0.1.0`. Windows releases are code-signed beginning with `0.4.0`.

## 0.4.1 - 2026-07-22

A Windows clipboard and release-integrity patch. Temporary dictation writes no
longer flood Windows clipboard history, and the release pipeline now provides
stronger checks and verification material for signed downloads. Windows (x64)
remains the validated target; the macOS and Linux code is inherited and
untested.

### Added

- Release assets now include `SHA256SUMS.txt` and GitHub artifact attestations,
  giving users stable checksum and provenance verification paths (#184, #188).
- The website download button resolves the latest signed NSIS installer and the
  verification guide documents the expected publisher, checksum, and provenance
  checks (#138, #182).

### Changed

- CI now compiles and tests the real Windows transcription engine before release,
  blocks rebrand regressions, and rejects copied English locale values, including
  locale-only plural categories (#183, #186, #199).
- The misleading Playwright smoke job was removed, and the public roadmap and
  signed-release guidance now match the checks that run in production (#187).

### Fixed

- Clipboard-based dictation keeps both the temporary transcript and the restored
  clipboard item out of Windows clipboard history. Repeated dictations no longer
  stack copies of the user's retained text, HTML, image, or file list while the
  original clipboard content is still restored after paste.
- Portable update prompts now open AudioBud's own GitHub releases instead of the
  upstream Handy repository (#185).

### Known issues

- Automatic update checks remain disabled until AudioBud has its own signed
  updater feed. Downloads and manual installs work normally.
- SmartScreen can still show a reputation warning for a new signed release. The
  signature identifies the publisher and detects modified files; it does not
  guarantee an immediate reputation score.

## 0.4.0 - 2026-07-21

A distribution release. The Windows installers, application executable, and
NSIS uninstaller are signed and timestamped through Microsoft Artifact Signing.
The public site now has privacy and terms pages for the verified publisher
domain. Windows (x64) remains the validated target; the macOS and Linux code is
inherited and untested.

### Added

- Public privacy and terms pages, plus the Microsoft publisher-domain
  association used to verify `audiobud.amditis.tech` for the AudioBud Entra
  application (#128).
- Local correction capture can recognize an immediate undo-and-retype correction
  and turn it into a deterministic learned replacement. The extractor rejects
  edits that do not look like transcription corrections (#125, #134).

### Changed

- Windows release builds use passwordless GitHub OIDC authentication and
  Microsoft Artifact Signing. The release job signs the application during
  packaging, signs the finished NSIS and MSI installers, verifies signatures and
  timestamps, extracts both packages to verify the bundled application and NSIS
  uninstaller, and uploads assets only after every check passes (#129-#133).
- The app and public website received a focused visual and accessibility pass,
  including clearer navigation, controls, responsive layouts, and updated
  screenshots (#127).

### Fixed

- Spoken clock times keep their opening punctuation when a meridiem resolves an
  otherwise ambiguous phrase (#113).
- Learned-replacement extraction rejects broader edits that happen to contain a
  shorter correction, reducing false learning (#134).

### Known issues

- Automatic update checks remain disabled until AudioBud has its own signed
  updater feed. Downloads and manual installs work normally.
- SmartScreen can still show a reputation warning for a new signed release. The
  signature identifies the publisher and detects modified files; it does not
  guarantee an immediate reputation score.

## 0.3.4 - 2026-07-20

A dictation-quality release. Spoken numbers now come through as digits and
symbols instead of spelled out, you can switch the output mode from the tray
without opening settings, and raw transcript mode gained the groundwork to
interpret spoken punctuation. Windows (x64) only; the macOS and Linux code
remains inherited and untested.

### Added

- Spoken numbers are formatted as digits on the normal dictation path: "twenty
  five dollars" becomes "$25", "ten percent" becomes "10%", and "three point
  five" becomes "3.5". The formatter is offline and deterministic -- it runs for
  every engine with no API key, and it leaves raw output and LLM post-processing
  untouched. It errs toward leaving text alone: an ambiguous run like "twenty
  twenty" is not merged, a bare "one" stays a word, an ordinal ("July twenty
  first") is not half-converted, and a spoken-zero decimal ("one point oh")
  becomes "1.0". On by default, with a toggle in Advanced settings (#111).
- An "Output mode" submenu in the system tray switches between Formatted and Raw
  transcript without opening the settings window; the tray and the settings
  window stay in sync when either one changes it (#111).
- Raw transcript mode can now interpret spoken punctuation -- "question mark"
  becomes "?", "new line" and "new paragraph" break the text, and paired
  commands like "open paren" attach to the correct side with the right spacing --
  and it runs the number formatter as well, so raw mode becomes usable for
  dictation with no model in the loop (#66). It is off by default, because raw
  mode's promise is that it prints what you said, so an upgrade should not change
  existing output. The settings toggle to turn it on ships next (#116; UI in
  #115).

## 0.3.3 - 2026-07-17

A naming release. Three releases after the fork, the app still introduced itself
as Handy: the tray tooltip you hover said so, `--help` said so, and every request
to a post-processing provider said so in its headers. All three now say AudioBud.
Windows (x64) only; the macOS and Linux code remains inherited and untested.

### Fixed

- The system tray tooltip read "Handy v0.3.2". It now names the product, and a
  test pins it to the product name in `tauri.conf.json` rather than to a literal,
  so the tooltip cannot drift from the app's own name again (#106).
- `audiobud --help` announced itself as "handy" and described "Handy - Speech to
  Text" (#106).
- Requests to post-processing providers identified the app as Handy 1.0 in their
  `Referer`, `User-Agent`, and `X-Title` headers, and pointed at the upstream
  repository. Providers surface those headers in their dashboards, so AudioBud's
  traffic was credited to a project it forked from. The headers now name AudioBud,
  link to this repository, and report the real build version instead of a frozen
  1.0 (#106).

### Internal

- The frontend build pinned an exact Bun version. The previous floating version
  made an authenticated call to the GitHub API to resolve itself, so a rate-limited
  or degraded API could fail a build that had nothing wrong with it (#103).

## 0.3.2 - 2026-07-16

A translation release. AudioBud ships 19 languages besides English, and about a
fifth of the interface showed English in every one of them -- the settings pages,
the dictionary, and the overlay all fell back. Those languages are now complete,
and the app calls itself AudioBud in them rather than Handy. If you use AudioBud
in English, the visible change is a counter that no longer reads "1 learned
words". Windows (x64) only; the macOS and Linux code remains inherited and
untested.

### Fixed

- The 19 non-English languages now cover the whole interface. Roughly a fifth of
  the app fell back to English in all of them, because those strings were added
  after the translations were inherited and never caught up. All 2,033 missing
  strings are filled, and the check that compares each language against English
  now fails the build instead of only warning -- so a new English string can no
  longer ship without its translations (#88).
- AudioBud called itself Handy in all 19 non-English languages, in text it
  showed you about your own dictation. The name is now correct in each of them,
  including where the language attaches word endings to it, as Turkish does
  (#95).
- The learned-words counter read "1 learned words" in English, and would have
  shown English mid-sentence in Russian, Polish, Ukrainian, Czech, Arabic, and
  Hebrew once corrected. It now agrees with the number in every language (#96).
- The MSI package now installs the third-party license notices alongside the
  app, matching what the setup wizard already installed (#70).

### Known issues

- The non-English path fix from 0.3.1 covers Parakeet, the default engine.
  Whisper models can still fail to load from a folder whose path is not valid
  UTF-8; the fault is in a library AudioBud depends on and needs a fix further
  down (#80).
- A few counts still read awkwardly in some non-English languages -- "seen 3
  times" and the import warnings do not bend the word to the number in Russian,
  Ukrainian, or Arabic. Nothing falls back to English; the wording disagrees
  with the number (#100).

## 0.3.1 - 2026-07-16

A stability patch for the default setup. Four bugs in this release could lose
your work or stop dictation outright: AudioBud failed to start transcribing at
all if your Windows username contained a non-English character, changing a
history setting could delete recordings you had not saved, pasting a transcript
wiped whatever you had copied earlier, and a stuck engine could hang the window
with no way out. Windows (x64) only; the macOS and Linux code remains inherited
and untested.

### Fixed

- AudioBud now starts and transcribes normally when your Windows user folder
  contains non-English characters. The default engine (Parakeet) previously
  failed to load its voice-detection model on those accounts, which broke
  dictation on a fresh install with no obvious cause (#56).
- Changing a history setting no longer deletes recordings you have not saved.
  Cleanup now runs when a new entry is saved rather than the moment you adjust
  the limit or the retention window, and a history list that is open on screen
  stays in sync when older entries are trimmed (#55).
- Pasting a transcript no longer destroys what was already on your clipboard.
  AudioBud restores the previous contents after it pastes -- including copied
  files, images, and formatted text, not just plain text -- and always leaves
  the transcript itself reachable if a restore cannot complete (#57).
- A transcription engine that stops responding no longer hangs the app. The
  transcription now runs under a watchdog that gives up after a timeout, tells
  you it timed out, and returns you to a working window instead of a frozen one
  (#58).

### Added

- The setup installer now bundles DirectML.dll, matching what the MSI package
  already shipped. Installing with the .exe and the .msi now produces the same
  working set of runtime files (#44).
- Third-party license notices for the Windows runtime libraries AudioBud
  bundles, installed alongside the app by the setup installer (#69). The MSI
  package does not carry them yet (#70).

### Known issues

- The non-English path fix above covers Parakeet, the default engine. Whisper
  models can still fail to load from a folder whose path is not valid UTF-8;
  the fault is in a library AudioBud depends on and needs a fix further down
  (#80).
- About a fifth of the interface still shows English in the other 19 languages
  AudioBud ships. The translations cover the app as it was at the fork point;
  the settings, dictionary, and overlay text added since then falls back to
  English until the locales are brought up to date (#88).

## 0.3.0 - 2026-06-25

Adds opt-in, on-device personalization: when you turn it on, AudioBud learns the
words you use most from your own transcription history -- all on your machine --
and applies the ones you accept to later dictations. It ships off by default.
Windows (x64) only; the macOS and Linux code remains inherited and untested.

### Added

- An opt-in personalization dictionary that learns from your transcription
  history. When enabled, AudioBud surfaces frequently used words as suggestions
  you can accept or dismiss, and applies the ones you accept to future
  dictations. Everything runs and stays on your machine; the feature is disabled
  until you switch it on, and it lives on the Advanced settings tab (#16).
- View, export, and reset controls for your learned data that stay available even
  while personalization is turned off, so stored data is always reachable and
  removable without re-enabling the feature. Export writes a JSON file through a
  native save dialog; reset clears all stored personalization data (#53, #54).
- A published [roadmap page](https://jamditis.github.io/audiobud/roadmap.html)
  and a backfilled changelog, so planned work and shipped changes are visible from
  the site (#60).

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
