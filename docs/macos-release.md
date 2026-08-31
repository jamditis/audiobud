# macOS release runbook

This runbook covers the public Apple Silicon release. AudioBud supports macOS
11 or later and publishes `AudioBud_<version>_macos_aarch64.dmg`. It does not
publish an Intel Mac artifact.

## Trust boundary

The `artifact-signing` GitHub environment is the signing boundary. It must stay
limited to reviewed `main` and `v*` release refs. The workflow runs frontend,
Rust, format, lint, workflow, and release-contract tests before it reads Apple
credentials.

The release job selects
`/Applications/Xcode_26.0.1.app/Contents/Developer`. It stops if this reviewed
toolchain is missing, if the selected SDK is not the macOS 26 SDK, or if the SDK
does not contain `FoundationModels.framework`. It writes the checked SDK path to
`SDKROOT`, so `build.rs` cannot select a different SDK. The final app binary
must link `FoundationModels.framework`; a stub build fails verification.

The environment uses these repository values:

- Variable: `APPLE_API_KEY`
- Variable: `APPLE_API_ISSUER`
- Secret: `APPLE_API_PRIVATE_KEY`
- Secret: `APPLE_CERTIFICATE`
- Secret: `APPLE_CERTIFICATE_PASSWORD`

Do not put their values in source, workflow logs, release notes, or support
messages. The workflow writes the API private key only to its temporary runner
directory and removes it after notarization.

Do not add `--verbose` to a Tauri signing or bundle command. Tauri can print
child-command arguments, including passwords supplied to certificate tools.
The release workflow test rejects this flag in the credential-bearing bundle
step.

## Build order

1. Select the reviewed Xcode toolchain and verify its SDK.
2. Build and test the unsigned application without bundles.
3. Write the temporary App Store Connect private-key file.
4. Import the Developer ID certificate through Tauri's signing process.
5. Build the signed app and DMG with
   `src-tauri/tauri.macos-signing.conf.json`.
6. Resolve the exact app and DMG paths. Reject missing or extra candidates.
7. Notarize and staple the DMG separately.
8. Remove the temporary API private-key file, even after failure.
9. Verify the app signature, notarization, architecture, dependencies, and
   Foundation Models link before generating checksums, the SBOM, provenance,
   and the release artifact.

## Notarize and staple the DMG separately

Tauri submits and staples the app bundle during its macOS build. That does not
put a notarization ticket on the outer DMG. Submit the finished DMG to Apple's
notary service, require an `Accepted` result, and staple that DMG before any
artifact is uploaded.

The workflow uses this command shape with environment values and a temporary
key path:

```bash
xcrun notarytool submit "$DMG_PATH" \
  --key "$APPLE_API_KEY_PATH" \
  --key-id "$APPLE_API_KEY" \
  --issuer "$APPLE_API_ISSUER" \
  --wait \
  --output-format json
xcrun stapler staple "$DMG_PATH"
```

Do not continue after a rejected, invalid, or unknown notarization result.

## Required verification

Run the equivalent checks against the exact artifact that will be published:

```bash
codesign --verify --deep --strict --verbose=2 "$APP_PATH"
codesign --verify --verbose=2 "$DMG_PATH"
spctl --assess --type execute --verbose=4 "$APP_PATH"
spctl --assess --type open --context context:primary-signature \
  --verbose=4 "$DMG_PATH"
xcrun stapler validate "$APP_PATH"
xcrun stapler validate "$DMG_PATH"
hdiutil verify "$DMG_PATH"
file "$APP_PATH/Contents/MacOS/audiobud"
otool -L "$APP_PATH/Contents/MacOS/audiobud"
```

The app and DMG must be accepted as notarized Developer ID software. The main
binary must be arm64 only. Runtime dependency output must contain system paths,
not Homebrew paths.

## Release output

The macOS release artifact contains:

- `AudioBud_<version>_macos_aarch64.dmg`
- `AudioBud_<version>_macos_aarch64_sbom.spdx.json`
- `SHA256SUMS-macos.txt`

GitHub also records provenance and SBOM attestations. The joint draft waits for
the Windows and macOS build jobs and the private Windows updater test. A build
artifact, updater-test artifact, or draft release is not approval to publish
the tag or release.

Syft must collect metadata for every staged app entry. Before attestation, the
workflow runs `scripts/validate-sbom-file-checksums.ts` against the app root. It
requires one unique SPDX record for every filesystem entry. Syft's placeholder
is permitted for actual directories and symlinks. Syft does not follow those
symlinks. Every regular-file checksum must be real, supported, and equal to the
staged bytes, and each regular-file record must include SHA-256. Any inventory
or checksum mismatch stops the job before attestation.

The Windows release job uses the same final validation contract. Syft 1.49.0
cannot collect directory-source file digests on Windows, so that job first
completes only Syft's exact zero-SHA-1 regular-file placeholder form from the
staged bytes. A changed document records the completion tool and a derived SPDX
namespace. A document with native valid digests stays byte-for-byte unchanged.
The macOS job does not use this Windows-only compatibility step.

## Local credential handling

Local signing can confirm the procedure before a remote run. Keep certificates,
private keys, and passwords outside the repository with owner-only permissions.
Never print them. Local success does not replace the protected workflow because
the public checksums and attestations must bind to the remote release bytes.
