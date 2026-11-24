"""Verify an approved publication file inventory. No network or runtime execution."""
import hashlib, json
from pathlib import Path
root = Path(__file__).resolve().parents[1]
manifest = json.loads((root / 'PUBLIC_MANIFEST.json').read_text())['sha256']
actual = {}
for p in root.rglob('*'):
    if '.git' in p.relative_to(root).parts: continue
    if p.is_symlink() or getattr(p, "is_junction", lambda: False)(): raise SystemExit('Unexpected link: ' + str(p))
    if p.is_file() and p.name != 'PUBLIC_MANIFEST.json':
        data = p.read_bytes()
        if b'\x00' in data: raise SystemExit('Binary file: ' + str(p))
        actual[p.relative_to(root).as_posix()] = hashlib.sha256(data).hexdigest()
if actual != manifest: raise SystemExit('Publication inventory changed; review diff before refreshing manifest')
print('Verified publication inventory:', len(actual), 'files')
