# SPDX-FileCopyrightText: 2026 Arinn contributors
# SPDX-License-Identifier: GPL-3.0-or-later

"""Master publication boundaries, immutable snapshots and verified assets."""
import copy
import hashlib
import importlib.util
from pathlib import Path
import tempfile
import unittest
from unittest import mock

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
            if method == "PATCH" and self.release:
                self.release.update(body)
            return {}
        if path == "/git/ref/heads/master":
            return {"object": {"type": "commit", "sha": SHA}}
        if path.startswith("/releases?per_page="):
            return [copy.deepcopy(self.release)] if self.release and not self.release["draft"] else []
        if path.startswith("/compare/"):
            return {"status": "identical"}
        if path.startswith("/git/ref/tags/"):
            return {"object": {"type": "commit", "sha": self.sha}} if self.sha else None
        if path.startswith("/releases/tags/"):
            return copy.deepcopy(self.release) if self.release and not self.release["draft"] else None
        if path == "/releases/12":
            return copy.deepcopy(self.release)
        raise AssertionError(path)

    def asset_bytes(self, asset_id):
        return checksum_bytes(self.release)


def checksum_bytes(item):
    return "".join(asset["digest"].removeprefix("sha256:") + "  " + asset["name"] + "\n"
                   for asset in item["assets"] if asset["name"] != "SHA256SUMS").encode()


def release(assets, draft=True):
    return {"id": 12, "tag_name": TAG, "draft": draft, "prerelease": True, "assets": [
        {"id": 12000+index, "name": name, "state": "uploaded", **entry} for index,(name,entry) in enumerate(assets.items())
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


class RetentionAPI:
    def __init__(self, rows, order, master=None):
        self.rows = copy.deepcopy(rows)
        self.order = order
        self.master = master or order[-1]
        self.tags = {row["tag_name"]: row["tag_name"].removeprefix("snapshot-") for row in rows}
        self.writes = []
        self.pages = []

    def request(self, method, path, body=None):
        if method == "GET":
            if path == "/git/ref/heads/master":
                return {"object": {"type": "commit", "sha": self.master}}
            if path.startswith("/git/ref/tags/"):
                sha = self.tags.get(path.removeprefix("/git/ref/tags/"))
                return {"object": {"type": "commit", "sha": sha}} if sha else None
            if path.startswith("/releases?per_page="):
                page = int(path.split("page=")[-1]); self.pages.append(page)
                return copy.deepcopy(self.rows[(page-1)*100:page*100])
            if path.startswith("/compare/"):
                base, head = path.removeprefix("/compare/").split("...")
                if base == head: return {"status": "identical"}
                if base not in self.order or head not in self.order: return {"status": "diverged"}
                return {"status": "ahead" if self.order.index(head) > self.order.index(base) else "behind"}
            if path.startswith("/releases/tags/"):
                tag = path.removeprefix("/releases/tags/")
                return next((copy.deepcopy(row) for row in self.rows if row["tag_name"] == tag and not row["draft"]), None)
            if path.startswith("/releases/"):
                return next((copy.deepcopy(row) for row in self.rows if str(row["id"]) == path.split("/")[-1]), None)
        self.writes.append((method,path,body))
        if method == "DELETE":
            self.rows = [row for row in self.rows if str(row["id"]) != path.split("/")[-1]]
            return None
        if method == "POST" and path == "/git/refs":
            self.tags[body["ref"].removeprefix("refs/tags/")] = body["sha"]
            return {}
        if method == "PATCH":
            next(row for row in self.rows if str(row["id"]) == path.split("/")[-1]).update(body)
            return {}
        raise AssertionError((method,path))

    def asset_bytes(self, asset_id):
        item = next(row for row in self.rows if any(asset["id"] == asset_id for asset in row["assets"]))
        return checksum_bytes(item)


def channel_release(number, sha, assets, **changes):
    value = release(assets,draft=False)
    value.update(id=number,tag_name="snapshot-"+sha,**changes)
    for index, asset in enumerate(value["assets"]): asset["id"] = number*1000+index
    return value


class RetentionTest(unittest.TestCase):
    def setUp(self):
        SnapshotTest.setUp(self)
        self.assets = snapshot.manifest(self.dist)
        self.new = "b"*40
        self.old = channel_release(1,SHA,self.assets)
        self.latest = channel_release(2,self.new,self.assets)

    def test_prune_uses_ancestry_not_publication_time_and_keeps_tags(self):
        self.old["published_at"] = "later"
        self.latest["published_at"] = "earlier"
        api = RetentionAPI([self.old,self.latest],[SHA,self.new])
        self.assertEqual(snapshot.prune(api),self.latest["tag_name"])
        self.assertEqual(api.writes,[("DELETE","/releases/1",None)])
        self.assertIn(self.old["tag_name"],api.tags)
        snapshot.prune(api)
        self.assertEqual(len(api.writes),1)

    def test_version_releases_drafts_stable_and_off_master_are_protected(self):
        extras=[channel_release(3,"c"*40,self.assets,draft=True),
                channel_release(4,"d"*40,self.assets,prerelease=False),
                {**channel_release(5,"e"*40,self.assets),"tag_name":"v0.1.0"},
                channel_release(6,"f"*40,self.assets),
                {**channel_release(7,"c"*40,self.assets),"tag_name":"snapshot-short"}]
        api=RetentionAPI([self.old,self.latest]+extras,[SHA,self.new])
        snapshot.prune(api)
        self.assertEqual([row["id"] for row in api.rows],[2,3,4,5,6,7])

    def test_bad_keeper_assets_or_tag_delete_nothing(self):
        for defect in ["missing","state","digest","size","tag"]:
            api=RetentionAPI([self.old,self.latest],[SHA,self.new])
            if defect=="missing": api.rows[1]["assets"].pop()
            elif defect=="tag": api.tags[self.latest["tag_name"]]=SHA
            else: api.rows[1]["assets"][0][defect]="wrong"
            with self.subTest(defect=defect),self.assertRaises(ValueError):snapshot.prune(api)
            self.assertEqual(api.writes,[])

    def test_all_release_pages_are_read_before_deleting(self):
        extras=[{**channel_release(10+n,SHA,self.assets),"tag_name":f"v0.0.{n}"} for n in range(100)]
        api=RetentionAPI(extras+[self.old,self.latest],[SHA,self.new])
        snapshot.prune(api)
        self.assertEqual(api.pages,[1,2])
        self.assertEqual(api.writes,[("DELETE","/releases/1",None)])

    def test_older_rerun_does_not_recreate_a_retired_release(self):
        api=RetentionAPI([self.latest],[SHA,self.new]);api.tags[TAG]=SHA
        metadata,skip=snapshot.prepare(api,SHA,"fixture/arinn",TAG,self.dist)
        self.assertTrue(skip);self.assertTrue(metadata["obsolete"])
        snapshot.publish(api,metadata)
        self.assertEqual(api.writes,[])

    def test_newer_release_arriving_after_prepare_keeps_old_draft_private(self):
        api=RetentionAPI([channel_release(1,SHA,self.assets,draft=True)],[SHA,self.new])
        metadata,skip=snapshot.prepare(api,SHA,"fixture/arinn",TAG,self.dist)
        self.assertFalse(skip)
        api.rows.append(copy.deepcopy(self.latest));api.tags[self.latest["tag_name"]]=self.new
        snapshot.publish(api,metadata,"1")
        self.assertEqual(api.writes,[("DELETE","/releases/1",None)])
        self.assertEqual([row["id"] for row in api.rows],[2])

    def test_unverified_newer_release_cannot_discard_usable_older_draft(self):
        api=RetentionAPI([channel_release(1,SHA,self.assets,draft=True),self.latest],[SHA,self.new])
        api.rows[1]["assets"][0]["state"]="new"
        with self.assertRaises(ValueError):
            snapshot.prepare(api,SHA,"fixture/arinn",TAG,self.dist)
        self.assertEqual(api.writes,[])
        metadata={"sha":SHA,"tag":TAG,"assets":self.assets}
        with self.assertRaises(ValueError):snapshot.publish(api,metadata,"1")
        self.assertEqual(api.writes,[])
        self.assertTrue(api.rows[0]["draft"])

    def test_owner_changed_obsolete_draft_is_protected(self):
        api=RetentionAPI([channel_release(1,SHA,self.assets,draft=True,prerelease=False),self.latest],[SHA,self.new])
        with self.assertRaises(ValueError):snapshot.publish(api,{"sha":SHA,"tag":TAG,"assets":self.assets},"1")
        self.assertEqual(api.writes,[])

    def test_checksum_content_and_redirect_authentication(self):
        api=RetentionAPI([self.old,self.latest],[SHA,self.new])
        api.asset_bytes=lambda asset_id:b"corrupt checksum bytes"
        with self.assertRaises(ValueError):snapshot.prune(api)
        self.assertEqual(api.writes,[])
        request=snapshot.urllib.request.Request("https://api.github.com/asset",headers={"Authorization":"Bearer fixture-token"})
        redirected=snapshot.SafeAssetRedirect().redirect_request(request,None,302,"Found",{},"https://release-assets.githubusercontent.com/asset")
        self.assertIsNone(redirected.get_header("Authorization"))
        with self.assertRaises(RuntimeError):
            snapshot.SafeAssetRedirect().redirect_request(request,None,302,"Found",{},"http://example.test/asset")

    def test_workflows_share_lock_and_cleanup_uses_trusted_master(self):
        root=SCRIPT.parents[1]
        ci=(root/".github/workflows/ci.yml").read_text()
        cleanup=(root/".github/workflows/snapshot-retention.yml").read_text()
        self.assertIn("group: arinn-master-snapshot-channel",ci)
        self.assertIn("group: arinn-master-snapshot-channel",cleanup)
        self.assertIn("cancel-in-progress: false",cleanup)
        self.assertIn("queue: max",ci)
        self.assertIn("queue: max",cleanup)
        self.assertIn("ref: master",cleanup)
        self.assertNotIn("pull_request",cleanup)
        self.assertNotIn("download-artifact",cleanup)
        self.assertIn("python scripts/snapshot-release.py prune",cleanup)

    def test_cleanup_event_fences_and_http_204(self):
        event=Path(self.temp.name)/"event.json"
        event.write_text(snapshot.json.dumps({"action":"published","repository":{"full_name":"fixture/arinn"},"release":self.latest}))
        env={"GITHUB_EVENT_NAME":"release","GITHUB_REPOSITORY":"fixture/arinn","GITHUB_EVENT_PATH":str(event)}
        self.assertEqual(snapshot.cleanup_context(env),"fixture/arinn")
        with self.assertRaises(ValueError): snapshot.cleanup_context({**env,"GITHUB_EVENT_NAME":"pull_request"})
        with self.assertRaises(ValueError): snapshot.cleanup_context({**env,"GITHUB_REPOSITORY":"other/arinn"})
        response=mock.MagicMock();response.__enter__.return_value=response;response.status=204
        with mock.patch.object(snapshot.urllib.request,"urlopen",return_value=response):
            self.assertIsNone(snapshot.GitHub("fixture/arinn","fixture-token").request("DELETE","/releases/1"))
        response.read.assert_not_called()


if __name__ == "__main__":
    unittest.main()
