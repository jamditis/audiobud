# Updater signing

AudioBud uses a Tauri minisign key to authenticate update payloads. This is
separate from Windows Authenticode signing: Authenticode establishes the
publisher identity for Windows, while the updater key binds a payload to the
update channel configured in the installed app.

The release workflow reads these repository secrets:

- `TAURI_SIGNING_PRIVATE_KEY`
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`

The public key lives in `src-tauri/tauri.conf.json`. The private values are
backed up in the encrypted credential authority on `houseofjawn` at these
pointers:

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
```

After restoring, run the release workflow without creating a release. It must
produce the `.nsis.zip` and `.nsis.zip.sig` CI artifacts and pass the archived
installer signature checks before a release build is attempted.

## Planned rotation

An installed app trusts the public key compiled into that version. A planned
rotation therefore needs a bridge release:

1. Pause normal release publication and generate the replacement keypair.
2. Keep the old key in the release secrets. Put the new public key in the app
   config and publish one bridge update signed by the old key.
3. Leave the bridge release as the latest update for the announced migration
   window. Confirm update telemetry and support reports before continuing.
4. Replace the repository secrets with the new private key and password, then
   publish the next release signed by the new key.
5. Retain the old key in the encrypted credential authority for incident
   investigation. Do not use it for new releases.

Clients that miss the bridge window cannot verify a release signed only by the
new key. Direct those users to the current Authenticode-signed installer and
its published checksum; installing it establishes the new public key.

## Suspected or confirmed compromise

Treat a leaked updater key as control of the update channel. There is no
revocation check inside an already installed app.

1. Stop release publication and disable the publish-update-feed workflow.
2. Remove `latest.json` from the current release so automatic checks fail
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
