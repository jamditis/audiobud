# AudioBud milestone A implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superjawn:subagent-driven-development (recommended) or superjawn:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stand up a working local AudioBud prototype — a detached fork of cjpais/Handy 0.8.3, minimally rebranded, with a `Ctrl+Alt+Space` default hotkey and a benchmark-chosen default STT engine — that captures speech and types it into the focused app on this RTX 4080 machine.

**Architecture:** Detached fork (Handy's tree imported as a squashed commit, `upstream` remote kept for future cherry-picks). Changes are surgical edits to Handy's existing Rust/Tauri code (`settings.rs`, `tauri.conf.json`, `Cargo.toml`, the Rust window builder, `model.rs` default). TDD applies to the thin testable seams (hotkey default, rebrand identity, engine default, a WER scorer — all greenfield since Handy has no unit tests); integration parts (build succeeds, transcription works, injection works) use explicit verification gates including a Windows manual smoke gate.

**Tech Stack:** Tauri 2.10.2, Rust (MSVC), Bun, React 18 + TypeScript + Vite, `transcribe-rs` (whisper.cpp on Vulkan, ONNX/Parakeet on DirectML), Tauri Store plugin for settings.

---

## Ground truth (from upstream inspection, 2026-06-21)

- Upstream: `github.com/cjpais/Handy`, default branch `master`, version `0.8.3`, inspected at commit `bc6a41e418dda66a1f8d0b123e6a83880a66b6a1`.
- `src-tauri/tauri.conf.json`: `productName: "Handy"`, `identifier: "com.pais.handy"`, `app.windows: []` (window built in Rust at runtime), `bundle.windows.signCommand` = cjpais Azure account, `plugins.updater` = cjpais endpoint + pubkey.
- `src-tauri/Cargo.toml`: `[package] name = "handy"`, `default-run = "handy"`, `description = "Handy"`, `authors = ["cjpais"]`; `[lib] name = "handy_app_lib"` (referenced by `main.rs` — invasive to rename; left as-is for milestone A).
- `package.json`: `name: "handy-app"`; scripts include `dev`, `build` (`tsc && vite build`), `lint` (`eslint src`), `format`, `format:check`, `test:playwright`. No vitest/jest, no standalone typecheck, no Rust tests. Only test = a Playwright smoke spec (`tests/app.spec.ts`).
- Default hotkey: `src-tauri/src/settings.rs` → `get_default_settings()`, Windows `default_shortcut = "ctrl+space"`. Shortcut string format: lowercase tokens joined by `+`.
- Engine default: `src-tauri/src/managers/model.rs` registers models; `settings.rs` `selected_model` defaults to `""`; `auto_select_model_if_needed()` then picks the first downloaded model. Model IDs: `small`, `medium`, `turbo` (large-v3-turbo), `large`, `parakeet-tdt-0.6b-v3`, `canary-1b-v2`.
- Build: `bun install`; the Silero VAD model must exist before building — `curl -o src-tauri/resources/models/silero_vad_v4.onnx https://blob.handy.computer/silero_vad_v4.onnx`; then `bun tauri dev` / `bun run tauri build`. Windows needs MS C++ Build Tools + (per our spec) Vulkan SDK + WebView2.
- `src-tauri/src/managers/transcription_mock.rs` exists only so CI can stub out Whisper/Vulkan for the Playwright build. Do NOT apply it — we want the real engines.

## File structure (created/modified in milestone A)

- Import (whole Handy tree) — created once in Task 1.
- Modify `src-tauri/src/settings.rs` — hotkey default, engine default, first Rust test module.
- Modify `src-tauri/tauri.conf.json` — identifier, productName.
- Modify `src-tauri/Cargo.toml` — package name, default-run, description, authors.
- Modify `package.json` — name.
- Modify the Rust window builder (`src-tauri/src/lib.rs` — exact file confirmed by grep in Task 6) — window title.
- Create `scripts/check-rebrand.ts` — rebrand identity assertions.
- Create `scripts/wer.ts` + `scripts/wer.test.ts` — WER scorer for the benchmark, with a Bun test.
- Create `bench/` — fixed audio sample, reference transcript, and `bench/RESULTS.md`.
- Create `docs/superpowers/SMOKE-GATE-milestone-a.md` — the manual smoke checklist.

A note on the build environment (both apply for every build/test step below):
- Redirect the Rust target dir out of OneDrive to avoid sync thrash and Windows long-path limits. Set once per shell: `export CARGO_TARGET_DIR="C:/cargo-target/audiobud"` (Bash) — and the plan's cargo/tauri commands inherit it.
- Work on a branch, not `master`: `git checkout -b milestone-a` (Task 0).

---

### Task 0: Prerequisites and build environment

**Files:**
- None (environment + branch only).

- [ ] **Step 1: Verify the toolchain is present**

Run:
```bash
rustc --version && cargo --version && bun --version && node --version
```
Expected: versions print for all four (rustc stable MSVC, cargo, bun, node). If `bun` is missing, stop and install Bun before continuing.

- [ ] **Step 2: Verify Windows native build prerequisites**

Run:
```bash
where cl 2>/dev/null || echo "MSVC cl not on PATH (ok if VS Build Tools installed; tauri finds it)"
ls "C:/VulkanSDK" 2>/dev/null && echo "Vulkan SDK present" || echo "Vulkan SDK MISSING - install from https://vulkan.lunarg.com/ before Task 2"
reg query "HKLM\\SOFTWARE\\WOW6432Node\\Microsoft\\EdgeUpdate\\Clients\\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}" >/dev/null 2>&1 && echo "WebView2 present" || echo "WebView2 check inconclusive (preinstalled on Win11)"
```
Expected: Vulkan SDK present. If missing, install the Vulkan SDK before Task 2 (it is a hard build+runtime dependency for the Whisper engine). VS Build Tools "Desktop development with C++" must be installed.

- [ ] **Step 3: Set the cargo target dir out of OneDrive (this shell)**

Run:
```bash
mkdir -p "C:/cargo-target/audiobud" && export CARGO_TARGET_DIR="C:/cargo-target/audiobud" && echo "CARGO_TARGET_DIR=$CARGO_TARGET_DIR"
```
Expected: prints the path. Re-export this in any new shell used for building.

- [ ] **Step 4: Create the working branch**

Run:
```bash
cd "C:/Users/amdit/OneDrive/Desktop/Crimes/playground/audiobud" && git checkout -b milestone-a && git branch --show-current
```
Expected: `milestone-a`.

---

### Task 1: Import Handy 0.8.3 as a detached fork

**Files:**
- Create: the entire Handy source tree under the repo root (alongside existing `docs/`).
- Create: `src-tauri/resources/models/silero_vad_v4.onnx` (downloaded, gitignored later).

- [ ] **Step 1: Clone upstream to a temp dir and capture the SHA**

Run:
```bash
rm -rf /tmp/handy-import && git clone --depth 1 https://github.com/cjpais/Handy /tmp/handy-import && git -C /tmp/handy-import rev-parse HEAD
```
Expected: clone succeeds; prints a commit SHA (record it for the commit message).

- [ ] **Step 2: Copy Handy's tree into the repo (excluding its .git)**

Run:
```bash
cd /tmp/handy-import && rm -rf .git && cp -r ./. "C:/Users/amdit/OneDrive/Desktop/Crimes/playground/audiobud/"
```
Expected: no error. Handy's files now sit alongside the existing `docs/superpowers/` (no path collisions — Handy has no `docs/superpowers`).

- [ ] **Step 3: Confirm the key files landed**

Run:
```bash
cd "C:/Users/amdit/OneDrive/Desktop/Crimes/playground/audiobud" && ls src-tauri/tauri.conf.json src-tauri/Cargo.toml package.json src-tauri/src/settings.rs src-tauri/src/managers/model.rs && grep -c '"identifier": "com.pais.handy"' src-tauri/tauri.conf.json
```
Expected: all paths listed; grep prints `1`.

- [ ] **Step 4: Add the upstream remote and clean OneDrive artifacts**

Run:
```bash
cd "C:/Users/amdit/OneDrive/Desktop/Crimes/playground/audiobud" && git remote add upstream https://github.com/cjpais/Handy && git remote -v && find .git -name desktop.ini -delete 2>/dev/null; echo done
```
Expected: `upstream` listed (fetch + push); `done`.

- [ ] **Step 5: Append build-output ignores to .gitignore**

Add these lines to `.gitignore` (create the file if Handy didn't supply one; if it did, append):
```gitignore
# AudioBud additions
/target/
node_modules/
dist/
desktop.ini
src-tauri/resources/models/silero_vad_v4.onnx
```

Run:
```bash
cd "C:/Users/amdit/OneDrive/Desktop/Crimes/playground/audiobud" && grep -q "AudioBud additions" .gitignore && echo "ignore added"
```
Expected: `ignore added`.

- [ ] **Step 6: Commit the import**

Run (substitute the SHA from Step 1):
```bash
cd "C:/Users/amdit/OneDrive/Desktop/Crimes/playground/audiobud" && git add -A && git commit --no-gpg-sign -m "Import Handy 0.8.3 source (upstream <SHA>)"
```
Expected: commit created with the full Handy tree.

---

### Task 2: Verify the unmodified upstream build runs (gate)

This proves the fork is buildable on this machine before any changes. No code changes here.

**Files:**
- None (build verification).

- [ ] **Step 1: Install frontend dependencies**

Run:
```bash
cd "C:/Users/amdit/OneDrive/Desktop/Crimes/playground/audiobud" && bun install
```
Expected: completes without error (the `postinstall` nix-deps check is a no-op on Windows or prints a skip).

- [ ] **Step 2: Fetch the Silero VAD model (required before building)**

Run:
```bash
cd "C:/Users/amdit/OneDrive/Desktop/Crimes/playground/audiobud" && mkdir -p src-tauri/resources/models && curl -L -o src-tauri/resources/models/silero_vad_v4.onnx https://blob.handy.computer/silero_vad_v4.onnx && ls -la src-tauri/resources/models/silero_vad_v4.onnx
```
Expected: file downloaded, non-zero size.

- [ ] **Step 3: Launch the dev build**

Run (with `CARGO_TARGET_DIR` exported from Task 0):
```bash
cd "C:/Users/amdit/OneDrive/Desktop/Crimes/playground/audiobud" && bun tauri dev
```
Expected: Rust compiles (first build is slow — pulls cjpais's patched Tauri + native deps), then the AudioBud-to-be (still "Handy") window opens. If it crashes on `MSVCP140.dll`, install the VC++ x64 redistributable and retry. Leave it running for Step 4.

- [ ] **Step 4: Manually confirm end-to-end transcription on upstream**

In the running app: complete onboarding, download the Parakeet V3 model (`parakeet-tdt-0.6b-v3`) when prompted, then press the current default hotkey (`Ctrl+Space`), speak a short phrase, and confirm text is transcribed and pasted into a focused text field (open Notepad first).
Expected: spoken phrase appears as text. This confirms audio capture, the engine, and injection all work on this machine before we change anything. Stop the dev server (Ctrl+C in the terminal) when confirmed.

- [ ] **Step 5: Record the baseline**

No commit (no file changes). Note in your working log: "Upstream Handy 0.8.3 builds and transcribes on this machine; Parakeet V3 downloaded." This is the green light to start editing.

---

### Task 3: First Rust test — assert the Windows hotkey default (failing)

Handy has no Rust tests; this adds the first `#[cfg(test)]` module.

**Files:**
- Modify: `src-tauri/src/settings.rs` (append a test module at end of file)
- Test: same file

- [ ] **Step 1: Write the failing test**

Append to the end of `src-tauri/src/settings.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_transcribe_default_is_ctrl_alt_space() {
        let settings = get_default_settings();
        let binding = settings
            .bindings
            .get("transcribe")
            .expect("transcribe binding should exist");
        assert_eq!(binding.default_binding, "ctrl+alt+space");
        assert_eq!(binding.current_binding, "ctrl+alt+space");
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run:
```bash
cd "C:/Users/amdit/OneDrive/Desktop/Crimes/playground/audiobud/src-tauri" && cargo test --lib windows_transcribe_default_is_ctrl_alt_space
```
Expected: FAIL — `assertion failed: left: "ctrl+space", right: "ctrl+alt+space"` (the current default is still `ctrl+space`).

- [ ] **Step 3: Commit the failing test**

Run:
```bash
cd "C:/Users/amdit/OneDrive/Desktop/Crimes/playground/audiobud" && git add src-tauri/src/settings.rs && git commit --no-gpg-sign -m "test: assert Windows transcribe default is ctrl+alt+space (failing)"
```
Expected: commit created.

---

### Task 4: Change the default hotkey to Ctrl+Alt+Space (make it pass)

**Files:**
- Modify: `src-tauri/src/settings.rs` (the Windows `default_shortcut` line in `get_default_settings()`)

- [ ] **Step 1: Edit the Windows default**

In `src-tauri/src/settings.rs`, inside `get_default_settings()`, change:
```rust
#[cfg(target_os = "windows")]
let default_shortcut = "ctrl+space";
```
to:
```rust
#[cfg(target_os = "windows")]
let default_shortcut = "ctrl+alt+space";
```
Leave the macOS/Linux defaults and the `transcribe_with_post_process` / `cancel` bindings unchanged.

- [ ] **Step 2: Run the test to verify it passes**

Run:
```bash
cd "C:/Users/amdit/OneDrive/Desktop/Crimes/playground/audiobud/src-tauri" && cargo test --lib windows_transcribe_default_is_ctrl_alt_space
```
Expected: PASS (1 passed).

- [ ] **Step 3: Commit**

Run:
```bash
cd "C:/Users/amdit/OneDrive/Desktop/Crimes/playground/audiobud" && git add src-tauri/src/settings.rs && git commit --no-gpg-sign -m "feat: default Windows hotkey to ctrl+alt+space"
```
Expected: commit created.

---

### Task 5: Rebrand-identity assertion script (failing)

A Bun script that fails while the product still identifies as Handy.

**Files:**
- Create: `scripts/check-rebrand.ts`

- [ ] **Step 1: Write the failing check script**

Create `scripts/check-rebrand.ts`:
```typescript
// Asserts AudioBud's brand identity across config files. Exits non-zero on any mismatch.
import { readFileSync } from "node:fs";

const fail: string[] = [];

const tauri = JSON.parse(readFileSync("src-tauri/tauri.conf.json", "utf8"));
if (tauri.identifier !== "tech.amditis.audiobud")
  fail.push(`tauri.conf.json identifier = ${tauri.identifier} (want tech.amditis.audiobud)`);
if (tauri.productName !== "AudioBud")
  fail.push(`tauri.conf.json productName = ${tauri.productName} (want AudioBud)`);

const cargo = readFileSync("src-tauri/Cargo.toml", "utf8");
if (!/^name\s*=\s*"audiobud"\s*$/m.test(cargo))
  fail.push('Cargo.toml [package] name is not "audiobud"');
if (!/^default-run\s*=\s*"audiobud"\s*$/m.test(cargo))
  fail.push('Cargo.toml default-run is not "audiobud"');

const pkg = JSON.parse(readFileSync("package.json", "utf8"));
if (pkg.name !== "audiobud")
  fail.push(`package.json name = ${pkg.name} (want audiobud)`);

if (fail.length) {
  console.error("REBRAND CHECK FAILED:\n" + fail.map((f) => " - " + f).join("\n"));
  process.exit(1);
}
console.log("REBRAND CHECK PASSED");
```

- [ ] **Step 2: Run it to verify it fails**

Run:
```bash
cd "C:/Users/amdit/OneDrive/Desktop/Crimes/playground/audiobud" && bun scripts/check-rebrand.ts; echo "exit=$?"
```
Expected: prints `REBRAND CHECK FAILED` listing identifier/productName/Cargo/package mismatches; `exit=1`.

- [ ] **Step 3: Commit the failing check**

Run:
```bash
cd "C:/Users/amdit/OneDrive/Desktop/Crimes/playground/audiobud" && git add scripts/check-rebrand.ts && git commit --no-gpg-sign -m "test: add rebrand-identity check (failing)"
```
Expected: commit created.

---

### Task 6: Apply the minimal rebrand (make it pass)

**Files:**
- Modify: `src-tauri/tauri.conf.json` (`identifier`, `productName`)
- Modify: `src-tauri/Cargo.toml` (`name`, `default-run`, `description`, `authors`)
- Modify: `package.json` (`name`)
- Modify: the Rust window builder (`src-tauri/src/lib.rs`) — window title

- [ ] **Step 1: Edit tauri.conf.json**

In `src-tauri/tauri.conf.json`:
- change `"productName": "Handy"` to `"productName": "AudioBud"`
- change `"identifier": "com.pais.handy"` to `"identifier": "tech.amditis.audiobud"`
Leave `version`, `bundle`, `plugins.updater`, and `signCommand` unchanged for milestone A (updater/signing are milestone B/C).

- [ ] **Step 2: Edit Cargo.toml**

In `src-tauri/Cargo.toml` `[package]`:
- `name = "handy"` to `name = "audiobud"`
- `default-run = "handy"` to `default-run = "audiobud"`
- `description = "Handy"` to `description = "AudioBud — local voice dictation"`
- `authors = ["cjpais"]` to `authors = ["Joe Amditis"]`
Leave `[lib] name = "handy_app_lib"` unchanged (internal; referenced by `main.rs` — renaming is out of scope for milestone A).

- [ ] **Step 3: Edit package.json**

In `package.json`, change `"name": "handy-app"` to `"name": "audiobud"`.

- [ ] **Step 4: Locate and edit the Rust window title**

Run to find it:
```bash
cd "C:/Users/amdit/OneDrive/Desktop/Crimes/playground/audiobud" && grep -rn '\.title("Handy")' src-tauri/src/
```
Expected: one or more hits (research indicates `src-tauri/src/lib.rs`). In each hit, change `.title("Handy")` to `.title("AudioBud")`. If the grep returns nothing, search broader: `grep -rn '"Handy"' src-tauri/src/ | grep -i title` and update the window-title string only (do not mass-replace every "Handy" string in milestone A — locale/about text is milestone C).

- [ ] **Step 5: Run the rebrand check to verify it passes**

Run:
```bash
cd "C:/Users/amdit/OneDrive/Desktop/Crimes/playground/audiobud" && bun scripts/check-rebrand.ts; echo "exit=$?"
```
Expected: `REBRAND CHECK PASSED`; `exit=0`.

- [ ] **Step 6: Rebuild to confirm the app still compiles and launches as AudioBud**

Run:
```bash
cd "C:/Users/amdit/OneDrive/Desktop/Crimes/playground/audiobud" && bun tauri dev
```
Expected: compiles; window opens titled "AudioBud". The first build after the crate rename recompiles the binary as `audiobud.exe`. Stop the dev server when confirmed.

- [ ] **Step 7: Commit**

Run:
```bash
cd "C:/Users/amdit/OneDrive/Desktop/Crimes/playground/audiobud" && git add src-tauri/tauri.conf.json src-tauri/Cargo.toml package.json src-tauri/src/ Cargo.lock src-tauri/Cargo.lock 2>/dev/null; git commit --no-gpg-sign -m "feat: minimal rebrand to AudioBud (identifier, productName, crate, window title)"
```
Expected: commit created.

---

### Task 7: STT engine benchmark — WER scorer (TDD) + measurement procedure

The default engine is chosen from real numbers on this machine. First build a tested scorer, then run both engines on a fixed sample.

**Files:**
- Create: `scripts/wer.ts` (word error rate)
- Create: `scripts/wer.test.ts` (Bun test)
- Create: `bench/reference.txt`, `bench/RESULTS.md`

- [ ] **Step 1: Write the failing WER test**

Create `scripts/wer.test.ts`:
```typescript
import { test, expect } from "bun:test";
import { wer } from "./wer";

test("identical strings have 0 WER", () => {
  expect(wer("the quick brown fox", "the quick brown fox")).toBe(0);
});

test("one substitution in four words is 0.25", () => {
  expect(wer("the quick brown fox", "the quick green fox")).toBeCloseTo(0.25, 5);
});

test("case and punctuation are normalized", () => {
  expect(wer("Hello, world.", "hello world")).toBe(0);
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run:
```bash
cd "C:/Users/amdit/OneDrive/Desktop/Crimes/playground/audiobud" && bun test scripts/wer.test.ts; echo "exit=$?"
```
Expected: FAIL — cannot resolve `./wer` (module does not exist yet); `exit` non-zero.

- [ ] **Step 3: Implement the WER scorer**

Create `scripts/wer.ts`:
```typescript
// Word error rate via Levenshtein distance over normalized word tokens.
function normalize(s: string): string[] {
  return s
    .toLowerCase()
    .replace(/[^\p{L}\p{N}\s]/gu, "")
    .split(/\s+/)
    .filter(Boolean);
}

export function wer(reference: string, hypothesis: string): number {
  const r = normalize(reference);
  const h = normalize(hypothesis);
  if (r.length === 0) return h.length === 0 ? 0 : 1;

  const d: number[][] = Array.from({ length: r.length + 1 }, () =>
    new Array(h.length + 1).fill(0),
  );
  for (let i = 0; i <= r.length; i++) d[i][0] = i;
  for (let j = 0; j <= h.length; j++) d[0][j] = j;
  for (let i = 1; i <= r.length; i++) {
    for (let j = 1; j <= h.length; j++) {
      const cost = r[i - 1] === h[j - 1] ? 0 : 1;
      d[i][j] = Math.min(d[i - 1][j] + 1, d[i][j - 1] + 1, d[i - 1][j - 1] + cost);
    }
  }
  return d[r.length][h.length] / r.length;
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run:
```bash
cd "C:/Users/amdit/OneDrive/Desktop/Crimes/playground/audiobud" && bun test scripts/wer.test.ts; echo "exit=$?"
```
Expected: 3 pass; `exit=0`.

- [ ] **Step 5: Create the fixed benchmark reference**

Create `bench/reference.txt` with a single known sentence to read aloud during the benchmark:
```
The quick brown fox jumps over the lazy dog while the engine transcribes every word.
```

Create `bench/RESULTS.md`:
```markdown
# AudioBud engine benchmark (milestone A)

Machine: Legion, RTX 4080 Super, Windows 11. Date: <fill>.
Reference sentence: see bench/reference.txt. Read it aloud 5 times per engine.

| Engine (model id) | Backend | Avg latency (s) | WER | Stable over 5 runs? |
|---|---|---|---|---|
| Parakeet V3 (parakeet-tdt-0.6b-v3) | DirectML/ONNX | | | |
| Whisper turbo (turbo) | Vulkan/whisper.cpp | | | |

Default chosen: <model id> — reason: <one line>.
```

- [ ] **Step 6: Run the benchmark (measurement gate)**

Procedure (with `bun tauri dev` running):
1. In settings, select the Parakeet model (`parakeet-tdt-0.6b-v3`); download if needed.
2. Read `bench/reference.txt` aloud 5 times via `Ctrl+Alt+Space` into Notepad; for each, note transcription latency (use the app log timestamps or a stopwatch) and capture the transcript.
3. Score each transcript: `bun -e "import {wer} from './scripts/wer'; console.log(wer(require('fs').readFileSync('bench/reference.txt','utf8'), '<paste transcript>'))"`.
4. Repeat for the Whisper turbo model (`turbo`).
5. Fill `bench/RESULTS.md` with avg latency, avg WER, and stability for each engine. Choose the default: lowest WER with acceptable latency and no crashes over 5 runs. Record the choice and reason.

Expected: `bench/RESULTS.md` fully filled with real numbers and a chosen default model id.

- [ ] **Step 7: Commit the scorer and results**

Run:
```bash
cd "C:/Users/amdit/OneDrive/Desktop/Crimes/playground/audiobud" && git add scripts/wer.ts scripts/wer.test.ts bench/ && git commit --no-gpg-sign -m "feat: WER scorer + engine benchmark results"
```
Expected: commit created.

---

### Task 8: Set the default engine from the benchmark (TDD)

Make the benchmark-chosen model the default for fresh installs. Below uses `parakeet-tdt-0.6b-v3` as the placeholder; **substitute the actual winner from `bench/RESULTS.md`** in both the test and the implementation.

**Files:**
- Modify: `src-tauri/src/settings.rs` (`selected_model` default + test module)

- [ ] **Step 1: Write the failing test**

Add to the `tests` module in `src-tauri/src/settings.rs`:
```rust
    #[test]
    fn default_selected_model_is_the_benchmarked_default() {
        let settings = get_default_settings();
        assert_eq!(settings.selected_model, "parakeet-tdt-0.6b-v3");
    }
```
(Replace `"parakeet-tdt-0.6b-v3"` with the winner if Whisper turbo won: `"turbo"`.)

- [ ] **Step 2: Run the test to verify it fails**

Run:
```bash
cd "C:/Users/amdit/OneDrive/Desktop/Crimes/playground/audiobud/src-tauri" && cargo test --lib default_selected_model_is_the_benchmarked_default
```
Expected: FAIL — `left: "", right: "parakeet-tdt-0.6b-v3"` (default is currently empty).

- [ ] **Step 3: Set the default**

In `src-tauri/src/settings.rs` `get_default_settings()`, change:
```rust
        selected_model: "".to_string(),
```
to:
```rust
        selected_model: "parakeet-tdt-0.6b-v3".to_string(),
```
(or `"turbo"` if it won the benchmark).

- [ ] **Step 4: Run the test to verify it passes**

Run:
```bash
cd "C:/Users/amdit/OneDrive/Desktop/Crimes/playground/audiobud/src-tauri" && cargo test --lib default_selected_model_is_the_benchmarked_default
```
Expected: PASS.

- [ ] **Step 5: Commit**

Run:
```bash
cd "C:/Users/amdit/OneDrive/Desktop/Crimes/playground/audiobud" && git add src-tauri/src/settings.rs && git commit --no-gpg-sign -m "feat: default to benchmarked STT engine"
```
Expected: commit created.

---

### Task 9: Windows manual smoke gate (integration verification)

Integration behaviors that can't be unit-tested get a documented, repeatable checklist.

**Files:**
- Create: `docs/superpowers/SMOKE-GATE-milestone-a.md`

- [ ] **Step 1: Write the smoke checklist**

Create `docs/superpowers/SMOKE-GATE-milestone-a.md`:
```markdown
# AudioBud milestone A — Windows smoke gate

Run on Legion (RTX 4080) from a fresh `bun tauri dev` build. Tick each; all must pass.

- [ ] App launches; window titled "AudioBud"; no MSVCP140.dll crash.
- [ ] Default hotkey is Ctrl+Alt+Space (Settings shows it; pressing Win alone does NOT trigger AudioBud).
- [ ] Default engine is the benchmarked winner (Settings shows it pre-selected on a fresh settings store).
- [ ] Press Ctrl+Alt+Space, speak into Notepad: transcript is pasted correctly.
- [ ] Whisper turbo also loads and transcribes when selected (engine switch works).
- [ ] Injection limit (known, not a bug): paste into an elevated app (e.g. an admin terminal) is blocked/no-ops rather than corrupting text — confirm it fails cleanly.
- [ ] Cancel binding (Escape) aborts an in-progress capture.

Tester: <name>. Date: <date>. Result: PASS / FAIL (notes).
```

- [ ] **Step 2: Execute the smoke gate**

Run `bun tauri dev`, then work through the checklist against the running app (delete or rename `%APPDATA%/tech.amditis.audiobud` first to verify fresh-install defaults). Fill in tester/date/result.
Expected: all items ticked PASS. Any FAIL becomes a bug → write a failing test/repro and fix before milestone A is complete (bug-fixing workflow).

- [ ] **Step 3: Commit the executed gate**

Run:
```bash
cd "C:/Users/amdit/OneDrive/Desktop/Crimes/playground/audiobud" && git add docs/superpowers/SMOKE-GATE-milestone-a.md && git commit --no-gpg-sign -m "test: milestone A Windows smoke gate (executed, passing)"
```
Expected: commit created.

---

### Task 10: Milestone A acceptance

**Files:**
- Modify: `docs/superpowers/specs/2026-06-21-audiobud-design.md` (status note)

- [ ] **Step 1: Run the full automated suite**

Run:
```bash
cd "C:/Users/amdit/OneDrive/Desktop/Crimes/playground/audiobud" && (cd src-tauri && cargo test --lib) && bun test scripts/wer.test.ts && bun scripts/check-rebrand.ts && bun run lint
```
Expected: cargo tests pass (hotkey default + engine default), WER tests pass, rebrand check passes, eslint passes.

- [ ] **Step 2: Confirm acceptance criteria**

Verify all are true: upstream builds (Task 2), hotkey default is Ctrl+Alt+Space (Task 4), rebrand passes (Task 6), benchmark recorded + default set (Tasks 7-8), smoke gate PASS (Task 9). If all hold, milestone A is met.

- [ ] **Step 3: Note completion in the spec and commit**

In `docs/superpowers/specs/2026-06-21-audiobud-design.md`, append to the status line: `Milestone A complete <date> (branch milestone-a).`

Run:
```bash
cd "C:/Users/amdit/OneDrive/Desktop/Crimes/playground/audiobud" && git add docs/superpowers/specs/2026-06-21-audiobud-design.md && git commit --no-gpg-sign -m "docs: mark milestone A complete" && git log --oneline | head -15
```
Expected: commit created; log shows the milestone-A task commits. Do NOT push or create a GitHub repo (milestone C; needs Joe's approval).

---

## Self-review

- **Spec coverage:** milestone A scope items all map to tasks — detached fork + upstream remote (Task 1), build verification (Task 2), minimal rebrand (Tasks 5-6), Ctrl+Alt+Space default (Tasks 3-4), engine benchmark + default-from-numbers (Tasks 7-8), models via existing downloader (Task 2 Step 4 / Task 7), text injection + seam tests + manual smoke gate (Tasks 9-10). Out-of-scope items (Win+H hook, R2 host, MODEL_NOTICES, redist installer logic, signing, docs site, CI, branch protection) are correctly absent. Frontend unit tests (vitest) are intentionally deferred — Handy has no vitest setup and milestone A's testable seams are Rust-side; adding a frontend test harness is milestone B/C.
- **Placeholder scan:** the only intentional substitution is the benchmark-winner model id in Task 8 (explicitly flagged to replace from `bench/RESULTS.md`); the `<SHA>` in Task 1 and `<date>`/tester fields are runtime values, not logic placeholders. No "add error handling"-style gaps.
- **Type/name consistency:** `get_default_settings()`, `AppSettings.bindings`, `ShortcutBinding.default_binding/current_binding`, `selected_model`, model id `parakeet-tdt-0.6b-v3`/`turbo`, and the `wer(reference, hypothesis)` signature are used identically across tasks.
