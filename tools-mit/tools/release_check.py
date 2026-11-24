"""Read-only release inventory, integrity and heuristic privacy checks.

Reports categories and locations, never matching secret values. No remote access.
"""
# SPDX-License-Identifier: MIT
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess

ROOT = Path(__file__).resolve().parents[1]
MANIFEST = "RELEASE_MANIFEST.json"
PATTERNS = {
    "private_key": re.compile(rb"-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----"),
    "github_token": re.compile(rb"(?:gh[pousr]_[A-Za-z0-9]{30,}|github_pat_[A-Za-z0-9_]{40,})"),
    "aws_key": re.compile(rb"(?:AKIA|ASIA)[A-Z0-9]{16}"),
    "webhook": re.compile(rb"https://(?:discord(?:app)?\.com)/api/webhooks/[0-9]+/[A-Za-z0-9_-]+"),
    "credential_url": re.compile(rb"https?://[^\s/@:]+:[^\s/@]+@"),
    "personal_path": re.compile(rb"(?i)(?:[a-z]:[\\/]Users[\\/]|/home/|/Users/)[A-Za-z0-9_.-]+"),
    "email": re.compile(rb"[A-Za-z0-9_.+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}"),
    "assigned_credential": re.compile(rb"(?i)(?:api_key|access_token|password|client_secret)\s*[:=]\s*['\"][A-Za-z0-9_./+=-]{16,}['\"]"),
}
ALLOWED_SUFFIXES = {".py", ".md", ".toml", ".json", ".txt", ".yml", ".typed"}
LOCAL_ARTIFACT_DIRS = {".venv", "venv", "build", "dist"}


def git_output(root, *args):
    return subprocess.run(["git", "--no-optional-locks", "-c", "core.fsmonitor=false", "-C", str(root), *args],
                          capture_output=True, timeout=30, check=True).stdout


def findings(data, location):
    result = []
    for kind, pattern in PATTERNS.items():
        for match in pattern.finditer(data):
            if kind == "email" and (match.group().endswith(b"@users.noreply.github.com")
                                    or match.group() == b"noreply@github.com"):
                # Public GitHub identities, including the one on the merge commit GitHub
                # synthesizes for pull-request CI runs; personal addresses still fail.
                continue
            result.append(dict(location=location, category=kind, line=data[:match.start()].count(b"\n") + 1))
    return result


def public_files(root):
    selected = {}
    for directory, dirs, files in os.walk(root, followlinks=False):
        parent = Path(directory)
        for name in list(dirs):
            path = parent / name
            if name == ".git":
                dirs.remove(name)
            elif path.is_symlink():
                selected[path.relative_to(root).as_posix()] = path
                dirs.remove(name)
            elif name == "__pycache__" or parent == root and name in LOCAL_ARTIFACT_DIRS:
                dirs.remove(name)
        for name in files:
            path = parent / name
            selected[path.relative_to(root).as_posix()] = path
    if (root / ".git").exists():
        # Ignored build outputs are local only, unless someone explicitly stages
        # them. Never hide a staged/tracked artifact from the publication gate.
        for name in git_output(root, "ls-files", "--cached", "-z").decode("utf-8").split("\0"):
            if name:
                selected[name] = root / name
    return selected


def check(root=ROOT, history_root=None):
    root = Path(root).resolve()
    files = public_files(root)
    issues = []
    manifest_file = files.get(MANIFEST)
    if manifest_file is None:
        return dict(ok=False, issues=[dict(category="missing_manifest")])
    manifest = json.loads(manifest_file.read_text(encoding="utf-8"))
    expected = manifest["files"]
    actual = set(files) - {MANIFEST}
    if actual != set(expected):
        issues.append(dict(category="manifest_file_set", added=sorted(actual - set(expected)), missing=sorted(set(expected) - actual)))
    for name, path in sorted(files.items()):
        if path.is_symlink():
            issues.append(dict(location=name, category="symlink_not_allowed"))
            continue
        if not path.is_file():
            issues.append(dict(location=name, category="tracked_file_missing"))
            continue
        if path.suffix not in ALLOWED_SUFFIXES and name not in {"LICENSE", ".gitignore", ".gitattributes"}:
            issues.append(dict(location=name, category="unexpected_file_type"))
        data = path.read_bytes()
        if len(data) > 1 << 20 or b"\0" in data:
            issues.append(dict(location=name, category="large_or_binary_file"))
        issues.extend(findings(data, name))
        if name in expected and hashlib.sha256(data).hexdigest() != expected[name]:
            issues.append(dict(location=name, category="manifest_digest_mismatch"))
    history_blobs, history_commits, history_bytes = 0, 0, 0
    history_state = "not_initialized"
    history_root = Path(history_root).resolve() if history_root is not None else root
    if (history_root / ".git").exists():
        def git(*args):
            return git_output(history_root, *args)
        commits = git("rev-list", "--all").splitlines()
        history_commits = len(commits)
        history_state = "all_refs_scanned" if commits else "empty_no_inherited_history"
        for line in git("rev-list", "--objects", "--all").splitlines():
            oid = line.split(b" ", 1)[0].decode("ascii")
            kind = git("cat-file", "-t", oid).strip()
            if kind not in {b"blob", b"commit", b"tag"}:
                continue
            size = int(git("cat-file", "-s", oid))
            if size > 1 << 20 or history_bytes + size > 32 << 20:
                issues.append(dict(location="git:" + oid, category="history_size_limit"))
                continue
            content = git("cat-file", kind.decode(), oid)
            if kind == b"blob" and b"\0" in content:
                issues.append(dict(location="git:" + oid, category="binary_history_blob"))
            issues.extend(findings(content, "git:" + oid))
            history_bytes += size
            history_blobs += kind == b"blob"
    return dict(ok=not issues, files=len(files), issues=issues, history_state=history_state,
                history_commits=history_commits, history_blobs=history_blobs, history_bytes=history_bytes,
                limitation="heuristic patterns; no guarantee of secret absence; unreachable Git objects/reflogs not scanned")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("root", nargs="?", type=Path, default=ROOT)
    parser.add_argument("--history-root", type=Path, help="Repository root when this component is nested")
    args = parser.parse_args()
    result = check(args.root, history_root=args.history_root)
    print(json.dumps(result, indent=2))
    raise SystemExit(0 if result["ok"] else 1)
