# Microsoft Store submission

This checklist records the published AudioBud Microsoft Store listing, the
first package checkpoint, and the servicing rules for replacement submissions.

Status: approved and available in the Microsoft Store on August 2, 2026.

Store listing: `https://apps.microsoft.com/detail/xpff8hfmd98gnd`.

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

## Replacement servicing decision

Replace the Store package with the signed Store-candidate NSIS build of the
current AudioBud release. It uses the same installer flavor and signed update
path as the direct release, while layering in the offline WebView2 runtime that
the Store candidate requires. Do not substitute the normal GitHub release asset:
record and host the exact Store-candidate binary verified by the workflow.

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
- Partner Center submission status: ready for the 0.4.4 package replacement;
  the published listing continues to serve 0.4.1 until Microsoft certifies it.

Use the generated NSIS executable for the replacement Store submission. The
NSIS bundle type is the runtime signal that enables AudioBud's signed updater,
and the release workflow must prove that the installed candidate can initialize
and check that feed before the artifact is accepted. After that one-time
transition, Store users receive signed updates through AudioBud's update feed.

Do not change the URL or replace the hosted bytes after Partner Center accepts
the package. Creating or submitting the Partner Center update remains an
explicit release action.

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

After Microsoft certifies the replacement, verify the public Store listing
installs 0.4.4. Then install a newer test build through the signed update feed
and confirm AudioBud reports, downloads, verifies, and applies the update.

## Listing copy

What's new in this version:

> AudioBud 0.4.4 adds a signed in-app update channel for new Store installs and
> completes the Windows updater path. The first Store 0.4.1 package requires one
> manual transition before it can receive later signed updates.

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

## References

- [Publish an update to an MSI/EXE app](https://learn.microsoft.com/en-us/windows/apps/publish/publish-your-app/msi/publish-update-to-your-app-on-store)
- [Microsoft Store MSI/EXE package requirements](https://learn.microsoft.com/en-us/windows/apps/publish/publish-your-app/msi/app-package-requirements)
- [Tauri Microsoft Store distribution](https://v2.tauri.app/distribute/microsoft-store/)
