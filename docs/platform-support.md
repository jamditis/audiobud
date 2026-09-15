# Platform support

## Decision

On September 15, 2026, the project owner approved removing Linux and Nix from
AudioBud's maintained scope for the current release. This resolves the Linux
portion of [#179](https://github.com/jamditis/audiobud/issues/179). It does not
reverse or expand the macOS work.

| Target | Status |
| --- | --- |
| Windows x64 | Validated public target. Keep the signed installers, update feed, and Store work. |
| Apple Silicon macOS | v0.6.0 release-candidate target. Existing clean-Mac, candidate, signing, and release-asset gates still apply. |
| Intel macOS | Inherited and unvalidated. No Intel artifact is planned; this decision does not expand its support. |
| Linux | Not a maintained application target. Remove the inherited application branches and Linux bundles. |
| Nix | No AudioBud flake, NixOS module, Home Manager module, or generated dependency path is maintained. |

Frontend development and platform-neutral checks can still run on Linux. A
Linux-hosted CI job does not imply support for a Linux desktop application.

## Scope of the removal

Remove the Nix files and installation hook, Linux-only input and clipboard
helpers, overlay integration, tray-artwork branches, direct dependencies, and
bundle configuration. Preserve shared logic and its tests. Do not delete code
merely because it is marked Unix or non-Windows: macOS uses shared Unix paths.

Keep Windows and macOS application behavior, the Swift bridge, Metal support,
permissions, signing, notarization, and release checks. Do not change persisted
settings, history, recordings, models, or their compatibility rules. Keep
upstream attribution and genuine dependency names.

Implementation is split into reviewable changes. Do not treat approval of this
support decision as evidence that removal or native validation has completed.
Do not merge or publish a release without the owner's approval.

## Existing Linux and Nix users

The last reviewed source revision before this removal is
[`afed54a29ea9ee3eb94ea92400f83cc72306ca01`](https://github.com/jamditis/audiobud/tree/afed54a29ea9ee3eb94ea92400f83cc72306ca01).
It contains both inherited paths. It is a source reference, not a validated
Linux release or a promise of updates, security fixes, or working Nix builds.

An existing user can retain that revision or maintain a downstream fork.
Updating a Nix input to a revision after removal requires removing or replacing
its module imports and package references. Back up personal configuration and
application data before changing installations. This repository cleanup does
not remove any installed application or user data.

[Upstream Handy](https://github.com/cjpais/Handy) maintains its own Linux path.
Consult its current installation instructions. AudioBud does not promise that
Handy can import AudioBud settings, history, recordings, or models unchanged.

## CI and merge requirements

Keep frontend, formatting, and secret checks on suitable runners. Move the
shared Rust tests and generated-binding checks from the Linux application job
to a retained target before disabling Linux application compilation. Preserve
required check names until a separately reviewed branch-protection change is
approved; do not weaken branch protection to make this cleanup pass.

Before merging the application removal, require passing frontend checks,
generated-binding checks, Windows real-engine tests, and Windows and Apple
Silicon macOS build validation. Build/test runners must not access signing
credentials or publish release artifacts. Desktop smoke tests remain distinct
from automated build validation.

## Issue tracking

- #179 records the approved Linux decision. Preserve any outstanding macOS
  validation work rather than closing it by association.
- #178 is completed only when the Nix removal merges.
- #177 is no longer planned only after its Linux tray path is removed. Do not
  label it as an artwork fix.
- #145 tracks the remaining de-fork work. Unrelated cleanup is not blocked by
  this platform decision.

## Reintroducing Linux

A future proposal needs a named maintainer, a defined distribution and desktop
baseline, a passing real-engine build, and recorded recording-to-paste desktop
tests. Nix support additionally needs a built package and tested NixOS and Home
Manager modules. Test every advertised architecture and desktop configuration;
compiling Vulkan support alone does not validate GPU or desktop behavior.

Git history preserves a starting point. Dormant files are not a substitute for
these checks.
