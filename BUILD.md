# Build instructions

This guide covers how to set up the development environment and build AudioBud from source across different platforms.

## Prerequisites

### All platforms

- [Rust](https://rustup.rs/) (latest stable)
- [Bun](https://bun.sh/) package manager
- [Tauri Prerequisites](https://tauri.app/start/prerequisites/)

### Platform-specific requirements

#### macOS

- Xcode Command Line Tools
- Install with: `xcode-select --install`

##### Apple Silicon Mac

The public macOS release is built for Apple Silicon. No Homebrew runtime library
is required:

```bash
bun install --frozen-lockfile
bun run tauri dev
```

Build the unsigned local app and DMG with:

```bash
bun run tauri build --bundles app,dmg
```

Intel Mac source builds are inherited but not validated. AudioBud does not
publish an Intel Mac artifact.

#### Windows

- Microsoft C++ Build Tools
- Visual Studio 2019/2022 with C++ development tools
- Or Visual Studio Build Tools 2019/2022

#### Linux

- Build essentials
- ALSA development libraries
- Install with:

  ```bash
  # Ubuntu/Debian
  sudo apt update
  sudo apt install build-essential libasound2-dev pkg-config libssl-dev libvulkan-dev vulkan-tools glslc libgtk-3-dev libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libgtk-layer-shell0 libgtk-layer-shell-dev patchelf cmake

  # Fedora/RHEL
  sudo dnf groupinstall "Development Tools"
  sudo dnf install alsa-lib-devel pkgconf openssl-devel vulkan-devel \
    gtk3-devel webkit2gtk4.1-devel libappindicator-gtk3-devel librsvg2-devel \
    gtk-layer-shell gtk-layer-shell-devel \
    cmake

  # Arch Linux
  sudo pacman -S base-devel alsa-lib pkgconf openssl vulkan-devel \
    gtk3 webkit2gtk-4.1 libappindicator-gtk3 librsvg gtk-layer-shell \
    cmake
  ```

## Setup instructions

### 1. Clone the repository

```bash
git clone git@github.com:jamditis/audiobud.git
cd audiobud
```

### 2. Install dependencies

```bash
bun install --frozen-lockfile
```

### 3. Start the development server

```bash
bun run tauri dev
```

### 4. Build for production

```bash
bun run tauri build
```

This compiles a release binary and generates platform-specific bundles. The
release workflow is the source of signed public artifacts. It builds an Apple
Silicon app and DMG on macOS, NSIS and MSI packages on Windows, and a Windows
Store candidate when requested.

## Signed macOS release

The protected `artifact-signing` environment stores the Apple certificate and
App Store Connect values. The release workflow runs all tests before it exposes
those credentials. Tauri signs and notarizes the app bundle. The workflow must
then submit, accept, and staple the finished DMG separately.

Do not use a local signed artifact as a public release. Use the protected
workflow so checksums, the SBOM, provenance, architecture checks, Gatekeeper
checks, and notarization checks all refer to the same bytes.

The credential names, command order, failure rules, and local verification
commands are in [docs/macos-release.md](docs/macos-release.md).

## Linux install from source

The raw binary (`src-tauri/target/release/audiobud`) cannot run standalone — it needs Tauri resource files (tray icons, sounds, VAD model) to be co-located at the expected path.

**Install from the deb bundle** (works on any Linux distro):

```bash
cd /tmp
ar x /path/to/audiobud/src-tauri/target/release/bundle/deb/AudioBud_*_amd64.deb data.tar.gz
tar xzf data.tar.gz
sudo cp usr/bin/audiobud /usr/bin/
sudo cp -r usr/lib/AudioBud /usr/lib/
sudo cp -r usr/share/icons/hicolor/* /usr/share/icons/hicolor/
sudo cp usr/share/applications/AudioBud.desktop /usr/share/applications/
```

After subsequent rebuilds, only the binary needs re-copying:

```bash
sudo cp src-tauri/target/release/audiobud /usr/bin/
```

Resources only need re-copying if they change upstream (new icons, sounds, etc.).

## Troubleshooting

### AppImage build fails on Arch or rolling-release distributions

`linuxdeploy` bundles its own `strip` binary which is too old to process system libraries built with newer toolchains on rolling-release distros (Arch, CachyOS, Manjaro, EndeavourOS).

The error from Tauri:

```
Bundling AudioBud_*_amd64.AppImage
failed to bundle project `failed to run linuxdeploy`
```

Tauri swallows the real linuxdeploy error. To see it, run linuxdeploy manually:

```bash
cd src-tauri/target/release/bundle/appimage
~/.cache/tauri/linuxdeploy-x86_64.AppImage --appimage-extract-and-run \
  --appdir AudioBud.AppDir --plugin gtk --output appimage
```

**Workaround:** The binary, deb, and rpm bundles all build fine — only the AppImage step fails. To skip it:

```bash
bun run tauri build --bundles deb
```

Then install using the deb extraction method above.
