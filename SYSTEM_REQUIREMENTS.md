# System requirements

AudioBud runs local speech-to-text engines with platform-specific acceleration.
The validated public target is Windows 10 or 11 on x64. The v0.6.0 release
candidate targets macOS 11 or later on Apple Silicon. The first-run preflight
check uses the same memory and disk limits listed here.

## Windows (validated)

| Requirement                       | Minimum              | Recommended               | Hard or soft                                                            |
| --------------------------------- | -------------------- | ------------------------- | ----------------------------------------------------------------------- |
| Architecture                      | 64-bit (x64)         | 64-bit (x64)              | Hard — the build is x64 only                                            |
| OS                                | Windows 10 (64-bit)  | Windows 11                | Hard — older builds lack the WebView2 and runtime support the app needs |
| WebView2 runtime                  | Installed or bundled | Current Evergreen runtime | Hard — the app window renders in it (#39)                               |
| Visual C++ runtime, Vulkan loader | Installed            | Installed                 | Hard — the engines link against them (#36, #44)                         |
| Memory (RAM)                      | 4 GB                 | 8 GB                      | Soft — below 4 GB, larger models can run out of memory                  |
| Free disk                         | 4 GB                 | 8 GB+                     | Soft — covers the app plus at least one model download                  |
| Acceleration                      | none (CPU)           | Vulkan or DirectML GPU    | Soft — CPU-only works but is slower                                     |

**Hard** requirements block launch when missing: the preflight check shows what
is missing and how to fix it instead of the app failing silently. **Soft**
requirements never block — a shortfall shows a plain-language warning (for
example, "pick a smaller model" on a low-memory machine) and lets you proceed.

The standard Windows setup can install WebView2 when it is missing. The larger
`AudioBud_<version>_x64-portable-webview-setup.exe` release asset carries a fixed
runtime for PCs that cannot download it during setup. The Microsoft Store
candidate carries Microsoft's offline WebView2 installer.

## macOS on Apple Silicon (v0.6.0 release candidate)

| Requirement  | Minimum                                  | Recommended   | Hard or soft                                           |
| ------------ | ---------------------------------------- | ------------- | ------------------------------------------------------ |
| Architecture | Apple Silicon (arm64)                    | Apple Silicon | Hard — no Intel Mac artifact is published              |
| OS           | macOS 11 or later                        | Current macOS | Hard — the signed app declares macOS 11 as its minimum |
| Permissions  | Microphone and Accessibility permissions | Granted       | Hard — recording and transcript delivery need them     |
| Memory (RAM) | 4 GB                                     | 8 GB          | Soft — larger models need more memory                  |
| Free disk    | 4 GB                                     | 8 GB+         | Soft — covers the app and at least one model           |
| Acceleration | none (CPU)                               | Metal         | Soft — CPU-only transcription works but is slower      |

The planned release is a signed and notarized Developer ID DMG. The tested
local candidate contains an arm64 app and does not depend on Homebrew libraries
at runtime. Publication waits for the remaining release gates.

Whisper Turbo is the recommended Mac model because AudioBud runs Whisper
through Metal on Apple Silicon. Whisper Small uses less disk space and memory.
Parakeet also works through CPU inference and does not require CUDA or an
NVIDIA GPU, but v0.6.0 does not enable Core ML for ONNX models.

Apple Intelligence is optional. AudioBud checks Apple's platform and feature
availability before it offers that post-processing path. Local transcription
does not require Apple Intelligence or macOS 26.

## Other platforms

Intel Mac and Linux builds are not validated. No Intel Mac artifact is
published. Contributors can still build inherited Linux and Intel paths from
source, but those paths are outside the v0.6.0 release support boundary.

## Why these numbers

The model files are the driver. AudioBud's speech models range from roughly
150 MB to about 3 GB each and load into RAM to run, so a 4 GB machine is the
floor (the larger models risk running out of memory mid-transcription) and 8 GB
is comfortable. The disk minimum covers the app plus one model download; add
more if you keep several models. These are a baseline meant to be tuned as real
model sizes settle — they live as constants in `src/lib/preflight.ts`
(`MIN_RAM_MB`, `RECOMMENDED_RAM_MB`, `MIN_FREE_DISK_MB`), so this page and the
in-app check move together when they change.
