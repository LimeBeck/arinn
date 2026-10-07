# SPDX-FileCopyrightText: 2026 Arinn contributors
#
# SPDX-License-Identifier: GPL-3.0-or-later

"""CI builds and tests each thing once (ADR-0092)."""

import pathlib
import re
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[2]
PKGBUILD = (ROOT / "packaging" / "arch" / "PKGBUILD").read_text()
CI = (ROOT / ".github" / "workflows" / "ci.yml").read_text()
PACKAGE = (ROOT / ".github" / "workflows" / "package.yml").read_text()


def function(name: str) -> str:
    match = re.search(rf"^{name}\(\) \{{\n(.*?)^\}}", PKGBUILD, re.M | re.S)
    assert match, name
    return match.group(1)


def evaluate(expression: str, **context: str) -> object:
    """Evaluate a GitHub Actions expression made of ==, !=, &&, || and !.

    Python's and/or short-circuit and return operands the way the Actions
    expression language does, so '' stays falsy in both.
    """
    body = re.fullmatch(r"\$\{\{(.*)\}\}", expression.strip()).group(1)
    for key, value in sorted(context.items(), key=lambda item: -len(item[0])):
        body = body.replace(f"github.{key}", repr(value))
    body = body.replace("&&", " and ").replace("||", " or ")
    body = re.sub(r"!(?!=)", " not ", body)
    assert "github." not in body, body
    return eval(body, {"__builtins__": {}})  # noqa: S307 - fixed workflow text


def value(text: str, key: str) -> str:
    match = re.search(rf"^\s*{key}: (\$\{{\{{.*\}}\}})$", text, re.M)
    assert match, key
    return match.group(1)


class PackageBuildTest(unittest.TestCase):
    def test_package_builds_only_the_installed_shell_target(self):
        self.assertIn("cmake --build build --target arinn-shell\n", function("build"))
        # Everything package() takes from the CMake tree must come from that
        # target or from configure time; anything else would not be built.
        used = set(re.findall(r"\bbuild/[^\s\"]+", function("package")))
        self.assertEqual(used, {
            "build/apps/arinn-shell/arinn-shell",
            "build/observability/native/licenses/*-LICENSE",
        })

    def test_release_mode_check_runs_on_nightly_manual_and_tag_builds_only(self):
        nocheck = value(PACKAGE, "NOCHECK")
        cases = {
            ("pull_request", "branch"): "--nocheck",
            ("push", "branch"): "--nocheck",
            ("schedule", "branch"): "",
            ("workflow_dispatch", "branch"): "",
            # release.yml calls CI from a tag push.
            ("push", "tag"): "",
        }
        for (event, ref_type), expected in cases.items():
            with self.subTest(event=event, ref_type=ref_type):
                self.assertEqual(evaluate(nocheck, event_name=event, ref_type=ref_type), expected)


def jobs() -> dict[str, str]:
    section = CI[CI.index("\njobs:\n"):]
    names = re.findall(r"^  (\w[\w-]*):\n", section, re.M)
    bodies = re.split(r"^  \w[\w-]*:\n", section, flags=re.M)[1:]
    return dict(zip(names, bodies))


class BuildJobTest(unittest.TestCase):
    def test_one_arch_job_runs_rust_checks_before_the_shell(self):
        job = jobs()["build"]
        steps = ["cargo fmt --all --check", "cargo clippy --locked --all-targets -- -D warnings",
                 "cargo test --locked\n", "cmake --build build", "ctest --test-dir build",
                 "python tests/integration/phase1.py"]
        positions = [job.index(step) for step in steps]
        self.assertEqual(positions, sorted(positions))
        self.assertIn("container: archlinux:latest", job)
        # No other job compiles the workspace in debug mode again.
        for name, body in jobs().items():
            if name != "build":
                with self.subTest(job=name):
                    self.assertNotRegex(body, r"cargo (build|test|clippy)")
        # The X11 step runs only the Xvfb tests; cargo test ran the others.
        self.assertIn("-p arinn-platform-x11 -- --ignored", job)

    def test_needs_name_existing_jobs(self):
        names = set(jobs())
        for name, body in jobs().items():
            for need in re.findall(r"^    needs: \[?([^\]\n]*)\]?$", body, re.M):
                for target in need.split(","):
                    with self.subTest(job=name, needs=target.strip()):
                        self.assertIn(target.strip(), names)
        self.assertIn("build", re.search(r"needs: \[(.*)\]", jobs()["snapshot"]).group(1))

    def test_build_restores_rust_and_compiler_caches(self):
        job = jobs()["build"]
        self.assertIn("Swatinem/rust-cache@v2", job)
        self.assertIn("actions/cache/restore@v4", job)
        self.assertIn("CMAKE_CXX_COMPILER_LAUNCHER: ccache", job)
        self.assertRegex(job, r"pacman -Syu [^\n]*\bccache\b")
        # Caches restore before the builds that use them.
        self.assertLess(job.index("rust-cache"), job.index("cargo clippy"))
        self.assertLess(job.index("cache/restore"), job.index("cmake -S"))

    def test_only_master_and_nightly_save_the_compiler_cache(self):
        job = jobs()["build"]
        save = job[job.index("Save compiler cache"):]
        condition = value(save, "if")
        cases = {
            ("pull_request", "refs/pull/1/merge"): False,
            ("push", "refs/heads/master"): True,
            ("schedule", "refs/heads/master"): True,
            ("workflow_dispatch", "refs/heads/master"): False,
            # release.yml calls CI from a tag push.
            ("push", "refs/tags/v1.0.0"): False,
        }
        for (event, ref), saved in cases.items():
            with self.subTest(event=event, ref=ref):
                self.assertEqual(bool(evaluate(condition, event_name=event, ref=ref)), saved)


class ConcurrencyTest(unittest.TestCase):
    def test_only_pull_requests_and_duplicate_master_runs_cancel(self):
        cancel = value(CI, "cancel-in-progress")
        cases = {
            ("pull_request", "refs/pull/1/merge"): True,
            # The group is per master commit, so this cancels only a second
            # run of the same commit (ADR-0086, ADR-0092).
            ("push", "refs/heads/master"): True,
            ("push", "refs/tags/v1.0.0"): False,
            ("schedule", "refs/heads/master"): False,
            ("workflow_dispatch", "refs/heads/master"): False,
        }
        for (event, ref), expected in cases.items():
            with self.subTest(event=event, ref=ref):
                self.assertEqual(evaluate(cancel, event_name=event, ref=ref), expected)
        self.assertIn("github.ref == 'refs/heads/master' && github.sha", CI)


if __name__ == "__main__":
    unittest.main()
