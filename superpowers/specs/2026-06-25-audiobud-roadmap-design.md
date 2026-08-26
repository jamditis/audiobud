# AudioBud roadmap design spec

- Date: 2026-06-25
- Revised: 2026-08-26, to match the actual GitHub milestones (issue #197). See "Revision history" at the end.
- Status: superseded by GitHub milestones for the current sequence. This file records the plan's origin and reasoning; GitHub milestones are the live source of truth for issue assignment.
- Owner: Joe Amditis
- Repo: `github.com/jamditis/audiobud` (public)
- Site: https://jamditis.github.io/audiobud/

## Summary

This spec defines AudioBud's release roadmap and a public roadmap page on the GitHub Pages site. It turns the backlog into a sequence of versioned milestones and publishes that sequence as `docs/roadmap.html`, styled to the frog/swamp brand.

The original 2026-06-25 plan proposed seven numbered milestones (v0.3.0 through v0.7.0, plus v1.0.0). Real work changed that sequence: a release-integrity milestone (v0.4.1) was inserted that the original plan did not anticipate, the stability/reliability milestone moved from v0.5.0 to v0.8.0, and v0.5.0 was reassigned to output routing and window targeting. The **"Milestone breakdown"** section below describes the milestones as they exist on GitHub today. The **"Original plan (2026-06-25) and how it diverged"** section keeps the original reasoning for reference, marked against what actually happened.

## Organizing principle

Two surfaces, one source.

- **GitHub milestones** are the working tool. They carry firm issue assignments and move constantly. They are where contributors pick up work.
- **The public roadmap page** is a promise. It renders at theme-and-status altitude — version, one-line theme, a status pill, and a link to the matching GitHub milestone. It does not list individual issue numbers or dates.

Both derive from the milestone breakdown below. Rendering the site at the higher altitude means an issue moving between milestones rarely forces an HTML edit.

Status vocabulary (both surfaces): `shipped`, `in progress`, `planned`, `exploring` (for research bets whose outcome is uncertain).

## Milestone breakdown (current, 2026-08-26)

### Shipped: v0.1.0 through v0.4.4

- `v0.1.0` — first working local prototype, forked from Handy with the frog/swamp identity.
- `v0.2.0` — self-contained installer, portable build, overlay placement grid, tray quick-toggles.
- `v0.3.0` through `v0.3.4` — on-device personalization, a stability patch for the default engine, translation completeness, app naming, and spoken-number formatting.
- `v0.4.0` — signed and timestamped Windows installers, verified packaged executables, a protected release pipeline, public publisher-domain pages.
- `v0.4.1` (milestone, closed 2026-08-01) — **release integrity**: make CI compile and test what actually ships, publish checkable provenance for every artifact, and turn on auto-update so a bad build can be fixed after it leaves. This milestone did not exist in the original 2026-06-25 plan; it was created and closed between v0.4.0 and v0.5.0. It shipped as CHANGELOG versions 0.4.1 through 0.4.4: SHA256SUMS.txt and build provenance attestations, CI that compiles and tests the real Windows transcription engine, a signed updater feed pointed at AudioBud's own releases (not upstream Handy's), and self-hosted fonts (closing out a v0.4.0 commitment that had been missed — see "What v0.4.0 missed" below). A handful of its issues carried over into later milestones rather than closing with it; the milestone itself is closed.

### v0.4.1 — release integrity (closed)

Covered above under "Shipped." Kept here as its own heading because it is the milestone the original plan did not predict.

### v0.5.0 — output routing and window targeting (in progress)

Pin dictation output to a window the user chose, and keep working somewhere else while it is pinned. This did not exist in the original plan, which had reliability/accessibility work in this version slot. The real v0.5.0 is a routing feature, not a stability release.

- Epic #119 — output routing and window targeting.
- Epic #142 — the per-dictation context: a value that carries the chosen destination from recording start through to paste time. This is the prerequisite the rest of the milestone builds on.
- #120 — target-lock: pin transcription output to a chosen window.
- #121 — show the locked output target and offer a quick unlock.
- #122 — queue and serialize dictations when the locked target isn't ready.
- #123 — per-application output profiles: paste method, auto-submit, and formatting.
- #124 (closed) — on-the-fly picker: send one dictation to a chosen window.
- #160 through #166 — the plumbing this depends on: retiring redundant raw-output recomputation, moving the paste off the Tauri main thread, giving `PasteMethod` a focus-capability model, adding the missing `windows` crate features for focus-borrow, excluding AudioBud's own windows from target capture, reporting which window received the transcript, and collapsing per-setting boilerplate.
- #197 — this spec-revision issue.
- #254 — validate window identity on the Windows backend so a recycled window handle cannot receive a pinned paste meant for a different window.
- #255 — wire the output-target lock indicator to the backend and all three surfaces (tray, overlay, settings).
- #259 — wire the one-shot picker end to end: enumerate windows, render the overlay, deliver the pick.
- #228 (closed) — produce and host the Microsoft Store candidate MSI. Housekeeping riding along in this milestone rather than feature work.

**No design spec exists yet for output routing.** Issue #197 flags this: 14 issues carry the feature with no shared design document behind them. Writing that spec is separate follow-up work, not part of this roadmap revision.

### v0.6.0 — transcription quality (planned)

Reframed from the original plan's "on-device AI cleanup (flagship) + transcription quality" into transcription quality as the whole theme. The AI-cleanup flagship (#59) is one item within it, not the milestone's sole identity.

- Epic #143 — one text pipeline, one tokenizer: the different formatting stages (raw punctuation, personalization, post-process) currently disagree with each other about token boundaries.
- #167 — define the shared token-boundary contract in the text pipeline.
- #168 — thread a language signal through the text pipeline.
- #59 — optional on-device AI transcript cleanup, local LLM only (never the cloud). The flagship differentiator from the original research scans.
- #66 — raw transcript mode: interpret spoken punctuation and format numbers, time, and money. Labeled `urgent`, sitting behind v0.4.1 and v0.5.0 in the sequence. **Open question, not yet decided:** issue comments on #66 (2026-07-21) ask whether this should be pulled forward given the `urgent` label, or whether the label is stale. This spec does not resolve that question — it is Joe's call, tracked on #66 and referenced from #197.
- #16 — opt-in on-device personalization (learn from saved transcripts to improve recognition).
- #67 — capture transcript corrections into the learned replacement set.
- #23 — evaluate sherpa-onnx ContextGraph + modified-beam-search for decode-time biasing (exploring; a genuine engine-adoption decision, not a decode tweak).
- #22 (closed) — the earlier, technically confused framing of #23 ("CTC biasing" on a transducer model with no CTC head). Closed in favor of #23's corrected framing.
- #108, #107 — Parakeet decoding artifacts (stray first-letter tokens; repeated letters mid-acronym).
- #112 — add the missing built-in post-processing prompts.
- #126 — extractor bug: learns global replacements from non-English grammar edits.
- #117 — clock-time stitching misses times that open with a bracket or quote.
- #170 — evaluate adopting transcribe.cpp.
- #200 — epic: streaming partial preview (composition overlay; paste stays atomic). This is the "real-time preview" item from the original plan, now its own epic rather than a v0.6.0 sub-bullet.
- #169 (closed) — the long-audio silent-drop bug the original plan moved here from v0.3.1. Fixed.
- #114 (closed) — a `format_numbers` line-break bug.
- Additional planned items: #189 (snippet/text-replacement expansion), #190 (deterministic filler-word removal toggle), #192 (self-correction handling — "no wait, Friday"), #195 (surface that larger Whisper models hallucinate more on silence), #198 (learning-based snippet/vocabulary suggestions), #213 (harden post-process prompts against spoken injection — the prompt-injection item the original plan placed in v0.5.0), #214 (journalism domain vocabulary pack), #215 (import dictionary/snippets from a file), #220 (skip post-process on empty transcript).

### v0.7.0 — interaction and personality (planned)

Close to the original plan's v0.7.0, with a few voice-command and workflow issues folded in.

- #7 — voice-driven cursor control and text-editing commands.
- #181 — wire the voice command parser into the dictation pipeline.
- #14 (closed) — in-app tutorial/overview on demand.
- #11 — animate the mascot's mouth with live input amplitude.
- #8 — customizable mascots/critters, more illustrations, design easter eggs.
- #105 — debug mode has no in-app affordance (Ctrl+Shift+D is the only way to turn it on).
- #203 — named dictation modes (prompt packs, optional mode hotkeys).
- #207 — history search/filter and re-paste into the focused or locked target.
- #208 — optional review-before-paste (IME-style commit).
- #210 — opt-in screen/selection/clipboard context for local cleanup.
- #17 — custom wake word. Still `exploring`/someday per the original plan's reasoning (no demand signal, no category precedent, conflicts with the privacy model), tracked against the Future bucket rather than v0.7.0's committed list.

### v0.8.0 — reliability and accessibility (planned)

This is the milestone the original plan called v0.5.0. The rename reflects where it actually landed in sequence, after output routing and transcription quality rather than before them.

- Epic #144 — accessibility pass (WCAG 2.1 AA follow-ups: listbox/keyboard semantics, tooltip roles, dialog focus management, progressbar roles, focus-visible rings, contrast, `aria-hidden` cleanup).
- Reliability bugs: #171 (`audio.rs` panics on lock poison in 38 places), #79 (transcription watchdog refactor to async `tokio::time::timeout`), #90 (a slow cold model load can trip the watchdog and refuse the retry), #204 (verify clipboard before the paste keystroke — the wrong-paste race from the original plan), #201 (Bluetooth/post-sleep mic stream rebuild when the device vanishes), #202 (cold-start: first hotkey after launch fails to record), #194 (crash recovery: auto-save the recording until paste succeeds), #219 (tray icon follows Windows light/dark theme), #221 (always-on/lazy-close discoverability), #234 (overlay level bars ignore `prefers-reduced-motion`), #193 (hotkey registration diagnostics and conflict warnings), #209 (cold-vs-warm model latency indicator), #211 (quiet/whispered speech mode), #212 (opt-in pre-ASR noise reduction), #216 (never-store-audio option), #172 (closed — dropped progress events could stall the model-download UI), #217 (closed — wired cancel on the onboarding model download, the original plan's "can't be cancelled" item).
- Path/UTF-8 hardening: #80 (whisper backend panics on non-UTF-8 model paths), #81 (audit `to_string_lossy()`/`to_str()` on paths crossing to the frontend), #75 (history cleanup deletes files without the safe-filename guard), #76 (history-trim confirmation UI), #77 (history cleanup miscounts deletions in logs), #72 (preflight P2 hardening follow-ups).
- Localization: #100 (count-interpolating strings disagree with their count in some locales).
- #251 — backend transcription errors reach the UI as English-only toast text.
- #252 — the transcription success path can unload another request's engine after a refused restore.

### v1.0.0 — cross-platform decision and hardening (planned)

Reframed from the original plan's "cross-platform and stable" into an explicit decision gate, per issue #179: **validate macOS and Linux, or shed them.** Right now neither platform is tested.

- Epic #145 — finish the de-fork and decide the platform story.
- #179 — the decision itself: validate macOS/Linux, or hand them back to upstream.
- #178 — rebrand or remove the Nix packaging.
- #177 — replace the Linux tray artwork.
- #176 — migrate the `handy_keys` persisted settings value.
- #175 — rename the Rust lib target, log file, and recording prefix.
- #174 — `AGENTS.md` platform notes and the model host are stale.
- #173 — the debug panel shows a settings directory that does not exist.
- #83 — Linux: restoring a cut file list loses the KDE/GNOME cut-selection marker.
- #94 — `CONTRIBUTING_TRANSLATIONS.md` still documents Handy, not AudioBud.
- #235 — the guide copy and the README describe the same behavior with no link or drift check.
- #245 — a blunt "just give me an .exe" issue; folded in here as a distribution-friction signal for the platform decision, not a scheduled feature.

### Future (not milestoned to a version)

- Epic #206 — local file and audio transcription (drag-drop, CLI). The original plan's highest-voted post-1.0 item (Handy #299 upstream, 48 upvotes), unchanged in priority.
- #205 — speaker diarization / interview mode (phase 2 of file transcription).
- #17 — custom wake word (see v0.7.0 above; tracked here as the `exploring` bucket item, not committed to any numbered version).
- #227 — plan paid feature tiers and a Stripe path.
- #226 — evaluate Microsoft Store distribution for SmartScreen friction. (The Store MSI groundwork, #228, already shipped inside v0.5.0 — this issue is the follow-on evaluation of going further.)
- #218 — research: Windows IDE/project vocabulary biasing.

## What v0.4.0 missed

The original plan's v0.4.0 section committed to self-hosting the Bungee/Fredoka fonts. `v0.4.0` shipped with zero GitHub tracking issues, so that commitment was not tracked and did not ship with it. Issue #196 ("Self-host Google Fonts — committed for v0.4.0, never done") filed the gap and closed under the v0.4.1 release-integrity milestone instead. The lesson carried into how later milestones are run: every version from v0.4.1 onward has had GitHub issues assigned to it before work started, not after.

## Original plan (2026-06-25) and how it diverged

The original spec (research-grounded, four scans on 2026-06-25: demand, competitive, biasing feasibility, code-signing) proposed this sequence:

1. v0.3.0 — personalization + packaging polish.
2. v0.3.1 — critical stability patch (inherited default-engine bugs).
3. v0.4.0 — signed and distributable (milestone B).
4. v0.5.0 — stability and reliability (the rest of the inherited backlog, plus accessibility).
5. v0.6.0 — on-device AI cleanup (flagship) + transcription quality.
6. v0.7.0 — interaction and personality.
7. v1.0.0 — cross-platform and stable.
8. Beyond 1.0 — local file transcription, speaker diarization.

What actually happened, and why:

- **v0.3.0 through v0.4.0 shipped close to plan.** v0.3.1's inherited-bug list, v0.4.0's signing and updater work, and the personalization feature all landed roughly as scoped.
- **v0.4.1 was inserted.** The original plan did not anticipate a dedicated release-integrity milestone. It emerged from the same v0.4.0 push: once installers were signed, the gaps in CI coverage, provenance, and the updater feed became their own body of work, closed 2026-08-01.
- **v0.5.0 was reassigned to output routing**, not stability/reliability. This was a deliberate reprioritization, not drift by neglect — output routing (pin dictation to a chosen window) became the next user-facing feature after distribution was solid. The original plan's stability/reliability content did not disappear; it moved to v0.8.0.
- **v0.6.0 kept its identity** (AI cleanup + transcription quality) but is now positioned after output routing rather than immediately following stability work, and folds in the text-pipeline unification (#143) that the original plan treated as background context rather than a scheduled epic.
- **v0.7.0 kept its identity** (interaction and personality), with several voice-command and workflow issues (#181, #203, #207, #208, #210) added that were not itemized in the original plan.
- **v1.0.0 was reframed from a stability gate into an explicit decision** (#179: validate macOS/Linux, or shed them), reflecting that the platforms remain untested rather than merely unhardened.
- **The future bucket grew** to include paid tiers (#227) and a Microsoft Store distribution evaluation (#226), neither anticipated in the original research scans.

## Public roadmap page design

`docs/roadmap.html`, styled from `docs/styles.css`, linked from the main `docs/index.html` nav. This section is unchanged from the original plan and reflects what is live today:

- Two parts: a theme-level status board (Shipped / In progress / Planned / Exploring), and a changelog-style "what shipped" list below it (version + date, action-verb phrasing).
- The status board renders each milestone as a card: version number, theme one-liner, a status pill, and a link to the matching GitHub milestone.
- Theme-and-status altitude only — no issue numbers, no dates, no individual issue content restated. Issue detail lives on GitHub, linked.
- The page currently shows v0.5.0 through v1.0.0 as `planned` cards and every version through v0.4.4 as `shipped`, which matches the milestone breakdown above.

## GitHub milestone bookkeeping notes

- Milestones live on GitHub: `v0.2.0` and `v0.2.x` (closed, retired into v0.3.0), `v0.3.0` through `v0.4.1` (closed/shipped), `v0.5.0` (open, in progress), `v0.6.0`, `v0.7.0`, `v0.8.0`, `v1.0.0` (open, planned), and an unmilestoned `Future` label/bucket for exploring-stage items.
- This spec does not reassign issues between milestones. It describes the assignments as they exist on GitHub at revision time (2026-08-26). Milestone membership is expected to keep moving; treat the numbered list above as a snapshot, and GitHub milestones as the live answer when the two disagree.

## Revision history

- 2026-06-25 — original spec, research-grounded, seven milestones plus a future bucket.
- 2026-08-26 — revised per issue #197 to match the actual milestone structure: v0.4.1 inserted, v0.5.0 reassigned to output routing, reliability/accessibility moved to v0.8.0, v1.0.0 reframed as a platform decision gate. `docs/roadmap.html` was already close to this structure and needed no card-level rewrite; see the commit that accompanies this revision for the specific corrections made to it.
