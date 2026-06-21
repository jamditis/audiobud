# AudioBud design spec

- Date: 2026-06-21
- Status: draft, pending user review (Codex 5.5 high review pending before user sign-off)
- Owner: Joe Amditis
- Repo: `github.com/jamditis/audiobud` (public, to be created)
- Local path: `C:\Users\amdit\OneDrive\Desktop\Crimes\playground\audiobud`

## Summary

AudioBud is a local, offline-first global dictation app for Windows (with macOS/Linux following from the upstream codebase). You press a hotkey, speak, and the transcribed text is typed into whatever app currently has focus. It replaces the Windows built-in Win+H voice typing with a faster, private, local speech-to-text engine.

AudioBud is a rebranded fork of [Handy](https://github.com/cjpais/Handy) (cjpais/Handy, MIT, Tauri 2 + Rust). We adopt Handy's working pipeline rather than rebuild it, then rebrand, repoint its infrastructure off the upstream's servers, ship two STT backends (Parakeet V3 default, Whisper large-v3-turbo option), and wire up the full repo treatment we use across the jawn projects.

This is a desktop dictation tool, not a terminal. It has no PTY, no xterm, no shell. (An early research pass over the AudioBash repo over-applied that template; this spec corrects it.) The only thing AudioBud borrows from AudioBash is the repo meta-structure: README/docs-site/version.js/CHANGELOG/CI/release conventions.

## Goals

- Press a hotkey anywhere in Windows, speak, get accurate text typed into the focused app.
- Fully local and offline by default. No audio leaves the machine. No API keys required.
- Two STT engines selectable in settings: Parakeet V3 (default) and Whisper large-v3-turbo.
- User-customizable shortcuts, with a sane working default and an opt-in path to use Win+H.
- Distinct product identity (name, icons, domain, updater) independent of upstream Handy.
- A real release pipeline: signed auto-updates, a changelog, patch notes, a marketing site.

## Non-goals

- Not a terminal or command runner (that is AudioBash's job).
- No cloud transcription provider integration for v1 (local only; revisit later if wanted).
- No agent/LLM post-processing of transcripts in v1.
- No custom-built settings UI for v1 — we keep Handy's existing React settings UI, rebranded. Rebuilding the UI is a separate, later two-way-door decision.
- We do not relicense or hide the upstream MIT attribution.

## Decisions already made (from prior AskUserQuestion rounds)

- Approach: fork Handy into a new repo (vs build from scratch or adopt as-is).
- Name: AudioBud. Repo: `audiobud`, public, under `jamditis`.
- STT: ship both; default Parakeet V3, Whisper large-v3-turbo as an option.
- Win+H: default trigger should be Win+H, but all shortcuts must be user-customizable. (See "The Win+H problem" — research shows a hard OS constraint here that needs a decision in spec review.)
- License: preserve Handy's MIT license and attribution.

## Background: why fork Handy

Global dictation is a solved problem; the work is integration and polish, not invention. Handy is the best-fit base:

- MIT licensed, so forkable and rebrandable.
- Tauri 2 + Rust: small binaries, no Electron bloat, native global hotkeys and text injection.
- Vendor-agnostic GPU path on Windows: Whisper runs on Vulkan, ONNX/Parakeet on DirectML, via the `transcribe-rs` crate. No CUDA/cuDNN DLL setup (the pain point with faster-whisper on Windows). The RTX 4080 is driven through Vulkan/DirectML, which the prebuilt releases already use.
- Already ships the exact pipeline we want: tray app, global hotkey, `cpal` audio capture, `rubato` resample to 16 kHz, Silero VAD, push-to-talk, and clipboard-paste text injection via `enigo`.
- Active project, Windows-tested, with a working CI release pipeline we can adapt.

## Research notes

Two background research agents (codebase + web/repo) ran on 2026-06-21. Full transcripts are in the session task outputs; load-bearing findings:

- **Build prerequisites (Windows, RTX 4080):** Rust stable MSVC, Bun (Handy's only documented package manager), VS C++ Build Tools, Vulkan SDK (hard runtime dep — see Handy issue #99), WebView2. End users also need the VC++ x64 redistributable at runtime (Handy issue #1527: `MSVCP140.dll` crash without it) — bundle it. No git submodules, but `Cargo.toml` pins git forks (`rdev` from rustdesk-org, `vad-rs`/`rodio` from cjpais) and a patched Tauri runtime via `[patch.crates-io]` (`cjpais/tauri.git` branch `handy-2.10.2`). These pins must be preserved or the build breaks. Build: `bun install` then `bun run tauri build` (produces MSI + NSIS .exe + updater artifacts).

- **The Win+H constraint (decision needed):** rdev/handy-keys cannot cleanly claim Super+H on Windows. handy-keys does map `win`/`super`/`meta` to a CMD modifier flag, but Handy's own issue #917 documents the exact failure: with a `Win+key` hotkey the bare WIN keydown propagates before the combo is recognized, and on release Windows sees a lone WIN press and opens the Start menu. The low-level hook can suppress the matched keystroke but cannot retroactively swallow the lone WIN. Disabling native Win+H via `HKCU\Software\Microsoft\Input\Settings\IsVoiceTypingKeyEnabled=0` only frees the shortcut from the OS voice-typing handler; it does not fix the Start-menu-on-WIN-release behavior. The clean fix is an AutoHotkey v2 technique (`~LWin::Send("{Blind}{vkE8}")` to absorb the lone WIN, plus `#h::Run(...)` to bind Win+H), which can be compiled to a standalone helper exe so users need not install AHK. Recommendation in this spec: ship a reliable default (`Ctrl+Alt+Space`) and offer Win+H as a one-click opt-in that writes the registry key and installs the helper. This honors "Win+H customizable" while not shipping a broken default.

- **Rebrand surface (one-way doors flagged):** the bundle `identifier` in `tauri.conf.json` (`com.pais.handy` -> e.g. `tech.amditis.audiobud`) is a one-way door — it keys the app-data dir, single-instance lock, autostart registration, and updater install path. The updater needs our own minisign keypair (`bun tauri signer generate`) and a repointed `endpoints` URL; the upstream `signCommand` (cjpais's Azure Trusted Signing) must be removed. Model downloads are hardcoded to `https://blob.handy.computer/...` across 15+ URLs in `src-tauri/src/managers/model.rs` with no base-URL constant — we introduce a `MODEL_BASE_URL` const and mirror the model blobs to our own host (Cloudflare R2 `pi-transfer`) or point at the HuggingFace originals. About-page links (`AboutSettings.tsx`), tray tooltip (`tray.rs`), window title (`lib.rs`), crate name (`Cargo.toml`), icons, NSIS template, and locale JSON all carry Handy branding. No telemetry/analytics exist in Handy — outbound calls are only model downloads, the updater endpoint, and donate/GitHub links; repointing those three makes AudioBud self-contained.

- **Release pipeline:** `tauri-apps/tauri-action@v0` matrix build, one call per OS, with `releaseDraft: true` (review before publish) and `includeUpdaterJson: true` (auto-generates `latest.json` with per-target signed URLs). Updater signing via `TAURI_SIGNING_PRIVATE_KEY` + `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` repo secrets. Changelog: Keep a Changelog format (`CHANGELOG.md`, Added/Changed/Fixed/Removed) reads better in release bodies than raw commit dumps for a desktop app.

- **Fork vs detached repo:** recommend a detached repo (clone, strip `.git`, `git init`, push) rather than GitHub's Fork button. Reasons: distinct product identity for the rebrand, clean GitHub Releases usable as the updater CDN, and no "forked from cjpais/Handy" linkage. Keep `upstream` as a named git remote so we can still pull upstream fixes via merge/cherry-pick. MIT attribution is preserved regardless (see Licensing).

- **Reusable local assets:** AudioBash's whisper.cpp install already exists on disk (`%APPDATA%\audiobash\whisper-cpp\`: `ggml-small.en.bin`, `main.exe`, `whisper.dll`). Not directly reused by AudioBud (Handy ships its own `transcribe-rs` whisper path), but confirms the toolchain works on this machine. Full toolchain (cargo/rustup/node/bun/ffmpeg/gh-as-jamditis) is present.

## Architecture

AudioBud inherits Handy's architecture. The data flow:

```
hotkey pressed (push-to-talk)
  -> cpal captures mic audio at native rate
  -> rubato resamples to 16 kHz mono
  -> Silero VAD trims silence (for VAD modes; push-to-talk uses explicit start/stop)
  -> transcribe-rs runs the selected engine:
       - Parakeet V3 via ONNX Runtime on DirectML (default), or
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

Each unit keeps Handy's existing boundaries; our changes are surgical (rebrand strings, model host, hotkey defaults, updater keys), not architectural. This keeps the fork mergeable against upstream.

## The Win+H problem (needs a decision in review)

Your stated intent: Win+H as the default, all shortcuts customizable. Research surfaced a hard OS constraint (above): rdev cannot cleanly own Win+H, and the registry disable alone does not stop the Start menu firing on lone-WIN release.

Three options, with my recommendation:

1. **Recommended: reliable default + one-click Win+H opt-in.** Ship `Ctrl+Alt+Space` as the working default (rdev handles it cleanly). In settings, offer "Replace Windows Win+H dictation" that (a) writes `IsVoiceTypingKeyEnabled=0`, and (b) installs and runs a bundled, compiled helper (AHK v2 logic compiled to an exe, no AHK install needed) that neutralizes the lone WIN and routes Win+H to AudioBud via `--toggle-transcription`. Reversible toggle. This delivers Win+H without shipping a broken default.

2. **Win+H by default via the helper.** Same helper, but enabled out of the box. Higher first-run friction (registry write + helper process + a restart/sign-out for the registry change to take), and a background helper process always running. Honors the literal "default Win+H" ask at the cost of a heavier, more surprising install.

3. **No Win+H, customizable only.** Ship `Ctrl+Alt+Space`, let users bind anything rdev supports, document why Win+H is special. Simplest and most reliable, but does not meet the Win+H ask.

I recommend option 1. It satisfies both halves of your decision (Win+H available and customizable) while keeping the default reliable. Flagging for your call in spec review.

## STT backends

- **Parakeet V3 (default):** NVIDIA Parakeet-TDT 0.6B via ONNX Runtime on DirectML. Lowest latency, strong English WER, native punctuation, ~478 MB. Best dictation experience on the 4080.
- **Whisper large-v3-turbo (option):** via whisper.cpp on Vulkan, ~1.6 GB. Higher accuracy ceiling and multilingual; slower. Selectable in settings for users who want it.

Both are already supported by `transcribe-rs` and listed in Handy's model registry; our work is defaulting to Parakeet, ensuring large-v3-turbo is present in the registry, and serving both from our own model host.

## Licensing and attribution

- Keep `LICENSE` (MIT) with cjpais's original copyright line intact (MIT requires retaining the notice).
- Add a `NOTICE` file plus a README attribution block:
  > AudioBud is a fork of [Handy](https://github.com/cjpais/Handy) by CJ Pais, used under the MIT License. AudioBud is an independent project and is not affiliated with or endorsed by the Handy authors.
- Add our own copyright for our changes alongside (not replacing) the original.
- `tauri.conf.json` `bundle.license` and `Cargo.toml` `license` stay MIT.
- Per house rules: no AI attribution anywhere in the repo, commits, docs, or releases.

## Repo deliverables (the full treatment)

Everything below ships as part of standing up the repo. These mirror the AudioBash conventions, adapted for Rust/Tauri/Bun.

### Top-level files

- `README.md` — badges (license, version, build, platforms), hero shot, features, per-platform install, usage + shortcuts table, the Win+H section, build-from-source (Bun/Tauri), tech stack, the Handy attribution block, license, author.
- `LICENSE` (MIT, upstream copyright retained) and `NOTICE` (attribution).
- `CLAUDE.md` — full project guide (see outline below).
- `CHANGELOG.md` — Keep a Changelog format, seeded with the 0.1.0 entry.
- `.gitignore` — Rust (`/target`, `*.rlib`), Node/Bun (`node_modules`, `dist`, `bun.lockb` policy), Tauri build output, OS junk, and `desktop.ini` (OneDrive injects it into `.git` on Legion and breaks fetch — known issue).
- `.gitattributes` — normalize line endings.

### CLAUDE.md outline

- Project overview: local global dictation, Tauri 2 + Rust + React, offline-first, fork of Handy.
- Tech stack: Rust/Tauri 2 backend, React + TypeScript frontend, `transcribe-rs` (Vulkan + DirectML), Bun package manager.
- Directory map (Handy's layout + our docs/.github/.claude additions).
- Build/run: `bun install`, `bun tauri dev`, `bun run tauri build`; Windows prereqs (Vulkan SDK, VC++ redist, VS Build Tools, WebView2).
- The Win+H mechanism and the helper (registry key + compiled helper exe).
- Model host: `MODEL_BASE_URL` and where blobs live.
- Updater: our minisign key, endpoints, signing secrets.
- Upstream sync: `upstream` remote, how to pull Handy fixes.
- Release process (points at `.claude/rules/release-process.md`).
- House rules carried over: no AI attribution, sentence case, no emojis in source/logs, Codex review before PR, Joe merges.

### `.github/`

- `workflows/build.yml` — `tauri-action@v0` matrix (Windows first; macOS/Linux as upstream supports), Vulkan SDK install step, long-path handling, `releaseDraft: true`, `includeUpdaterJson: true`, signing secrets.
- `workflows/ci.yml` — `cargo test`, `cargo clippy`, frontend lint/typecheck + `bun test` on push/PR.
- `FUNDING.yml` — GitHub Sponsors (jamditis) + Venmo.
- Issue/PR templates.

### `.claude/rules/`

- `tauri-patterns.md` — IPC via `#[tauri::command]`, no preload/contextBridge, permissions via Tauri capabilities, global-shortcut lifecycle.
- `security.md` — Tauri capability allowlist, CSP, Windows signing posture, no secrets in repo.
- `testing.md` — `cargo test` (Rust) + `bun test`/Vitest (frontend); TDD; regression tests for every bug.
- `release-process.md` — version-bump locations, tag, draft release, updater `latest.json`, patch-notes template.
- `aesthetic.md` — the marketing-site aesthetic (see below).

### `docs/` (GitHub Pages marketing site)

- `index.html` (landing + downloads), `manual.html`, `releases.html`, `about.html`, `CNAME` (domain TBD, e.g. `audiobud.app`), `favicon.svg` (inline SVG emoji per house rule), `js/version.js` (single source of truth: `AUDIOBUD_VERSION`, populates `[data-version]`/`[data-download]`), `screenshots/`.
- Download URLs follow Tauri artifact names (`AudioBud_X.Y.Z_x64-setup.exe` / `.msi`, plus mac/linux when built).
- Aesthetic: distinct from AudioBash's exact theme but a sibling in the jawn family. Proposed direction in `aesthetic.md` for review — a warm, friendly "bud/companion" identity (the name invites it) rather than cloning the void/brutalist terminal look, since AudioBud is a calm background utility, not a terminal. Final aesthetic is a two-way door; we lock it during the docs-site task.

### Release and changelog process

- Single source of version truth: `src-tauri/tauri.conf.json` `version`, mirrored to `Cargo.toml`, `package.json`, and `docs/js/version.js`.
- Flow: bump versions -> update `CHANGELOG.md` -> `cargo test` + `bun test` green -> commit + tag `vX.Y.Z` -> CI builds a draft release with signed artifacts + `latest.json` -> review -> publish. Joe publishes/merges; never auto-merge.
- Patch-notes template (release body):
  ```markdown
  ## AudioBud vX.Y.Z — YYYY-MM-DD
  ### New
  ### Improved
  ### Fixed
  ### Known issues
  Auto-update installs this automatically. Manual: download below.
  ```

## Testing strategy

TDD throughout, per house rules. Because this is a fork of working software, tests focus on the seams we change, not on re-testing Handy wholesale:

- **Rust unit tests (`cargo test`):** `MODEL_BASE_URL` URL construction (every model resolves to our host); hotkey-config parsing for the default and customized bindings; the registry-toggle helper's enable/disable being reversible (mock the registry write).
- **Frontend tests (`bun test`/Vitest):** settings rendering for engine selection (Parakeet default selected), shortcut-customization UI, the Win+H opt-in toggle state.
- **Build/smoke gate (manual, documented):** `bun run tauri build` produces an installer; install on the 4080; confirm Parakeet (DirectML) and Whisper (Vulkan) both load and transcribe; confirm text injection into a focused app; confirm VC++ redist present.
- **Regression tests:** any bug found gets a failing test first, then the fix (per the bug-fixing workflow).
- The "working prototype" bar for this loop: a locally built AudioBud that, on this machine, captures audio on the default hotkey and types a correct local-Parakeet transcription into the focused app, with at least the seam-level tests above passing.

## Build and toolchain

Confirmed present on Legion: cargo/rustup, node, bun, ffmpeg, gh (as jamditis). To verify/add before building: Vulkan SDK, VS C++ Build Tools (Desktop development with C++), WebView2 (preinstalled on Win 11). First implementation step is an unmodified `bun install` + `bun tauri dev` to prove the upstream builds before we change anything.

## Risks and open questions

- **Win+H (decision):** which of the three options above. Recommend option 1.
- **Model hosting (one-way-ish):** mirror blobs to R2 `pi-transfer` vs point at HuggingFace originals. R2 gives us control and uptime; HF avoids hosting cost/maintenance. Lean R2 for reliability, decide during the de-Handy task.
- **Identifier (one-way door):** propose `tech.amditis.audiobud`. Confirm before first build, since it keys app-data/updater paths.
- **Marketing domain:** `audiobud.app` vs a path under an existing domain. Two-way door; decide at docs-site time.
- **Upstream patched-Tauri pin:** Handy depends on a forked Tauri runtime branch. If we later bump Tauri, we inherit that maintenance. Acceptable for v1; note it.
- **Cross-platform scope:** v1 targets Windows (the actual need). macOS/Linux build configs come from upstream and can be enabled later; not gating the prototype.

## Out of scope for v1 (revisit later)

- Cloud STT providers, agent/LLM transcript post-processing, a custom-built settings UI, custom-dictionary/vocabulary support (note: the related AudioBash bug is filed as jamditis/audiobash#41), and non-Windows release automation.
