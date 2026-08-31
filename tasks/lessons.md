# Lessons

## Confirm the product before selecting project guidance

- The product is AudioBud, not AudioBash.
- Use AudioBud's repository files and Tauri architecture as the source of truth.
- Do not apply similarly named AudioBash or Electron guidance unless the user asks for it.
- Verify that a sibling feature has the same delivery boundary before reusing
  its implementation. AudioBash pins its own overlay and writes to an internal
  terminal. It does not activate an external application window.

## Use the corrected debloat source

- For this release, “debloat” means the rules in `/Users/jamditis/Desktop/complexity-and-readability.md`.
- The earlier signing-secret path was sent by mistake. Do not read it or use it as project guidance.
- Prefer measured, behavior-preserving cleanup over broad rewrites.

## Respect tool restrictions from the user

- Do not use external skills or plugins for this work.
- Use repository instructions, local tools, tests, and review agents.

## Treat release text as one product surface

- At the end of the v0.6.0 work, sweep the README, changelog, patch notes, repository docs, help pages, public website, download page, and release notes.
- Make all surfaces agree on the version, supported systems, download choice, install steps, permission behavior, update behavior, and known limits.
- Do not publish while a release-facing surface has stale Windows-only, AudioBash, Handy, version, or download text.

## Keep signing commands quiet

- Do not use Tauri `--verbose` while signing passwords or certificate values are in the environment. Tauri can include child-command arguments in its log output.
- A regression test must reject `tauri bundle --verbose` in the release workflow.
- If a signing password appears in a log, stop the build, rotate the password and matching GitHub secret, remove temporary keychains or files, and verify the new credential before another signing run.
- Do not assume that a temporary keychain password and an export password have the same risk. Rotate each reusable value that appeared; temporary keychain values expire when the keychain is removed.

## Verify live coordination scope

- Count the live pull requests before sending a coordination message. The August 28 Office queue is exactly seven pull requests: 311 through 317.
- Address a cross-host peer by the broker-discovered peer ID. A stale named inbox is not proof of delivery.
- A poll-only remote message is delivered only after the recipient checks its peer messages. Keep a durable GitHub comment when coordination affects merge safety.

## Bound release review loops

- Define the release acceptance boundary before the final review pass.
- After the full gate is green, run one final review for critical and important
  defects in the changed paths. Record minor or unrelated ideas as follow-up
  work instead of restarting the release review.
