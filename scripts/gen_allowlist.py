"""Regenerate scripts/publication_allowlist.json from the runtime's current findings.

Run only after reviewing every finding it prints: each entry exempts one exact file content
(SHA-256) and line from one pattern of tools-mit/tools/release_check.py. No network access."""
# SPDX-License-Identifier: MIT
import hashlib
import importlib.util
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('release_check', ROOT / 'tools-mit/tools/release_check.py')
gate = importlib.util.module_from_spec(spec)
spec.loader.exec_module(gate)
REASONS = {
    'personal_path': 'placeholder path in a test, not a real user directory',
    'credential_url': 'loopback URL with dummy credentials in a URL-parsing test',
}
entries = []
for path in sorted((ROOT / 'runtime').rglob('*')):
    if not path.is_file():
        continue
    data = path.read_bytes()
    rel = path.relative_to(ROOT).as_posix()
    for f in gate.findings(data, rel):
        line = data.split(b'\n')[f['line'] - 1]
        reason = REASONS.get(f['category'], 'REVIEW')
        if b'xboxlive.com' in line:
            reason = 'Xbox Live service URL (its users-me path), not a user directory'
        entries.append(dict(file=rel, line=f['line'], category=f['category'],
                            sha256=hashlib.sha256(data).hexdigest(), reason=reason))
        print(rel, f['line'], f['category'], '-', reason)
note = ('Reviewed false positives of tools-mit/tools/release_check.py patterns. Each entry matches '
        'one exact file content (SHA-256) and line; any edit to the file invalidates it.')
(ROOT / 'scripts/publication_allowlist.json').write_text(
    json.dumps({'note': note, 'entries': entries}, indent=1, ensure_ascii=False) + '\n', encoding='utf-8', newline='\n')
print(len(entries), 'entries')
