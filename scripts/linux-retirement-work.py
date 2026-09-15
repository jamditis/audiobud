from pathlib import Path
import json
import re
import subprocess
from tree_sitter import Language, Parser
import tree_sitter_rust

BASE = '720896a55e5bfecdd8dcfd7dd1716dfce833c729'
assert subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip() == BASE
parser = Parser(Language(tree_sitter_rust.language()))

def read(path):
    return Path(path).read_text()

def write(path, text):
    Path(path).parent.mkdir(parents=True, exist_ok=True)
    Path(path).write_text(text)

def replace(path, old, new, count=1):
    text = read(path)
    assert text.count(old) == count, (path, 'unexpected replacement count', old, text.count(old))
    write(path, text.replace(old, new))

def walk(node):
    yield node
    for child in node.named_children:
        yield from walk(child)

def apply_edits(data, edits):
    selected = []
    for start, end, value in sorted(edits, key=lambda e: (e[0], -e[1])):
        if selected and start >= selected[-1][0] and end <= selected[-1][1]:
            continue
        assert not selected or start >= selected[-1][1], ('overlapping edits', selected[-1], (start, end))
        selected.append((start, end, value))
    for start, end, value in reversed(selected):
        data = data[:start] + value + data[end:]
    return data

def whole_lines(data, start, end):
    line_start = data.rfind(b'\n', 0, start) + 1
    if not data[line_start:start].strip():
        start = line_start
    line_end = data.find(b'\n', end)
    if line_end == -1:
        line_end = len(data)
    if not data[end:line_end].strip(b' \t,;'):
        end = min(line_end + 1, len(data))
    return start, end

def test_names(data):
    root = parser.parse(data).root_node
    names = set()
    for node in walk(root):
        if node.type != 'function_item':
            continue
        prev = node.prev_named_sibling
        attrs = []
        while prev and prev.type in ('attribute_item', 'line_comment', 'block_comment'):
            attrs.append(prev.text)
            prev = prev.prev_named_sibling
        if any(b'#[test]' in attr for attr in attrs):
            names.add(node.child_by_field_name('name').text.decode())
    return names

before_tests = {str(p): test_names(p.read_bytes()) for p in Path('src-tauri/src').rglob('*.rs')}
removed_linux_tests = set()

# Delete only syntax nodes guarded exclusively for Linux. Preserve comments and
# platform-neutral functions outside each removed node. Reject unknown cfg forms.
for path in sorted(Path('src-tauri/src').rglob('*.rs')):
    data = path.read_bytes()
    root = parser.parse(data).root_node
    assert not root.has_error, ('baseline Rust parse failed', str(path))
    edits = []
    for node in walk(root):
        if node.type != 'attribute_item' or b'linux' not in node.text:
            continue
        compact = re.sub(rb'\s+', b'', node.text)
        if compact == b'#[cfg(target_os="linux")]':
            target = node.next_named_sibling
            while target and target.type in ('attribute_item', 'line_comment', 'block_comment'):
                target = target.next_named_sibling
            assert target is not None, ('missing cfg target', str(path), node.start_point)
            start = node.start_byte
            previous = node.prev_named_sibling
            while previous and previous.type in ('attribute_item', 'line_comment', 'block_comment'):
                gap = data[previous.end_byte:start]
                if gap.strip() or gap.count(b'\n') > 1:
                    break
                start = previous.start_byte
                previous = previous.prev_named_sibling
            end = target.end_byte
            if node.parent.type in ('parameters', 'arguments'):
                after = target.next_sibling
                if after and after.type == ',':
                    end = after.end_byte
            start, end = whole_lines(data, start, end)
            if target.type == 'function_item':
                name = target.child_by_field_name('name').text.decode()
                if name in before_tests[str(path)]:
                    removed_linux_tests.add((str(path), name))
            edits.append((start, end, b''))
        elif compact == b'#[cfg(not(target_os="linux"))]':
            start, end = whole_lines(data, node.start_byte, node.end_byte)
            edits.append((start, end, b''))
        elif compact == b'#[cfg_attr(not(target_os="linux"),allow(dead_code))]':
            edits.append((node.start_byte, node.end_byte, b'#[cfg(test)]'))
        elif compact in (b'#[cfg(any(windows,target_os="linux"))]', b'#[cfg(any(target_os="windows",target_os="linux"))]'):
            edits.append((node.start_byte, node.end_byte, b'#[cfg(target_os = "windows")]'))
        elif compact.startswith(b'#[cfg(not(any('):
            text = node.text.decode()
            text = re.sub(r',\s*target_os\s*=\s*"linux"', '', text)
            assert 'linux' not in text
            edits.append((node.start_byte, node.end_byte, text.encode()))
        else:
            raise AssertionError(('unreviewed cfg', str(path), node.text))
    changed = apply_edits(data, edits)
    assert not parser.parse(changed).root_node.has_error, ('edited Rust parse failed', str(path))
    if changed != data:
        path.write_bytes(changed)

def function(path, name, text):
    data = Path(path).read_bytes()
    nodes = [n for n in walk(parser.parse(data).root_node) if n.type == 'function_item' and n.child_by_field_name('name').text.decode() == name]
    assert len(nodes) == 1, (path, name, len(nodes))
    n = nodes[0]
    Path(path).write_bytes(data[:n.start_byte] + text.encode() + data[n.end_byte:])

function('src-tauri/src/clipboard.rs', 'send_paste_key_combo', '''fn send_paste_key_combo(enigo: &mut Enigo, paste_method: &PasteMethod) -> Result<(), String> {
    match paste_method {
        PasteMethod::CtrlV => input::send_paste_ctrl_v(enigo),
        PasteMethod::CtrlShiftV => input::send_paste_ctrl_shift_v(enigo),
        PasteMethod::ShiftInsert => input::send_paste_shift_insert(enigo),
        _ => Err("Invalid paste method for clipboard paste".to_string()),
    }
}''')
function('src-tauri/src/audio_toolkit/utils.rs', 'get_cpal_host', '''pub fn get_cpal_host() -> cpal::Host {
    cpal::default_host()
}''')
function('src-tauri/src/shortcut/mod.rs', 'get_available_typing_tools', '''pub fn get_available_typing_tools() -> Vec<String> {
    // Keep the existing command contract for older webviews. Linux helper
    // discovery is retired; persisted typing_tool values are inert.
    vec!["auto".to_string()]
}''')

# Remove now-redundant final returns without touching the retained defaults.
for value in ['KeyboardImplementation::HandyKeys', 'PasteMethod::CtrlV', 'OverlayPosition::Bottom']:
    path = 'src-tauri/src/settings.rs'
    old = 'return ' + value + ';'
    if old in read(path):
        replace(path, old, value)

# Retain the serialized field and all old variants. They cannot run a helper.
replace('src-tauri/src/settings.rs', 'pub enum TypingTool {', '''// Compatibility-only: keep old settings and generated bindings readable.
// No retained platform discovers or executes these retired Linux tools.
pub enum TypingTool {''')
with Path('src-tauri/src/settings.rs').open('a') as out:
    out.write('''
#[cfg(test)]
mod platform_retirement_tests {
    use super::TypingTool;

    #[test]
    fn legacy_typing_tool_values_round_trip_without_a_platform_backend() {
        for value in ["auto", "wtype", "kwtype", "dotool", "ydotool", "xdotool"] {
            let encoded = serde_json::Value::String(value.to_string());
            let tool: TypingTool = serde_json::from_value(encoded.clone()).unwrap();
            assert_eq!(serde_json::to_value(tool).unwrap(), encoded);
        }
    }
}
''')

replace('src-tauri/src/tray.rs', '    Colored, // Pink/colored theme for Linux\n', '')
function('src-tauri/src/tray.rs', 'get_current_theme', '''pub fn get_current_theme(app: &AppHandle) -> AppTheme {
    if let Some(main_window) = app.get_webview_window("main") {
        match main_window.theme().unwrap_or(Theme::Dark) {
            Theme::Light => AppTheme::Light,
            _ => AppTheme::Dark,
        }
    } else {
        AppTheme::Dark
    }
}''')
path = 'src-tauri/src/tray.rs'
text = read(path)
text, n = re.subn(r'        // Colored theme[^\n]*\n(?:        \(AppTheme::Colored[^\n]*\n){3}', '', text)
assert n == 1
text = text.replace('/// Gets the current app theme, with Linux defaulting to Colored theme', '/// Gets the current app theme, using dark artwork when no window exists.')
write(path, text)
with Path(path).open('a') as out:
    out.write('''
#[cfg(test)]
mod platform_retirement_tests {
    use super::{get_icon_path, AppTheme, TrayIconState};

    #[test]
    fn retained_tray_states_resolve_existing_audiobud_assets() {
        for theme in [AppTheme::Dark, AppTheme::Light] {
            for state in [TrayIconState::Idle, TrayIconState::Recording, TrayIconState::Transcribing] {
                let path = get_icon_path(theme.clone(), state);
                assert!(path.starts_with("resources/tray_"));
                assert!(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(path).is_file());
            }
        }
    }
}
''')
for name in ['handy.png', 'recording.png', 'transcribing.png']:
    path = Path('src-tauri/resources') / name
    assert path.is_file()
    for source in Path('src-tauri/src').rglob('*.rs'):
        assert f'"resources/{name}"' not in source.read_text(), (name, source)
    path.unlink()

# Current comments must not describe a removed backend. Historical reports and
# unsupported-platform regression cases remain untouched.
comment_replacements = {
    'Windows/Linux': 'Windows',
    'Windows and Linux': 'Windows',
    'Windows + Linux': 'Windows',
    'the Linux typing tool, ': '',
    'Linux typing tool, ': '',
}
for path in Path('src-tauri/src').rglob('*.rs'):
    text = path.read_text()
    lines = text.splitlines(keepends=True)
    for i, line in enumerate(lines):
        if line.lstrip().startswith('//'):
            for old, new in comment_replacements.items():
                line = line.replace(old, new)
            lines[i] = line
    changed = ''.join(lines)
    if changed != text:
        path.write_text(changed)

cargo = 'src-tauri/Cargo.toml'
text = read(cargo)
text, n = re.subn(r'\n\[target\.\'cfg\(target_os = "linux"\)\'\.dependencies\]\n.*?(?=\n\[)', '\n', text, flags=re.S)
assert n == 1
write(cargo, text)
config_path = 'src-tauri/tauri.conf.json'
config = json.loads(read(config_path))
assert 'linux' in config['bundle']
del config['bundle']['linux']
write(config_path, json.dumps(config, indent=2, ensure_ascii=False) + '\n')
path = 'src-tauri/capabilities/desktop.json'
config = json.loads(read(path))
config['platforms'].remove('linux')
write(path, json.dumps(config, indent=2) + '\n')
replace('src-tauri/build.rs', 'fn main() {', '''fn main() {
    // Inspect the compilation target, not the OS running this build script.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("linux") {
        panic!("AudioBud no longer supports Linux application builds. See docs/platform-support.md. Frontend-only development remains available.");
    }
''')

# No command or settings-schema removal: bindings stay compatible. Only the
# Linux-only control and implementation are removed from the frontend graph.
Path('src/components/settings/TypingTool.tsx').unlink()
path = 'src/components/settings/advanced/AdvancedSettings.tsx'
text = read(path)
text, imports = re.subn(r'^import .*TypingTool.*\n', '', text, flags=re.M)
text, uses = re.subn(r'^\s*<TypingToolSetting\b[^>]*\/>\s*\n', '\n', text, flags=re.M)
assert imports == 1 and uses == 1, (imports, uses)
write(path, text)
path = 'src/components/settings/general/GeneralSettings.tsx'
text = read(path)
text = re.sub(r'^import \{ type \} from "@tauri-apps/plugin-os";\n', '', text, flags=re.M)
text = re.sub(r'^\s*const isLinux = [^\n]+\n', '', text, flags=re.M)
text = text.replace('!isLinux && ', '').replace(' and on Linux', '').replace(' and Linux', '')
write(path, text)
path = 'src/components/settings/ShowOverlay.tsx'
replace(path, 'import { useOsType } from "../../hooks/useOsType";\n', '')
text = read(path)
text, n = re.subn(r'    // The fine grid free-positions.*?    const supportsFineGrid = osType !== "linux";\n', '    // Both retained platforms honor the fine grid through set_position.\n', text, flags=re.S)
assert n == 1
write(path, text.replace(' && supportsFineGrid', ''))
path = 'src/components/settings/history/HistorySettings.tsx'
replace(path, 'import { readFile } from "@tauri-apps/plugin-fs";\n', '')
replace(path, 'import { useOsType } from "@/hooks/useOsType";\n', '')
replace(path, '  const osType = useOsType();\n', '')
text = read(path)
text, n = re.subn(r'          if \(osType === "linux"\) \{.*?\n          \}\n', '', text, flags=re.S)
assert n == 1
write(path, text.replace('    [osType],', '    [],'))
replace('src/hooks/useOsType.ts', ' || osType === "linux"', '')
replace('src/lib/utils/keyboard.ts', ' | "linux"', '')
path = 'src/lib/paste-methods.ts'
replace(path, ' || osType === "linux"', '', count=2)
replace(path, '  if (osType === "linux") {\n    methods.push("external_script");\n  }\n\n', '')
path = 'src/lib/paste-methods.test.ts'
text = read(path).replace('pasteMethodModifierForOs("linux")', 'pasteMethodModifierForOs("unknown")')
text = text.replace('on Windows and Linux', 'on Windows')
text, n = re.subn(r'    expect\(pasteMethodsForOs\("linux"\)\)\.toEqual\(\[.*?\]\);\n', '    expect(pasteMethodsForOs("unknown")).toEqual(["ctrl_v", "none"]);\n', text, flags=re.S)
assert n == 1
text = text.replace('filters the confirmation-gated external script from Linux profiles', 'does not offer external scripts on retained platforms')
text = text.replace('profilePasteMethodsForOs("linux")', 'profilePasteMethodsForOs("windows")')
write(path, text)
replace('src/lib/preflight.ts', ' | "linux"', '')
replace('src/lib/preflight-facts.test.ts', 'platform: "linux"', 'platform: "macos"')
replace('src-tauri/resources/default_settings.json', 'Platform-specific: ctrl+space (Windows/Linux), alt+space (macOS)', 'Platform-specific: ctrl+alt+space (Windows), option+space (macOS)')

# Remove matching obsolete locale keys in every locale; retain translated prose.
for path in Path('src/i18n/locales').glob('*/translation.json'):
    data = json.loads(path.read_text())
    advanced = data['settings']['advanced']
    assert 'typingTool' in advanced, str(path)
    del advanced['typingTool']
    def clean(obj):
        if isinstance(obj, dict):
            for key in list(obj):
                if key == 'linux':
                    del obj[key]
                else:
                    obj[key] = clean(obj[key])
        elif isinstance(obj, str):
            obj = re.sub(r'Windows\s*(?:/|and|y|e|et|und|i|и|및|和|、|と|ve|atau|og|och|ja|a|en|&|および)\s*Linux', 'Windows', obj)
        return obj
    clean(data)
    description = advanced['overlay']['description']
    if re.search('Linux', description, re.I):
        pieces = re.split(r'(?<=[.!?。！？])\s*', description)
        kept = [piece for piece in pieces if piece and not re.search('Linux', piece, re.I)]
        if kept:
            advanced['overlay']['description'] = ' '.join(kept)
    path.write_text(json.dumps(data, indent=2, ensure_ascii=False) + '\n')

# Current docs change; prior release entries and design records remain archival.
replace('README.md', 'candidate target. Intel Mac and Linux builds are not validated.', 'candidate target. Intel Mac builds are not validated. Linux application builds\nand Nix packaging are no longer supported. See [platform support](docs/platform-support.md)\nfor the decision and existing-user guidance.')
replace('README.md', '   - Linux: `~/.config/tech.amditis.audiobud/`\n', '')
replace('README.md', 'on Windows and Linux,', 'on Windows,')
path = 'SYSTEM_REQUIREMENTS.md'
text = read(path)
start = text.index('## Other platforms\n')
end = text.index('## Why these numbers', start)
write(path, text[:start] + '''## Other platforms

Intel Mac builds are not validated. No Intel Mac artifact is published. The
inherited Intel source path remains outside the v0.6.0 release support boundary.

Linux application builds and Nix packaging are no longer supported. See
[platform support](docs/platform-support.md) for the last reviewed source
revision and existing-user guidance. Frontend-only development can still run on
Linux; it is not a Linux application build.

''' + text[end:])
path = 'BUILD.md'
text = read(path)
start = text.index('#### Linux\n')
end = text.index('## Setup instructions', start)
text = text[:start] + text[end:]
text = text[:text.index('## Linux install from source')]
text = text.replace('This guide covers how to set up the development environment and build AudioBud from source across different platforms.', '''This guide covers native AudioBud development on Windows and macOS. Linux
application builds and Nix packaging are no longer supported. Frontend-only
work (`bun run dev`, `bun run build`, and unit tests) can still run on Linux.
See [platform support](docs/platform-support.md) for scope and transition notes.''')
text += '''## Unsupported native targets

`bun run tauri dev`, `build`, and `bundle` reject a Linux application target
with the support-policy message before starting the native build. The Rust
build script also checks the compilation target for direct Cargo builds.
Neither guard blocks frontend-only development. No Linux bundle or Nix package
is published or maintained.
'''
write(path, text)
path = 'AGENTS.md'
text = read(path).replace('Windows/Linux', 'Windows')
old = '- **Linux and Intel Mac**: Inherited source-build paths only; neither is a validated release target. Linux uses Vulkan, has limited Wayland support, and its overlay uses GTK layer shell (disable with `HANDY_NO_GTK_LAYER_SHELL=1`).'
assert old in text
text = text.replace(old, '''- **Intel Mac**: Inherited and unvalidated source path. No Intel artifact is planned.
- **Linux and Nix**: Retired from the maintained application scope. Do not restore Linux backends, bundles, or Nix hooks without a new approved validation proposal. See [platform support](docs/platform-support.md). Linux-hosted frontend checks do not imply application support.''')
text = text.replace('Handy supports command-line parameters on all platforms', 'AudioBud supports command-line parameters on its retained platforms')
write(path, text)
path = 'CONTRIBUTING.md'
text = read(path)
first = text.index('\n\n')
write(path, text[:first] + '''

Native application work targets Windows and the Apple Silicon macOS release
candidate. Linux application support and Nix packaging are retired. Frontend
work can still use Linux. Read [platform support](docs/platform-support.md)
before changing platform code or CI.''' + text[first:])
path = 'RELEASE_NOTES.md'
text = read(path).replace('Linux builds are not validated for this release.', 'Linux application builds and Nix packaging are no longer supported. See\n[platform support](docs/platform-support.md) for existing-user guidance.')
text = text.replace('- Intel Mac and Linux builds are not validated.', '- Intel Mac builds are not validated. Linux application builds and Nix packaging are retired.')
write(path, text)
path = 'CHANGELOG.md'
text = read(path)
end = text.index('\n## ', text.index('## 0.6.0') + 1)
current = text[:end].replace('Intel Mac and Linux builds are not validated.', 'Intel Mac builds are not validated.')
current = current.replace('### Changed\n', '''### Changed

- Removed unmaintained Linux application backends and bundles, Nix packaging,
  generated Nix dependency files, and the Nix installation hook. Windows and
  Apple Silicon macOS work are retained. Existing users should read
  [platform support](docs/platform-support.md) before updating source revisions.
''', 1)
write(path, current + text[end:])
path = 'docs/roadmap.html'
text = read(path)
text = text.replace('v1.0.0 &mdash; remaining platforms', 'v1.0.0 &mdash; platform hardening')
old = '''                Finish the rebrand down to the plumbing, then decide whether to
                validate Intel Mac and Linux builds or leave them as unsupported
                source-build paths. Apple Silicon macOS is the active v0.6.0
                release candidate.'''
assert old in text
text = text.replace(old, '''                Finish the rebrand and harden the retained platforms. Linux
                application support and Nix packaging are retired. Intel Mac
                builds remain unvalidated. Apple Silicon macOS is the active
                v0.6.0 release candidate.''')
write(path, text)
path = 'superpowers/DEFERRED-issues.md'
text = read(path)
first = text.index('\n\n')
write(path, text[:first] + '''

> Platform scope update, September 15, 2026: Linux application support and Nix
> packaging are retired. Apple Silicon macOS remains the active release
> candidate. The older platform findings below are historical context, not the
> current support contract. See [platform support](../docs/platform-support.md).''' + text[first:])
replace('scripts/release-documentation.test.ts', 'expect(requirements).toContain("Intel Mac and Linux builds are not validated");', '''expect(requirements).toContain("Intel Mac builds are not validated");
    expect(requirements).toContain("Linux application builds and Nix packaging are no longer supported");''')

write('scripts/native-platform.ts', '''/** Linux is a development host for the frontend, not an application target. */
export const linuxBuildMessage =
  "AudioBud no longer supports Linux application builds or Nix packaging. See docs/platform-support.md. Frontend-only development remains available with bun run dev or bun run build.";

export function nativeTargetError(
  args: readonly string[],
  host: string,
  configuredTarget?: string,
): string | null {
  if (!["dev", "build", "bundle"].includes(args[0] ?? "")) return null;
  const nativeArgs = args.slice(0, args.indexOf("--") < 0 ? args.length : args.indexOf("--"));
  if (nativeArgs.includes("--help") || nativeArgs.includes("-h")) return null;
  let target = configuredTarget;
  for (let i = 1; i < nativeArgs.length; i++) {
    const arg = nativeArgs[i];
    if (arg === "--target" || arg === "-t") target = nativeArgs[++i];
    else if (arg.startsWith("--target=")) target = arg.slice("--target=".length);
  }
  if (target) return /(^|-)linux(-|$)/.test(target) ? linuxBuildMessage : null;
  return host === "linux" ? linuxBuildMessage : null;
}
''')
write('scripts/tauri.ts', '''import { nativeTargetError } from "./native-platform";

const args = process.argv.slice(2);
const error = nativeTargetError(args, process.platform, process.env.CARGO_BUILD_TARGET);
if (error) {
  console.error(error);
  process.exit(1);
}

try {
  // Resolve the installed CLI, never download a different package version.
  const result = Bun.spawnSync(["bunx", "--no-install", "tauri", ...args], {
    stdin: "inherit",
    stdout: "inherit",
    stderr: "inherit",
  });
  process.exit(result.exitCode);
} catch (cause) {
  console.error("Could not start the installed Tauri CLI. Run bun install --frozen-lockfile.", cause);
  process.exit(1);
}
''')
pkg = json.loads(read('package.json'))
assert pkg['scripts']['tauri'] == 'tauri'
pkg['scripts']['tauri'] = 'bun scripts/tauri.ts'
write('package.json', json.dumps(pkg, indent=2) + '\n')
write('scripts/platform-support.test.ts', '''import { describe, expect, test } from "bun:test";
import { existsSync, readFileSync, readdirSync } from "node:fs";
import { resolve, join } from "node:path";
import { linuxBuildMessage, nativeTargetError } from "./native-platform";

const root = resolve(import.meta.dir, "..");
const read = (path: string) => readFileSync(resolve(root, path), "utf8");

describe("native platform boundary", () => {
  test("rejects native Linux targets before invoking Tauri", () => {
    for (const command of ["dev", "build", "bundle"]) {
      expect(nativeTargetError([command], "linux")).toBe(linuxBuildMessage);
      expect(nativeTargetError([command, "--target", "aarch64-unknown-linux-gnu"], "darwin")).toBe(linuxBuildMessage);
      expect(nativeTargetError([command, "--target=x86_64-unknown-linux-gnu"], "win32")).toBe(linuxBuildMessage);
      expect(nativeTargetError([command, "-t", "x86_64-unknown-linux-gnu"], "win32")).toBe(linuxBuildMessage);
      expect(nativeTargetError([command], "win32", "x86_64-unknown-linux-gnu")).toBe(linuxBuildMessage);
    }
  });

  test("preserves retained targets, help, and non-build commands", () => {
    for (const host of ["win32", "darwin"]) {
      expect(nativeTargetError(["build", "--no-bundle", "--ci"], host)).toBeNull();
    }
    for (const target of ["aarch64-apple-darwin", "x86_64-apple-darwin", "x86_64-pc-windows-msvc"]) {
      expect(nativeTargetError(["build", "--target", target], "linux")).toBeNull();
    }
    for (const args of [["--version"], ["info"], ["build", "--help"], ["dev", "-h"]]) {
      expect(nativeTargetError(args, "linux")).toBeNull();
    }
    expect(nativeTargetError(["build", "--target", "aarch64-apple-darwin"], "darwin", "x86_64-unknown-linux-gnu")).toBeNull();
    expect(nativeTargetError(["dev", "--", "--target", "x86_64-unknown-linux-gnu"], "darwin")).toBeNull();
  });

  test("keeps macOS and Windows configuration without Linux packaging", () => {
    const cargo = read("src-tauri/Cargo.toml");
    expect(cargo).not.toContain('cfg(target_os = "linux")');
    expect(cargo).not.toMatch(/^gtk(?:-layer-shell)?\\s*=/m);
    expect(cargo).toContain('"whisper-metal"');
    expect(cargo).toContain('"ort-directml"');
    expect(cargo).toContain('tauri-nspanel');
    const config = JSON.parse(read("src-tauri/tauri.conf.json"));
    expect(config.identifier).toBe("tech.amditis.audiobud");
    expect(config.bundle.linux).toBeUndefined();
    expect(config.bundle.macOS.minimumSystemVersion).toBe("11.0");
    expect(config.bundle.macOS.hardenedRuntime).toBe(true);
    expect(config.bundle.windows.nsis.template).toBe("nsis/installer.nsi");
    const capability = JSON.parse(read("src-tauri/capabilities/desktop.json"));
    expect(capability.platforms).toEqual(["macOS", "windows"]);
  });

  test("does not restore Linux-only source branches or tray resources", () => {
    const inspect = (directory: string) => {
      for (const entry of readdirSync(directory, { withFileTypes: true })) {
        const path = join(directory, entry.name);
        if (entry.isDirectory()) inspect(path);
        else if (entry.name.endsWith(".rs")) {
          expect(readFileSync(path, "utf8")).not.toMatch(/#\\[cfg(?:_attr)?\\([^\\]]*target_os\\s*=\\s*"linux"/);
        }
      }
    };
    inspect(resolve(root, "src-tauri/src"));
    for (const path of ["src/components/settings/TypingTool.tsx", "src-tauri/resources/handy.png", "src-tauri/resources/recording.png", "src-tauri/resources/transcribing.png"]) {
      expect(existsSync(resolve(root, path))).toBe(false);
    }
  });

  test("required shared check validates the real Mac engine and generated bindings", () => {
    const ci = read(".github/workflows/ci.yml");
    const shared = ci.split("  rust-tests:\\n")[1].split("  rust-tests-windows:\\n")[0];
    expect(shared).toContain("name: Rust tests (mock transcription, no GPU)");
    expect(shared).toContain("runs-on: macos-15");
    expect(shared).toContain("cargo test --locked");
    expect(shared).toContain("--bin generate-bindings");
    expect(shared).toContain("git diff --exit-code -- ../src/bindings.ts");
    expect(shared).toContain("bun run tauri build --no-bundle --ci");
    expect(shared).not.toContain("transcription_mock.rs");
    expect(shared).not.toContain("artifact-signing");
    expect(ci).toContain("runs-on: ubuntu-latest");
    expect(read(".github/workflows/engine.yml")).toContain("bun run tauri build --no-bundle --ci");
  });
});
''')

# Keep the required check name. Replace the Linux mock job with real Apple
# Silicon tests, the same binding export assertion, and an unsigned native build.
release = read('.github/workflows/release.yml')
start = release.index('      - name: Select reviewed Xcode 26 toolchain', release.index('  build-macos:'))
end = release.index('\n      - ', start + 1)
xcode_step = release[start:end]
shared = '''  rust-tests:
    # Legacy name is required by branch protection. This job now validates the
    # real Apple Silicon engine instead of a Linux mock. Do not rename the
    # check without a separate branch-protection change.
    name: Rust tests (mock transcription, no GPU)
    runs-on: macos-15
    timeout-minutes: 90
    permissions:
      contents: read
    env:
      MACOSX_DEPLOYMENT_TARGET: "11.0"
    steps:
      - uses: actions/checkout@11d5960a326750d5838078e36cf38b85af677262
        with:
          persist-credentials: false
      - name: Require Apple Silicon host
        run: test "$(uname -m)" = arm64
''' + xcode_step + '''
      - uses: dtolnay/rust-toolchain@4cda84d5c5c54efe2404f9d843567869ab1699d4
        with:
          toolchain: stable
      - uses: oven-sh/setup-bun@0c5077e51419868618aeaa5fe8019c62421857d6
        with:
          bun-version: 1.3.14
      - name: Install frontend dependencies
        run: bun install --frozen-lockfile
      - uses: swatinem/rust-cache@v2
        with:
          workspaces: "./src-tauri -> target"
          shared-key: macos-real-engine
      - name: Download the VAD model
        working-directory: src-tauri
        run: |
          mkdir -p resources/models
          model="resources/models/silero_vad_v4.onnx"
          curl --fail --location --retry 3 -o "$model" "$MODEL_ASSET_BASE_URL/silero_vad_v4.onnx"
          test "$(wc -c < "$model" | tr -d ' ')" -eq "$SILERO_VAD_BYTES"
          echo "$SILERO_VAD_SHA256  $model" | shasum -a 256 -c -
      - name: Test the real Apple Silicon transcription engine
        working-directory: src-tauri
        run: cargo test --locked
      - name: Check generated Rust bindings
        working-directory: src-tauri
        run: |
          cargo run --locked --features generate-bindings --bin generate-bindings
          git diff --exit-code -- ../src/bindings.ts
      - name: Build the unsigned Apple Silicon application
        # No bundle, signing environment, credentials, release writes, or uploads.
        run: bun run tauri build --no-bundle --ci
'''
path = '.github/workflows/ci.yml'
text = read(path)
start = text.index('  rust-tests:\n')
end = text.index('  rust-tests-windows:\n', start)
text = text[:start] + shared + '\n' + text[end:]
text = '''# AudioBud correctness checks. Linux hosts frontend and secret checks only.
# The legacy required Rust check runs real Apple Silicon tests and a native
# build. Windows keeps its mock regression job plus the real-engine workflow.
# Signing and release publication remain confined to release.yml.
''' + text[text.index('name: CI'):]
write(path, text)
path = '.github/workflows/engine.yml'
text = read(path)
text = '''# Real Windows engine tests, Clippy, and unsigned native application build.
# The required shared Rust check in ci.yml covers the real Apple Silicon engine,
# generated bindings, and its unsigned native application build.
''' + text[text.index('name: Engine'):]
text = text.replace('    timeout-minutes: 60', '    timeout-minutes: 90')
text = text.replace('      - uses: actions/checkout@v4\n', '''      - uses: actions/checkout@v4
      - uses: oven-sh/setup-bun@0c5077e51419868618aeaa5fe8019c62421857d6
        with:
          bun-version: 1.3.14
      - name: Install frontend dependencies
        run: bun install --frozen-lockfile
''', 1)
text += '''
      - name: Build the unsigned Windows application
        # Compile without bundles, signing credentials, or release publication.
        env:
          GGML_NATIVE: "OFF"
          GGML_AVX: "OFF"
          GGML_AVX2: "OFF"
          GGML_FMA: "OFF"
          GGML_F16C: "OFF"
        run: bun run tauri build --no-bundle --ci
'''
text = text.replace('      - ".github/workflows/engine.yml"', '''      - ".github/workflows/engine.yml"
      - ".github/workflows/ci.yml"
      - "src/**"
      - "package.json"
      - "bun.lock"
      - "vite.config.*"
      - "tsconfig*.json"
      - "*.html"
      - "scripts/tauri.ts"
      - "scripts/native-platform.ts"''')
write(path, text)

# Verify no existing retained-target unit test was removed. Only the explicitly
# cfg(linux) test may disappear; Windows path regressions must remain.
for name, old in before_tests.items():
    current = test_names(Path(name).read_bytes())
    allowed = {test for path, test in removed_linux_tests if path == name}
    assert old - current <= allowed, ('lost retained tests', name, old - current)
print('Linux-only tests removed:', sorted(removed_linux_tests))
print('Retained baseline Rust test names:', sum(len(x) for x in before_tests.values()) - len(removed_linux_tests))
for path in Path('src-tauri/src').rglob('*.rs'):
    assert not parser.parse(path.read_bytes()).root_node.has_error, ('final Rust parse', str(path))

# Release plumbing and Swift sources are not part of this retirement.
for name in ['.github/workflows/release.yml', '.github/workflows/publish-update-feed.yml', *[str(p) for p in Path('src-tauri/swift').rglob('*') if p.is_file()]]:
    original = subprocess.check_output(['git', 'show', f'{BASE}:{name}'])
    assert Path(name).read_bytes() == original, ('protected source changed', name)
original_config = json.loads(subprocess.check_output(['git', 'show', f'{BASE}:src-tauri/tauri.conf.json']))
current_config = json.loads(read('src-tauri/tauri.conf.json'))
for key in ['macOS', 'windows']:
    assert original_config['bundle'][key] == current_config['bundle'][key]
assert original_config['plugins'] == current_config['plugins']

print('\nRemaining active-source Linux references for review:')
for root in ['src-tauri/src', 'src', 'scripts']:
    for path in sorted(Path(root).rglob('*')):
        if path.suffix not in ['.rs', '.ts', '.tsx', '.json', '.css', '.mjs'] or 'bindings' in path.name:
            continue
        for number, line in enumerate(path.read_text().splitlines(), 1):
            if re.search(r'linux|wayland|gtk.layer.shell', line, re.I):
                print(f'{path}:{number}: {line.strip()}')
