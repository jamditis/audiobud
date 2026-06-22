# Changelog

All notable changes to AudioBud are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

AudioBud is a detached fork of [Handy](https://github.com/cjpais/Handy) by CJ Pais
(MIT). This log records what AudioBud changed from its fork point; it does not
restate Handy's own history. AudioBud versions independently of Handy, starting at
`0.1.0`. Releases are not yet code-signed; signing arrives in milestone B.

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
