#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Arinn contributors
# SPDX-License-Identifier: GPL-3.0-or-later

"""Prepare/verify the master snapshot; only un-draft after every asset verifies."""
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import urllib.error
import urllib.request


def context(env):
    if env.get("GITHUB_EVENT_NAME") != "push" or env.get("GITHUB_REF") != "refs/heads/master":
        raise ValueError("Snapshots require a master push")
    sha = env.get("GITHUB_SHA", "")
    repo = env.get("GITHUB_REPOSITORY", "")
    if not re.fullmatch(r"[0-9a-f]{40}", sha) or not re.fullmatch(r"[A-Za-z0-9][\w.-]*/[A-Za-z0-9][\w.-]*", repo):
        raise ValueError("Invalid snapshot identity")
    return sha, repo, "snapshot-" + sha


class SafeAssetRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, request, fp, code, msg, headers, newurl):
        if not newurl.startswith("https://"):
            raise RuntimeError("Unsafe checksum asset redirect")
        redirected = super().redirect_request(request, fp, code, msg, headers, newurl)
        if redirected is not None:
            redirected.remove_header("Authorization")
        return redirected


class GitHub:
    def __init__(self, repo, token):
        self.base = "https://api.github.com/repos/" + repo
        self.token = token

    def request(self, method, path, body=None):
        data = None if body is None else json.dumps(body).encode()
        request = urllib.request.Request(self.base + path, data=data, method=method, headers={
            "Authorization": "Bearer " + self.token,
            "Accept": "application/vnd.github+json",
            "X-GitHub-Api-Version": "2022-11-28",
            "Content-Type": "application/json",
        })
        try:
            with urllib.request.urlopen(request, timeout=60) as reply:
                return None if reply.status == 204 else json.load(reply)
        except urllib.error.HTTPError as error:
            if method in {"GET", "DELETE"} and error.code == 404:
                return None
            raise RuntimeError(f"GitHub {method} failed ({error.code})") from None

    def asset_bytes(self, asset_id):
        if type(asset_id) is not int or asset_id <= 0:
            raise ValueError("Invalid checksum asset ID")
        request = urllib.request.Request(self.base + "/releases/assets/" + str(asset_id), headers={
            "Authorization": "Bearer " + self.token,
            "Accept": "application/octet-stream",
            "X-GitHub-Api-Version": "2022-11-28",
        })
        try:
            with urllib.request.build_opener(SafeAssetRedirect()).open(request, timeout=60) as reply:
                data = reply.read(256 * 1024 + 1)
        except (urllib.error.URLError, OSError):
            raise RuntimeError("Cannot read checksum asset") from None
        if len(data) > 256 * 1024:
            raise ValueError("Checksum asset exceeds its bound")
        return data


def check_tag(api, tag, sha, required=False):
    ref = api.request("GET", "/git/ref/tags/" + tag)
    if ref is None:
        if required:
            raise ValueError("Snapshot tag is missing")
        return False
    obj = ref["object"]
    for _ in range(4):
        if obj["type"] == "commit":
            if obj["sha"] != sha:
                raise ValueError("Snapshot tag points to another commit")
            return True
        if obj["type"] != "tag":
            break
        obj = api.request("GET", "/git/tags/" + obj["sha"])["object"]
    raise ValueError("Invalid snapshot tag")


def manifest(dist):
    files = sorted(p for p in dist.iterdir() if p.name != "SHA256SUMS")
    if not any(p.name.endswith(".pkg.tar.zst") for p in files) or not any(p.name.endswith(".tar.gz") for p in files):
        raise ValueError("Package and source archive are required")
    for path in files:
        if path.is_symlink() or not path.is_file() or not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9._-]*", path.name):
            raise ValueError("Invalid snapshot asset")
    if (dist / "SHA256SUMS").is_symlink():
        raise ValueError("Invalid checksum asset")
    hashes = {p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in files}
    (dist / "SHA256SUMS").write_text("".join(f"{digest}  {name}\n" for name, digest in hashes.items()))
    files.append(dist / "SHA256SUMS")
    return {p.name: {"size": p.stat().st_size, "digest": "sha256:" + hashlib.sha256(p.read_bytes()).hexdigest()} for p in files}


def check_assets(release, expected):
    actual = {a["name"]: a for a in release["assets"]}
    if set(actual) != set(expected):
        raise ValueError("Snapshot asset set is incomplete or unexpected")
    for name, item in expected.items():
        if actual[name].get("state") != "uploaded" or any(actual[name].get(key) != item[key] for key in ("size", "digest")):
            raise ValueError("Snapshot asset verification failed")
    if not release["prerelease"]:
        raise ValueError("Snapshot must be a pre-release")



def release_sha(release):
    if release.get("draft") is not False or release.get("prerelease") is not True:
        return None
    match = re.fullmatch(r"snapshot-([0-9a-f]{40})", release.get("tag_name", ""))
    if not match:
        return None
    if type(release.get("id")) is not int or release["id"] <= 0:
        raise ValueError("Invalid snapshot release ID")
    return match[1]


def relation(api, base, head):
    result = api.request("GET", f"/compare/{base}...{head}")
    status = result.get("status") if result else None
    if status not in {"ahead", "behind", "identical", "diverged"}:
        raise ValueError("Cannot establish snapshot commit ancestry")
    return status


def channel(api):
    master = api.request("GET", "/git/ref/heads/master")
    if not master or master["object"]["type"] != "commit" or not re.fullmatch(r"[0-9a-f]{40}", master["object"]["sha"]):
        raise ValueError("Invalid master reference")
    tip = master["object"]["sha"]
    snapshots, ids = [], set()
    # Finish enumeration before deleting: changing pagination mid-delete skips rows.
    for page in range(1, 1001):
        rows = api.request("GET", f"/releases?per_page=100&page={page}")
        if not isinstance(rows, list):
            raise ValueError("Invalid release listing")
        for release in rows:
            sha = release_sha(release)
            if sha is None or release["id"] in ids:
                continue
            ids.add(release["id"])
            check_tag(api, release["tag_name"], sha, required=True)
            if relation(api, sha, tip) in {"ahead", "identical"}:
                snapshots.append(release)
        if len(rows) < 100:
            break
    else:
        raise ValueError("Release listing exceeds its page bound")
    newest = None
    for release in snapshots:
        if newest is None:
            newest = release
            continue
        status = relation(api, release_sha(newest), release_sha(release))
        if status == "ahead":
            newest = release
        elif status == "diverged":
            raise ValueError("Snapshot history is ambiguous")
    return snapshots, newest


def superseded(api, sha):
    _, newest = channel(api)
    if newest is None or release_sha(newest) == sha:
        return False
    status = relation(api, sha, release_sha(newest))
    if status == "diverged":
        raise ValueError("Snapshot history is ambiguous")
    if status == "ahead":
        verified_keeper(api, newest)
        return True
    return False



def verified_keeper(api, keeper):
    # Re-read the keeper and verify usable assets before deleting any fallback.
    live = api.request("GET", "/releases/" + str(keeper["id"]))
    if not live or release_sha(live) != release_sha(keeper) or live.get("id") != keeper["id"]:
        raise ValueError("Snapshot keeper changed")
    assets = live.get("assets", [])
    names = [asset.get("name", "") for asset in assets]
    if len(names) != len(set(names)) or "SHA256SUMS" not in names or not any(name.endswith(".pkg.tar.zst") for name in names) or not any(name.endswith(".tar.gz") for name in names):
        raise ValueError("Snapshot keeper assets are incomplete")
    for asset in assets:
        if asset.get("state") != "uploaded" or type(asset.get("size")) is not int or asset["size"] <= 0 or not re.fullmatch(r"sha256:[0-9a-f]{64}", asset.get("digest", "")):
            raise ValueError("Snapshot keeper assets are unverified")
    sums = next(asset for asset in assets if asset["name"] == "SHA256SUMS")
    data = api.asset_bytes(sums.get("id"))
    if len(data) != sums["size"] or "sha256:" + hashlib.sha256(data).hexdigest() != sums["digest"]:
        raise ValueError("Snapshot checksum asset verification failed")
    try:
        lines = data.decode("utf-8").splitlines()
    except UnicodeError:
        raise ValueError("Invalid snapshot checksums") from None
    expected = {}
    for line in lines:
        match = re.fullmatch(r"([0-9a-f]{64})  ([A-Za-z0-9][A-Za-z0-9._-]*)", line)
        if not match or match[2] in expected:
            raise ValueError("Invalid snapshot checksums")
        expected[match[2]] = "sha256:" + match[1]
    if set(expected) != set(names) - {"SHA256SUMS"} or any(asset["digest"] != expected[asset["name"]] for asset in assets if asset["name"] != "SHA256SUMS"):
        raise ValueError("Snapshot keeper checksum manifest disagrees")
    check_tag(api, live["tag_name"], release_sha(live), required=True)
    return live


def prune(api):
    snapshots, keeper = channel(api)
    if keeper is None:
        return None
    live = verified_keeper(api, keeper)
    for candidate in snapshots:
        if candidate["id"] == keeper["id"]:
            continue
        current = api.request("GET", "/releases/" + str(candidate["id"]))
        if current is None:
            continue
        if release_sha(current) != release_sha(candidate) or current.get("id") != candidate["id"]:
            raise ValueError("Snapshot candidate changed")
        # Never delete a concurrently appearing newer commit, even outside our lock.
        if relation(api, release_sha(current), release_sha(live)) == "ahead":
            api.request("DELETE", "/releases/" + str(current["id"]))
    return live["tag_name"]


def cleanup_context(env):
    repo = env.get("GITHUB_REPOSITORY", "")
    if not re.fullmatch(r"[A-Za-z0-9][\w.-]*/[A-Za-z0-9][\w.-]*", repo):
        raise ValueError("Invalid cleanup repository")
    if env.get("GITHUB_EVENT_NAME") == "workflow_dispatch" and env.get("GITHUB_REF") == "refs/heads/master":
        return repo
    if env.get("GITHUB_EVENT_NAME") == "release":
        event = json.loads(Path(env["GITHUB_EVENT_PATH"]).read_text())
        if event.get("action") == "published" and event.get("repository", {}).get("full_name") == repo and release_sha(event.get("release", {})):
            return repo
    raise ValueError("Cleanup requires a published snapshot or master dispatch")

def prepare(api, sha, repo, tag, dist):
    expected = manifest(dist)
    tagged = check_tag(api, tag, sha)
    if superseded(api, sha):
        return {"sha": sha, "repo": repo, "tag": tag, "assets": expected, "obsolete": True}, True
    release = api.request("GET", "/releases/tags/" + tag)
    published = release is not None and not release["draft"]
    if published:
        check_tag(api, tag, sha, required=True)
        check_assets(release, expected)
    if not tagged:
        api.request("POST", "/git/refs", {"ref": "refs/tags/" + tag, "sha": sha})
        check_tag(api, tag, sha, required=True)
    return {"sha": sha, "repo": repo, "tag": tag, "assets": expected}, published


def publish(api, metadata, release_id=None):
    if superseded(api, metadata["sha"]):
        if release_id:
            if not re.fullmatch(r"[0-9]+", release_id):
                raise ValueError("Invalid draft release ID")
            owned = api.request("GET", "/releases/" + release_id)
            if owned and owned.get("tag_name") != metadata["tag"]:
                raise ValueError("Release points to another snapshot tag")
            if owned and owned.get("draft") is True:
                if owned.get("prerelease") is not True or str(owned.get("id")) != release_id:
                    raise ValueError("Obsolete draft identity changed")
                api.request("DELETE", "/releases/" + release_id)
        prune(api)
        return
    check_tag(api, metadata["tag"], metadata["sha"], required=True)
    if release_id:
        if not re.fullmatch(r"[0-9]+", release_id):
            raise ValueError("Invalid draft release ID")
        release = api.request("GET", "/releases/" + release_id)
    else:
        release = api.request("GET", "/releases/tags/" + metadata["tag"])
    if release is None:
        raise ValueError("Snapshot release is missing")
    if release.get("tag_name") != metadata["tag"]:
        raise ValueError("Release points to another snapshot tag")
    check_assets(release, metadata["assets"])
    if release["draft"]:
        api.request("PATCH", "/releases/" + str(release["id"]), {"draft": False, "prerelease": True, "make_latest": "false"})
    prune(api)


def main():
    if sys.argv[1] == "prune":
        repo = cleanup_context(os.environ)
        token = os.environ.get("GH_TOKEN", "")
        if not token:
            raise ValueError("SNAPSHOT_RELEASE_TOKEN is required")
        print("Retained master snapshot:", prune(GitHub(repo, token)) or "none")
        return
    sha, repo, tag = context(os.environ)
    if subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip() != sha:
        raise ValueError("Checkout differs from the tested commit")
    token = os.environ.get("GH_TOKEN", "")
    if not token:
        raise ValueError("SNAPSHOT_RELEASE_TOKEN is required (Contents and Workflows write)")
    api = GitHub(repo, token)
    path = Path("snapshot.json")
    if sys.argv[1] == "prepare":
        metadata, published = prepare(api, sha, repo, tag, Path("dist"))
        path.write_text(json.dumps(metadata))
        changes = subprocess.check_output([sys.executable, "scripts/release.py", "unreleased"], text=True)
        Path("snapshot-notes.md").write_text(f"Automatic master snapshot from `{sha}`.\n\nThe workspace version is unchanged; the package version carries the commit count and hash. Full CI, package build and installation smoke tests passed for this commit. No real-desktop qualification is claimed.\n\n" + changes)
        with open(os.environ["GITHUB_OUTPUT"], "a") as output:
            output.write(f"tag={tag}\nalready_published={'true' if published else 'false'}\n")
    elif sys.argv[1] == "publish":
        metadata = json.loads(path.read_text())
        if (metadata["sha"], metadata["repo"], metadata["tag"]) != (sha, repo, tag):
            raise ValueError("Prepared snapshot identity changed")
        publish(api, metadata, os.environ.get("RELEASE_ID"))
    else:
        raise ValueError("Expected prepare, publish or prune")


if __name__ == "__main__":
    main()
