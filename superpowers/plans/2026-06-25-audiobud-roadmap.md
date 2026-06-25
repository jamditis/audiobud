# AudioBud roadmap setup implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stand up the AudioBud roadmap as concrete artifacts — seven GitHub milestones, reassigned and newly filed issues, a public `docs/roadmap.html` page, a current CHANGELOG, and a cleaned-up deferred-issues ledger — so contributors and users can see what ships when.

**Architecture:** This is roadmap _setup_, not feature implementation. Work is mostly `gh` CLI calls against `jamditis/audiobud`, one static HTML page added to the existing zero-build Pages site under `docs/`, and Markdown edits. No app code, no Rust, no React. Each task ends with a verification command (a `gh` query, a file check, or a browser render) standing in for a unit test, since GitHub state and static HTML are not unit-testable.

**Tech Stack:** GitHub CLI (`gh`) + REST API, static HTML/CSS matching `docs/styles.css`, Keep a Changelog 1.1.0 format, Markdown.

## Global Constraints

- Sentence case everywhere — milestone titles' descriptions, page headings, CHANGELOG headings, issue titles. Never title case. (Copied verbatim from the user's standing rule and CCM style.)
- The public roadmap page shows theme and status only — no dates, no individual inherited-bug numbers, no restated issue bodies. Issue detail lives on GitHub, linked.
- AI cleanup is local-LLM only. Any issue/page copy about it must not imply a cloud provider.
- No AI-attribution branding in commits, PRs, issues, code, or docs.
- Conventional commit prefixes (`feat:`, `fix:`, `docs:`, `chore:`). Message says _why_, not _what_.
- Before filing any issue, read `.github/ISSUE_TEMPLATE/bug_report.md` and fill every section; bugs get `bug`, feature/enhancement issues get `enhancement` (AudioBud has Discussions disabled). Before opening the PR, read `.github/PULL_REQUEST_TEMPLATE.md` and complete each section.
- Do not push or open the PR (Task 11) or commit the wiki (Task 10) without Joe's explicit go.
- Repo: `jamditis/audiobud`. Milestone themes are copied verbatim from the approved spec `superpowers/specs/2026-06-25-audiobud-roadmap-design.md`.

---

### Task 1: Create the working branch and commit the approved spec

**Files:**

- Modify: git branch state (currently on `main`)
- Commit: `superpowers/specs/2026-06-25-audiobud-roadmap-design.md` (already written, uncommitted)

**Interfaces:**

- Produces: branch `joe/roadmap-setup` that all later commits land on.

- [ ] **Step 1: Confirm clean tree and current branch**

Run: `git -C "$REPO" status -s && git -C "$REPO" branch --show-current`
Expected: only `superpowers/specs/2026-06-25-audiobud-roadmap-design.md` (and this plan file) untracked; branch `main`.

- [ ] **Step 2: Create and switch to the branch**

Run: `git -C "$REPO" switch -c joe/roadmap-setup`
Expected: "Switched to a new branch 'joe/roadmap-setup'".

- [ ] **Step 3: Commit the spec and plan**

```bash
git -C "$REPO" add superpowers/specs/2026-06-25-audiobud-roadmap-design.md superpowers/plans/2026-06-25-audiobud-roadmap.md
git -C "$REPO" commit -m "docs(roadmap): add roadmap design spec and setup plan"
```

- [ ] **Step 4: Verify the commit**

Run: `git -C "$REPO" log --oneline -1`
Expected: the `docs(roadmap): add roadmap design spec and setup plan` commit at HEAD.

---

### Task 2: Create the seven GitHub milestones

**Files:** none (GitHub state via `gh api`).

**Interfaces:**

- Produces: milestones `v0.3.0`, `v0.3.1`, `v0.4.0`, `v0.5.0`, `v0.6.0`, `v0.7.0`, `v1.0.0`, each `open` with a one-line theme description. Later tasks assign issues to these by title.

- [ ] **Step 1: Check existing milestones first (idempotency)**

Run: `gh api repos/jamditis/audiobud/milestones --jq '.[] | "\(.number) \(.title) \(.state)"'`
Expected: shows `v0.2.0` (closed) and `v0.2.x` (open). If any `v0.3.0`+ already exist, skip creating those in Step 2.

- [ ] **Step 2: Create each milestone**

```bash
gh api repos/jamditis/audiobud/milestones -f title="v0.3.0" -f state="open" \
  -f description="Personalization (parity) and Windows packaging polish: finalize the on-device learned dictionary and tighten the installer."
gh api repos/jamditis/audiobud/milestones -f title="v0.3.1" -f state="open" \
  -f description="Critical stability patch: default-engine data-loss and crash fixes (history auto-delete, non-ASCII paths, clipboard, stuck-transcribing watchdog)."
gh api repos/jamditis/audiobud/milestones -f title="v0.4.0" -f state="open" \
  -f description="Signed and distributable (milestone B): Authenticode-signed installer, repointed updater, and a self-contained install."
gh api repos/jamditis/audiobud/milestones -f title="v0.5.0" -f state="open" \
  -f description="Stability and reliability: the remaining inherited reliability bugs and the accessibility pass."
gh api repos/jamditis/audiobud/milestones -f title="v0.6.0" -f state="open" \
  -f description="On-device AI cleanup (flagship) and transcription quality: local-LLM cleanup, cheaper accuracy wins, the long-audio fix, and a streaming preview."
gh api repos/jamditis/audiobud/milestones -f title="v0.7.0" -f state="open" \
  -f description="Interaction and personality: voice-driven editing commands, an in-app tutorial, and mascot personality."
gh api repos/jamditis/audiobud/milestones -f title="v1.0.0" -f state="open" \
  -f description="Cross-platform and stable: validate macOS and Linux, finish remaining hardening, and complete the docs."
```

- [ ] **Step 3: Verify all seven exist with descriptions**

Run: `gh api repos/jamditis/audiobud/milestones --jq '.[] | "\(.title): \(.description)"'`
Expected: the seven new titles each printed with the theme text above.

- [ ] **Step 4: Commit a record of the milestone map**

No file changed yet; the milestone map is captured in the spec. Skip the commit for this task (GitHub-only state). Proceed to Task 3.

---

### Task 3: Reassign existing open issues, reconcile v0.2.x, demote #17, reframe #22

**Files:** none (GitHub state).

**Interfaces:**

- Consumes: the milestones from Task 2.
- Produces: every existing open issue assigned to its milestone (or deliberately unassigned for #17).

- [ ] **Step 1: Create the `exploring` label if missing**

Run: `gh label list --repo jamditis/audiobud | grep -i exploring || gh label create exploring --repo jamditis/audiobud --description "On the roadmap as a research bet, not a scheduled feature" --color BFD4C2`
Expected: label exists after this step.

- [ ] **Step 2: Assign issues to milestones**

```bash
for n in 16 43 44 45 51 53 54; do gh issue edit $n --repo jamditis/audiobud --milestone "v0.3.0"; done
gh issue edit 39 --repo jamditis/audiobud --milestone "v0.4.0"
for n in 22 23; do gh issue edit $n --repo jamditis/audiobud --milestone "v0.6.0"; done
for n in 7 8 11 14; do gh issue edit $n --repo jamditis/audiobud --milestone "v0.7.0"; done
```

- [ ] **Step 3: Demote #17 (wake word) to exploring, no milestone**

```bash
gh issue edit 17 --repo jamditis/audiobud --add-label "exploring" --remove-milestone
```

- [ ] **Step 4: Reframe #22 (CTC biasing is technically wrong for a transducer)**

```bash
gh issue comment 22 --repo jamditis/audiobud --body "Roadmap note (2026-06-25): scheduled under v0.6.0, but the framing needs correcting. Parakeet-TDT is a token-and-duration transducer, and the ONNX export ships no CTC head, so \"decode-time CTC biasing\" does not map onto this model. The realistic path is transducer-level biasing, which is what #23 (sherpa-onnx ContextGraph + modified-beam-search) evaluates. Treat this issue as folded into #23 unless a separate CTC-head export is pursued."
gh issue edit 22 --repo jamditis/audiobud --title "Spike: transducer-level decode-time biasing for Parakeet (was: CTC biasing)"
```

- [ ] **Step 5: Move #16 off v0.2.x and retire that milestone**

Step 2 already set #16 to v0.3.0 (overwrites v0.2.x). Now close the empty milestone:

```bash
V2X=$(gh api repos/jamditis/audiobud/milestones --jq '.[] | select(.title=="v0.2.x") | .number')
gh api -X PATCH repos/jamditis/audiobud/milestones/$V2X -f state="closed"
```

- [ ] **Step 6: Verify assignments**

Run: `gh issue list --repo jamditis/audiobud --state open --json number,milestone,labels --jq '.[] | "#\(.number) [\(.milestone.title // "none")] \(.labels|map(.name)|join(","))"' | sort`
Expected: #16/#43/#44/#45/#51/#53/#54 on v0.3.0; #39 on v0.4.0; #22/#23 on v0.6.0; #7/#8/#11/#14 on v0.7.0; #17 with `exploring` and no milestone.

---

### Task 4: File the four v0.3.1 stability bugs and the AI-cleanup tracking issue

**Files:** none yet (issue numbers recorded into the ledger in Task 8).

**Interfaces:**

- Consumes: the `v0.3.1` and `v0.6.0` milestones.
- Produces: five new AudioBud issues; their numbers are needed by Task 8.

- [ ] **Step 1: Read the issue template**

Run: `cat "$REPO/.github/ISSUE_TEMPLATE/bug_report.md"`
Expected: the bug template sections. Fill all of them in each bug body below; adapt headings to match the template if they differ.

- [ ] **Step 2: File the four data-loss/crash bugs**

Each body cites the upstream Handy issue, the file:line from `superpowers/DEFERRED-issues.md`, and the fix approach. Example for the first; mirror the structure for the others using the ledger's "Fix-now" entries.

```bash
gh issue create --repo jamditis/audiobud --label bug --milestone "v0.3.1" \
  --title "History limit / auto-delete silently destroys recordings (data loss)" \
  --body "## Summary
Lowering the history limit or changing retention immediately deletes unsaved recordings and WAVs with no warning.

## Source
Inherited from cjpais/Handy #1262 (upstream draft fix PR #1311, open/unmerged). In our tree: src-tauri/src/commands/history.rs:121 and :150 call cleanup_old_entries() synchronously -> history.rs:330 delete_entries_and_files -> fs::remove_file.

## Fix
Drop the two immediate cleanup calls (let cleanup run lazily via save_entry()); add a confirmation/toast. Write a failing test first.

## Severity
Data loss, default path."
```

Repeat for: Parakeet non-ASCII Windows path crash (Handy #574, audio.rs:121/:282, take `&Path`), clipboard paste destroys non-text contents (Handy #921, clipboard.rs:24/:66-76, save/restore full ClipboardContent — upstream fix #1013 was closed unmerged so we write it), and stuck-on-transcribing watchdog (Handy #1213/#1228, actions.rs:548 no timeout, add a watchdog that surfaces an error state).

- [ ] **Step 3: File the AI-cleanup flagship tracking issue**

```bash
gh issue create --repo jamditis/audiobud --label enhancement --milestone "v0.6.0" \
  --title "Optional on-device AI transcript cleanup (local LLM only)" \
  --body "## What
An optional, off-by-default cleanup pass over the transcript using a local LLM (Ollama or an embedded llama.cpp), with a few built-in prompts (remove filler words, reformat as an email or chat message, tidy rambling).

## Why
The most-praised feature across the dictation category (Wispr Flow, superwhisper, VoiceInk, Aqua), and the one capability upstream Handy lacks. Privacy-honest competitors do it with a local LLM, never the cloud.

## Constraint
Local only. No cloud LLM provider for cleanup — that would break the no-audio/text-leaves-the-machine promise. Off by default; user picks the local model.

## Notes
Flagship of the v0.6.0 milestone. Pairs with the cheaper text-level vocabulary/N-best correction also scheduled there. Voice-editing commands (#7) build on this layer."
```

- [ ] **Step 4: Verify and capture the new numbers**

Run: `gh issue list --repo jamditis/audiobud --state open --milestone "v0.3.1" --json number,title; gh issue list --repo jamditis/audiobud --state open --milestone "v0.6.0" --json number,title`
Expected: four bugs under v0.3.1, the AI-cleanup issue (plus #22/#23) under v0.6.0. Record the four bug numbers for Task 8.

---

### Task 5: Bring the CHANGELOG current (0.2.0 entry + unreleased 0.3.0 section)

**Files:**

- Modify: `CHANGELOG.md` (currently stops at 0.1.0)

**Interfaces:**

- Consumes: nothing.
- Produces: a documented 0.2.0 entry and an `[Unreleased]` / 0.3.0 section that the phase-2 v0.3.0 work appends to.

- [ ] **Step 1: Gather the real 0.2.0 content from git**

Run: `git -C "$REPO" log v0.1.0..v0.2.0 --oneline`
Expected: the commits between the two tags — the source material for the 0.2.0 entry (runtime DLL bundling, word-replacement overflow fix, overlay reposition, dictionary guidance, release-asset/pages work).

- [ ] **Step 2: Insert the unreleased and 0.2.0 sections**

Insert immediately above the `## 0.1.0 (milestone A) - 2026-06-21` heading. Use sentence case, Keep a Changelog subsections. Example shape (fill Added/Fixed from Step 1's commit list):

```markdown
## [Unreleased] - 0.3.0

In progress; see the [roadmap](https://jamditis.github.io/audiobud/roadmap.html).

## 0.2.0 - 2026-06-25

### Added

- Opt-in, on-device personalization: a learnable dictionary mined from saved transcripts (#16).

### Fixed

- Bundle the VC++ CRT and the Vulkan loader so a clean machine can install and run.
- Stage runtime DLLs by target platform; support debug bundles.
- Keep the word-replacements Add button inside its container on overflow (#47).
- Clear a stale overlay anchor on coarse pick and restore the default on reset (#9).
```

- [ ] **Step 3: Verify the file parses as Markdown and reads in order**

Run: `head -40 "$REPO/CHANGELOG.md"`
Expected: `[Unreleased] - 0.3.0`, then `0.2.0 - 2026-06-25`, then `0.1.0 (milestone A)`, newest first.

- [ ] **Step 4: Commit**

```bash
git -C "$REPO" add CHANGELOG.md
git -C "$REPO" commit -m "docs(changelog): backfill 0.2.0 and open an unreleased 0.3.0 section"
```

---

### Task 6: Build the public roadmap page

**Files:**

- Create: `docs/roadmap.html`
- Modify: `docs/styles.css` (append a scoped roadmap block)
- Reference: `docs/index.html` (head/nav/footer markup to mirror), `docs/styles.css` (existing classes and color variables)

**Interfaces:**

- Consumes: the milestone themes/status from the spec.
- Produces: a page reachable at `/roadmap.html`, linked in Task 7.

- [ ] **Step 1: Read the stylesheet to reuse variables and classes**

Run: `cat "$REPO/docs/styles.css"`
Expected: the CSS custom properties (background, accent, text colors) and the existing classes (`site-header`, `nav`, `wrap`, `section`, `section-head`, `tag`, `button`, `site-footer`). Reuse these; only add new `.roadmap-*` rules.

- [ ] **Step 2: Create `docs/roadmap.html`**

Mirror `index.html`'s `<head>` (title "AudioBud roadmap", canonical `…/roadmap.html`, same stylesheet/favicon/OG block), `site-header` nav (add a "Roadmap" link, link the others back to `index.html#…`), and `site-footer`. Body: a status-board section then a changelog section. Concrete skeleton (status pill classes get colors in Step 3):

```html
<main id="top">
  <section class="section">
    <div class="wrap">
      <div class="section-head">
        <h2>Where AudioBud is headed</h2>
        <p>
          Themes and status, not dates. Each milestone links to its issues on
          GitHub.
        </p>
      </div>
      <div class="roadmap-grid">
        <article class="roadmap-card">
          <div class="roadmap-card-head">
            <h3>v0.3.0 — personalization and packaging</h3>
            <span class="status-pill status-progress">in progress</span>
          </div>
          <p>
            Finalize the on-device learned dictionary and tighten the Windows
            installer.
          </p>
          <a href="https://github.com/jamditis/audiobud/milestone/?title=v0.3.0"
            >View issues</a
          >
        </article>
        <!-- repeat one roadmap-card per milestone, in order: -->
        <!-- v0.3.1 (planned) critical stability patch — data-loss and crash fixes -->
        <!-- v0.4.0 (planned) signed and distributable — no SmartScreen warning, auto-updates back on -->
        <!-- v0.5.0 (planned) stability and reliability — remaining bugs and accessibility -->
        <!-- v0.6.0 (planned) on-device AI cleanup and better accuracy — local LLM, never the cloud -->
        <!-- v0.7.0 (planned) interaction and personality — voice editing, tutorial, mascots -->
        <!-- v1.0.0 (planned) cross-platform and stable — validate macOS and Linux -->
      </div>
    </div>
  </section>
  <section class="section alt">
    <div class="wrap">
      <div class="section-head"><h2>What has shipped</h2></div>
      <div class="roadmap-grid">
        <article class="roadmap-card">
          <div class="roadmap-card-head">
            <h3>v0.2.0</h3>
            <span class="status-pill status-shipped">shipped</span>
          </div>
          <p>
            On-device personalization, a self-contained installer, and overlay
            and word-replacement fixes.
          </p>
          <a href="https://github.com/jamditis/audiobud/releases/tag/v0.2.0"
            >Release notes</a
          >
        </article>
        <article class="roadmap-card">
          <div class="roadmap-card-head">
            <h3>v0.1.0</h3>
            <span class="status-pill status-shipped">shipped</span>
          </div>
          <p>
            First working local prototype, forked from Handy with the frog/swamp
            identity.
          </p>
          <a href="https://github.com/jamditis/audiobud/releases/tag/v0.1.0"
            >Release notes</a
          >
        </article>
      </div>
    </div>
  </section>
</main>
```

Use the exact GitHub milestone URLs from `gh api repos/jamditis/audiobud/milestones --jq '.[] | "\(.title) \(.html_url)"'` rather than the placeholder `?title=` links above. Wake word and file transcription are _not_ shown (exploring / post-1.0).

- [ ] **Step 3: Append the scoped styles to `docs/styles.css`**

Reuse the existing color variables found in Step 1. Add:

```css
.roadmap-grid {
  display: grid;
  gap: 1rem;
  grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
}
.roadmap-card {
  border: 1px solid var(--border, #2a2f2a);
  border-radius: 12px;
  padding: 1.1rem 1.2rem;
  background: var(--surface, #161f17);
}
.roadmap-card-head {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: 0.5rem;
}
.status-pill {
  font-size: 0.72rem;
  text-transform: lowercase;
  padding: 0.15rem 0.5rem;
  border-radius: 999px;
  white-space: nowrap;
}
.status-shipped {
  background: #1f3a24;
  color: #8fe3a3;
}
.status-progress {
  background: #3a341f;
  color: #e3cf8f;
}
.status-planned {
  background: #1f2a3a;
  color: #8fb6e3;
}
.status-exploring {
  background: #2a2333;
  color: #c2a3e3;
}
```

(Replace `var(--border,...)`/`var(--surface,...)` fallbacks with the real variable names from Step 1 if they exist.)

- [ ] **Step 4: Render and eyeball the page**

Run: `python -m http.server 8000 --directory "$REPO/docs"` then open `http://localhost:8000/roadmap.html` (or use the `verify`/`run` skill / a Playwright snapshot).
Expected: header/footer match the home page; seven planned/in-progress cards in version order; two shipped cards; status pills colored; milestone links resolve to real GitHub milestone pages. Stop the server after.

- [ ] **Step 5: Commit**

```bash
git -C "$REPO" add docs/roadmap.html docs/styles.css
git -C "$REPO" commit -m "docs(site): add a public roadmap page with milestone status and changelog"
```

---

### Task 7: Link the roadmap from the home page nav

**Files:**

- Modify: `docs/index.html:51-56` (the `.nav-links` block)

- [ ] **Step 1: Add the nav link**

In `docs/index.html`, add to `.nav-links` (after "Install"):

```html
<a href="./roadmap.html">Roadmap</a>
```

- [ ] **Step 2: Verify**

Run: `grep -n "roadmap.html" "$REPO/docs/index.html"`
Expected: one match in the nav block.

- [ ] **Step 3: Commit**

```bash
git -C "$REPO" add docs/index.html
git -C "$REPO" commit -m "docs(site): link the roadmap page from the home nav"
```

---

### Task 8: Clean up the deferred-issues ledger

**Files:**

- Modify: `superpowers/DEFERRED-issues.md`

**Interfaces:**

- Consumes: the four new bug issue numbers from Task 4.

- [ ] **Step 1: Fix the stale header**

Replace the opening paragraph ("There is no `jamditis/audiobud` GitHub repo yet…") with a line stating the repo exists, items are filed as GitHub issues as they are scheduled, and this file remains the working ledger of not-yet-filed findings.

- [ ] **Step 2: Mark the already-shipped milestone-B installer items resolved**

Under "Milestone B (installer / packaging)", mark the VC++ runtime / Vulkan loader item (#1527/#99/#290) resolved against the v0.2.0 commits (`b9dfcb6` bundle VC++ CRT and Vulkan loader; `deb000d` stage runtime DLLs). Use the `[x]` legend with the commit noted inline, matching the file's existing style.

- [ ] **Step 3: Cross-reference the newly filed issues**

For the four entries filed in Task 4 (#1262, #574, #921, #1213/#1228), append `→ filed as audiobud #<n>` inline, using the numbers captured in Task 4 Step 4.

- [ ] **Step 4: Verify and commit**

Run: `grep -nE "filed as audiobud|b9dfcb6|repo exists|GitHub issues as they are scheduled" "$REPO/superpowers/DEFERRED-issues.md"`
Expected: the header rewrite, the resolved installer item, and four cross-references.

```bash
git -C "$REPO" add superpowers/DEFERRED-issues.md
git -C "$REPO" commit -m "docs(deferred): refresh ledger header, mark shipped items, link filed issues"
```

---

### Task 9: Self-check the whole roadmap surface

**Files:** none.

- [ ] **Step 1: Confirm milestones, issues, page, and changelog agree**

Run: `gh api repos/jamditis/audiobud/milestones --jq '.[] | "\(.title): open \(.open_issues) closed \(.closed_issues)"'`
Expected: the seven milestones present; v0.3.0 carries its seven issues; v0.3.1 carries four; v0.6.0 carries three (incl. AI cleanup); v0.7.0 carries four; v0.2.x closed.

- [ ] **Step 2: Confirm the page matches the milestone set**

Run: `grep -c "roadmap-card" "$REPO/docs/roadmap.html"`
Expected: 9 cards (7 planned/in-progress + 2 shipped). Wake word and file transcription absent.

---

### Task 10: Wiki update (GATED — cross-node, needs Joe's go)

**Files:**

- Modify: `../../sandbox/jawn-atlas/bundle/legion2025/projects/audiobud.md` (this file lives in **legion2025's** bundle, not a4000's)

**Interfaces:**

- Consumes: the shipped roadmap.

- [ ] **Step 1: Coordinate before editing**

Run: `list_peers` (claude-peers, scope machine). If a legion2025 peer is online, message it that a4000 will update `legion2025/projects/audiobud.md` to reflect v0.2.0+ and the new roadmap. If none is online, note that and proceed only with Joe's go.

- [ ] **Step 2: Update the concept file**

Update the schema table (current version past v0.2.0, status line), add a roadmap pointer, and note an a4000 working copy now exists at `C:\Users\Joe Amditis\Desktop\Crimes\playground\audiobud`. Keep OKF frontmatter rules: refresh `verified`/`timestamp`, no secret values, factual sentence case. Run `python scripts/validate.py --bundle bundle` from the jawn-atlas repo and confirm exit 0.

- [ ] **Step 3: Do NOT commit the wiki without Joe's explicit go and (if a peer is online) HOJ/legion agreement.**

---

### Task 11: Open the PR (GATED — needs Joe's go to push)

**Files:** none.

- [ ] **Step 1: Read the PR template**

Run: `cat "$REPO/.github/PULL_REQUEST_TEMPLATE.md"` and complete every section.

- [ ] **Step 2: Push and open the PR (only after Joe says go)**

```bash
git -C "$REPO" push -u origin joe/roadmap-setup
gh pr create --repo jamditis/audiobud --base main --head joe/roadmap-setup \
  --title "Roadmap: milestones, public roadmap page, and changelog hygiene" \
  --body "<filled-in template>"
```

Expected: PR opened against `main`. Do not merge without Joe's explicit go.

---

## Notes on scope

This plan is phase 1 (roadmap setup). Phase 2 — executing the actual v0.3.0 feature work (#53, #54, #44, #45, #51, #43, finalizing #16) and cutting the v0.3.0 release — is a separate spec → plan → implementation cycle, per the design spec. `$REPO` = `C:\Users\Joe Amditis\Desktop\Crimes\playground\audiobud`.
