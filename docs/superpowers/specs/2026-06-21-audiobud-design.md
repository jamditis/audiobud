# AudioBud design spec

- Date: 2026-06-21
- Status: draft, Codex 5.5 high reviewed (findings incorporated); four open decisions pending your call, then ready for sign-off
- Owner: Joe Amditis
- Repo: `github.com/jamditis/audiobud` (public, to be created)
- Local path: `C:\Users\amdit\OneDrive\Desktop\Crimes\playground\audiobud`

## Summary

AudioBud is a local global dictation app for Windows (with macOS/Linux following from the upstream codebase). You press a hotkey, speak, and the transcribed text is typed into whatever app currently has focus. It replaces the Windows built-in Win+H voice typing with a faster, private, local speech-to-text engine. Audio never leaves the machine; transcription runs on-device.

AudioBud is a rebranded fork of [Handy](https://github.com/cjpais/Handy) (cjpais/Handy, MIT, Tauri 2 + Rust). We adopt Handy's working pipeline rather than rebuild it, then rebrand, repoint its infrastructure off the upstream's servers, ship two STT backends (Parakeet V3, Whisper large-v3-turbo), and wire up the full repo treatment we use across the jawn projects.

This is a desktop dictation tool, not a terminal. It has no PTY, no xterm, no shell. (An early research pass over the AudioBash repo over-applied that template; this spec corrects it.) The only thing AudioBud borrows from AudioBash is the repo meta-structure: README/docs-site/version.js/CHANGELOG/CI/release conventions.

## Goals

- Press a hotkey anywhere in Windows, speak, get accurate text typed into the focused app.
- On-device transcription: no audio ever leaves the machine, no API keys, no per-use cost. Runs fully offline after the one-time first-run model download (see "Model delivery").
- Two STT engines selectable in settings: Parakeet V3 and Whisper large-v3-turbo (default decided by a Windows smoke benchmark — see "STT backends").
- User-customizable shortcuts, with a reliable working default and an opt-in path to use Win+H.
- Distinct product identity (name, icons, domain, updater) independent of upstream Handy.
- A real release pipeline: auto-updates with signed update payloads, a changelog, patch notes, a marketing site.

## Non-goals

- Not a terminal or command runner (that is AudioBash's job).
- No cloud transcription provider integration for v1 (local only; revisit later if wanted).
- No agent/LLM post-processing of transcripts in v1.
- No custom-built settings UI for v1 — we keep Handy's existing React settings UI, rebranded. Rebuilding the UI is a separate, later two-way-door decision.
- We do not relicense or hide the upstream MIT attribution or the model licenses.

## Decisions already made (from prior AskUserQuestion rounds)

- Approach: fork Handy into a new repo (vs build from scratch or adopt as-is).
- Name: AudioBud. Repo: `audiobud`, public, under `jamditis`.
- STT: ship both Parakeet V3 and Whisper large-v3-turbo.
- License: preserve Handy's MIT license and attribution.

## Open decisions (need your call before sign-off)

The Codex review surfaced four decisions that change implementation. These are flagged here and asked via AskUserQuestion; answers fold back into this spec.

1. **Win+H default.** Win+H cannot be cleanly owned by the in-app hotkey backend (see "The Win+H problem"). Options: (1) ship `Ctrl+Alt+Space` default with a one-click Win+H opt-in; (2) Win+H on by default via a bundled helper; (3) no Win+H, customizable only. Recommend option 1.
2. **Windows code-signing posture for v1.** Tauri's updater signature is not Windows Authenticode (see "Signing"). Options: ship unsigned installers (SmartScreen "more info -> run anyway" on first launch) plus signed updater payloads — free, recommend for v1; or buy an Authenticode/OV-or-EV cert (recurring cost — requires your approval, never purchased without it).
3. **Model delivery.** Download the default model on first run (smaller installer, needs network once) — recommend; or bundle the default model in the installer (larger installer, works offline immediately).
4. **Scope of this loop.** Confirm the loop targets milestone A (working local prototype) and that packaging/public-release (milestones B/C) follow after. Recommend yes.

## Background: why fork Handy

Global dictation is a solved problem; the work is integration and polish, not invention. Handy is the best-fit base:

- MIT licensed, so forkable and rebrandable.
- Tauri 2 + Rust: small binaries, no Electron bloat, native global hotkeys and text injection.
- Vendor-agnostic GPU path on Windows: Whisper runs on Vulkan, ONNX/Parakeet on DirectML, via the `transcribe-rs` crate. No CUDA/cuDNN DLL setup (the pain point with faster-whisper on Windows). The RTX 4080 is driven through Vulkan/DirectML, which the prebuilt releases already use.
- Already ships the exact pipeline we want: tray app, global hotkey, `cpal` audio capture, `rubato` resample to 16 kHz, Silero VAD, push-to-talk, and clipboard-paste text injection via `enigo`.
- Active project, Windows-tested, with a working CI release pipeline we can adapt.

## Research notes

Two background research agents (codebase + web/repo) ran on 2026-06-21. Full transcripts are in the session task outputs; load-bearing findings:

- **Build prerequisites (Windows, RTX 4080):** Rust stable MSVC, Bun (Handy's only documented package manager), VS C++ Build Tools, Vulkan SDK (hard runtime dep — see Handy issue #99), WebView2. End users also need the VC++ x64 redistributable at runtime (Handy issue #1527: `MSVCP140.dll` crash without it) — installer must detect/install it (see "Packaging"). No git submodules, but `Cargo.toml` pins git forks (`rdev` from rustdesk-org, `vad-rs`/`rodio` from cjpais) and a patched Tauri runtime via `[patch.crates-io]` (`cjpais/tauri.git` branch `handy-2.10.2`). These pins must be preserved or the build breaks. Build: `bun install` then `bun run tauri build` (produces MSI + NSIS .exe + updater artifacts).

- **The Win+H constraint:** rdev/handy-keys cannot cleanly claim Super+H on Windows. handy-keys does map `win`/`super`/`meta` to a CMD modifier flag, but Handy's own issue #917 documents the exact failure: with a `Win+key` hotkey the bare WIN keydown propagates before the combo is recognized, and on release Windows sees a lone WIN press and opens the Start menu. The low-level hook can suppress the matched keystroke but cannot retroactively swallow the lone WIN. Disabling native Win+H via `HKCU\Software\Microsoft\Input\Settings\IsVoiceTypingKeyEnabled=0` only frees the shortcut from the OS voice-typing handler; it does not fix the Start-menu-on-WIN-release behavior. The clean fix is an AutoHotkey v2 technique (`~LWin::Send("{Blind}{vkE8}")` to absorb the lone WIN, plus `#h::Run(...)` to bind Win+H), which can be compiled to a standalone helper exe so users need not install AHK.

- **Rebrand surface (one-way doors flagged):** the bundle `identifier` in `tauri.conf.json` (`com.pais.handy` -> e.g. `tech.amditis.audiobud`) is a one-way door — it keys the app-data dir, single-instance lock, autostart registration, and updater install path. The updater needs our own minisign keypair (`bun tauri signer generate`) and a repointed `endpoints` URL; the upstream `signCommand` (cjpais's Azure Trusted Signing) must be removed. Model downloads are hardcoded to `https://blob.handy.computer/...` across 15+ URLs in `src-tauri/src/managers/model.rs` with no base-URL constant — we introduce a `MODEL_BASE_URL` const and mirror the model blobs to our own host (Cloudflare R2 `pi-transfer`) or point at the HuggingFace originals. About-page links (`AboutSettings.tsx`), tray tooltip (`tray.rs`), window title (`lib.rs`), crate name (`Cargo.toml`), icons, NSIS template, and locale JSON all carry Handy branding. No telemetry/analytics exist in Handy — outbound calls are only model downloads, the updater endpoint, and donate/GitHub links; repointing those three makes AudioBud self-contained.

- **Release pipeline:** `tauri-apps/tauri-action@v0` matrix build, one call per OS, with `releaseDraft: true` (review before publish) and `includeUpdaterJson: true` (auto-generates `latest.json` with per-target signed URLs). Updater payload signing via `TAURI_SIGNING_PRIVATE_KEY` + `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` repo secrets. Note: this is updater-payload signing, not Windows Authenticode (see "Signing"). Changelog: Keep a Changelog format (`CHANGELOG.md`) reads better in release bodies than raw commit dumps for a desktop app.

- **Fork vs detached repo:** recommend a detached repo (clone, strip `.git`, `git init`, push) rather than GitHub's Fork button, for distinct product identity and clean GitHub Releases as the updater CDN. Trade-off (Codex finding 7): a detached repo loses GitHub's fork-network linkage, so upstream tracking and security-advisory propagation become manual. Mitigation is an explicit upstream sync policy (see "Upstream sync policy"), not an implicit "stays mergeable" assumption.

- **Reusable local assets:** AudioBash's whisper.cpp install already exists on disk (`%APPDATA%\audiobash\whisper-cpp\`). Not directly reused by AudioBud (Handy ships its own `transcribe-rs` whisper path), but confirms the toolchain works on this machine. Full toolchain (cargo/rustup/node/bun/ffmpeg/gh-as-jamditis) is present.

## Architecture

AudioBud inherits Handy's architecture. The data flow:

```
hotkey pressed (push-to-talk)
  -> cpal captures mic audio at native rate
  -> rubato resamples to 16 kHz mono
  -> Silero VAD trims silence (for VAD modes; push-to-talk uses explicit start/stop)
  -> transcribe-rs runs the selected engine:
       - Parakeet V3 via ONNX Runtime on DirectML, or
       - Whisper large-v3-turbo via whisper.cpp on Vulkan
  -> transcribed text
  -> enigo: snapshot clipboard, set text, send Ctrl+V (layout-independent VK), restore clipboard
  -> text appears in whatever app has focus
```

Components (Handy's modules, rebranded):

- `src-tauri/src/lib.rs` — app setup, main window, single-instance + autostart plugins, tray, global shortcut registration, the `--toggle-transcription` CLI entrypoint.
- `src-tauri/src/managers/model.rs` — model registry and downloads (the `MODEL_BASE_URL` change lands here).
- `src-tauri/src/tray.rs` — tray icon + tooltip.
- `transcribe-rs` crate (Cargo dep) — the engine abstraction over whisper.cpp (Vulkan) and ONNX (DirectML).
- `handy-keys` / `rdev` (Cargo deps) — low-level global hotkey backend.
- `src/` (React + TypeScript) — Handy's existing settings UI, rebranded text/icons only for v1.

Each unit keeps Handy's existing boundaries; our changes are surgical (rebrand strings, model host, hotkey defaults, updater keys), not architectural.

### Injection limits (Codex finding 9)

Clipboard-paste injection types into the focused app, but not universally. By Windows design it cannot or should not target: elevated apps when AudioBud runs unelevated (UIPI blocks input to higher-integrity windows), password fields that reject programmatic paste, remote-desktop/VDI sessions, and apps that aggressively monitor or lock the clipboard. The clipboard snapshot/restore also races against clipboard-manager utilities. These are documented as known limits in the README/manual, and each is a named case in the manual smoke gate. Not a bug to fix in v1; a boundary to state.

## The Win+H problem

Win+H cannot be cleanly owned by the in-app hotkey backend (research above; Handy issue #917). This affects both the default-hotkey decision (open decision 1) and scope.

Three options:

1. **Reliable default + one-click Win+H opt-in (recommended).** Ship `Ctrl+Alt+Space` as the working default (rdev handles it cleanly). In settings, offer "Replace Windows Win+H dictation" that (a) writes `IsVoiceTypingKeyEnabled=0`, and (b) installs and runs a bundled, compiled helper (AHK v2 logic compiled to an exe, no AHK install needed) that neutralizes the lone WIN and routes Win+H to AudioBud via `--toggle-transcription`. Reversible toggle.
2. **Win+H by default via the helper.** Same helper, enabled out of the box. Higher first-run friction (registry write + helper process + a sign-out/reboot for the registry change to take) and a background hook process always running.
3. **No Win+H, customizable only.** Ship `Ctrl+Alt+Space`, let users bind anything rdev supports, document why Win+H is special.

Codex finding 2 (blocker): the Win+H helper is not a small toggle — it is a compiled helper exe with install, autostart, uninstall, and crash-recovery behavior, plus a registry edit that needs a sign-out to take effect. Therefore Win+H is **not** part of the prototype (milestone A). Milestone A ships `Ctrl+Alt+Space` only. The chosen Win+H option is built and hardened in milestone B (packaging), where the helper's full lifecycle is prototyped and smoke-tested before it ships. This keeps the prototype reliable and quarantines the risky surface.

## STT backends

- **Parakeet V3:** NVIDIA Parakeet-TDT 0.6B via ONNX Runtime on DirectML. Low latency, strong English WER, native punctuation, ~478 MB.
- **Whisper large-v3-turbo:** via whisper.cpp on Vulkan, ~1.6 GB. Higher accuracy ceiling and multilingual; slower.

Both are already supported by `transcribe-rs` and present in Handy's model registry; our work is serving both from our own model host and selecting the default.

**Default is decided by a Windows smoke benchmark, not assumed (Codex finding 6).** Parakeet's model card centers Linux/NeMo, and Handy has recent Parakeet-on-Windows failure reports. So milestone A benchmarks both engines on this RTX 4080 (latency, accuracy on a fixed dictation sample, and stability across repeated runs). Parakeet is the intended default and ships as default only if it passes; otherwise Whisper large-v3-turbo becomes the default and Parakeet stays selectable. The decision and its numbers are recorded in the spec/changelog.

## Model delivery (open decision 3)

Models are 478 MB (Parakeet) to ~1.6 GB (Whisper turbo). "Offline" means after a one-time download, not at install. Two options:

- **Download on first run (recommended):** small installer; AudioBud fetches the default model from `MODEL_BASE_URL` on first launch with a progress UI (Handy already does this). Needs network once. Download integrity is verified (size + checksum) before the model is marked ready.
- **Bundle the default model:** installer carries the default model so it works offline immediately, at the cost of a much larger installer. Non-default models still download on demand.

Either way, the README/marketing copy says "runs offline after first-run model download," never "offline by default" (Codex finding 3).

## Licensing and attribution

Two licensing layers: the app code, and the models.

App code:
- Keep `LICENSE` (MIT) with cjpais's original copyright line intact (MIT requires retaining the notice).
- Add a `NOTICE` file plus a README attribution block:
  > AudioBud is a fork of [Handy](https://github.com/cjpais/Handy) by CJ Pais, used under the MIT License. AudioBud is an independent project and is not affiliated with or endorsed by the Handy authors.
- Add our own copyright for our changes alongside (not replacing) the original.
- `tauri.conf.json` `bundle.license` and `Cargo.toml` `license` stay MIT.

Models (Codex finding 4 — must not be skipped if we mirror/redistribute):
- **Parakeet-TDT 0.6B V3:** NVIDIA, CC BY 4.0. Redistributing or mirroring it requires attribution to NVIDIA and a copy of the license terms.
- **Whisper large-v3-turbo:** OpenAI, MIT.
- Add `MODEL_NOTICES.md` (or a `licenses/` dir) listing each model, its source, license, and required attribution, and surface model attribution in the app's about page and the docs. If we mirror the blobs to R2, the attribution travels with them.
- Per house rules: no AI authorship attribution anywhere in the repo, commits, docs, or releases (this is separate from the model/source attribution above, which is required by those licenses).

## Milestones (Codex finding 8 — the deliverables are split, not one loop)

The repo deliverables are too wide for a single prototype loop. Three milestones:

- **Milestone A — working local prototype (this loop's target).** Detached repo with upstream remote; unmodified upstream build verified (`bun tauri dev`); minimal rebrand to run as AudioBud (identifier, productName, window title, crate name); `Ctrl+Alt+Space` default hotkey; both engines wired and benchmarked, default chosen; model download-on-first-run working with integrity check; text injection verified into a focused app; seam-level tests passing; the Windows manual smoke gate passing. No Win+H helper, no signing, no docs site, no release automation yet.
- **Milestone B — packaging.** `MODEL_BASE_URL` + model host (R2 or HF), `MODEL_NOTICES.md`; the chosen Win+H option and its helper lifecycle (install/autostart/uninstall/crash), hardened and smoke-tested; VC++ redist detection/install in the installer; minisign updater key + repointed endpoints; updater end-to-end test; CHANGELOG seeded.
- **Milestone C — public release.** README/NOTICE/CLAUDE.md/.claude rules; docs GitHub Pages marketing site; `.github` CI (light) + release workflow (tag-only); branch protection on `main`; signing posture per decision 2; first tagged draft release reviewed and published by Joe.

The `/loop` runs until milestone A is met (a working, tested prototype). B and C are separate, sequenced work after you review the prototype.

## Repo deliverables (full treatment, across milestones B and C)

Mirrors the AudioBash conventions, adapted for Rust/Tauri/Bun.

### Top-level files

- `README.md` — badges (license, version, build, platforms), hero shot, features, per-platform install, usage + shortcuts table, the Win+H section, injection-limits note, "offline after first-run download" wording, build-from-source (Bun/Tauri), tech stack, Handy attribution block, model attributions, license, author.
- `LICENSE` (MIT, upstream copyright retained), `NOTICE` (fork attribution), `MODEL_NOTICES.md` (model licenses/attribution).
- `CLAUDE.md` — full project guide (see outline below).
- `CHANGELOG.md` — Keep a Changelog format, seeded with the 0.1.0 entry.
- `.gitignore` — Rust (`/target`, `*.rlib`), Node/Bun (`node_modules`, `dist`), Tauri build output, OS junk, and `desktop.ini` (OneDrive injects it into `.git` on Legion and breaks fetch — known issue).
- `.gitattributes` — normalize line endings.

### CLAUDE.md outline

- Project overview: local global dictation, Tauri 2 + Rust + React, on-device, fork of Handy.
- Tech stack: Rust/Tauri 2 backend, React + TypeScript frontend, `transcribe-rs` (Vulkan + DirectML), Bun.
- Directory map (Handy's layout + our docs/.github/.claude additions).
- Build/run: `bun install`, `bun tauri dev`, `bun run tauri build`; Windows prereqs (Vulkan SDK, VC++ redist, VS Build Tools, WebView2).
- The Win+H mechanism and the helper (registry key + compiled helper exe), and its lifecycle.
- Injection limits (elevated apps, password fields, RDP, clipboard monitors).
- Model host: `MODEL_BASE_URL`, where blobs live, integrity checks, model licenses.
- Updater: minisign key, endpoints, signing secrets, and the Authenticode distinction.
- Upstream sync policy (below).
- Release process (points at `.claude/rules/release-process.md`).
- House rules carried over: no AI attribution, sentence case, no emojis in source/logs, Codex review before PR, Joe merges.

### Upstream sync policy (Codex finding 7)

- Add `upstream` git remote = `https://github.com/cjpais/Handy`.
- On a cadence (and whenever upstream ships a security fix): `git fetch upstream`, review the changelog/commits, cherry-pick or merge fixes that touch code we still share, and re-run the smoke gate. Because we rebrand a small surface and keep Handy's module boundaries, most upstream fixes apply cleanly; conflicts concentrate in the rebranded files (identifier, model host, hotkey defaults).
- Watch upstream's releases/advisories manually (no fork-network propagation): subscribe to upstream releases; note the last-synced upstream commit in `CHANGELOG.md`.

### `.github/`

CI is split so day-to-day work runs cheap and the expensive cross-platform build only fires on releases. GitHub bills macOS runners at 10x and Windows at 2x a Linux minute, so: light checks on every PR/push (Linux only), heavy installer build on tags only.

- `workflows/ci.yml` (light, every PR + push to `main`): runs on `ubuntu-latest` (1x). Steps: `cargo fmt --check`, `cargo clippy -- -D warnings` (lint-only; no full Tauri build), frontend `bun run lint` + typecheck, and fast `cargo test` / `bun test` for the units we touch. Concurrency group cancels superseded runs. Path filters skip CI for docs-only changes. Target: ~1-3 Linux minutes per PR.
- `workflows/release.yml` (heavy, tags `v*` only): `tauri-action@v0` matrix (Windows first; macOS/Linux as upstream supports), Vulkan SDK install, long-path handling, `releaseDraft: true`, `includeUpdaterJson: true`, signing secrets. The only workflow that does a full Tauri build; runs only when we cut a release.
- `FUNDING.yml` — GitHub Sponsors (jamditis) + Venmo.
- Issue/PR templates.

### Branch protection on `main`

Applied once the repo is pushed to GitHub (push needs your approval per house rules; not done in this loop):

- Require a pull request before merging (no direct pushes to `main`).
- Require the light `ci.yml` checks to pass (clippy, fmt, lint, typecheck, fast tests).
- Require the branch to be up to date before merging.
- Do not require the heavy release build as a status check (it doesn't run on PRs).
- No extra required-reviewer gate beyond Joe's merge step (Codex review happens locally pre-PR per house rules).

### `.claude/rules/`

- `tauri-patterns.md`, `security.md`, `testing.md`, `release-process.md`, `aesthetic.md` (marketing-site aesthetic).

### `docs/` (GitHub Pages marketing site)

- `index.html`, `manual.html`, `releases.html`, `about.html`, `CNAME`, `favicon.svg` (inline SVG per house rule), `js/version.js` (`AUDIOBUD_VERSION`, populates `[data-version]`/`[data-download]`), `screenshots/`.
- Download URLs follow Tauri artifact names (`AudioBud_X.Y.Z_x64-setup.exe` / `.msi`, plus mac/linux when built).
- Aesthetic: distinct from AudioBash's exact theme but a sibling in the jawn family — a warm, friendly "bud/companion" identity rather than the void/brutalist terminal look, since AudioBud is a calm background utility. Final aesthetic is a two-way door; locked during the docs-site task.

### Release and changelog process

- Single source of version truth: `src-tauri/tauri.conf.json` `version`, mirrored to `Cargo.toml`, `package.json`, `docs/js/version.js`.
- Flow: bump versions -> update `CHANGELOG.md` -> `cargo test` + `bun test` green -> commit + tag `vX.Y.Z` -> CI builds a draft release with artifacts + `latest.json` -> review -> Joe publishes. Never auto-merge, never auto-publish.
- Patch-notes template (release body):
  ```markdown
  ## AudioBud vX.Y.Z — YYYY-MM-DD
  ### New
  ### Improved
  ### Fixed
  ### Known issues
  Auto-update installs this automatically. Manual: download below.
  ```

## Signing (Codex finding 5 — two different things)

- **Updater payload signing (Tauri minisign):** signs the update bundle so the installed app verifies an update came from us before applying it. Free; uses our `TAURI_SIGNING_PRIVATE_KEY`. This is what "signed auto-updates" means here.
- **Windows Authenticode code signing:** what removes the SmartScreen "unknown publisher" warning on download/first-launch. This is separate, needs an OV or EV certificate (recurring cost), and the upstream Azure Trusted Signing `signCommand` cannot be reused.
- v1 posture is open decision 2: recommend unsigned installers (documented SmartScreen step) + signed updater payloads for v1; revisit Authenticode later. Any cert purchase needs your explicit approval.

## Testing strategy

TDD throughout, per house rules. Because this is a fork of working software, automated tests focus on the seams we change; the OS-level behaviors get a documented manual gate (Codex finding 12).

Automated:
- **Rust unit tests (`cargo test`):** `MODEL_BASE_URL` URL construction (every model resolves to our host); model-download integrity check (size/checksum verification logic, with a mocked download); hotkey-config parsing for default and customized bindings; the Win+H registry-toggle helper's enable/disable reversibility (mock the registry write) — milestone B.
- **Frontend tests (`bun test`/Vitest):** settings rendering for engine selection (correct default selected), shortcut-customization UI, the Win+H opt-in toggle state — milestone B.

Windows manual smoke gate (exact pass/fail; run on this RTX 4080):
- Install produces a launchable app; VC++ redist present or installed by the installer (no `MSVCP140.dll` crash).
- Default hotkey (`Ctrl+Alt+Space`) starts/stops capture.
- Both engines load: Parakeet on DirectML, Whisper turbo on Vulkan; each transcribes a fixed spoken sample to expected text.
- Benchmark recorded: per-engine latency and accuracy on the fixed sample; default engine chosen from results.
- Text injection verified into a normal editor; injection-limit cases (elevated app, password field) confirmed as known-unsupported, not silent corruption.
- Win+H opt-in (milestone B): enabling writes the registry key and installs the helper; Win+H toggles AudioBud and does not open the Start menu; disabling fully reverts; helper survives a restart and a crash.
- Updater (milestone B/C): a bumped version is detected, downloaded, signature-verified, and applied.

Regression tests: any bug found gets a failing test first, then the fix (bug-fixing workflow).

**Milestone A "working prototype" bar:** a locally built AudioBud that, on this machine, captures audio on `Ctrl+Alt+Space` and types a correct local transcription (from the benchmarked default engine) into a focused editor, with the seam-level tests and the milestone-A portion of the manual gate passing.

## Build and toolchain

Confirmed present on Legion: cargo/rustup, node, bun, ffmpeg, gh (as jamditis). To verify/add before building: Vulkan SDK, VS C++ Build Tools (Desktop development with C++), WebView2 (preinstalled on Win 11). First implementation step is an unmodified `bun install` + `bun tauri dev` to prove the upstream builds before we change anything.

### Packaging (milestone B)

- VC++ x64 redistributable: the installer (NSIS/MSI) must detect the redist and install or repair it, not merely assume it (Codex finding 10; Handy issue #1527). Add this as a release gate — a fresh-VM/clean-profile launch must not crash on `MSVCP140.dll`.
- Vulkan runtime: documented prerequisite; verify behavior on a machine without the Vulkan SDK (runtime DLL vs SDK).

## Codex 5.5 high review: findings and resolutions

Reviewed 2026-06-21, `gpt-5.5`, high reasoning. Twelve findings; resolutions:

1. (blocker) Win+H default contradiction -> moved to open decision 1; prototype defaults to `Ctrl+Alt+Space`.
2. (blocker) Win+H helper underestimated -> Win+H excluded from milestone A; helper lifecycle hardened in milestone B with a smoke gate.
3. (blocker) "offline by default" vs model download -> reworded throughout to "offline after first-run download"; model delivery is open decision 3.
4. (major) Parakeet CC BY 4.0 attribution -> added `MODEL_NOTICES.md` + in-app/docs model attribution.
5. (major) "signed" conflation -> added "Signing" section distinguishing updater minisign from Windows Authenticode; v1 posture is open decision 2.
6. (major) Parakeet default unverified on Windows -> default now decided by a milestone-A Windows smoke benchmark.
7. (major) detached fork loses upstream linkage -> added explicit "Upstream sync policy."
8. (major) deliverables too wide -> added "Milestones" A/B/C; loop targets A.
9. (major) "whatever app has focus" overpromise -> added "Injection limits."
10. (major) VC++ redist install logic unspecified -> added "Packaging" redist detect/install release gate.
11. (minor) cross-platform scope -> non-Windows releases disabled until each has a smoke pass (see "Risks").
12. (minor) tests miss real hotkey/helper/injection/download -> added the Windows manual smoke gate with exact cases.

## Risks and open questions

- **The four open decisions** above (Win+H default, signing posture, model delivery, loop scope).
- **Model hosting (one-way-ish):** R2 `pi-transfer` vs HuggingFace originals. R2 gives control/uptime and lets attribution travel with the blobs; HF avoids hosting cost. Lean R2; decided in milestone B.
- **Identifier (one-way door):** propose `tech.amditis.audiobud`. Confirm before first build; it keys app-data/updater paths.
- **Marketing domain:** `audiobud.app` vs a path under an existing domain. Two-way door; decide at docs-site time.
- **Upstream patched-Tauri pin:** Handy depends on a forked Tauri runtime branch; bumping Tauri later inherits that maintenance. Acceptable for v1; noted.
- **Cross-platform scope:** v1 is Windows-only (Win+H, registry, DirectML, VC++ redist are Windows-specific). macOS/Linux release jobs stay disabled until each platform passes its own smoke gate (Codex finding 11).

## Out of scope for v1 (revisit later)

- Cloud STT providers, agent/LLM transcript post-processing, a custom-built settings UI, custom-dictionary/vocabulary support (the related AudioBash bug is filed as jamditis/audiobash#41), Windows Authenticode signing (pending decision 2), and non-Windows release automation.
