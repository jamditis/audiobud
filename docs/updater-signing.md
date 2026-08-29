# Updater signing

AudioBud uses a Tauri minisign key to authenticate update payloads. This channel
supports Windows NSIS builds only. It is separate from Windows Authenticode signing:
Authenticode establishes the Windows publisher identity, while the updater key
binds a payload to the update channel configured in the installed app.

macOS uses manual updates in v0.6.0. Its footer links to the current release,
and its DMG uses Developer ID signing and Apple notarization instead of this
minisign update channel. See [macos-release.md](macos-release.md).

The release workflow reads these repository secrets:

- `TAURI_SIGNING_PRIVATE_KEY`
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`

The matching outer-base64 public key lives in the repository variable
`TAURI_SIGNING_PUBLIC_KEY`. Normal releases require it to match the client
public key in `src-tauri/tauri.conf.json`. A planned bridge release is the only
exception: a reviewed `updater-key-bridge.json` pins the release version, old
signing public key, and replacement client public key. Release validation fails
for any other mismatch and also rejects a stale bridge declaration. Each
release publishes and attests the decoded signing public key beside its updater
archive so feed publication and later retries verify the key that signed that
release.

The private values are backed up in the encrypted credential authority on
`houseofjawn` at these pointers:

- `claude/audiobud/updater-private-key`
- `claude/audiobud/updater-private-key-password`

Do not print either value or place it in a command argument. Restore the GitHub
secrets through pipes so the values travel from the credential authority to
GitHub without appearing in terminal output:

```bash
ssh houseofjawn "~/.claude/pass-get claude/audiobud/updater-private-key" |
  gh secret set TAURI_SIGNING_PRIVATE_KEY --repo jamditis/audiobud
ssh houseofjawn "~/.claude/pass-get claude/audiobud/updater-private-key-password" |
  gh secret set TAURI_SIGNING_PRIVATE_KEY_PASSWORD --repo jamditis/audiobud
jq -r '.plugins.updater.pubkey' src-tauri/tauri.conf.json |
  gh variable set TAURI_SIGNING_PUBLIC_KEY --repo jamditis/audiobud
```

After restoring, run the release workflow without creating a release. It must
produce the `.nsis.zip` and `.nsis.zip.sig` CI artifacts and pass the archived
installer signature checks before a release build is attempted.

## Feed publication

The `update-feed` release is a permanent, non-latest container for the mutable
manifest. Create it once before the first updater-capable app release:

```bash
gh release create update-feed --repo jamditis/audiobud --target main \
  --title "AudioBud update feed" \
  --notes "Permanent release container for AudioBud's signed Windows update manifest." \
  --latest=false
```

The tag release workflow tests the updater before the draft becomes public. It
downloads the exact private draft archive with the workflow token, verifies the
tag, commit, provenance, and minisign signature, and then serves only that
archive and an in-memory manifest through localhost HTTPS on a disposable
GitHub-hosted runner. The runner verifies the v0.5.0 installer's Authenticode
identity, installs v0.5.0, creates a non-default setting, and installs the pinned
Moonshine model asset. It applies the private candidate and confirms that the
setting and exact model-file inventory survive. It also verifies the installed
version, Authenticode identity, updater-process quiescence, candidate uninstall
registration, and normal uninstall cleanup. The workflow stops and checks the
server process, clears the temporary credential, and removes and checks the
temporary certificate entries and files, readiness file, model archive, and the
exact updater extraction directory from this run. The preserved app data stays
on the disposable runner until GitHub destroys it. The workflow first stores
the evidence as a private Actions artifact. After all release jobs pass, the
evidence is attested and attached to the joint draft. It does not add a
candidate manifest to the draft or change the live feed.

If any later release job fails after `verify-updater-candidate` succeeds, use
GitHub's `Re-run all jobs` action. Do not use `Re-run failed jobs`. A partial
rerun keeps evidence from the earlier failed attempt, so feed publication will
reject it.

After a stable app release is published, `publish-update-feed.yml` downloads
and verifies the public updater assets and attached evidence again. It requires
evidence from the exact successful release workflow run and attempt for the same
tag, commit, archive hash, and v0.5.0 source installation. It uses the strict
production generator to prepare
`latest.json` inside the workflow run. Before promotion, it saves the current
`update-feed/latest.json`. It uploads the new manifest, downloads it again, and
compares the exact bytes. If upload or read-back verification fails, it restores
and verifies the saved manifest. Before each feed mutation, it also stores and
reads back a durable backup asset named from the outgoing version and full
SHA-256. It never overwrites that asset. The fixed feed tag keeps model mirrors
and other repository releases from changing the URL installed clients query.
Releases whose tags are not exact `vMAJOR.MINOR.PATCH` versions skip this
workflow without changing the live feed.

To restore an older feed, run `publish-update-feed.yml` manually with
`operation` set to `rollback`. Supply the saved release `tag`, its
`manifest_sha256`, the current live feed's `expected_live_sha256`, and set
`confirm_rollback` to true. The workflow derives the durable backup asset name,
checks both hashes, verifies the saved manifest and its signed updater archive,
stores the outgoing live feed as another durable backup, and then publishes the
exact saved bytes. It stops if the live feed changed after approval.

## Planned rotation

An installed app trusts the public key compiled into that version. A planned
rotation therefore needs a bridge release:

1. Pause normal release publication and generate the replacement keypair.
2. Keep the old key in the release secrets and keep its public half in the
   `TAURI_SIGNING_PUBLIC_KEY` variable. Put the new public key in the app config.
   Add `updater-key-bridge.json` to the bridge release change with exactly these
   fields: `version` for the bridge app version, `signing_public_key` for the old
   outer-base64 public key, and `client_public_key` for the replacement
   outer-base64 public key. Publish one bridge update signed and verified by the
   old key. The release workflow accepts the mismatch only when all three values
   match the reviewed declaration.
3. Leave the bridge release as the latest update for the announced migration
   window. Confirm update telemetry and support reports before continuing.
4. Replace the repository secrets with the new private key and password, update
   `TAURI_SIGNING_PUBLIC_KEY` to the matching new public key, remove
   `updater-key-bridge.json`, then publish the next release signed by the new
   key.
5. Retain the old key in the encrypted credential authority for incident
   investigation. Do not use it for new releases.

Clients that miss the bridge window cannot verify a release signed only by the
new key. Direct those users to the current Authenticode-signed installer and
its published checksum; installing it establishes the new public key.

## Suspected or confirmed compromise

Treat a leaked updater key as control of the update channel. There is no
revocation check inside an already installed app.

1. Stop release publication and disable the publish-update-feed workflow.
2. Remove `latest.json` from the `update-feed` release so automatic checks fail
   closed while the incident is investigated.
3. Replace both GitHub secrets, audit repository access and workflow logs, and
   inspect release assets and tags for unauthorized changes.
4. Generate a new keypair and update the public key in the app config.
5. Publish recovery builds through the Authenticode-signed installer path and
   communicate that a manual install is required. Do not use a confirmed
   compromised old key to deliver a bridge release.
6. Restore `latest.json` publication only after the recovery build, release
   assets, checksums, attestations, and new updater signature are verified.

Record the incident timeline and every credential change without recording key
material. Preserve the compromised encrypted backup until the investigation is
closed, then retire it according to the credential-authority policy.
