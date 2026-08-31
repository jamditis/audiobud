# AudioBud v0.6.0 draft release notes

Status: draft. AudioBud v0.6.0 is not published yet. Clean-Mac checks, remote
Windows and macOS candidate tests, and exact draft-asset verification must pass
before publication.

AudioBud v0.6.0 prepares the first supported Mac release and hardens the shared
app for Windows and macOS.

## What is new

- The release candidate targets Apple Silicon Macs running macOS 11 or later
  with a signed and notarized Developer ID DMG.
- Whisper Turbo is the recommended Mac model and uses Metal acceleration.
  Whisper Small is the lighter option. Parakeet remains available through CPU
  inference and does not require NVIDIA hardware.
- macOS permission checks now stay in sync with Microphone and Accessibility
  access, including access that is later revoked.
- Shortcut, settings, slider, input, audio, and model-loading failures now
  recover without duplicate listeners, stale state, or stuck modifier keys.
- The pre-final local candidate binary was about 4.7% smaller than the v0.5.0
  baseline. Its measured idle memory use was about 4.4% lower on the test Mac.
- Release assets include SHA-256 checksums, an SPDX SBOM, and GitHub build
  provenance.

## Planned downloads

- macOS: `AudioBud_<version>_macos_aarch64.dmg`
- Windows setup: `AudioBud_<version>_x64-setup.exe`
- Windows MSI: `AudioBud_<version>_x64_en-US.msi`

These names describe the planned v0.6.0 release assets. They are not public
downloads until the release is published. The Mac DMG is for Apple Silicon. No
Intel Mac build is included. Linux builds are not validated for this release.

## Install on macOS after publication

1. Open the DMG.
2. Drag AudioBud to Applications.
3. Open AudioBud from Applications.
4. Grant Microphone and Accessibility permissions when asked.
5. Select or download a local transcription model. Whisper Turbo is the Mac
   recommendation; Whisper Small uses less disk space and memory.

macOS updates are manual in v0.6.0. Use the release link in the app footer,
download the next DMG, and replace AudioBud in Applications.

## Install on Windows

Use the setup executable for a normal or portable install. Windows NSIS builds
continue to use AudioBud's signed in-app update feed. The Microsoft Store still
serves v0.4.4 until the separate v0.6.0 Partner Center update is tested,
submitted, and accepted.

## Verify a download

Compare Windows installers against `SHA256SUMS.txt` and the Mac DMG against
`SHA256SUMS-macos.txt` on the GitHub release. Windows users must also verify the
Authenticode publisher. Mac users must confirm that Gatekeeper accepts the DMG
and that its notarization ticket validates. The full commands are in
[README.md](README.md#verify-your-download).

## Known limits

- Experimental output targeting is Windows-only and off by default. Windows
  can refuse to activate the selected window. If this occurs, AudioBud does not
  send input to a different window. The transcript stays available in history
  and clipboard when copying is enabled.
- Apple Intelligence post-processing is optional and depends on Apple's own
  availability checks. It is not required for local transcription.
- A native macOS shortcut constructor that never returns can outlive its
  five-second caller timeout. AudioBud continues startup, but only the upstream
  native library can end that detached call.
- macOS updates are manual. The in-app updater is enabled for Windows NSIS
  builds only.
- Intel Mac and Linux builds are not validated.
