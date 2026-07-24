# Microsoft Store submission

This checklist records the first AudioBud Microsoft Store submission and the
package rules for future submissions.

Status: submitted for Microsoft Store review on July 24, 2026. Partner Center
shows the app as `In review` with a three-business-day review SLA.

## Submitted app setup

- Product type: `EXE or MSI app`.
- Product name: `AudioBud`.
- Partner Center ID: `f8dadf8b-6512-4707-a4d8-14eea2530bdf`.
- Availability: `United States` only.
- New-region distribution: unchecked.
- Discoverability: available and discoverable in the Microsoft Store.
- Pricing: `Free: no payment necessary`.
- Category: `Productivity`.
- Secondary category: `Utilities & tools`.
- Privacy policy URL: `https://audiobud.amditis.tech/privacy.html`.
- Website: `https://audiobud.amditis.tech/`.
- Support contact: `https://github.com/jamditis/audiobud/issues`.
- Product declarations:
  - Non-Microsoft drivers or NT services: unchecked.
  - Accessibility guidelines tested: unchecked until we complete a Store-ready
    accessibility pass.
  - Pen and ink input: unchecked.
  - Generative AI features: checked.
- Age ratings: IARC preview returned ESRB `Everyone`, PEGI `3+`, Microsoft
  Store `3+`, and similar all-ages ratings.

## Certification notes

Use this text unless the implementation changes:

> AudioBud is a local-first Windows dictation app. Speech-to-text runs on the
> user's device with local models. Optional post-processing is disabled by
> default and only sends text to a user-configured provider after the user
> enables it. Microphone access is required for dictation. No non-Microsoft
> drivers or NT services are installed. The Windows installers support silent
> install and uninstall.

## Package values

Use an MSI package for the first submission because Partner Center supports MSI
silent install parameters directly.

- Submitted package ID: `55846694`.
- Submitted package URL:
  `https://share.amditis.tech/audiobud/downloads/0.4.1/AudioBud_0.4.1_x64_en-US.msi`.
- Submitted MSI SHA-256:
  `9ee9d66d75abf7522bd5986c0c3bb0bb629d6274c80dafe35826aea29ccca3c3`.
- Package validation: completed.
- Malware check: clean.
- Code sign check: signed.
- Silent install status: unknown in Partner Center, with install error code `0`.
- Package URL rule: use a versioned, immutable HTTPS URL for the tested MSI.
- Do not use a GitHub Actions artifact URL. Actions artifacts require
  authentication and expire.
- Host the exact tested MSI at a durable public HTTPS URL before entering it in
  Partner Center.
- Do not use a `/latest` URL.
- App type: `MSI`.
- Architecture: `x64`.
- Installer parameters: `/qn /norestart`.
- Language: `English (United States)`.

For future submissions, record the servicing decision before submitting.
Microsoft Store MSI/EXE distribution uses our hosted installer URL and the app
or installer remains responsible for updates; MSIX is the Store path with
built-in update delivery.

## Store candidate build

After the application has been built without bundling in the signed Windows
release environment, build the Store candidate installers with:

```bash
bun run bundle:store
```

That command layers configs in this order:

1. `src-tauri/tauri.signing.conf.json`
2. `src-tauri/tauri.microsoftstore.conf.json`

The order matters: the package must keep the Artifact Signing command and add
the Store-only offline WebView2 install mode.

Use the generated MSI for Partner Center. The NSIS output is retained so the
same signing and package-verification checks keep running in the release
workflow.

## Package verification

Before saving the package in Partner Center:

1. Verify the MSI Authenticode signature and timestamp.
2. Extract the MSI payload and verify every packaged `.exe` and `.dll` has a
   valid Authenticode signature that chains to a trusted CA.
3. Verify AudioBud-owned packaged files are signed by the expected publisher.
4. For Store candidates, verify the MSI-embedded WebView2 offline installer is
   Authenticode-signed by Microsoft and those extracted bytes are included in
   the SBOM scan payload.
5. Run the real MSI silent install command:

   ```powershell
   msiexec.exe /i .\AudioBud_<version>_x64_en-US.msi /qn /norestart
   ```

6. Run the real MSI silent uninstall command:

   ```powershell
   msiexec.exe /x .\AudioBud_<version>_x64_en-US.msi /qn /norestart
   ```

7. Install and uninstall on a clean Windows machine.
8. Confirm the hosted URL downloads the same SHA-256 digest that was tested.
9. Archive the exact submitted MSI outside the 30-day CI artifact retention
   window.
10. Freeze that URL. Do not replace the binary behind the URL after submission.

## Listing copy

Short description:

> Local dictation for Windows that types your speech into the app you are using.

Description:

> AudioBud is a local-first dictation app for Windows. Press a hotkey, speak,
> and AudioBud types the transcript into the focused text field. Speech-to-text
> runs on your device with local models. Optional post-processing stays off
> until you enable it and configure a provider.
>
> AudioBud can start and stop recording with a hotkey press, or use
> hold-to-talk if you prefer to keep the shortcut held while speaking.
>
> AudioBud includes configurable shortcuts, microphone selection, model
> management, transcript formatting, custom words, word replacements, and
> local personalization controls.

Search terms:

> dictation, speech to text, transcription, voice typing, local speech, whisper

## Screenshot inventory

Existing assets that can seed the Store listing:

- `screenshots/app-general.png`
- `screenshots/app-personalization.png`
- `screenshots/models.png`
- `docs/assets/installer-wizard.png`
- `docs/assets/og-image.png`

Before submission, review each Store upload slot in Partner Center and generate
replacement screenshots if any required size or aspect ratio is missing.
