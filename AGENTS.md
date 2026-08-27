# AGENTS.md

This file provides guidance to AI coding assistants working with code in this repository.

## Development Commands

**Prerequisites:**

- [Rust](https://rustup.rs/) (latest stable)
- [Bun](https://bun.sh/) package manager

**Core Development:**

```bash
# Install dependencies
bun install

# Run in development mode
bun run tauri dev
# If cmake error on macOS:
CMAKE_POLICY_VERSION_MINIMUM=3.5 bun run tauri dev

# Build for production
bun run tauri build

# Frontend only development
bun run dev        # Start Vite dev server
bun run build      # Build frontend (TypeScript + Vite)
bun run preview    # Preview built frontend
```

**Linting and Formatting (run before committing):**

```bash
bun run lint              # ESLint for frontend
bun run lint:fix          # ESLint with auto-fix
bun run format            # Prettier + cargo fmt
bun run format:check      # Check formatting without changes
bun run format:frontend   # Prettier only
bun run format:backend    # cargo fmt only
```

**Model Setup (Required for Development):**

```bash
mkdir -p src-tauri/resources/models
curl -L -o src-tauri/resources/models/silero_vad_v4.onnx https://github.com/jamditis/audiobud/releases/download/model-assets-v1/silero_vad_v4.onnx
```

For detailed platform-specific build setup, see [BUILD.md](BUILD.md).

## Architecture Overview

Handy is a cross-platform desktop speech-to-text application built with Tauri 2.x (Rust backend + React/TypeScript frontend).

### Backend Structure (src-tauri/src/)

- `lib.rs` - Main entry point, Tauri setup, manager initialization
- `managers/` - Core business logic:
  - `audio.rs` - Audio recording and device management
  - `model.rs` - Model downloading and management
  - `transcription.rs` - Speech-to-text processing pipeline
  - `history.rs` - Transcription history storage
- `audio_toolkit/` - Low-level audio processing:
  - `audio/` - Device enumeration, recording, resampling
  - `vad/` - Voice Activity Detection (Silero VAD)
- `commands/` - Tauri command handlers for frontend communication
- `cli.rs` - CLI argument definitions (clap derive)
- `shortcut/` - Global keyboard shortcut handling, including the generic `update_setting` command
- `settings.rs` - Application settings management: `apply_setting_value` type-checks a JSON value against `AppSettings`, `update_setting` persists it and runs that key's declared side effects, and a process-wide cache backs `get_settings` so reads don't hit the store plugin every time
- `overlay.rs` - Recording overlay window (platform-specific)
- `signal_handle.rs` - `send_transcription_input()` reusable function
- `utils.rs` - Platform detection helpers
- `output_target.rs` + `output_target/backend.rs` - Target lock: pin transcript delivery to a chosen window (Windows). The platform-independent lock/unlock state machine, window-identity re-validation, and self-window exclusion live in `output_target.rs`; the focus-borrow paste (save foreground, activate the pinned window, paste, restore) is Windows-only, in `backend.rs`
- `window_picker.rs` + `window_picker/backend.rs` - One-shot window picker: route a single transcript to a chosen window without locking. Same split as `output_target`: platform-independent candidate filtering and pick lifecycle in `window_picker.rs`, window enumeration and the picker UI in `backend.rs`
- `output_profile.rs` - Per-application output profiles: which profile applies to a delivery, and what that delivery's paste method, auto-submit, and clipboard handling become as a result. Profiles are hand-configured only and never written back into settings
- `dictation_context.rs` - Per-dictation context: the output target and other delivery intent are captured once at recording start and carried unchanged to paste time, rather than re-read from live settings
- `delivery_queue.rs` - Bounded FIFO coordination for finished transcripts waiting on the delivery worker
- `delivery_worker.rs` - The dedicated thread deliveries (pastes) run on, so a long paste -- especially a pinned target's foreground switch -- never blocks the overlay or tray. Panics in one delivery are caught so they can't take down the worker

### Frontend Structure (src/)

- `App.tsx` - Main component with onboarding flow
- `components/` - React UI components:
  - `settings/` - Settings UI
  - `model-selector/` - Model management interface
  - `onboarding/` - First-run experience
  - `overlay/` - Recording overlay UI
  - `update-checker/` - App update notifications
  - `shared/`, `ui/`, `icons/`, `footer/` - Shared components
- `hooks/useSettings.ts` - Settings state management hook
- `stores/settingsStore.ts` - Zustand store for settings
- `bindings.ts` - Auto-generated Tauri type bindings (via tauri-specta)
- `overlay/` - Recording overlay window entry point
- `lib/types.ts` - Shared TypeScript type definitions

### Key Architecture Patterns

**Manager Pattern:** Core functionality organized into managers (Audio, Model, Transcription) initialized at startup and managed via Tauri state.

**Command-Event Architecture:** Frontend → Backend via Tauri commands; Backend → Frontend via events.

**Pipeline Processing:** Audio → VAD → Whisper/Parakeet → Text output → Clipboard/Paste

**State Flow:** Zustand → Tauri Command → Rust State → Persistence (tauri-plugin-store)

### Technology Stack

**Core Libraries:**

- `whisper-rs` - Local Whisper inference with GPU acceleration
- `cpal` - Cross-platform audio I/O
- `vad-rs` - Voice Activity Detection
- `rdev` - Global keyboard shortcuts
- `rubato` - Audio resampling
- `rodio` - Audio playback for feedback sounds

### Application Flow

1. **Initialization:** App starts minimized to tray, loads settings, initializes managers
2. **Model Setup:** First-run downloads preferred Whisper model (Small/Medium/Turbo/Large)
3. **Recording:** Global shortcut triggers audio recording with VAD filtering
4. **Processing:** Audio sent to Whisper model for transcription
5. **Output:** Text pasted to active application via system clipboard

### Settings System

Settings are stored using Tauri's store plugin with reactive updates:

- Keyboard shortcuts (configurable, supports push-to-talk)
- Audio devices (microphone/output selection)
- Model preferences (Small/Medium/Turbo/Large Whisper variants)
- Audio feedback and translation options
- Output targeting (target lock, output profiles) and delivery options

A single generic `update_setting(key, value)` command replaced roughly 33 bespoke per-setting commands. It type-checks the incoming value against `AppSettings`, persists it, and then runs that key's side effects from a declared table -- so writes are fallible (a failed persist is reported instead of silently applied) and always persist before their effects run. A few settings that need to prompt the user first (`paste_method`, `external_script_path`) or that live outside `AppSettings` keep their own dedicated commands.

### Single Instance Architecture

The app enforces single instance behavior — launching when already running brings the settings window to front rather than creating a new process. Remote control flags (`--toggle-transcription`, etc.) work by launching a second instance that sends args to the running instance via `tauri_plugin_single_instance`, then exits.

## Internationalization (i18n)

All user-facing strings must use i18next translations. ESLint enforces this (no hardcoded strings in JSX).

**Adding new text:**

1. Add key to `src/i18n/locales/en/translation.json`
2. Use in component: `const { t } = useTranslation(); t('key.path')`

**File structure:**

```
src/i18n/
├── index.ts           # i18n setup
├── languages.ts       # Language metadata
└── locales/
    ├── en/translation.json  # English (source)
    ├── de/, es/, fr/, ja/, ru/, zh/, ...
    └── ...
```

For translation contribution guidelines, see [CONTRIBUTING_TRANSLATIONS.md](CONTRIBUTING_TRANSLATIONS.md).

## Code Style

**Rust:**

- Run `cargo fmt` and `cargo clippy` before committing
- Handle errors explicitly (avoid unwrap in production)
- Use descriptive names, add doc comments for public APIs

**TypeScript/React:**

- Strict TypeScript, avoid `any` types
- Functional components with hooks
- Tailwind CSS for styling
- Path aliases: `@/` → `./src/`

## CLI Parameters

Handy supports command-line parameters on all platforms for integration with scripts, window managers, and autostart configurations.

**Implementation:** `cli.rs` (definitions), `main.rs` (parsing), `lib.rs` (applying), `signal_handle.rs` (shared logic)

| Flag                     | Description                                                |
| ------------------------ | ---------------------------------------------------------- |
| `--toggle-transcription` | Toggle recording on/off on a running instance              |
| `--toggle-post-process`  | Toggle recording with post-processing on/off               |
| `--cancel`               | Cancel the current operation on a running instance         |
| `--start-hidden`         | Launch without showing the main window (tray icon visible) |
| `--no-tray`              | Launch without system tray (closing window quits the app)  |
| `--debug`                | Enable debug mode with verbose (Trace) logging             |

**Key design decisions:**

- CLI flags are runtime-only overrides — they do NOT modify persisted settings
- Remote control flags work via `tauri_plugin_single_instance`: second instance sends args, then exits
- `send_transcription_input()` in `signal_handle.rs` is shared between signal handlers and CLI

## Debug Mode

Access debug features: `Cmd+Shift+D` (macOS) or `Ctrl+Shift+D` (Windows/Linux)

## Platform Notes

- **macOS**: Metal acceleration, accessibility permissions required for keyboard shortcuts
- **Windows**: Vulkan acceleration, code signing
- **Linux**: OpenBLAS + Vulkan, limited Wayland support, overlay uses GTK layer shell (disable with `HANDY_NO_GTK_LAYER_SHELL=1`)

## Troubleshooting

See the [Troubleshooting](README.md#troubleshooting) section in README.md.

## GitHub workflow for AI coding assistants

**Before opening any PR or issue in this repo: read the relevant template file and fill in every section it lists.** A generic Summary/Test-plan layout is not a substitute for the template.

- **Opening a PR:** Read [`.github/PULL_REQUEST_TEMPLATE.md`](.github/PULL_REQUEST_TEMPLATE.md) and complete each section (Before submitting, Description, Related issues, Testing). If you skip a checklist item, say why in that section.
- **Opening an issue:** Read [`.github/ISSUE_TEMPLATE/`](.github/ISSUE_TEMPLATE/). Use `bug_report.md` for bugs. AudioBud has GitHub Discussions disabled, so feature requests are filed as issues with the `enhancement` label (blank issues are enabled — see `.github/ISSUE_TEMPLATE/config.yml`).
- **Translations:** Follow [CONTRIBUTING_TRANSLATIONS.md](CONTRIBUTING_TRANSLATIONS.md).
- **Full contributor workflow:** [CONTRIBUTING.md](CONTRIBUTING.md).

**Commits:** Use conventional commit prefixes (`feat:`, `fix:`, `docs:`, `refactor:`, `chore:`). Focus the message on _why_, not _what_.

## Partner center submission notes

- For user-approved metadata-only release corrections, bypass the full PR review gate and avoid redundant CI reruns; verify the diff and final release SHA.
- On the Partner Center package validation page, expanded validation sections may have been opened by the user. Do not infer that automation expanded them or that their helper text is the final validation result. Wait for the overall package validation run to leave `In progress` before deciding whether follow-up work is needed.
- Joe's AudioBud workflow is press once to start recording, then press again to stop and send the transcript. Do not describe his workflow as "hold the hotkey." If docs need to describe default app behavior, verify the current `push_to_talk` default first.

## Commits and attribution

No AI attribution in commits, PR bodies, issues, docs, or code. `.claude/settings.json` disables automatic session links and blanks the commit and PR attribution strings. Web and Remote Control sessions can otherwise add that metadata by default. The setting lives in the repo rather than `~/.claude/settings.json` because cloud sessions clone the repo and never read user-level config. Don't reintroduce any of it by hand.

No `Co-authored-by` trailers of any kind, including Joe's own aliases.

Git identity — set before committing, in every worktree and every agent session:

```sh
git config user.name "Joe Amditis"
git config user.email "6799804+jamditis@users.noreply.github.com"
```

Any other author email either trips GitHub's email-privacy push block (GH007) or makes a squash merge inject a `Co-authored-by` line into the merge body.
