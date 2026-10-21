# AudioBud v0.6.5 release notes

This release provides Windows x64 installers and a signed Windows in-app update.
macOS publication is deferred.

The self-contained Windows portable installer updates its bundled WebView2
runtime from 150.0.4078.105 to Microsoft's signed 155.0.4283.62 release. Fixed
runtimes require a new application package to receive security updates. The
standard installer continues to use the shared Evergreen runtime, which
receives Microsoft updates separately.

The runtime download is checked against an exact byte count and SHA-256, and
both the CAB and extracted browser must have valid Microsoft signatures. The
portable installer retains its data directory and removes superseded fixed
runtimes during upgrades.

This is a runtime maintenance update based on v0.6.0. The application's model
recommendations and other behavior are unchanged.

Experimental output targeting remains Windows-only and off by default.
Windows can refuse to activate the selected window. If that happens, AudioBud
does not send input to a different window; the transcript stays in history
and clipboard (when copying is enabled).
