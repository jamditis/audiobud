from pathlib import Path

path = Path('/tmp/linux-retirement-work.py')
text = path.read_text()

def change(old, new):
    global text
    assert text.count(old) == 1, (old, text.count(old))
    text = text.replace(old, new)

change("before_tests = {", '''def syntax_errors(data):
    from collections import Counter
    return Counter((node.type, node.text) for node in walk(parser.parse(data).root_node)
                   if node.type == 'ERROR' or node.is_missing)

before_tests = {''')
change("    assert not root.has_error, ('baseline Rust parse failed', str(path))", "    baseline_errors = syntax_errors(data)")
change("    assert not parser.parse(changed).root_node.has_error, ('edited Rust parse failed', str(path))", "    assert syntax_errors(changed) <= baseline_errors, ('new Rust parse errors', str(path), syntax_errors(changed) - baseline_errors)")
change("    assert not parser.parse(path.read_bytes()).root_node.has_error, ('final Rust parse', str(path))", "    original = subprocess.check_output(['git', 'show', f'{BASE}:{path}'])\n    assert syntax_errors(path.read_bytes()) <= syntax_errors(original), ('final Rust parse', str(path), syntax_errors(path.read_bytes()) - syntax_errors(original))")
old = 'expect(requirements).toContain("Intel Mac and Linux builds are not validated");'
new = 'expect(requirements).toContain(\n      "Intel Mac and Linux builds are not validated",\n    );'
change(repr(old), repr(new))
change("# Verify no existing retained-target unit test was removed.", '''# Remove obsolete implementation commentary without changing retained behavior.
replace('src/components/settings/general/GeneralSettings.tsx', ' (dynamic shortcut instability)', '')
replace('src-tauri/src/settings.rs', '// Default to CtrlV for macOS and Windows, Direct for Linux', '// Default to clipboard delivery on both retained platforms.')
replace('src-tauri/src/clipboard.rs', 'copy belongs in Windows clipboard history. Wayland keeps its wl-copy path\\n/// for non-ASCII compatibility.', 'copy belongs in Windows clipboard history.')

# Verify no existing retained-target unit test was removed.''')
path.write_text(text)
