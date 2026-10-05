# SPDX-FileCopyrightText: 2026 Arinn contributors
# SPDX-License-Identifier: GPL-3.0-or-later

"""Master publication boundaries, immutable snapshots and verified assets."""
import copy
import hashlib
import importlib.util
from pathlib import Path
import tempfile
import unittest

SCRIPT = Path(__file__).resolve().parents[2] / "scripts/snapshot-release.py"
spec = importlib.util.spec_from_file_location("snapshot", SCRIPT)
snapshot = importlib.util.module_from_spec(spec)
spec.loader.exec_module(snapshot)
SHA = "a" * 40
TAG = "snapshot-" + SHA


class API:
    def __init__(self, release=None, sha=SHA):
        self.release = release
        self.sha = sha
        self.writes = []

    def request(self, method, path, body=None):
        if method != "GET":
            self.writes.append((method, path, body))
            if method == "POST" and path == "/git/refs":
                self.sha = body["sha"]
            return {}
        if path.startswith("/git/ref/tags/"):
            return {"object": {"type": "commit", "sha": self.sha}} if self.sha else None
        if path.startswith("/releases/tags/"):
            return copy.deepcopy(self.release) if self.release and not self.release["draft"] else None
        if path == "/releases/12":
            return copy.deepcopy(self.release)
        raise AssertionError(path)


def release(assets, draft=True):
    return {"id": 12, "tag_name": TAG, "draft": draft, "prerelease": True, "assets": [
        {"name": name, "state": "uploaded", **entry} for name, entry in assets.items()
    ]}


class SnapshotTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.dist = Path(self.temp.name)
        (self.dist / "arinn-0.1.0.pkg.tar.zst").write_bytes(b"package fixture")
        (self.dist / "arinn-0.1.0.tar.gz").write_bytes(b"source fixture")

    def test_only_master_push_with_full_immutable_sha(self):
        env = {"GITHUB_EVENT_NAME": "push", "GITHUB_REF": "refs/heads/master", "GITHUB_SHA": SHA, "GITHUB_REPOSITORY": "fixture/arinn"}
        self.assertEqual(snapshot.context(env), (SHA, "fixture/arinn", TAG))
        for key, value in [("GITHUB_EVENT_NAME", "pull_request"), ("GITHUB_EVENT_NAME", "schedule"), ("GITHUB_EVENT_NAME", "workflow_dispatch"), ("GITHUB_REF", "refs/tags/v0.1.0"), ("GITHUB_REF", "refs/heads/feature"), ("GITHUB_SHA", "master"), ("GITHUB_REPOSITORY", "../other")]:
            with self.subTest(key=key, value=value), self.assertRaises(ValueError):
                snapshot.context({**env, key: value})

    def test_manifest_includes_verified_checksums(self):
        assets = snapshot.manifest(self.dist)
        for line in (self.dist / "SHA256SUMS").read_text().splitlines():
            digest, name = line.split("  ")
            self.assertEqual(digest, hashlib.sha256((self.dist / name).read_bytes()).hexdigest())
            self.assertEqual(assets[name]["digest"], "sha256:" + digest)
        self.assertIn("SHA256SUMS", assets)
        self.assertEqual(snapshot.manifest(self.dist), assets)

    def test_missing_package_source_or_symlink_prevents_publication(self):
        for filename in ["arinn-0.1.0.pkg.tar.zst", "arinn-0.1.0.tar.gz"]:
            path = self.dist / filename
            original = path.read_bytes()
            path.unlink()
            with self.assertRaises(ValueError):
                snapshot.manifest(self.dist)
            path.write_bytes(original)
        (self.dist / "alias").symlink_to(self.dist / "arinn-0.1.0.tar.gz")
        with self.assertRaises(ValueError):
            snapshot.manifest(self.dist)

    def test_tag_collision_is_rejected_without_writes(self):
        api = API(sha="b" * 40)
        with self.assertRaises(ValueError):
            snapshot.prepare(api, SHA, "fixture/arinn", TAG, self.dist)
        self.assertEqual(api.writes, [])

    def test_complete_draft_is_published_after_verification(self):
        metadata, exists = snapshot.prepare(API(sha=None), SHA, "fixture/arinn", TAG, self.dist)
        self.assertFalse(exists)
        api = API(release(metadata["assets"]))
        snapshot.publish(api, metadata, "12")
        self.assertEqual(api.writes, [("PATCH", "/releases/12", {"draft": False, "prerelease": True, "make_latest": "false"})])

    def test_missing_bad_or_unuploaded_asset_keeps_draft(self):
        assets = snapshot.manifest(self.dist)
        metadata = {"sha": SHA, "tag": TAG, "assets": assets}
        for defect in ["missing", "digest", "size", "state", "prerelease", "tag", "release_tag"]:
            item = release(assets)
            if defect == "missing":
                item["assets"].pop()
            elif defect == "prerelease":
                item["prerelease"] = False
            elif defect == "release_tag":
                item["tag_name"] = "other"
            elif defect != "tag":
                item["assets"][0][defect] = "wrong"
            api = API(item, sha=None if defect == "tag" else SHA)
            with self.subTest(defect=defect), self.assertRaises(ValueError):
                snapshot.publish(api, metadata, "12")
            self.assertEqual(api.writes, [])

    def test_published_rerun_is_immutable(self):
        assets = snapshot.manifest(self.dist)
        api = API(release(assets, draft=False))
        metadata, exists = snapshot.prepare(api, SHA, "fixture/arinn", TAG, self.dist)
        self.assertTrue(exists)
        snapshot.publish(api, metadata)
        self.assertEqual(api.writes, [])
        (self.dist / "arinn-0.1.0.pkg.tar.zst").write_bytes(b"different build")
        with self.assertRaises(ValueError):
            snapshot.prepare(api, SHA, "fixture/arinn", TAG, self.dist)
        self.assertEqual(api.writes, [])


if __name__ == "__main__":
    unittest.main()
