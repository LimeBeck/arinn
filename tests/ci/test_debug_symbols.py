# SPDX-FileCopyrightText: 2026 Arinn contributors
# SPDX-License-Identifier: GPL-3.0-or-later

"""Real ELF fixtures and a recording CLI; never contacts Sentry or builds Arinn."""

import contextlib
import importlib.util
import io
import json
import os
import pathlib
import shutil
import subprocess
import tempfile
import unittest
from unittest import mock

from test_ci_build_scope import evaluate, jobs, value

ROOT = pathlib.Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("debug_symbols", ROOT / "scripts/debug-symbols.py")
symbols = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(symbols)
COMMIT = "123456789abc" + "0" * 28
BUILD = "0.1.0+g" + COMMIT[:12]


@unittest.skipUnless(all(shutil.which(tool) for tool in ("cc", "rustc", "objcopy", "readelf")),
                     "ELF fixture tests require cc, rustc and binutils")
class SymbolsTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.fixture = tempfile.TemporaryDirectory(prefix="arinn-symbol-fixture-")
        cls.source = pathlib.Path(cls.fixture.name)
        (cls.source / "main.c").write_text("int main(void) { return 0; }\n")
        (cls.source / "main.rs").write_text("fn main() { std::process::exit(0); }\n")
        subprocess.run(["cc", "-g1", "-Wl,--build-id=sha1", "main.c", "-o", "native"],
                       cwd=cls.source, check=True, capture_output=True)
        subprocess.run(["rustc", "-C", "debuginfo=line-tables-only", "-C", "strip=none",
                        "-C", "link-arg=-Wl,--build-id=sha1", "main.rs", "-o", "rust"],
                       cwd=cls.source, check=True, capture_output=True)

    @classmethod
    def tearDownClass(cls):
        cls.fixture.cleanup()

    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="arinn-symbol-test-")
        self.addCleanup(temporary.cleanup)
        self.base = pathlib.Path(temporary.name)
        self.root = self.base / "package"
        (self.root / "usr/bin").mkdir(parents=True)
        shutil.copyfile(self.source / "rust", self.root / "usr/bin/arinnd")
        shutil.copyfile(self.source / "native", self.root / "usr/bin/arinn-shell")
        self.artifacts = self.base / "symbols"
        for flavor in symbols.FLAVORS:
            symbols.collect(self.root, self.artifacts / flavor, flavor, BUILD)

    def test_split_debug_files_match_the_actual_stripped_packages(self):
        before = {c: symbols.elf_id(self.root / "usr/bin" / c) for c in symbols.COMPONENTS}
        for component in symbols.COMPONENTS:
            binary = self.root / "usr/bin" / component
            # Mirrors the package manager's removal of debug sections.
            subprocess.run(["objcopy", "--strip-debug", str(binary)], check=True)
            self.assertNotIn(".debug_line", symbols.run("readelf", "-SW", str(binary)))
            self.assertEqual(symbols.elf_id(binary), before[component])
        for flavor in symbols.FLAVORS:
            artifact = self.artifacts / flavor
            symbols.verify(self.root, artifact, flavor, BUILD)
            manifest = json.loads((artifact / "manifest.json").read_text())
            self.assertEqual(manifest["release"], f"arinn@{BUILD}")
            self.assertNotIn(str(self.base), json.dumps(manifest))
            for component in symbols.COMPONENTS:
                debug = artifact / f"{component}.debug"
                self.assertEqual(symbols.elf_id(debug), before[component])
                self.assertIn(".debug_line", symbols.run("readelf", "-SW", str(debug)))

    def test_collection_does_not_modify_the_staged_binaries(self):
        before = {c: symbols.digest(self.root / "usr/bin" / c) for c in symbols.COMPONENTS}
        symbols.collect(self.root, self.base / "new", "arch", BUILD)
        self.assertEqual(before, {c: symbols.digest(self.root / "usr/bin" / c)
                                  for c in symbols.COMPONENTS})

    def test_inspection_tools_never_receive_the_upload_token(self):
        actual = subprocess.run
        def checked(*args, **kwargs):
            self.assertNotIn("SENTRY_AUTH_TOKEN", kwargs["env"])
            return actual(*args, **kwargs)
        with mock.patch.dict(os.environ, {"SENTRY_AUTH_TOKEN": "synthetic-token"}), \
                mock.patch.object(subprocess, "run", side_effect=checked):
            symbols.elf_id(self.root / "usr/bin/arinnd")

    def test_a_different_packaged_binary_is_rejected(self):
        shutil.copyfile(self.source / "native", self.root / "usr/bin/arinnd")
        with self.assertRaisesRegex(ValueError, "packaged binary"):
            symbols.verify(self.root, self.artifacts / "arch", "arch", BUILD)

    def test_rejects_stale_or_incomplete_collection(self):
        with self.assertRaises(FileExistsError):
            symbols.collect(self.root, self.artifacts / "arch", "arch", BUILD)
        with self.assertRaisesRegex(ValueError, "clean commit"):
            symbols.collect(self.root, self.base / "dirty", "arch", BUILD + "-dirty")
        subprocess.run(["objcopy", "--strip-debug", str(self.root / "usr/bin/arinnd")], check=True)
        # A string table is not a line table, despite the shared name prefix.
        strings = self.base / "line-strings"
        strings.write_bytes(b"\0synthetic.rs\0")
        subprocess.run(["objcopy", "--add-section", f".debug_line_str={strings}",
                        str(self.root / "usr/bin/arinnd")], check=True)
        with self.assertRaisesRegex(ValueError, "line tables"):
            symbols.collect(self.root, self.base / "stripped", "arch", BUILD)

    def test_rejects_wrong_release_tampering_extra_files_and_symlinks(self):
        artifact = self.artifacts / "arch"
        with self.assertRaisesRegex(ValueError, "this build"):
            symbols.validate(artifact, "arch", "0.1.0+gffffffffffff")
        with self.assertRaisesRegex(ValueError, "this build"):
            symbols.validate(artifact, "debian", BUILD)
        debug = artifact / "arinnd.debug"
        original = debug.read_bytes()
        debug.write_bytes(original + b"altered")
        with self.assertRaisesRegex(ValueError, "contents"):
            symbols.validate(artifact, "arch", BUILD)
        debug.write_bytes(original)
        (artifact / "source.zip").write_bytes(b"not allowed")
        with self.assertRaisesRegex(ValueError, "unexpected file"):
            symbols.validate(artifact, "arch", BUILD)
        (artifact / "source.zip").unlink()
        outside = self.base / "outside.debug"
        debug.rename(outside)
        debug.symlink_to(outside)
        with self.assertRaisesRegex(ValueError, "regular file"):
            symbols.validate(artifact, "arch", BUILD)

    def environment(self, **changes):
        return {"PATH": os.environ["PATH"], "GITHUB_REPOSITORY": "LimeBeck/arinn",
                "GITHUB_EVENT_NAME": "push", "GITHUB_REF": "refs/heads/master",
                "GITHUB_SHA": COMMIT, "SENTRY_AUTH_TOKEN": "synthetic-ci-token", **changes}

    def test_upload_only_verified_debug_files_with_fixed_project_and_private_home(self):
        recorded = self.base / "recorded.json"
        cli = self.base / "record-cli"
        cli.write_text("#!/usr/bin/env python3\nimport json, os, pathlib, sys\n"
                       "assert all(pathlib.Path(p).is_file() for p in sys.argv[-4:])\n"
                       f"pathlib.Path({str(recorded)!r}).write_text(json.dumps(\n"
                       " {'args':sys.argv[1:], 'env':dict(os.environ), 'cwd':os.getcwd()}))\n"
                       "print(os.environ['SENTRY_AUTH_TOKEN'])\n")
        cli.chmod(0o700)
        output = io.StringIO()
        # A real HOME's .sentryclirc and SENTRY_* overrides must not reach the CLI.
        env = self.environment(HOME=str(self.base), SENTRY_ORG="wrong", SENTRY_PROJECT="wrong",
                               SENTRY_URL="http://invalid.example", SENTRY_LOG_LEVEL="debug")
        with contextlib.redirect_stdout(output):
            symbols.upload(self.artifacts, BUILD, env, cli)
        self.assertNotIn("synthetic-ci-token", output.getvalue())
        call = json.loads(recorded.read_text())
        self.assertEqual(call["env"]["SENTRY_ORG"], "limebeckdev")
        self.assertEqual(call["env"]["SENTRY_PROJECT"], "arinn")
        self.assertEqual(call["env"]["SENTRY_URL"], "https://sentry.io/")
        self.assertNotEqual(call["env"]["HOME"], str(self.base))
        self.assertEqual(call["cwd"], call["env"]["HOME"])
        self.assertEqual(call["args"][:8], ["debug-files", "upload", "--type", "elf",
                                            "--no-sources", "--wait-for", "300",
                                            str(self.artifacts / "arch/arinnd.debug")])
        self.assertEqual(len(call["args"][-4:]), 4)
        self.assertNotIn("--include-sources", call["args"])
        self.assertTrue(all(pathlib.Path(p).is_absolute() for p in call["args"][-4:]))

    def test_malformed_manifest_is_rejected(self):
        manifest = self.artifacts / "arch/manifest.json"
        for data in (None, 42, [{"unexpected": "object"}]):
            manifest.write_text(json.dumps(data))
            with self.assertRaises(ValueError):
                symbols.validate(self.artifacts / "arch", "arch", BUILD)

    def test_wrong_workflow_commit_and_missing_artifacts_never_invoke_cli(self):
        cli = self.base / "nonexistent"
        with self.assertRaisesRegex(ValueError, "workflow commit"):
            symbols.upload(self.artifacts, BUILD, self.environment(GITHUB_SHA="f" * 40), cli)
        (self.artifacts / "debian/arinn-shell.debug").unlink()
        with self.assertRaises(ValueError):
            symbols.upload(self.artifacts, BUILD, self.environment(), cli)


class UploadGuardTest(unittest.TestCase):
    def test_untrusted_events_or_missing_token_do_not_read_files_or_download_cli(self):
        env = {"GITHUB_REPOSITORY": "LimeBeck/arinn", "GITHUB_EVENT_NAME": "push",
               "GITHUB_REF": "refs/heads/master", "SENTRY_AUTH_TOKEN": "synthetic-ci-token"}
        cases = [{"GITHUB_EVENT_NAME": "pull_request"},
                 {"GITHUB_EVENT_NAME": "workflow_dispatch"},
                 {"GITHUB_EVENT_NAME": "schedule"},
                 {"GITHUB_REPOSITORY": "example/fork"},
                 {"GITHUB_REF": "refs/heads/feature"},
                 {"GITHUB_REF": "refs/tags/snapshot-" + COMMIT},
                 {"GITHUB_REF": "refs/tags/vanything"}, {"SENTRY_AUTH_TOKEN": ""}]
        for changes in cases:
            with self.subTest(changes=changes), mock.patch.object(symbols, "validate") as validate, \
                    mock.patch.object(symbols, "install_cli") as install, \
                    contextlib.redirect_stdout(io.StringIO()):
                symbols.upload(pathlib.Path("/nonexistent"), BUILD, {**env, **changes})
                validate.assert_not_called()
                install.assert_not_called()
        self.assertTrue(symbols.trusted({**env, "GITHUB_REF": "refs/tags/v0.1.0"}))

    def test_cli_checksum_is_checked_before_execution_permission(self):
        class Response(io.BytesIO):
            url = "https://example.invalid/synthetic-cli"
        with tempfile.TemporaryDirectory() as temporary:
            cli = pathlib.Path(temporary) / "cli"
            with mock.patch.object(symbols.urllib.request, "urlopen", return_value=Response(b"wrong")):
                with self.assertRaisesRegex(ValueError, "checksum"):
                    symbols.install_cli(cli)
            self.assertEqual(cli.stat().st_mode & 0o111, 0)
            payload = b"synthetic cli"
            with mock.patch.object(symbols.urllib.request, "urlopen", return_value=Response(payload)), \
                    mock.patch.object(symbols, "CLI_SHA256", symbols.hashlib.sha256(payload).hexdigest()):
                symbols.install_cli(cli)
            self.assertEqual(cli.stat().st_mode & 0o777, 0o700)

    def test_workflow_uses_only_tested_packages_and_keeps_token_out_of_build_jobs(self):
        ci = jobs()
        upload = ci["symbols"]
        self.assertIn("needs: [scope, images, build, licenses, docs, package]", upload)
        condition = value(upload, "if")
        for event, ref, kind, allowed in [
            ("push", "refs/heads/master", "branch", True),
            ("push", "refs/tags/v0.1.0", "tag", True),
            ("pull_request", "refs/pull/1/merge", "branch", False),
            ("workflow_dispatch", "refs/heads/master", "branch", False),
            ("schedule", "refs/heads/master", "branch", False),
        ]:
            self.assertEqual(bool(evaluate(condition, event_name=event, ref=ref, ref_type=kind)), allowed)
        for name, body in ci.items():
            if name != "symbols":
                self.assertNotIn("secrets.SENTRY_AUTH_TOKEN", body, name)
        self.assertIn("python -m unittest discover -s tests/ci -p test_debug_symbols.py", ci["build"])
        self.assertIn("package, symbols]", ci["snapshot"])
        self.assertIn("name: arinn-arch-symbols", upload)
        self.assertIn("name: arinn-deb-symbols", upload)
        release = (ROOT / ".github/workflows/release.yml").read_text()
        self.assertIn("SENTRY_AUTH_TOKEN: ${{ secrets.SENTRY_AUTH_TOKEN }}", release)
        package = (ROOT / ".github/workflows/package.yml").read_text()
        self.assertNotIn("SENTRY_AUTH_TOKEN", package)
        self.assertEqual(package.count("scripts/debug-symbols.py verify"), 2)


if __name__ == "__main__":
    unittest.main()
