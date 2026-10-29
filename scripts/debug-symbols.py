#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Arinn contributors
# SPDX-License-Identifier: GPL-3.0-or-later

"""Package-matched Sentry debug files (ADR-0116). Never uploads source bundles."""

import argparse
import hashlib
import json
import os
import re
import subprocess
import sys
import tempfile
import urllib.request
from pathlib import Path

COMPONENTS = ("arinnd", "arinn-shell")
FLAVORS = ("arch", "debian")
BUILD = re.compile(r"\d+\.\d+\.\d+\+g[0-9a-f]{12}")
CLI_VERSION = "3.8.2"
CLI_SHA256 = "c958865f9d4000183509ea21b1f04925d5390de6d8d87386b9e69f2b305070c3"
CLI_URL = (f"https://github.com/getsentry/sentry-cli/releases/download/"
           f"{CLI_VERSION}/sentry-cli-Linux-x86_64")


def run(*args):
    # Tool errors may contain compiler paths: do not forward their output.
    result = subprocess.run(args, capture_output=True, env={**{k: v for k, v in os.environ.items() if k != "SENTRY_AUTH_TOKEN"},
                                 "LC_ALL": "C"},
                            timeout=120, check=False)
    if result.returncode:
        raise ValueError("debug-file tool failed")
    return result.stdout.decode("utf-8", errors="replace")


def elf_id(path):
    found = re.findall(r"Build ID: ([0-9a-f]+)", run("readelf", "-n", str(path)))
    if len(found) != 1 or not re.fullmatch(r"[0-9a-f]{16,128}", found[0]):
        raise ValueError("missing or ambiguous ELF build ID")
    return found[0]


def has_lines(path):
    return bool(re.search(r"\]\s+\.debug_line\s", run("readelf", "-SW", str(path))))


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def regular(path, root):
    if path.is_symlink() or not path.is_file() or not path.resolve().is_relative_to(root.resolve()):
        raise ValueError("expected a regular file inside the artifact root")
    return path


def collect(root, output, flavor, build_id):
    if not BUILD.fullmatch(build_id):
        raise ValueError("symbols require a clean commit-qualified build ID")
    # A new directory rules out stale symbols from an earlier cached build.
    output.mkdir(parents=True, exist_ok=False)
    rows = []
    for component in COMPONENTS:
        binary = regular(root / "usr/bin" / component, root)
        identity = elf_id(binary)
        debug = output / f"{component}.debug"
        run("objcopy", "--only-keep-debug", str(binary), str(debug))
        if elf_id(debug) != identity or not has_lines(debug):
            raise ValueError("debug file lacks matching build ID or line tables")
        rows.append({"component": component, "elf_build_id": identity,
                     "sha256": digest(debug)})
    manifest = {"schema": 1, "flavor": flavor, "release": f"arinn@{build_id}", "files": rows}
    (output / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")


def validate(artifact, flavor, build_id):
    manifest_path = regular(artifact / "manifest.json", artifact)
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    if (not isinstance(manifest, dict) or set(manifest) != {"schema", "flavor", "release", "files"}
            or manifest["schema"] != 1 or manifest["flavor"] != flavor
            or not BUILD.fullmatch(build_id) or manifest["release"] != f"arinn@{build_id}"):
        raise ValueError("symbol manifest does not match this build")
    rows = manifest["files"]
    if not isinstance(rows, list) or len(rows) != len(COMPONENTS):
        raise ValueError("incomplete symbol manifest")
    paths = []
    for component, row in zip(COMPONENTS, rows):
        if (not isinstance(row, dict) or set(row) != {"component", "elf_build_id", "sha256"}
                or row["component"] != component):
            raise ValueError("unexpected symbol component")
        debug = regular(artifact / f"{component}.debug", artifact)
        if digest(debug) != row["sha256"] or elf_id(debug) != row["elf_build_id"]:
            raise ValueError("symbol contents do not match their manifest")
        if not has_lines(debug):
            raise ValueError("symbol file has no line tables")
        paths.append(debug)
    # No source bundles, extra binaries or stale outputs in an upload artifact.
    if {p.name for p in artifact.iterdir()} != {"manifest.json", *(p.name for p in paths)}:
        raise ValueError("unexpected file in symbol artifact")
    return manifest, paths


def verify(root, artifact, flavor, build_id):
    manifest, _ = validate(artifact, flavor, build_id)
    for row in manifest["files"]:
        binary = regular(root / "usr/bin" / row["component"], root)
        if elf_id(binary) != row["elf_build_id"]:
            raise ValueError("packaged binary does not match its debug file")


def trusted(env):
    return (env.get("GITHUB_REPOSITORY") == "LimeBeck/arinn"
            and env.get("GITHUB_EVENT_NAME") == "push"
            and (env.get("GITHUB_REF") == "refs/heads/master"
                 or re.fullmatch(r"refs/tags/v\d+\.\d+\.\d+", env.get("GITHUB_REF", ""))))


def install_cli(destination):
    with urllib.request.urlopen(CLI_URL, timeout=60) as response:
        if not response.url.startswith("https://"):
            raise ValueError("CLI download must use HTTPS")
        destination.write_bytes(response.read(64 * 1024 * 1024))
    if digest(destination) != CLI_SHA256:
        raise ValueError("Sentry CLI checksum mismatch")
    destination.chmod(0o700)


def upload(artifacts, build_id, env, cli=None):
    if not trusted(env):
        print("Sentry symbols: skipped for an untrusted or non-publication run")
        return
    if not env.get("SENTRY_AUTH_TOKEN"):
        print("::notice::Sentry symbols skipped: SENTRY_AUTH_TOKEN is not configured")
        return
    if not re.fullmatch(r"[0-9a-f]{40}", env.get("GITHUB_SHA", "")) or not build_id.endswith(
            "+g" + env["GITHUB_SHA"][:12]):
        raise ValueError("symbol release does not match the workflow commit")
    paths = []
    for flavor in FLAVORS:
        _, files = validate(artifacts / flavor, flavor, build_id)
        paths.extend(path.resolve() for path in files)
    # Auth only reaches this subprocess, after all artifacts were validated.
    # A private HOME excludes a runner's .sentryclirc, defaults and caches.
    with tempfile.TemporaryDirectory(prefix="arinn-symbol-upload-") as temporary:
        directory = Path(temporary)
        if cli is None:
            cli = directory / "sentry-cli"
            install_cli(cli)
        upload_env = {"PATH": env.get("PATH", "/usr/bin:/bin"), "HOME": temporary,
                      "LC_ALL": "C", "SENTRY_AUTH_TOKEN": env["SENTRY_AUTH_TOKEN"],
                      "SENTRY_ORG": "limebeckdev", "SENTRY_PROJECT": "arinn",
                      "SENTRY_URL": "https://sentry.io/", "SENTRY_LOG_LEVEL": "error"}
        for key in ("HTTP_PROXY", "HTTPS_PROXY", "ALL_PROXY", "NO_PROXY",
                    "http_proxy", "https_proxy", "all_proxy", "no_proxy",
                    "SSL_CERT_FILE", "SSL_CERT_DIR"):
            if key in env:
                upload_env[key] = env[key]
        result = subprocess.run([str(cli), "debug-files", "upload", "--type", "elf",
                                 "--no-sources", "--wait-for", "300", *map(str, paths)],
                                env=upload_env, cwd=temporary, capture_output=True,
                                timeout=600, check=False)
        if result.returncode:
            raise ValueError("Sentry debug-file upload failed (CLI output suppressed)")
    print(f"Sentry symbols uploaded: {len(paths)} files for arinn@{build_id}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    for name in ("collect", "verify"):
        command = commands.add_parser(name)
        command.add_argument("root", type=Path)
        command.add_argument("artifact", type=Path)
        command.add_argument("--flavor", required=True, choices=FLAVORS)
        command.add_argument("--build-id", required=True)
    command = commands.add_parser("upload")
    command.add_argument("artifacts", type=Path)
    command.add_argument("--build-id", required=True)
    args = parser.parse_args()
    try:
        if args.command == "upload":
            upload(args.artifacts, args.build_id, os.environ)
        else:
            globals()[args.command](args.root, args.artifact, args.flavor, args.build_id)
    except (ValueError, OSError, subprocess.SubprocessError):
        # Deliberately omit payloads, paths and token-bearing diagnostics.
        print("Sentry symbols: validation, tool or upload failure", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
