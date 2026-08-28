# Microsoft Store submission

This checklist records the published AudioBud Microsoft Store listing, its
package checkpoints, and the servicing rules for later submissions.

Status: approved and available in the Microsoft Store. The listing has served
the current v0.4.4 package since August 3, 2026.

Store listing: `https://apps.microsoft.com/detail/xpff8hfmd98gnd`.

## v0.6.0 update submission

The Microsoft Store still serves v0.4.4. Submit a new Windows package after the
v0.6.0 release candidate passes the remote Windows workflow. The macOS DMG does
not go to Partner Center.

Do not upload or submit before all items below are complete:

1. Commit and push the reviewed v0.6.0 source after explicit approval.
2. Build the Store candidate through the protected release workflow.
3. Verify Authenticode, packaged files, silent install, launch, update probe,
   silent uninstall, checksums, the SBOM, and provenance on clean Windows.
4. Host the exact `AudioBud_<version>_x64-setup.exe` candidate at an immutable,
   versioned HTTPS URL. Do not use `/latest`, an expiring Actions URL, or bytes
   that can change behind the URL.
5. Download the hosted file and confirm that its SHA-256 equals the tested
   candidate.
6. Enter the package URL and release text in Partner Center, then save the draft
   for final review.
7. Submit only after the separate publication approval.

Use this Windows Store "What's new" draft:

> AudioBud 0.6.0 improves shortcut startup, settings saves, audio recovery,
> transcript delivery, and release verification. Windows x64 keeps the signed
> in-app update channel. This update also includes smaller binaries and clearer
> error recovery.

The final Partner Center record must capture the submission ID, hosted URL,
SHA-256, workflow run, commit, validation result, and certification result.

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

## Published 0.4.1 package checkpoint

The first submission used an MSI because Partner Center supports MSI silent
install parameters directly. Keep these values as an immutable record of what
Microsoft certified and published:

- Submitted package ID: `55846694`.
- Submitted package URL:
  `https://share.amditis.tech/audiobud/downloads/0.4.1/AudioBud_0.4.1_x64_en-US.msi`.
- Submitted MSI SHA-256:
  `9ee9d66d75abf7522bd5986c0c3bb0bb629d6274c80dafe35826aea29ccca3c3`.
- Package validation: completed.
- Malware check: clean.
- Code sign check: signed.
- Silent install status: unknown in Partner Center, with install error code `0`.
- Package URL rule: use a versioned, immutable HTTPS URL for the tested package.
- Do not use a GitHub Actions artifact URL. Actions artifacts require
  authentication and expire.
- Host the exact tested package at a durable public HTTPS URL before entering it
  in Partner Center.
- Do not use a `/latest` URL.
- App type: `MSI`.
- Architecture: `x64`.
- Installer parameters: `/qn /norestart`.
- Language: `English (United States)`.

The published 0.4.1 MSI cannot receive AudioBud's signed in-app updates because
the app deliberately enables that channel only for NSIS installations. A Store
update submission makes the current package available to new customers, but
Microsoft does not automatically or manually service existing unpackaged
MSI/EXE installations. Existing Store 0.4.1 users therefore need one manual
transition: uninstall the 0.4.1 Store package, then install the current signed
NSIS release to cross onto the supported channel.

## Published 0.4.4 package checkpoint

Microsoft certified and published the signed Store-candidate NSIS build of
v0.4.4. It uses the same installer flavor and signed update path as the direct
release, while layering in the offline WebView2 runtime required for the Store
candidate. The published bytes are the exact Store-candidate binary verified by
the workflow, not the normal GitHub release asset.

- Replacement target version: `0.4.4`.
- Replacement app type: `EXE`.
- Architecture: `x64`.
- Replacement installer parameters: `/S`.
- Immutable package URL:
  `https://share.amditis.tech/audiobud/downloads/0.4.4/AudioBud_0.4.4_x64-setup.exe`.
- Replacement SHA-256:
  `102fcce8214292d2d6f03cd3bf766b8b96b2f934b9e9add9a524de3ae86cf5d5`.
- Language: `English (United States)`.

Verified candidate record on August 2, 2026:

- Candidate tag: `v0.4.4-store-candidate-cd7b3a3e256a`.
- Candidate commit: `cd7b3a3e256aae2c7ecca329733edc9690199652`.
- Protected signing run:
  `https://github.com/jamditis/audiobud/actions/runs/30773521899`.
- The protected workflow passed the Store WebView2, Authenticode, packaged-PE,
  silent install/update-probe/uninstall, SBOM, checksum, and provenance gates.
- SLSA provenance and SPDX 2.3 attestations bind the NSIS digest above to the
  candidate tag, commit, and `.github/workflows/release.yml`.
- A full public HTTPS download from the immutable URL reproduced the same
  SHA-256 after deployment.
- Partner Center package validation: `Passed` on August 3, 2026. Malware is
  `Clean`, code signing is `Signed`, silent install is `Unknown`, and the
  install error code is `0`.
- Partner Center submission ID: `1152921505701569526`.
- Certification and catalog publishing: approved and complete on August 3, 2026.
- The public listing serves v0.4.4 and publishes the submission 2 "What's new"
  notes.

The NSIS bundle type is the runtime signal that enables AudioBud's signed
updater. The release workflow proved that the installed candidate could
initialize and check that feed before accepting the artifact. New Store installs
now check AudioBud's signed update feed by default.

Do not change the URL or replace the hosted bytes after Partner Center accepts
the package. Any later Partner Center replacement remains an explicit release
action.

## Store candidate build

After the application has been built without bundling in the signed Windows
release environment, build the Store candidate installers with:

```bash
bun run bundle:store
```

To produce the signed candidate from a reviewed commit before merge, create an
immutable versioned candidate tag whose suffix binds the tag to that commit,
then dispatch the release workflow at the tag:

```bash
candidate_sha=$(git rev-parse HEAD)
candidate_version=$(jq -r '.version' src-tauri/tauri.conf.json)
candidate_tag="v${candidate_version}-store-candidate-${candidate_sha:0:12}"
git tag -a "$candidate_tag" "$candidate_sha" \
  -m "AudioBud ${candidate_version} Store candidate ${candidate_sha:0:12}"
git push origin "refs/tags/$candidate_tag"
gh workflow run release.yml \
  --ref "$candidate_tag" \
  -f make_release=false \
  -f store_candidate=true \
  -f expected_commit_sha="$candidate_sha"
```

The `artifact-signing` environment must remain limited to `main` and `v*` tags.
Do not add feature branches to the environment policy. The workflow suppresses
the candidate tag's automatic push build and accepts it only through a manual
Store-candidate dispatch. It then requires the tag name's version and short SHA,
the supplied 40-character SHA, and GitHub's dispatched commit to agree exactly.
These workflow checks catch operator mistakes; the protected environment's
deployment policy is the signing authorization boundary. Do not move, reuse, or
delete a candidate tag after its artifact is submitted. Candidate artifacts are
never published to a GitHub release.

That command layers configs in this order:

1. `src-tauri/tauri.signing.conf.json`
2. `src-tauri/tauri.microsoftstore.conf.json`

The order matters: the package must keep the Artifact Signing command and add
the Store-only offline WebView2 install mode.

Use the generated NSIS executable for Partner Center. The MSI output remains in
the workflow so GitHub release compatibility and the existing package checks do
not regress, but it is not the Store servicing package.

## Package verification

Before saving the replacement package in Partner Center:

1. Verify the NSIS executable's Authenticode signature and timestamp.
2. Silently install the NSIS candidate into a clean directory with `/S` and
   verify every packaged `.exe` and `.dll` has a valid Authenticode signature
   that chains to a trusted CA.
3. Verify AudioBud-owned packaged files are signed by the expected publisher.
4. Extract the Store NSIS candidate and verify its embedded WebView2 offline
   installer is Authenticode-signed by Microsoft; include those exact bytes in
   the SBOM scan payload.
5. Run the installed candidate's `--install-update` probe against
   `https://github.com/jamditis/audiobud/releases/download/update-feed/latest.json`
   and require a clean exit.
6. Launch the installed app, complete one dictation, quit, relaunch, and complete
   another dictation. If the test machine has a virtual microphone such as
   NVIDIA Broadcast, also verify that an unavailable or restarting virtual
   device produces a recoverable recording error rather than terminating the
   app.
7. Run the real NSIS silent uninstall command:

   ```powershell
   .\uninstall.exe /S
   ```

8. Install and uninstall on a clean Windows machine.
9. Confirm the hosted URL downloads the same SHA-256 digest that was tested.
10. Archive the exact submitted NSIS executable outside the 30-day CI artifact
    retention window.
11. Freeze that URL. Do not replace the binary behind the URL after submission.

Post-publication verification confirmed that the public Store listing serves
v0.4.4 and that automatic checks against AudioBud's signed update feed are live.
For the next release, use a Store-installed v0.4.4 copy to confirm AudioBud
reports, downloads, verifies, and applies the update.

## Listing copy

The public English (United States) listing currently shows the "What's new"
text, short description, description, feature list, and system requirements
below. Keep this record aligned with the live listing rather than an earlier
draft.

Published "What's new" for v0.4.4:

> AudioBud 0.4.4 adds a signed in-app update channel for new Store installs and
> completes the Windows updater path. The first Store 0.4.1 package requires one
> manual transition before it can receive later signed updates.

Short description:

> Private Windows dictation: press a hotkey, speak, and paste local
> speech-to-text into the app you already use.

Description:

> AudioBud is a local-first dictation app for Windows. Press a hotkey, speak,
> and AudioBud types the transcript into the focused text field, so you can
> write in email, documents, chats, notes, browsers, and other desktop apps
> without switching workflows.
>
> It is built for people who need more control than built-in voice typing:
> configurable shortcuts, hold-to-talk or toggle recording, model choice,
> custom vocabulary, text formatting, history, and optional auto-submit after
> dictation.
>
> AudioBud runs speech-to-text on your computer with local transcription
> models. Audio stays on your device unless you explicitly enable optional
> cloud post-processing and add your own provider key. The default Windows
> setup uses Parakeet V3 for fast local dictation, with additional model options
> available for different language and accuracy needs.
>
> You can tune AudioBud for the way you write: microphone and output device
> selection, spoken-number formatting, custom words, word replacements, recent
> transcription history, and optional on-device personalization from your own
> accepted suggestions. Auto-submit can press Enter after a transcription when
> you want hands-free sending in chats, search boxes, and forms.
>
> Install it, choose a model, set your shortcut, and dictate into the apps you
> already use.

Published feature list:

> - Dictate into any focused Windows text field with a global hotkey.
> - Run speech-to-text locally with Parakeet, Whisper, and other model options.
> - Keep audio on your device unless you choose optional cloud post-processing.
> - Tune shortcuts, hold-to-talk or toggle recording, paste behavior, and audio feedback.
> - Format spoken numbers, currency, and percentages before pasting.
> - Review recent transcriptions, retry entries, and save useful results.
> - Use optional auto-submit to press Enter after dictation in chats, search, and forms.
> - Add custom words and replacements for names, jargon, and recurring mishears.
> - Opt in to on-device learning from accepted dictation suggestions, with export and reset controls.

Published system requirements:

- PC, x64 processor.
- Windows 10 or later. The web listing currently renders the Store-generated
  text "Windows 10 version 0.0 or higher"; AudioBud does not publish `0.0` as
  an application or OS requirement version.
- Memory: 4 GB minimum, 8 GB recommended.
- Graphics: no dedicated GPU required; a Vulkan- or DirectML-compatible GPU is
  recommended.
- Additional project requirements and installer behavior are documented in
  [`SYSTEM_REQUIREMENTS.md`](SYSTEM_REQUIREMENTS.md).

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

## References

- [Publish an update to an MSI/EXE app](https://learn.microsoft.com/en-us/windows/apps/publish/publish-your-app/msi/publish-update-to-your-app-on-store)
- [Microsoft Store MSI/EXE package requirements](https://learn.microsoft.com/en-us/windows/apps/publish/publish-your-app/msi/app-package-requirements)
- [Tauri Microsoft Store distribution](https://v2.tauri.app/distribute/microsoft-store/)
