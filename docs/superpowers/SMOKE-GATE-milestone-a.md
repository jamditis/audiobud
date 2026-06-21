# Milestone A smoke gate (Windows, manual)

The automated suite (`cargo test --lib`, `bun test`, `check-rebrand`) proves the
units in isolation. This gate proves the assembled app does the milestone-A job
on real hardware: launches rebranded, ships the right fresh-install defaults, and
runs the full audio -> VAD -> Parakeet V3 -> text-injection pipeline with a real
mic. It must be run by a human at the keyboard on the target machine (Legion,
RTX 4080 Super, Windows 11).

## Prerequisites

- [ ] All milestone-A commits built into the dev binary (`dev-audiobud.bat`).
- [ ] User config backed up at `C:\Users\amdit\tools\audiobud-config-backup\`
      (this gate renames the app data dir; restore from the `.bak` rename or the backup).
- [ ] App data dir: `C:\Users\amdit\AppData\Roaming\tech.amditis.audiobud\`.

## Test 1: fresh-install defaults (integration)

Proves the running app persists the defaults from `get_default_settings()` on a
clean profile -- not just that the unit test passes.

1. Quit AudioBud.
2. Rename `...\Roaming\tech.amditis.audiobud` to `...\tech.amditis.audiobud.bak`
   (rename, do not delete -- preserves the downloaded models for restore).
3. Launch the dev app. It creates a fresh profile.
4. Inspect the freshly written `settings_store.json`.

Expected:
- [ ] A new `tech.amditis.audiobud\` dir is created (confirms the rebranded identifier is live).
- [ ] Window title bar reads `AudioBud`.
- [ ] Tray icon present.
- [ ] `bindings.transcribe.default_binding` == `ctrl+alt+space` and `current_binding` == `ctrl+alt+space`.
- [ ] `selected_model` == `parakeet-tdt-0.6b-v3`.

## Test 2: end-to-end dictation on the shipping build

1. Quit AudioBud. Delete the fresh profile dir; rename `...audiobud.bak` back to `...audiobud` (restores the user's config and downloaded models).
2. Relaunch the dev app.
3. Open Notepad, focus the text area.
4. Hold the transcribe hotkey, say a sentence, release.

Expected:
- [ ] Recording overlay appears while held.
- [ ] On release, the transcribed text is injected into Notepad.
- [ ] Text matches the spoken sentence (allowing for proper-noun/number quirks; see bench/RESULTS.md).

## Test 3: cancel aborts cleanly

1. Hold the transcribe hotkey, start speaking.
2. Press the cancel binding (default `escape`) before releasing.

Expected:
- [ ] Recording aborts.
- [ ] No text is injected into the focused field.

## Test 4: no Win+H collision

Milestone A does not hook Win+H (that is milestone B). AudioBud uses
Ctrl+Alt+Space, a distinct chord.

Expected:
- [ ] Pressing Win+H still invokes Windows' own voice typing, not AudioBud.
- [ ] Pressing Win alone does not trigger AudioBud.

## Test 5: CSP and asset scope do not break the UI (security pass)

The security pass set a strict production CSP plus a looser `devCsp`, and narrowed
the asset-protocol scope to the recordings dir (with a runtime allow for the
portable-aware path). Confirm the renderer still works under it.

1. Launch the dev app (uses `devCsp`).
2. Open DevTools (`Ctrl+Shift+D` debug mode, then the console) and watch for
   `Content-Security-Policy` violation errors.
3. Go to Settings -> History and play back a saved recording.

Expected:
- [ ] The Bungee wordmark and Fredoka body text render in their real fonts, not a
      system fallback (confirms `style-src`/`font-src` allow the Google Fonts hosts).
- [ ] Audio playback of a saved recording works (confirms `media-src` + the runtime
      asset-scope allow cover the recordings dir under the narrowed `$APPDATA/recordings/**` scope).
- [ ] No CSP violation errors in the console during normal use.
- [ ] (dev only) Editing a frontend file hot-reloads (confirms `devCsp` allows the
      Vite HMR websocket). The dev `connect-src` allows the `ws:`/`wss:` schemes, so
      HMR works on default localhost and on a `TAURI_DEV_HOST` (`ws://<host>:1421`)
      setup alike. If HMR is ever blocked, confirm the dev policy still lists `ws:`
      and re-test.

## Test 6: external-script paste confirmation gate (security pass)

Arming the external-script paste method (which runs an external program on every
paste) now requires a native OS confirmation that a compromised webview cannot satisfy.

1. Settings -> paste method -> select "external script".

Expected:
- [ ] A native OS dialog appears asking to confirm enabling external-script paste.
- [ ] Cancel/No leaves the paste method unchanged (does NOT switch to external script)
      and shows a "couldn't save" toast (the optimistic selection rolls back).
- [ ] OK/Yes enables it.

2. With external-script enabled, set or change the script path field.

Expected:
- [ ] Setting a non-empty path pops the confirm dialog again, naming the path.
- [ ] Cancel leaves the path unchanged; OK persists it.

## Notes

- Model download path: verified earlier this session -- the user downloaded
  Parakeet V3, Canary 180M flash, and Whisper turbo via the in-app downloader
  (upstream Handy downloader, unchanged in milestone A).
- Elevated-target limitation (known, upstream): injection into a window running
  as administrator fails unless AudioBud also runs elevated. Out of scope for milestone A.

## Result

- Date run:
- Outcome (pass / fail + notes):
