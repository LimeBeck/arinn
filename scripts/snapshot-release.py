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
                return json.load(reply)
        except urllib.error.HTTPError as error:
            if method == "GET" and error.code == 404:
                return None
            raise RuntimeError(f"GitHub {method} failed ({error.code})") from None


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


def prepare(api, sha, repo, tag, dist):
    expected = manifest(dist)
    tagged = check_tag(api, tag, sha)
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


def main():
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
        Path("snapshot-notes.md").write_text(f"Automatic master snapshot from `{sha}`.\n\nWorkspace/package version is unchanged. Full CI, package build and installation smoke tests passed for this commit. No real-desktop qualification is claimed.\n\n" + changes)
        with open(os.environ["GITHUB_OUTPUT"], "a") as output:
            output.write(f"tag={tag}\nalready_published={'true' if published else 'false'}\n")
    elif sys.argv[1] == "publish":
        metadata = json.loads(path.read_text())
        if (metadata["sha"], metadata["repo"], metadata["tag"]) != (sha, repo, tag):
            raise ValueError("Prepared snapshot identity changed")
        publish(api, metadata, os.environ.get("RELEASE_ID"))
    else:
        raise ValueError("Expected prepare or publish")


if __name__ == "__main__":
    main()
