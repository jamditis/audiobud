# AudioBud

A local-first, offline speech-to-text app. Press a hotkey, speak, and your words appear in whatever text field has focus. No audio leaves your machine.

AudioBud is a detached fork of [Handy](https://github.com/cjpais/Handy) by CJ Pais (MIT). It keeps Handy's local transcription engine and Tauri architecture, with AudioBud's own defaults, a frog/swamp visual identity, and a Windows-first focus.

**Website:** <https://jamditis.github.io/audiobud/> &middot; **Download:** [latest release](https://github.com/jamditis/audiobud/releases/latest) &middot; **Changelog:** [CHANGELOG.md](CHANGELOG.md)

![AudioBud's general settings: the swamp background, the Bungee wordmark, and the transcription, microphone, and audio controls.](screenshots/app-general.png)

*The shortcuts shown above are customized; AudioBud's Windows default is `Ctrl+Alt+Space`.*

## Status

Milestone A: a working local prototype, packaged as a Windows build (v0.1.0) on the [releases page](https://github.com/jamditis/audiobud/releases/latest). The build is not yet code-signed, so Windows SmartScreen shows a warning on first launch; choose **More info -> Run anyway** to proceed. You can also [build it from source](#build-from-source). Automatic update checks are disabled, because the inherited updater still points at upstream Handy's release feed; they return once that feed is repointed to AudioBud and its builds are signed, in milestone B. The cross-platform code is inherited from Handy, but AudioBud has only been validated on Windows so far.

## How it works

1. Press the hotkey (default `Ctrl+Alt+Space`) to start and stop recording.
2. Speak while it records.
3. AudioBud transcribes locally and types the text into the focused field.

Everything runs on your machine:

- Silero VAD filters out silence.
- Transcription uses your choice of local models:
  - **Parakeet V3** (ONNX, DirectML) is the default on Windows: ~640 MB, sub-second on warm runs, accurate, with automatic language detection.
  - **Whisper** models (small, medium, turbo, large) run through whisper.cpp with Vulkan acceleration.

## Features

- **Local transcription.** Audio never leaves your machine. Silero VAD trims silence before the engine runs.
- **A choice of engines.** Parakeet (V2/V3, ONNX/DirectML) and Whisper (small through large, whisper.cpp/Vulkan), plus Moonshine, SenseVoice, GigaAM, Canary, and Cohere. Each shows accuracy and speed at a glance, and downloads from inside the app.
- **Languages.** Parakeet V3 detects and transcribes 25 European languages; Whisper adds many more and can translate to English. The interface itself ships in more than a dozen languages.
- **Push-to-talk or toggle,** a configurable global hotkey, and an optional auto-submit key.
- **Optional LLM post-processing.** Send the transcript through a provider of your choice (Claude, Gemini, or a custom endpoint) with your own prompt. It is off by default, and your API key stays on your machine.
- **Custom words.** Bias the output toward names and jargon it would otherwise miss, with a `.txt` import.
- **History and retention.** Keep recent transcriptions and recordings, or set them to expire.
- **Audio feedback** with selectable sound themes, a live input meter, and an output test.
- **GPU control.** Pick the Whisper and ONNX accelerators (auto, CPU, CUDA, DirectML, ROCm) and the GPU device.
- **Tray, autostart, and command-line flags** for running it the way you want.

![The model picker, with Parakeet V3 active and other engines available to download.](screenshots/models.png)

## Build from source

Prerequisites: [Rust](https://rustup.rs/) (stable), [Bun](https://bun.sh/), and the platform build tools. On Windows that means Visual Studio 2022 (v143 toolset), the Vulkan SDK, and Ninja. See [BUILD.md](BUILD.md) for the full, platform-specific setup.

```bash
bun install
bun run tauri dev      # run in development
bun run tauri build    # produce a local build
```

On first run, AudioBud opens onboarding, where you pick a model and download it. Grant microphone permission (and, on macOS, accessibility) when prompted.

## AudioBud defaults

- **Hotkey:** `Ctrl+Alt+Space` (Handy ships a different default).
- **Engine on Windows:** `parakeet-tdt-0.6b-v3`, chosen from a local benchmark as the smallest engine that transcribes reliably on this build's DirectML path. The numbers and the decision are in [bench/RESULTS.md](bench/RESULTS.md).

## Command-line flags

AudioBud accepts flags for controlling a running instance and for customizing startup. Remote-control flags are sent to an already-running instance through the single-instance plugin.

```bash
audiobud --toggle-transcription   # toggle recording on/off
audiobud --toggle-post-process    # toggle recording with post-processing
audiobud --cancel                 # cancel the current operation
audiobud --start-hidden           # start without showing the main window
audiobud --no-tray                # start without the system tray icon
audiobud --debug                  # enable verbose logging
audiobud --help                   # list all flags
```

## Debug mode

Open the debug menu with `Ctrl+Shift+D` (Windows and Linux) or `Cmd+Shift+D` (macOS). It shows the app data directory and other diagnostics.

## Manual model installation

If a proxy or firewall blocks the in-app downloader, install models by hand. The model files are hosted by upstream Handy and are publicly reachable from any browser.

1. Find your app data directory. It is shown in **Settings -> About**, or open the debug menu (above). The default paths are:
   - **Windows:** `C:\Users\{username}\AppData\Roaming\tech.amditis.audiobud\`
   - **macOS:** `~/Library/Application Support/tech.amditis.audiobud/`
   - **Linux:** `~/.config/tech.amditis.audiobud/`
2. Create a `models` folder inside it if one does not exist.
3. Download the models you want:
   - Whisper small (487 MB): `https://blob.handy.computer/ggml-small.bin`
   - Whisper turbo (1600 MB): `https://blob.handy.computer/ggml-large-v3-turbo.bin`
   - Parakeet V3 (478 MB): `https://blob.handy.computer/parakeet-v3-int8.tar.gz`
4. Install them:
   - Whisper `.bin` files go directly into `models/`. Keep the exact filenames.
   - Parakeet archives are extracted into `models/`; the extracted directory must be named `parakeet-tdt-0.6b-v3-int8`.
5. Restart AudioBud. The models appear as "Downloaded" under **Settings -> Models**.

AudioBud also auto-discovers custom Whisper GGML `.bin` models placed in the `models` directory. The display name is derived from the filename.

## Platform support

Windows (x64) is the validated target for milestone A. The macOS and Linux code is inherited from Handy and may work, but AudioBud has not been tested there yet. For platform-specific notes in the meantime, see Handy's [documentation](https://github.com/cjpais/Handy).

## Acknowledgments

AudioBud builds directly on [Handy](https://github.com/cjpais/Handy) by CJ Pais and its contributors. Thanks also to:

- **OpenAI Whisper** for the speech recognition model
- **whisper.cpp and ggml** for cross-platform inference and acceleration
- **Silero** for the lightweight VAD
- **Tauri** for the Rust-based app framework

## License

MIT, see [LICENSE](LICENSE). AudioBud is a fork of Handy; the original copyright is retained alongside AudioBud's.
