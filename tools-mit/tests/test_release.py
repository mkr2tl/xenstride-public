"""Offline packaging and release-gate tests; writes only temporary owned data."""
# SPDX-License-Identifier: MIT
import base64
import csv
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import tempfile
import unittest
from zipfile import ZipFile

ROOT = Path(__file__).resolve().parents[1]


def load_tool(name):
    spec = importlib.util.spec_from_file_location(name, ROOT / "tools" / (name + ".py"))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


builder = load_tool("build_wheel")
release = load_tool("release_check")


class WheelTests(unittest.TestCase):
    def test_reproducible_wheel_and_record_hashes(self):
        with tempfile.TemporaryDirectory() as temp:
            one = builder.build(Path(temp) / "one")
            two = builder.build(Path(temp) / "two")
            self.assertEqual(one.read_bytes(), two.read_bytes())
            with ZipFile(one) as archive:
                record = archive.read("xenstride-0.1.0.dist-info/RECORD").decode()
                names = set(archive.namelist())
                self.assertIn("xenstride/pe.py", names)
                self.assertFalse(any(name.startswith(("tests/", "tools/")) for name in names))
                self.assertIn("xenstride-0.1.0.dist-info/licenses/LICENSE", names)
                for name, digest, size in csv.reader(io.StringIO(record)):
                    if not digest:
                        continue
                    body = archive.read(name)
                    expected = base64.urlsafe_b64encode(hashlib.sha256(body).digest()).rstrip(b"=").decode()
                    self.assertEqual(digest, "sha256=" + expected)
                    self.assertEqual(len(body), int(size))
                self.assertEqual(len(list(csv.reader(io.StringIO(record)))), len(names))


class ReleaseTests(unittest.TestCase):
    def test_secret_categories_detected_without_value_disclosure(self):
        values = [b"ghp_" + b"A" * 36, b"AKIA" + b"B" * 16,
                  b"https://discord.com/api/webhooks/" + b"123/owned-test-token",
                  b"C:" + b"/Us" + b"ers/fictional-person", b"person" + b"@" + b"example.org"]
        for value in values:
            result = release.findings(value, "fixture")
            self.assertTrue(result)
            self.assertNotIn(value.decode(), json.dumps(result))

    def test_plain_metadata_is_not_a_secret(self):
        self.assertFalse(release.findings(b"format RDS5; block 4; size 4096", "fixture"))

    def test_public_noreply_identity_is_allowed(self):
        self.assertFalse(release.findings(b"1+contributor" + b"@users.noreply.github.com", "fixture"))
        self.assertFalse(release.findings(b"committer GitHub <noreply" + b"@github.com>", "fixture"))
        self.assertTrue(release.findings(b"someone.noreply" + b"@github.com.example.org", "fixture"))

    def test_reachable_history_is_scanned_without_creating_commits(self):
        from types import SimpleNamespace
        from unittest.mock import patch
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            (root / ".git").mkdir()
            body = b"owned"
            (root / "README.md").write_bytes(body)
            manifest = {"files": {"README.md": hashlib.sha256(body).hexdigest()}}
            (root / "RELEASE_MANIFEST.json").write_text(json.dumps(manifest))
            commit_id, blob_id = "c" * 40, "b" * 40
            secret = b"ghp_" + b"A" * 36  # generated fixture, never a real credential
            objects = {commit_id: (b"commit", b"owned synthetic commit metadata"), blob_id: (b"blob", secret)}
            def fake_git(args, **kwargs):
                if args[-3:] == ["ls-files", "--cached", "-z"]:
                    return SimpleNamespace(stdout=b"README.md\0RELEASE_MANIFEST.json\0")
                if args[-2:] == ["rev-list", "--all"]:
                    return SimpleNamespace(stdout=commit_id.encode() + b"\n")
                if args[-3:] == ["rev-list", "--objects", "--all"]:
                    return SimpleNamespace(stdout=(commit_id + "\n" + blob_id + " old.py\n").encode())
                kind, content = objects[args[-1]]
                if args[-2] == "-t": output = kind
                elif args[-2] == "-s": output = str(len(content)).encode()
                else: output = content
                return SimpleNamespace(stdout=output)
            with patch.object(release.subprocess, "run", side_effect=fake_git):
                result = release.check(root)
            self.assertFalse(result["ok"])
            self.assertEqual((result["history_commits"], result["history_blobs"]), (1, 1))
            self.assertTrue(any(v["category"] == "github_token" for v in result["issues"]))
            self.assertNotIn(secret.decode(), json.dumps(result))

    def test_nested_component_scans_explicit_parent_history(self):
        from unittest.mock import patch
        with tempfile.TemporaryDirectory() as temp:
            repo = Path(temp)
            (repo / '.git').mkdir()
            component = repo / 'tools-mit'
            component.mkdir()
            (component / 'RELEASE_MANIFEST.json').write_text(json.dumps({'files': {}}))
            oid = 'b' * 40
            secret = b'ghp_' + b'A' * 36
            def git(root, *args):
                self.assertEqual(root, repo.resolve())
                if args == ('rev-list', '--all'): return (oid + '\n').encode()
                if args == ('rev-list', '--objects', '--all'): return (oid + ' outside-tools.txt\n').encode()
                if args[1] == '-t': return b'blob'
                if args[1] == '-s': return str(len(secret)).encode()
                return secret
            with patch.object(release, 'git_output', side_effect=git):
                result = release.check(component, history_root=repo)
            self.assertFalse(result['ok'])
            self.assertEqual(result['history_state'], 'all_refs_scanned')
            self.assertTrue(any(i['category'] == 'github_token' for i in result['issues']))

    def test_digest_mismatch_and_unexpected_binary_are_blocked(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            (root / "README.md").write_text("owned", encoding="utf-8")
            (root / "RELEASE_MANIFEST.json").write_text(json.dumps({"files": {"README.md": "0" * 64}}))
            (root / "forbidden.dll").write_bytes(b"MZ\0synthetic")
            result = release.check(root)
            self.assertFalse(result["ok"])
            categories = {v["category"] for v in result["issues"]}
            self.assertTrue({"manifest_digest_mismatch", "unexpected_file_type", "large_or_binary_file"}.issubset(categories))

    def test_missing_manifest_is_blocked(self):
        with tempfile.TemporaryDirectory() as temp:
            self.assertFalse(release.check(Path(temp))["ok"])

    def test_readme_build_artifacts_are_not_public_files(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            body = b"owned"
            (root / "README.md").write_bytes(body)
            (root / "RELEASE_MANIFEST.json").write_text(json.dumps({"files": {"README.md": hashlib.sha256(body).hexdigest()}}))
            for directory in [".venv", "dist"]:
                (root / directory).mkdir()
                (root / directory / "local.dll").write_bytes(b"\0local build output")
            self.assertTrue(release.check(root)["ok"])

    def test_force_tracked_build_artifact_is_still_blocked(self):
        from types import SimpleNamespace
        from unittest.mock import patch
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            (root / "dist").mkdir()
            (root / "dist" / "bad.dll").write_bytes(b"\0owned negative control")
            (root / ".git").mkdir()
            (root / "RELEASE_MANIFEST.json").write_text(json.dumps({"files": {}}))
            def fake_git(args, **kwargs):
                return SimpleNamespace(stdout=b"dist/bad.dll\0" if args[-3:] == ["ls-files", "--cached", "-z"] else b"")
            with patch.object(release.subprocess, "run", side_effect=fake_git):
                result = release.check(root)
            self.assertFalse(result["ok"])
            self.assertTrue(any(i["category"] == "unexpected_file_type" for i in result["issues"]))


if __name__ == "__main__":
    unittest.main()
