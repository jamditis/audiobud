# AudioBud v0.6.1 release notes

Status: Security candidate; publication is pending.

The self-contained Windows portable installer updates its bundled WebView2
runtime from 150.0.4078.105 to Microsoft's signed 155.0.4283.62 release. Fixed
runtimes require a new application package to receive security updates. The
standard installer continues to use the shared Evergreen runtime, which
receives Microsoft updates separately.

The runtime download is checked against an exact byte count and SHA-256, and
both the CAB and extracted browser must have valid Microsoft signatures. The
portable installer retains its data directory and removes superseded fixed
runtimes during upgrades.

This patch is based on v0.6.0 and excludes later application changes from main.
Experimental output targeting remains Windows-only and off by default. The
application's model recommendations and other behavior are unchanged. Windows can refuse to activate the selected window. If that happens, AudioBud does not send input to a different window; the transcript stays in history and clipboard.

Keep Windows certificate trust information and Evergreen WebView2 updated for
the eMudhra response. AudioBud does not embed PHP, Node.js, or a Next.js server.

The Microsoft Store package and signed updater feed require separate approval
and publication. The existing version-tag workflow can also stage a macOS DMG
in the draft release. Keep that artifact unpublished unless it has received
separate validation and approval; this security task prepares Windows assets.

Public release artifacts must be signed, checksummed, and attested by the
protected workflow before publication. These local smoke-test builds are
unsigned candidates.
