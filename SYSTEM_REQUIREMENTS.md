# System requirements

AudioBud runs local speech-to-text engines with platform-specific acceleration,
so the machine needs a few things in place before it works well. The first-run
preflight check (`src/lib/preflight.ts`, issue #51) reads these same numbers and
tells you at launch if something is missing, so this page and the in-app check
never disagree.

Windows x64 is the validated target for the current milestone. macOS and Linux
are inherited from upstream Handy and not yet validated in this fork, so their
rows are best-effort.

## Windows (validated)

| Requirement                       | Minimum             | Recommended            | Hard or soft                                                            |
| --------------------------------- | ------------------- | ---------------------- | ----------------------------------------------------------------------- |
| Architecture                      | 64-bit (x64)        | 64-bit (x64)           | Hard — the build is x64 only                                            |
| OS                                | Windows 10 (64-bit) | Windows 11             | Hard — older builds lack the WebView2 and runtime support the app needs |
| WebView2 runtime                  | Installed           | Installed              | Hard — the app window renders in it (#39)                               |
| Visual C++ runtime, Vulkan loader | Installed           | Installed              | Hard — the engines link against them (#36, #44)                         |
| Memory (RAM)                      | 4 GB                | 8 GB                   | Soft — below 4 GB, larger models can run out of memory                  |
| Free disk                         | 4 GB                | 8 GB+                  | Soft — covers the app plus at least one model download                  |
| Acceleration                      | none (CPU)          | Vulkan or DirectML GPU | Soft — CPU-only works but is slower                                     |

**Hard** requirements block launch when missing: the preflight check shows what
is missing and how to fix it instead of the app failing silently. **Soft**
requirements never block — a shortfall shows a plain-language warning (for
example, "pick a smaller model" on a low-memory machine) and lets you proceed.

## macOS and Linux (inherited, not yet validated)

| Requirement  | Minimum    | Recommended                               |
| ------------ | ---------- | ----------------------------------------- |
| Memory (RAM) | 4 GB       | 8 GB                                      |
| Free disk    | 4 GB       | 8 GB+                                     |
| Acceleration | none (CPU) | Metal (macOS), Vulkan or OpenBLAS (Linux) |

On these platforms the preflight check runs only the soft checks and warns
rather than claiming a hard pass the fork has not verified. Apple Intelligence
post-processing has its own separate check (`check_apple_intelligence_available`:
Apple Silicon plus a recent macOS).

## Why these numbers

The model files are the driver. AudioBud's speech models range from roughly
150 MB to about 3 GB each and load into RAM to run, so a 4 GB machine is the
floor (the larger models risk running out of memory mid-transcription) and 8 GB
is comfortable. The disk minimum covers the app plus one model download; add
more if you keep several models. These are a baseline meant to be tuned as real
model sizes settle — they live as constants in `src/lib/preflight.ts`
(`MIN_RAM_MB`, `RECOMMENDED_RAM_MB`, `MIN_FREE_DISK_MB`), so this page and the
in-app check move together when they change.
