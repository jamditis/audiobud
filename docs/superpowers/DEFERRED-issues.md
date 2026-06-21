# Deferred issues (AudioBud)

Tracking file for work found but deliberately deferred. There is no `jamditis/audiobud`
GitHub repo yet, so these live here until the repo exists, at which point each becomes a
GitHub issue. Keep entries action-ready: file, symptom, fix.

Status legend: `[ ]` open, `[x]` resolved (with the resolving commit/date noted inline).

---

## Security review findings (full-codebase audit, 2026-06-21)

Six-agent security review of the whole tree (our diff + inherited Handy code) before going public.
Our diff introduced no findings; all of the below are inherited from cjpais/Handy 0.8.3 (already public
upstream, so publishing our fork adds no net-new exposure). The three HIGH items were fixed in the
milestone-A security pass (TDD, commits noted inline); the sub-threshold ones are tracked.

### Resolved (milestone-A security pass)

- [x] **HIGH - arbitrary code execution via external-script paste setting.** Fixed 2026-06-21 (`8a8f3dd`;
      UX follow-ups `60feb0b`, `d7c8c33`).
      `src-tauri/src/shortcut/mod.rs:683-700` (`change_paste_method_setting`) + `:738-746`
      (`change_external_script_path_setting`) let the webview silently arm
      `Command::new(script_path).arg(text)` (`src-tauri/src/clipboard.rs:507`). Pointing the path at an
      interpreter (e.g. `powershell.exe`) turns the transcript into executed code; a renderer compromise
      becomes native RCE, and the setting persists. Fix (decided 2026-06-21): native OS confirmation
      dialog on arm - persist the external-script method/path only after the user confirms in a backend
      dialog the webview cannot satisfy on its own.

- [x] **HIGH - path traversal -> arbitrary file read via `get_audio_file_path`.** Fixed 2026-06-21 (`47ce403`).
      `src-tauri/src/managers/history.rs:584-586` joins a webview-supplied `file_name` with no validation;
      `src-tauri/src/commands/history.rs:36-47` exposes it; the wide `assetProtocol` scope (below) then
      serves any path on disk via `convertFileSrc`. A `..\..\` or absolute `file_name` reads arbitrary
      files, including `settings_store.json` (stored LLM API keys). Fix: extract a pure
      `is_safe_recording_filename` (reject separators, `..`, absolute/drive prefixes, empty), return
      `Result<PathBuf>`, validate before join. **Failing test first.**

- [x] **HIGH (defense-in-depth) - CSP disabled + wildcard asset scope.** Fixed 2026-06-21 (`32b64f6`;
      dev-HMR follow-up `681f76b`). Asset scope narrowed to `$APPDATA/recordings/**` plus a runtime
      `allow_directory` for the portable-aware path; strict prod CSP + looser `devCsp` for Vite HMR.
      `src-tauri/tauri.conf.json:16` `csp: null` and `:17-23` `assetProtocol.scope.allow: ["**"]`. No CSP
      means an injected-content/XSS foothold has nothing blocking inline/`eval`/external loads - the
      multiplier that makes the two HIGH exploits above realistic. Fix: set a restrictive CSP (allow the
      Bungee/Fredoka font hosts `fonts.googleapis.com`/`fonts.gstatic.com`, `asset:`/`ipc:` sources,
      `style-src 'unsafe-inline'` for Tailwind/Vite) and narrow the asset scope to the app-data dir.
      Verify the renderer still loads at the smoke gate (fonts, recording playback).

### Tracked (not blocking publish)

- [ ] **MEDIUM - custom post-process provider base_url SSRF / API-key exfil.**
      `src-tauri/src/shortcut/mod.rs:827-854` lets the webview set the `custom` provider `base_url`;
      `src-tauri/src/llm_client.rs:63-96,147-194` then sends the stored API key there. Under a webview
      compromise the key/transcript can be exfiltrated to an attacker host. Built-in provider hosts are
      correctly locked (non-`custom` edits rejected). Confidence 7 (reviewers split: intended for a
      user-chosen custom host vs. exploitable under XSS). Fix later: confirm/validate the destination
      before attaching a stored key. Largely mitigated once the CSP lands.

- [ ] **Provenance - updater feed + signing still point at cjpais/Handy.**
      `src-tauri/tauri.conf.json:61` (`signCommand` ... `Handy`, `cjpais-dev`) and `:69-72` (updater
      `pubkey` + `endpoints` = `github.com/cjpais/Handy`). A detached fork would pull/trust upstream's
      signed releases, not AudioBud's. Not an egress/crypto vuln (TLS + minisign chain intact), but wrong
      provenance. Fix in milestone B (release pipeline) - cross-ref the Milestone B section.

- [ ] **Hardening - self-host the Bungee/Fredoka fonts.** `index.html:7-9` loads the wordmark/body
      fonts from `fonts.googleapis.com`/`fonts.gstatic.com`, which forced those hosts into the CSP
      `style-src`/`font-src`. For a local-first dictation app this is a per-launch request to Google (a
      privacy/telemetry leak and an offline failure: no network -> fallback fonts). Vendor the woff2 files
      under `src/assets/fonts/`, add `@font-face` rules, drop the `<link>`s, and tighten the CSP to
      `font-src 'self'` / `style-src 'self' 'unsafe-inline'`. Two-way door.

- [ ] **Lint - inherited unused import warning.** `src-tauri/src/helpers/clamshell.rs:66` has an unused
      `use super::*;` in its test module (warns on every `cargo test`/`clippy`). Trivial; remove or `#[allow]`.
      Inherited from upstream; surfaces once CI runs clippy with warnings-as-errors.

- [ ] **CI - `bun test` (bare) collides with the Playwright specs.** `tests/app.spec.ts` calls
      `test.describe`, which Bun's built-in runner picks up and errors on; the unit tests live in `scripts/`
      and `src/`. There is no `test` script in `package.json`. Until CI lands, run unit tests with
      `bun test scripts src`. Fix in #30 (CI): add a `test` script scoped to `scripts src` and keep
      Playwright on `test:playwright`, so a bare `bun test` does not fail.

### Codex local review (pre-PR, 2026-06-21)

Multi-pass local review of the milestone-A security pass per the mandatory pre-PR flow (Codex 5.4 low,
then iterated 5.5 high on the post-fix code). Core security logic passed; reviewers found UX/config
follow-ups and one regression introduced by an earlier fix, all addressed before publish. The final
5.5 high pass (base `80ac0db`, through `15d4564`) returned clean - no actionable findings. Converged.

- [x] **5.4 low - dev CSP did not cover Vite HMR over a non-localhost host/port.** `devCsp` allowed only
      `ws://localhost:1420`, but `vite.config.ts` uses `ws://<host>:1421` under `TAURI_DEV_HOST`. Fixed by
      allowing the `ws:`/`wss:` schemes in `devCsp` (dev-only) - `681f76b`.
- [x] **5.5 high - declined confirmation did not roll back the optimistic UI.** `updateSetting` ignored
      the tauri-specta `{ status: "error" }` result and showed "saved". Fixed (root cause, all settings) -
      `60feb0b`.
- [x] **5.5 high - external-script confirm dialog fired per keystroke.** Path input committed on every
      `onChange`. Fixed by committing on blur/Enter - `d7c8c33`.
- [x] **5.5 high (P3) - long external-script `Err` return left unformatted.** `cargo fmt` brought the
      multi-line return into shape; the store test was prettier-normalized in the same pass - `5ab8ca9`.
- [x] **5.5 high (P2) - invalid/tampered history rows became undeletable (regression from #22).** The
      path-traversal guard made `delete_entry` use `get_audio_file_path(...)?`, so a row whose `file_name`
      failed validation returned before the DB `DELETE`. Fixed by extracting a testable
      `delete_entry_with_conn` that skips the audio-file unlink for an unsafe name but always removes the
      row; security property (no path escape) preserved. TDD red->green, +2 unit tests - `15d4564`.

---

## Inherited upstream bugs (cjpais/Handy audit, 2026-06-21)

We are a detached fork of Handy 0.8.3. Of Handy's ~80 open bugs, the ones below are present in
**our copied code** (verified by file:line in our tree) or in pinned dependencies. All cited
upstream issues are OPEN/unmerged, so rebasing won't fix them — local patches are the only path.
Most Linux/macOS/AppImage/Wayland/Pipewire issues were reviewed and excluded as out of scope for
the Windows-first target.

### Fix-now (code defect in our tree, high value)

- [ ] **Latent dead fall-through in `change_binding` cancel branch (inherited).**
      `src-tauri/src/shortcut/mod.rs` (the `id == "cancel"` block, ~lines 150-161) re-fetches the cancel
      binding via `settings.bindings.get(&id).cloned()` inside an `if let` after `binding_to_modify` was
      already resolved above; if that lookup ever missed it would silently fall through to the
      register/unregister path that the cancel case must never take. Harmless today (the `cancel` key is
      always present), so deferred. Fix: use the already-resolved `binding_to_modify` and `return`
      unconditionally; drop the re-fetch and clone. **Write a test for the missing-key case first** since
      this is a settings-write path. Surfaced by the milestone-A /simplify pass.

- [ ] **#1262 — History Limit / Auto-Delete silently destroys recordings (data loss).**
      `src-tauri/src/commands/history.rs:121` and `:150` call `cleanup_old_entries()` synchronously
      the moment a user lowers the limit or changes retention → `history.rs:330` →
      `delete_entries_and_files` → `fs::remove_file` deletes unsaved recordings + WAVs with no warning.
      `cleanup_by_count` (`history.rs:380-407`) keeps only `saved=0` rows DESC and nukes the tail.
      Upstream fix PR #1311 is open/unmerged. Fix: drop the two immediate cleanup calls (let cleanup
      run lazily via `save_entry()` on next recording); add a confirm/toast. **Write a failing test first.**

- [ ] **#574 — Parakeet fails to load on non-ASCII Windows profile paths (crash on our DEFAULT engine).**
      `src-tauri/src/managers/audio.rs:121` (`vad_path: &str`) + `:282` (`vad_path.to_str().unwrap()`)
      lose non-UTF-8/Unicode path components; any user whose Windows profile has Cyrillic/CJK/accented
      characters hits model-load failure (also surfaces as `model.rs:1426` "model directory not found").
      We default to `parakeet-tdt-0.6b-v3`, so this is a default-path crash. Upstream fix PR #1187 is
      open/unmerged. Fix: take `&Path` instead of `&str`; audit `transcribe-rs` model-path handoffs in
      `transcription.rs:304-378`. **Failing test first** (simulate a Unicode path).

- [ ] **#1409 — post-processing runs on empty transcription.** `src-tauri/src/actions.rs:363-377`
      (`process_transcription_output`) calls `post_process_transcription` whenever the flag is set,
      with no `final_text.trim().is_empty()` guard (the empty-text guard at `:602` is _after_ the LLM
      call). Fix: early-return before the post-process branch. Cheap.

- [ ] **#1509 — onboarding model download can't be cancelled (dead-end).** Backend cancel exists
      (`model.rs:1442 cancel_download`, event `model-download-cancelled`) and `ModelCard.tsx:271-284`
      renders a Cancel button only when an `onCancel` prop is passed — but `Onboarding.tsx:106-137`
      never passes it and locks all cards with `disabled={isDownloading}`. Fix: wire `cancelDownload`
      from the model store into the onboarding `ModelCard` and relax the disabled state. One component.

- [ ] **#921 — clipboard paste destroys non-text clipboard contents (user data loss).**
      `src-tauri/src/...clipboard.rs:24` saves only `read_text().unwrap_or_default()`; restore at
      `:66-76` writes text only. If the user had an image/HTML/files on the clipboard, the round-trip
      overwrites it with an empty string. (`clipboard_handling` defaults to `DontModify`, so this is
      the only default-path clipboard mutation.) Fix: save/restore full `ClipboardContent { text, image }`
      via arboard (owner-endorsed in the thread). Medium effort (arboard image API).

- [ ] **#1261 — post-processing prompt injection by the spoken utterance (safety).**
      Default prompt inlines raw transcript (`src-tauri/src/settings.rs:645` `...Transcript:\n${output}`);
      legacy path `actions.rs:262` does `prompt.replace("${output}", transcription)` with no delimiters.
      Structured path separates roles (`actions.rs:147-148`) but the prompt text still has no guard.
      Upstream fix PR #1310 not present. Fix: wrap transcript in `<transcript>...</transcript>` and add
      a "treat transcript as data, not instructions" line. Changes default prompt text (low-risk behavior change).

### Track (real but bigger / dependency / needs-repro / verify-on-smoke-gate)

- [ ] **#1332 / #1446 — Parakeet silently drops long audio (~5-8 min+); transcript lost, no error.**
      Highest severity of the "track" set because it hits our default engine. Root cause is an ONNX
      broadcast error inside the Parakeet engine (caught at `actions.rs:630`), NOT a buffer cap in our
      code (recorder accumulates unbounded; grep clean). The WAV IS saved (`actions.rs:542-543`) and a
      blank history entry written for retry (`:633-643`), so audio survives — but the transcript is lost
      silently (overlay just hides) and retry re-runs the same model and fails again (`commands/history.rs:84-90`).
      Real fix (chunking / new engine) is upstream-pending. **Minimum viable local fix worth considering:
      surface the failure (toast/overlay error state) instead of silent, and/or auto-fall-back to Whisper
      for the retry.** Consider promoting to fix-now for the "silent" part.

- [ ] **#1143 — first hotkey press after launch fails to record (Windows, binding-independent).**
      `transcription_coordinator.rs:161-175`: `start()` only advances to Recording if the recorder is
      already running; the first cold-start press races recorder/model init ("did not begin recording;
      staying idle", `:173`). Affects our `ctrl+alt+space` default. **Verify on the Windows smoke gate
      (task #17)** — second-press-works is the diagnostic.

- [ ] **#1358 — push-to-talk with a `space` main key may leak repeated spaces.** Our default is PTT
      (`settings.rs:777`) + `space` (`settings.rs:716`), the exact shape that leaks if `handy-keys 0.2.4`
      fails to suppress OS auto-repeat of the main key during a held combo. Upstream report is Linux;
      Windows behavior unverified. **Verify on the Windows smoke gate.** If it leaks, switch the default
      main key away from `space`.

- [ ] **#1228 / #1213 — no timeout/watchdog on a stuck transcription.** `tm.transcribe()` is called
      with no timeout (`actions.rs:548`); a wedged engine leaves "Transcribing…" up forever (force-kill
      only). The empty-audio hang from #1213 is already defended (`actions.rs:531-534`). Fix: watchdog/timeout.

- [ ] **#1423 — wrong tray icon on Windows dark/custom mode (interacts with our icon rebrand).**
      `tray.rs:29-45 get_current_theme` picks the icon from `main_window.theme()` (app mode), but the
      Windows taskbar follows the _system_ mode, which can differ ("custom") → our new dark-variant icon
      can render black-on-dark. `tray.rs:222 set_icon_as_template(true)` is a macOS no-op on Windows.
      Fix: read the Windows taskbar theme (`SystemUsesLightTheme` registry value) for icon choice.

- [ ] **#1206 / #1528 — Canary auto-translates to English even with translate=false (dependency).**
      Our call site is correct (`transcription.rs:599-613` passes `language: None` + `translate: setting`).
      Bug is in pinned dep `transcribe-rs 0.3.8` `src/onnx/canary/mod.rs:279-285` (`unwrap_or("en")` +
      forced `target_language`). Triggered by our defaults (`selected_language="auto"`, `translate=false`)
      but only when a user selects a Canary model (default is Parakeet). #1528 ships a patch (pass language
      through; `target_language = translate ? Some("en") : None`; `vocab.rs build_prompt` take `Option<&str>`).
      Not merged in any released transcribe-rs. Fix path: `[patch.crates-io]` override or vendored fork —
      only worth it if Canary is promoted.

- [ ] **#502 — wrong-paste race (pastes old clipboard instead of transcript).** Timed
      write→`sleep(paste_delay_ms=60)`→Ctrl+V (`clipboard.rs:43-62`) with no verify; under load the app
      can paste stale contents, and success is logged purely on enigo not erroring (`actions.rs:611`).
      Load-dependent. Fix: verify/poll clipboard before sending the keystroke.

- [ ] **#828 — subsequent recordings capture nothing (Bluetooth/cpal, device-after-sleep).** Mic
      lifecycle rebuilds the cpal stream each on-demand cycle (`audio.rs:289-363`); after a device
      vanishes it silently falls back to the default device. Lower priority on the desktop target.

- [ ] **#537 / #554 — SIGILL on CPUs without FMA3/AVX2.** The FMA guard (`transcription.rs:789`)
      only protects GPU _enumeration_, not the whisper-vulkan inference or ONNX/Parakeet load. Crash
      originates in bundled ggml/ONNX native libs (our Rust is built baseline x86-64; no `target-cpu`
      pin). Low priority for the RTX 4080 target. Fix: document a minimum-CPU requirement (AVX2 + FMA3).

- [ ] **#261 / #132 / #436 — Whisper foreign-exception aborts.** A C++ exception in whisper.cpp init
      aborts the process; Rust `catch_unwind` (`transcription.rs:526`) can't catch it and `WhisperEngine::load`
      (`:304`) isn't wrapped anyway. Unfixable in our Rust (whisper-rs binding limit). Mitigated by our
      Parakeet default. Keep Parakeet default; surface a clear error if a user switches to Whisper.

- [ ] **#1265 — Breeze (code-switching) model listed under English.** `model.rs:236-257` gives
      `breeze-asr` `supported_languages: whisper_languages` (incl. "en"). Metadata judgment, not a UI bug. Low.

- [ ] **#1418 / #1005 / #1063 / #508 — platform/by-design/dependency, none block.** #1418 = WebView2
      idle overhead from the intentional hide-not-close overlay (`overlay.rs:373-385`) + a `mic-level`
      listener kept mounted (`RecordingOverlay.tsx:38-49`); #1005 = deliberate close-to-tray
      (`lib.rs:578-580`); #1063 = cpal crackle (needs a cpal bump); #508 = pre-0.8.3 overlay desync,
      likely resolved by the `transcription_coordinator` rewrite.

### Milestone B (installer / packaging)

- [ ] **#1527 / #99 / #290 — installer ships neither the VC++ runtime nor the Vulkan loader.**
      `src-tauri/nsis/installer.nsi` only handles WebView2 (no VC++ redist check/bundle, no `vulkan-1.dll`).
      This is the whole "crashes with no error, fixed by installing a DLL" class. Add (a) a VC++
      Redistributable presence-check/bundle and (b) ship `vulkan-1.dll` (LunarG runtime) beside the exe.
      Add a Windows troubleshooting section to the README. (Milestone B already lists VC++ redist installer logic.)

- [ ] **#1489 — 0.8.3 startup crash regression (Win10/11), no logging.** Reporters confirm 0.8.2
      works and 0.8.3 crashes on the same machine (MSVCP140.dll, 0xc0000005) with VC++ already installed.
      Our tree == 0.8.3, so we inherit it. Root cause not pinned; needs a Win10 repro. Overlaps #1527.

### Incidental rebrand gaps (spotted during the audit)

- [ ] **`src-tauri/src/tray.rs:90-92` hardcodes `"Handy v{...}"` in the tray tooltip/version label.**
      Real rebrand miss (check-rebrand doesn't scan tray.rs). Rename to AudioBud.

---

## Accessibility (from the WCAG 2.1 AA audit, 2026-06-21)

The audit's critical/quick wins were fixed in-session (overlay cancel button, sidebar
nav buttons, Button focus-visible ring, InputLevelMeter invalid `role="meter"`, SoundPicker
label, overlay status live region + reduced-motion gate). The items below are the larger or
lower-severity findings held back to avoid regressions in shared widgets mid-flow.

### Important

- [ ] **`src/components/ui/Dropdown.tsx` — no listbox/combobox semantics or keyboard model.**
      Trigger lacks `aria-haspopup="listbox"`/`aria-expanded`; panel lacks `role="listbox"`,
      items lack `role="option"`/`aria-selected`; no ArrowUp/Down, no Escape-to-close, no focus
      moved into/restored from the panel (only closes on outside `mousedown`). Used by SoundPicker
      and others. Fix: full combobox pattern + `focus-visible:ring-2` on the trigger.

- [ ] **`src/components/model-selector/ModelDropdown.tsx` — same gap on the model picker.**
      `role="button"` items in a non-listbox container, `focus:outline-none` with no replacement,
      no Escape/arrow handling. Fix: `role="listbox"`/`role="option"`+`aria-selected`, arrow/Escape
      handlers, `focus-visible:ring-2`.

- [ ] **Live regions for model download/status text.** The overlay status now has
      `role="status" aria-live="polite"`, but model download/status text
      (`ModelSelector.tsx` / `DownloadProgressDisplay.tsx`) still updates silently. Fix: add a
      polite live region mirroring download/status text. (Toast errors via sonner are already covered.)

- [ ] **`src/components/ui/SettingContainer.tsx` (info tooltip, both occurrences) — interactive
      `role="button"` placed directly on an `<svg>`, with a hardcoded English `aria-label`.**
      A focusable `<svg>` is unreliable across browsers/AT, and the keydown lives on the SVG while
      the mouse handlers live on the wrapping `<div>`. Fix: move `role="button"`, `tabIndex`,
      `aria-label={t("common.moreInfo")}` (key already added), and handlers onto a real `<button>`
      wrapping an `aria-hidden` SVG — mirror the correct `CustomWords.tsx` InfoTag pattern.

- [ ] **`src/components/ui/Tooltip.tsx` — portaled content has no `role="tooltip"` and no
      `id`/`aria-describedby` link to its trigger.** SR users focusing a trigger never hear the
      tooltip text. Fix: `role="tooltip" id={id}` on content, `aria-describedby={id}` on triggers.

- [ ] **`src/components/ui/Input.tsx` and `src/components/ui/Dropdown.tsx` triggers —
      `focus:outline-none` replaced only by a background tint.** A subtle bg change can fall below
      the 3:1 non-text contrast requirement (WCAG 2.4.11). Fix: add `focus-visible:ring-2` to both.

- [ ] **Color contrast on reduced-opacity secondary text.** Placeholder text
      (`Dropdown.tsx:91`, `Select.tsx:118` at `mid-gray 65%`) and small `text-text/60` footer/
      version/timestamp text (`Footer.tsx:26`, `AudioPlayer.tsx:248,267`) land near or below the
      4.5:1 AA threshold on `--color-background` (#161f17). Fix: raise placeholders to full-opacity
      `text-mid-gray` (or lighter) and bump small `text-text/60` to `/75`.

- [ ] **`src/components/update-checker/UpdateChecker.tsx:187-215` — portable-update modal is a
      bare `fixed inset-0` overlay with no dialog semantics.** No `role="dialog"`/`aria-modal`, no
      focus trap, no Escape, no focus restore. Fix: add dialog semantics + focus management.

- [ ] **`src/components/model-selector/ModelCard.tsx` (onboarding cards) — Enter-only
      activation and no focus-visible style.** Fix: handle Space as well, add `focus-visible:ring-2`.

### Minor

- [ ] **Progress bars are visual-only.** `ModelCard.tsx` accuracy bars and
      `DownloadProgressDisplay.tsx` download fills are `<div>` fills with no
      `role="progressbar"`+`aria-valuenow`/`min`/`max`. Fix: add progressbar roles.

- [ ] **`src/components/ui/AudioPlayer.tsx` — hardcoded English play/pause label; seek
      `<input type="range">` has no `aria-label`.** Fix: route through `t()`, add `aria-label` (Seek).

- [ ] **`src/components/ui/Alert.tsx:57-61` — plain `<div>` with no role.** Fix: `role="alert"`
      for error/warning, `role="status"` for info/success; `aria-hidden` the icon.

- [ ] **`src/components/ui/ToggleSwitch.tsx:42-47` — the `sr-only` checkbox has no `aria-label`
      tying it to the visible `<h3>` title.** Fix: add `aria-label={label}` to the input.

- [ ] **Decorative Lucide icons throughout lack `aria-hidden`** (remaining instances after the
      Sidebar/SoundPicker fixes). Low impact (adjacent text labels). Fix: pass `aria-hidden="true"`.

- [ ] **Overlay waveform bars not gated by reduced-motion.** `RecordingOverlay.tsx:81-91` uses
      JS-driven inline `transition`. The `transcribing-pulse` keyframe is now gated; the bars are
      not. Fix: freeze/skip the bar height transitions under `prefers-reduced-motion: reduce`.

---

## Design / branding

- [ ] **Replace the generic Lucide sidebar tab icons with swamp/pond/frog-themed custom icons,
      matching the General tab's style.** The General tab already uses the custom frog icon
      `src/components/icons/HandyHand.tsx`; the other tabs still use stock Lucide glyphs. Design and
      swap in same-style custom icons for the 4 always-visible tabs (`src/components/Sidebar.tsx`
      `SECTIONS_CONFIG`):
  - Models — currently `Cpu` (lucide). Idea: tadpole/egg-cluster or a lily-pad "brain".
  - Advanced — currently `Cog` (lucide). Idea: gear made of reeds, or a frog-on-a-cog.
  - History — currently `History` (lucide). Idea: ripple rings / scroll on a lily pad.
  - About — currently `Info` (lucide). Idea: a frog "i" / cattail info marker.

  Also covers the two conditional tabs when shown: Postprocessing (`Sparkles` → fireflies/pond
  glow) and Debug (`FlaskConical` → swamp specimen jar). Keep stroke weight, viewBox, and sizing
  consistent with `HandyHand` so they read as one set; pass `aria-hidden="true"` (already wired on
  the nav icons). Part of the broader frog/swamp/wetland rebrand pass.

## Test infrastructure

- [ ] **Bare `bun test` collides with the Playwright e2e spec.** Bun's test globber matches
      `*.spec.ts`, so it picks up `tests/app.spec.ts` (a `@playwright/test` spec meant for
      `bun run test:playwright`), which then errors ("two different versions of @playwright/test" /
      `test.describe()` in a config-imported file). Result: bare `bun test` reports 1 fail + 1 error
      even though all 13 logic tests pass (`bun test scripts/` is clean). This makes the milestone-A
      acceptance gate (task #18, which lists a bare `bun test`) falsely red. Decide the test layout:
      either scope the acceptance command to `bun test scripts/`, or add a `bunfig.toml [test]` root /
      ignore so Bun never globs `tests/*.spec.ts`. Pick one deliberately — it sets the unit-vs-e2e
      boundary for the repo.

- [ ] **`tests/app.spec.ts` describe block still reads `"Handy App"`.** Minor rebrand leak in a
      test label (check-rebrand passed, so it doesn't scan `tests/`). Rename to "AudioBud App".

## Milestone B (out of milestone-A scope)

- [ ] **Updater still points at `cjpais/Handy`.** `src/components/update-checker/UpdateChecker.tsx:206`
      and `src-tauri/tauri.conf.json:71` reference the upstream Handy release feed. Repoint to the
      AudioBud release feed once it exists. (Milestone B: R2 model host + release pipeline.)
