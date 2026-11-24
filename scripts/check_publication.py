"""Check component inventory, publication privacy and the enclosing Git history."""
# SPDX-License-Identifier: MIT
import argparse
import importlib.util
import json
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--require-history', action='store_true')
    args = parser.parse_args()
    subprocess.run([sys.executable, '-B', str(ROOT / 'scripts/check_manifest.py')], check=True)
    spec = importlib.util.spec_from_file_location('release_check', ROOT / 'tools-mit/tools/release_check.py')
    gate = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(gate)
    result = gate.check(ROOT / 'tools-mit', history_root=ROOT)
    for name, path in gate.public_files(ROOT).items():
        if path.is_symlink() or getattr(path, 'is_junction', lambda: False)():
            result['issues'].append(dict(location=name, category='unexpected_link'))
        elif path.is_file():
            data = path.read_bytes()
            if len(data) > 1 << 20 or b'\0' in data:
                result['issues'].append(dict(location=name, category='large_or_binary_file'))
            result['issues'].extend(gate.findings(data, name))
    if args.require_history:
        if result['history_state'] != 'all_refs_scanned':
            result['issues'].append(dict(category='repository_history_not_scanned'))
        else:
            shallow = gate.git_output(ROOT, 'rev-parse', '--is-shallow-repository').strip()
            if shallow != b'false': result['issues'].append(dict(category='shallow_history'))
    result['ok'] = not result['issues']
    print(json.dumps(result, indent=2))
    return 0 if result['ok'] else 1

if __name__ == '__main__': raise SystemExit(main())
