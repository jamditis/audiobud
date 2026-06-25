# AudioBud roadmap design spec

- Date: 2026-06-25
- Status: design approved in brainstorming and revised against four research scans (demand, competitive, biasing feasibility, code-signing). Awaiting user review of this written spec before writing-plans.
- Owner: Joe Amditis
- Repo: `github.com/jamditis/audiobud` (public)
- Canonical working copy: `C:\Users\amdit\OneDrive\Desktop\Crimes\playground\audiobud` (legion2025)
- This session's working copy: `C:\Users\Joe Amditis\Desktop\Crimes\playground\audiobud` (a4000, fresh clone)
- Site: https://jamditis.github.io/audiobud/

## Summary

This spec defines AudioBud's release roadmap from the next version through a named `v1.0.0` (plus a post-1.0 bucket), and a public roadmap page on the GitHub Pages site. It turns the scattered backlog — 15 open GitHub issues plus a larger unfiled engineering backlog in `superpowers/DEFERRED-issues.md` — into a sequence of versioned milestones, and publishes that sequence as a `docs/roadmap.html` page styled to the frog/swamp brand.

The plan is grounded in four research scans run 2026-06-25 (see "Research findings"). The headline change from those scans: **on-device AI transcript cleanup** is added as the flagship pre-1.0 differentiator. Two independent scans found it is the single most-praised feature in the category and the one capability upstream Handy lacks — and that a privacy-honest, fully-offline version (local LLM, never the cloud) is open white space nobody occupies.

The roadmap is organized as theme-named release trains: each version has a one-line theme and assigned issues, sequenced toward a `v1.0.0` "cross-platform and stable" gate. Near-term milestones (v0.3.0, v0.3.1, v0.4.0, v0.5.0) carry firm issue lists; the differentiator and feature milestones (v0.6.0, v0.7.0) carry themes with assigned issues that may shift; v1.0.0 is a quality gate.

## Current state

- `v0.2.0` shipped 2026-06-25. Its GitHub milestone is fully closed (12/12).
- The personalization feature (issue #16 — opt-in, on-device learned dictionary) slipped past v0.2.0 and is now the most recent commit on `main` (PR #52), unreleased. Whatever ships as v0.3.0 already contains it.
- The CHANGELOG stops at `0.1.0`. No `0.2.0` entry, no `[Unreleased]` section. Fix as part of v0.3.0.
- No `v0.3.0` GitHub milestone yet. Two milestones exist: `v0.2.0` (closed) and `v0.2.x` (one open issue, #16).
- `superpowers/DEFERRED-issues.md` tracks a separate engineering backlog from the milestone-A audits. Its entries reference **upstream cjpais/Handy issue numbers**, so they are invisible in AudioBud's GitHub milestone view and are not yet filed as AudioBud issues. Its header is stale ("no GitHub repo yet"), and some milestone-B installer items (VC++ CRT, Vulkan loader) already shipped in v0.2.0.

## Goals

- A versioned milestone sequence from v0.3.0 through v1.0.0 that any contributor can read and act on.
- The category's most-praised feature (on-device AI cleanup) added as the flagship differentiator, done in a way that keeps the "nothing leaves the machine" promise.
- The worst default-engine data-loss/crash bugs scheduled early, not buried behind feature releases.
- A public roadmap page that communicates direction and status without needing edits every time an issue moves.
- GitHub milestones created and populated to match this plan.
- The CHANGELOG brought current (0.2.0 + 0.3.0 entries).
- The jawn-atlas wiki entry for AudioBud updated to reflect the live state.

## Non-goals

- This spec plans the roadmap; it does not implement the features or fixes inside each milestone. Each milestone gets its own spec → plan → implementation cycle when it comes up.
- The public roadmap page does not restate issue content or commit to dates. It shows theme and status and links out to GitHub.
- No certificate is purchased and no signing identity provisioned as part of this roadmap work. v0.4.0 schedules that behind an explicit approval gate; v0.3.0 starts only the (free) identity-validation step because of its lead time.
- AI cleanup is local-LLM only. No cloud LLM provider is added for cleanup; doing so would break the privacy promise.

## Organizing principle

Two surfaces, one source.

- **GitHub milestones** are the working tool. They carry firm issue assignments and move constantly. They are where contributors pick up work.
- **The public roadmap page** is a promise. It renders at theme-and-status altitude — version, one-line theme, a status pill, and a link to the matching GitHub milestone. It does not list individual inherited-bug numbers or dates.

Both derive from the milestone breakdown below. Rendering the site at the higher altitude means an issue moving between milestones rarely forces an HTML edit.

Status vocabulary (both surfaces): `shipped`, `in progress`, `planned`, `exploring` (for research bets whose outcome is uncertain).

## Milestone breakdown

### v0.3.0 — personalization (parity) + packaging polish (next release)

Close out the personalization feature already on `main` and tighten the Windows install. Mostly ready; low research risk.

- #16 — finalize and close on-device personalization (already merged via PR #52).
- #53 — view/export/reset learned data while the feature toggle is off.
- #54 — constrain `export_personalization` to a user-approved path (P2 security follow-up).
- #44 — bundle `DirectML.dll` in the NSIS installer for parity with the MSI.
- #45 — third-party license notices for bundled runtime DLLs.
- #51 — check system requirements before install / first run and warn gracefully.
- #43 — overlay drag-nudge clamp should respect safe top/bottom offsets (small bug).
- Housekeeping: write the missing `0.2.0` CHANGELOG entry and the `0.3.0` entry.
- Note (from research): custom vocabulary is now category parity, not a headline — Handy, VoiceInk, superwhisper, and nerd-dictation all ship word replacement. The only novel part is auto-learning, whose demand is unproven versus a manual word list. Finish the feature, but do not over-invest before validating auto-learning.
- Start now (lead-time item): begin Azure Artifact Signing individual identity validation. It is free, but the review takes days to weeks and is a prerequisite for v0.4.0.
- Localization can start here and run continuously in parallel (community-contributed); it is not gated behind a later milestone.

### v0.3.1 — critical stability patch (fast-follow, reshaped)

The worst default-engine data-loss/crash bugs from `DEFERRED-issues.md`, reshaped after the demand scan. These reference upstream Handy issue numbers and must be filed as AudioBud issues first. Each needs a failing test first (TDD).

- Handy #1262 — lowering the history limit / changing retention silently deletes recordings (data loss). Upstream draft fix PR #1311 (open, unmerged) is adaptable: drop the synchronous cleanup; run it lazily; add a confirm/toast.
- Handy #574 — Parakeet (the default engine) fails to load on non-ASCII Windows profile paths. Upstream PR #1187 (open, unmerged) is adaptable: take `&Path` instead of `&str`.
- Handy #921 — paste destroys non-text clipboard contents (data loss, same class as #1262). Upstream fix PR #1013 was **closed without merging**, so we must write it ourselves: save/restore full clipboard content (text + image) via arboard.
- Handy #1213 / #1228 — transcription gets stuck on "transcribing" forever (no timeout), especially after Windows sleep/unlock. #1213 carries the upstream `critical` label (14 comments, two open-but-unmerged fixes). Add a transcription watchdog/timeout that surfaces an error state instead of hanging.
- Moved out: Handy #1332 (long-audio silent drop) is **not** a quick patch — it is a fundamental ONNX/Parakeet ceiling (the maintainer states the ONNX path cannot do long audio). It moves to v0.6.0 as engine work (chunking / Whisper fallback), paired with #1446.

### v0.4.0 — signed and distributable (milestone B)

Remove the SmartScreen "unknown publisher" warning and turn auto-updates back on with correct provenance.

- Sign the Windows installer with **Azure Artifact Signing** (formerly Trusted Signing): ~$10/month, open to US individual developers, cloud HSM with no hardware token, CI-friendly via `trusted-signing-cli` in Tauri's `signCommand`. Identity validation started in v0.3.0. **Certificate/subscription spend is an explicit approval gate — nothing is purchased without Joe's go.**
- Reality check from research: EV certificates no longer grant instant SmartScreen reputation (changed 2024). No signing product gives a brand-new binary instant trust; reputation accrues over downloads under a consistent publisher identity. Signing is still the highest-ROI item in the plan because it is the gate on first-download trust.
- Repoint the updater off `cjpais/Handy`: set `createUpdaterArtifacts: true`; add the missing `plugins.updater` block with AudioBud's own `endpoints` and a freshly generated `pubkey`; flip `UPDATER_FEED_READY` to `true` (backend and frontend gates); repoint the hardcoded `cjpais/Handy/releases/latest` portable-update link.
- #39 — portable mode: bundle the WebView2 fixed runtime for self-contained installs. This also eliminates the missing-VC++-runtime crash-on-launch class (Handy #1489 / #1527).
- Self-host the Bungee/Fredoka fonts (drop the per-launch request to Google; privacy + offline) and tighten the CSP accordingly.
- Native build/packaging validation in CI: a Windows `tauri build` (or a real `cargo build` with the Vulkan SDK) so packaging regressions are caught before release.

### v0.5.0 — stability and reliability

The rest of the inherited reliability backlog plus the accessibility debt.

- Reliability bugs: Handy #1283 (mic-init latency clips the start of speech — a whole-fleet UX papercut surfaced by the demand scan), #502 (wrong-paste race — verify the clipboard before the keystroke), #1423 (wrong tray icon on Windows dark/custom mode), #1509 (onboarding model download can't be cancelled), #1261 (post-process prompt-injection hardening), and the latent `change_binding` cancel dead-fall-through.
- Accessibility (WCAG 2.1 AA follow-ups): Dropdown/ModelDropdown listbox + keyboard semantics, Tooltip `role`/`aria-describedby`, dialog semantics + focus management on the portable-update modal, progressbar roles, focus-visible rings, contrast bumps, remaining `aria-hidden`.
- Localization is not parked here — it runs continuously from v0.3.x.

### v0.6.0 — on-device AI cleanup (flagship) + transcription quality

The differentiator milestone, reframed after the demand, competitive, and feasibility scans.

- **Flagship — optional on-device AI cleanup.** A pluggable local LLM (Ollama or an embedded llama.cpp), off by default, with a few built-in prompts (remove filler words, reformat as an email or chat message, tidy rambling). **Local only — never the cloud**, which is exactly the privacy-honest pattern superwhisper, MacWhisper, OpenWhispr, and BlahST use, and the gap the cloud-based leaders (Wispr Flow, Aqua, Willow) cannot fill. This is the white space: Windows-first + Parakeet default + fully offline cleanup.
- Cheap quality wins: phonetic + vocabulary/N-best-aware correction, extending the existing learned dictionary. This is the text-level "biasing" that captures most of the rare-word recall gain without a decoder swap. Prerequisite: expose an N-best list from the decoder (the current greedy ONNX path returns only 1-best).
- Long-audio fix (moved from v0.3.1): Handy #1332 / #1446 — chunk the audio (the upstream "eager segmented transcription" approach, PR #1515) or auto-fall-back to Whisper past N minutes, and surface the failure instead of dropping silently.
- Optional real-time / streaming preview: the same eager-segmentation engine work enables a partial-results preview (superwhisper headlines this; Handy lacks it). Medium priority within the milestone.
- Exploring — sherpa-onnx ContextGraph + modified-beam-search (#23): a genuine, separately-scoped spike, honestly framed as an *engine-adoption* decision (sherpa-onnx would replace the `transcribe-rs` path for Parakeet), not a decode tweak. It became viable for offline Parakeet-TDT only in Feb 2026 (sherpa-onnx PR #3077). The spike output is a benchmark (beam-search real-time-factor on DirectML / the A4000), a verified Rust hotword example, and a target-word WER delta versus the text-level approach above.
- Reframe/close #22: "decode-time CTC biasing for Parakeet" is technically confused — Parakeet-TDT is a transducer, and the ONNX export has no CTC head. Fold it into #23 or rewrite as "transducer biasing" so a contributor is not sent down a dead end.

### v0.7.0 — interaction and personality

The voice-control and personality bets, sequenced after the AI-cleanup layer they depend on.

- #7 — voice-driven cursor and text-editing commands (validated: Aqua Voice's most-praised feature). This is LLM-with-a-voice-trigger, so it is built on the v0.6.0 cleanup layer and must come after it.
- #14 — in-app tutorial / overview on demand (from About/Settings).
- #11 — animate the mascot's mouth with live input amplitude.
- #8 — customizable mascots/critters, more illustrations, design easter eggs. Personality is a durable, cheap moat in a category nicknamed "Whisper wrappers."
- Swamp/pond-themed custom sidebar tab icons (design backlog).
- #17 — custom wake word: **demoted to `exploring`/someday**, not a scheduled feature. No demand signal across 403 upstream discussions, no competitor in the push-to-talk dictation category ships it, and always-listening conflicts with the privacy model.

### v1.0.0 — cross-platform and stable

The gate that turns a Windows-first prototype into a real 1.0.

- Validate the inherited macOS and Linux code, currently untested. The real landmine is the Wayland first-character-drop bug (Handy #429 — the highest-reacted open upstream issue), not the porting itself.
- Remaining hardening folded in here: custom post-process `base_url` SSRF/API-key exfil (MEDIUM), SIGILL on CPUs without AVX2/FMA3 (document a minimum-CPU requirement), Whisper foreign-exception handling, and lower-priority inherited bugs not pulled earlier.
- Resolve outstanding P1/P2 issues; complete the docs.

### Beyond 1.0 (future bucket)

Demand is real but the use case is adjacent to cursor dictation; scheduled post-1.0 so it does not crowd the dictation-first path.

- Local audio-file transcription (drag-drop a `.wav`/`.mp3`, or a CLI `audiobud file.mp3`) — the single highest-voted upstream request (Handy #299, 48 upvotes). Reuses the existing engines but broadens the product beyond live dictation.
- Speaker diarization / meeting mode (recurring upstream; pairs naturally with file transcription).

## Research findings (2026-06-25)

Four parallel subagent scans grounded the revisions above. Findings are weighted by convergence — where two independent scans agreed, the change is treated as high-confidence.

- **Demand scan (upstream cjpais/Handy, 24.9k stars, 403 discussions, 174 issues).** Feature requests live in Discussions, not Issues. Strongest signals: local audio-file transcription (#299, 48 upvotes — the single highest); model support (75 discussions); LLM post-processing/cleanup (32 discussions, flagship #168); real-time preview (11 discussions); custom vocabulary (10 discussions — already parity). Handy is under an explicit feature freeze ("bug fixes are the top priority"). Inherited-bug pain: #1213 is `critical`-labeled (14 comments, two unmerged fixes); #1332 has no fix and is a Parakeet/ONNX ceiling; #921's fix was closed unmerged.
- **Competitive scan (Whispering, WhisperWriter, Vibe, VoiceInk, OpenWhispr, Buzz, superwhisper, MacWhisper, Aqua, Wispr Flow, Talon, etc.).** Custom vocabulary is table-stakes. The most-praised differentiator across the field is AI transcript cleanup, which Handy lacks; the privacy-honest apps do it with a local LLM (Ollama / llama.cpp). Wake word is shipped by essentially no one in this category. Windows-first + Parakeet + offline cleanup is open white space. Roadmap-page pattern: theme-level Now/Next/Later or Shipped/In-progress/Planned/Exploring + a changelog section, no dates (models: GitHub's public roadmap; superwhisper's changelog).
- **Biasing feasibility scan.** Issue #22 (CTC biasing on Parakeet) is technically mis-specified — Parakeet-TDT is a transducer with no CTC head in the ONNX export. Issue #23 (sherpa-onnx ContextGraph) is feasible but means adopting sherpa-onnx as the engine; viable for offline Parakeet-TDT only since Feb 2026. Text-level phonetic + N-best/vocabulary-aware correction captures most of the benefit cheaply and overlaps with the AI-cleanup feature.
- **Code-signing scan.** EV no longer buys instant SmartScreen reputation (2024 change); nothing does. Azure Artifact Signing reopened to US/Canada individuals (~$10/month, no hardware token, CI-friendly) — the best fit; identity validation has a days-to-weeks lead time. Runner-up: Certum open-source cert (own name, ~$108/year). Fallback: SignPath (free OSS, but signs under "SignPath Foundation"). The repo's updater config (`createUpdaterArtifacts: false`, no `plugins.updater` block, `UPDATER_FEED_READY = false`) must be repointed off upstream for v0.4.0.

## Treatment of the DEFERRED-issues backlog

`superpowers/DEFERRED-issues.md` stays the working ledger, but the roadmap relies on GitHub issues, so:

- The items pulled into v0.3.1 (#1262, #574, #921, #1213) are filed as AudioBud GitHub issues first, labeled (`bug`, severity), and assigned to the v0.3.1 milestone, with the new AudioBud issue number noted inline in the ledger.
- The reliability, accessibility, and localization items assigned to v0.5.0 are filed in a batch when v0.5.0 is planned, to avoid a wall of stale issues now.
- The ledger's stale header is corrected (the repo now exists) and its already-shipped milestone-B installer items (VC++ CRT, Vulkan loader) are marked resolved against the v0.2.0 commits.

## Public roadmap page design

A new `docs/roadmap.html`, styled from the existing `docs/styles.css`, linked from the main `docs/index.html` nav.

- Two parts (the pattern the competitive scan found works): a theme-level status board on top (Shipped / In progress / Planned / Exploring), and a changelog-style "what shipped" list below (version + date, action-verb phrasing).
- The status board renders each milestone as a card: version number, theme one-liner, a status pill, and a link to the matching GitHub milestone.
- `v0.1.0` and `v0.2.0` appear as `shipped`, so the page doubles as a changelog-at-a-glance.
- Theme-and-status altitude only — no inherited-bug numbers, no dates, no individual issue IDs restated. Issue detail lives on GitHub, linked.
- Brand option: the frog/swamp identity allows playful status labels over the same legible vocabulary (e.g. "in the pond" = in progress, "next in the lily pads" = planned, "someday in the bog" = exploring). Optional; the underlying status words stay clear.

## GitHub milestone implementation notes

- Create milestones: `v0.3.0`, `v0.3.1`, `v0.4.0`, `v0.5.0`, `v0.6.0`, `v0.7.0`, `v1.0.0`, each with its one-line theme as the description.
- Reassign existing open issues: #16/#43/#44/#45/#51/#53/#54 → v0.3.0; #39 → v0.4.0; #22/#23 → v0.6.0 (with #22 reframed or closed); #7/#8/#11/#14 → v0.7.0; #17 → no milestone, labeled `exploring`.
- Reconcile the existing `v0.2.x` milestone: its only issue (#16) moves to v0.3.0; close or retire `v0.2.x`.
- File the v0.3.1 stability bugs (#1262, #574, #921, #1213 equivalents) as new AudioBud issues and assign them.
- Add a new tracking issue for the v0.6.0 flagship (on-device AI cleanup) since it is not yet in the backlog.

## Wiki update

After the milestones and roadmap page exist, update `jawn-atlas/bundle/legion2025/projects/audiobud.md` to reflect the live state: current version past v0.2.0, the new roadmap, and the existence of an a4000 working copy. Per the OKF federation contract this file lives in legion2025's bundle; coordinate with legion2025 (no live peer at time of writing) and do not commit the wiki without Joe's go.

## Decisions made

Round one (scope framing):

1. Horizon: plan through a named `v1.0.0` (plus a post-1.0 bucket).
2. v0.3.0 headline: personalization + packaging polish.
3. Code-signing: its own milestone, `v0.4.0` (milestone B).
4. Roadmap site: a dedicated `docs/roadmap.html` page.
5. Inherited bug backlog: pull the worst default-engine bugs forward (v0.3.1), add a dedicated stability milestone (v0.5.0), fold remaining hardening into v1.0.

Round two (research-grounded):

6. Add on-device AI cleanup as the flagship pre-1.0 milestone (v0.6.0), local LLM only.
7. Wake word (#17): demote to `exploring`, not a scheduled feature.
8. Local audio-file transcription: scheduled post-1.0 (future bucket), not pre-1.0.
9. Reshape v0.3.1: #1262 + #574 + #921 + #1213 watchdog; move #1332 to v0.6.0 engine work.

## Open question for review

- Milestone count: seven numbered milestones to v1.0 plus a future bucket. v0.6.0 is the heaviest (flagship AI cleanup + quality + long-audio fix + streaming). If it is too large, the clean split is a v0.6.x that carries the long-audio fix and streaming preview separately from the AI-cleanup flagship.
