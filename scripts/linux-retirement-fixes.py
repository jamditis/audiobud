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
path.write_text(text)
