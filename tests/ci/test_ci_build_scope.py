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
DEB_RULES = (ROOT / "packaging" / "debian" / "debian" / "rules").read_text()
ARCH_IMAGE = (ROOT / "ci" / "images" / "arch" / "Dockerfile").read_text()


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
    # The self-hosted runner label, or '' for GitHub-hosted runners.
    body = body.replace("vars.ARINN_RUNNER", repr(context.pop("runner", "")))
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


# The arinn-kwin-background target's QML module (ADR-0087).
KWIN_BACKGROUND = {
    f"build/platform/kde/background/qml/org/arinn/background/{name}"
    for name in ("qmldir", "arinn-kwin-background.qmltypes", "libarinn-kwin-background.so")
}


class PackageBuildTest(unittest.TestCase):
    def test_package_builds_only_the_installed_shell_target(self):
        self.assertIn("cmake --build build --target arinn-shell arinn-kwin-background\n",
                      function("build"))
        # Everything package() takes from the CMake tree must come from that
        # target or from configure time; anything else would not be built.
        used = set(re.findall(r"\bbuild/[^\s\"]+", function("package")))
        self.assertEqual(used, {
            "build/apps/arinn-shell/arinn-shell",
            *KWIN_BACKGROUND,
            "build/observability/native/licenses/*-LICENSE",
        })

    def test_debian_package_builds_only_the_installed_shell_target(self):
        build = re.search(r"^override_dh_auto_build:\n(.*?)\n\n", DEB_RULES, re.M | re.S).group(1)
        self.assertIn("cmake --build build --target arinn-shell arinn-kwin-background\n",
                      build + "\n")
        install = re.search(r"^override_dh_auto_install:\n(.*?)\n\n", DEB_RULES, re.M | re.S).group(1)
        self.assertEqual(set(re.findall(r"\bbuild/[^\s\"]+", install)), {
            "build/apps/arinn-shell/arinn-shell",
            *KWIN_BACKGROUND,
            "build/observability/native/licenses/*-LICENSE",
        })

    def test_release_mode_check_runs_on_nightly_manual_and_tag_builds_only(self):
        for key, skip in (("NOCHECK", "--nocheck"), ("DEB_BUILD_OPTIONS", "nocheck")):
            expression = value(PACKAGE, key)
            cases = {
                ("pull_request", "branch"): skip,
                ("push", "branch"): skip,
                ("schedule", "branch"): "",
                ("workflow_dispatch", "branch"): "",
                # release.yml calls CI from a tag push.
                ("push", "tag"): "",
            }
            for (event, ref_type), expected in cases.items():
                with self.subTest(key=key, event=event, ref_type=ref_type):
                    self.assertEqual(evaluate(expression, event_name=event, ref_type=ref_type), expected)


def jobs(text: str | None = None) -> dict[str, str]:
    text = CI if text is None else text
    section = text[text.index("\njobs:\n"):]
    names = re.findall(r"^  (\w[\w-]*):\n", section, re.M)
    bodies = re.split(r"^  \w[\w-]*:\n", section, flags=re.M)[1:]
    return dict(zip(names, bodies))


def package_jobs() -> dict[str, str]:
    return jobs(PACKAGE)


class BuildJobTest(unittest.TestCase):
    def test_one_arch_job_runs_rust_checks_before_the_shell(self):
        job = jobs()["build"]
        steps = ["cargo fmt --all --check", "cargo clippy --locked --all-targets -- -D warnings",
                 "cargo test --locked\n", "cmake --build build", "ctest --test-dir build",
                 "python tests/integration/phase1.py"]
        positions = [job.index(step) for step in steps]
        self.assertEqual(positions, sorted(positions))
        # The prebuilt Arch Linux image (ADR-0104).
        self.assertIn("image: ${{ needs.images.outputs.arch_build }}", job)
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

    def test_build_keeps_its_caches_in_ci_cache(self):
        # ADR-0101: the Cargo home, target directory and ccache live in
        # /ci-cache, a host directory on the self-hosted runner.
        job = jobs()["build"]
        self.assertIn("- /var/cache/arinn-ci/arch-ci:/ci-cache", job)
        self.assertIn("CARGO_HOME: /ci-cache/cargo", job)
        self.assertIn("CCACHE_DIR: /ci-cache/ccache", job)
        self.assertIn("CMAKE_CXX_COMPILER_LAUNCHER: ccache", job)
        self.assertRegex(ARCH_IMAGE[ARCH_IMAGE.index("AS build"):], r"pacman -Syu [^\n]*\bccache\b")
        self.assertIn('ln -sfn "$target" target', job)
        # Caches are in place before the builds that use them.
        self.assertLess(job.index("cache/restore"), job.index("cargo fmt"))
        self.assertLess(job.index("ln -sfn"), job.index("cargo fmt"))

    def test_each_self_hosted_runner_instance_has_its_own_target_directory(self):
        # ADR-0113: runner instances on one host share /ci-cache; a shared
        # target directory let one job overwrite another's rlibs and binaries
        # while its tests and phase1.py ran (#485). GitHub-hosted runs keep
        # the single cached /ci-cache/target.
        job = jobs()["build"]
        self.assertIn("RUNNER_SLOT: ${{ vars.ARINN_RUNNER && runner.name || '' }}", job)
        self.assertIn('target="$target-', job)
        self.assertIn("target=/ci-cache/target\n", job)

    def test_no_build_job_uses_the_shared_runner_home_for_cargo(self):
        # On the self-hosted runner /github/home is shared by every job:
        # rustup's proxies from the Debian job made the Arch job's
        # cargo fmt resolve to a toolchain without rustfmt (#411).
        for job in (jobs()["build"], package_jobs()["build-deb"]):
            with self.subTest(job=job.splitlines()[0]):
                self.assertNotIn("rust-cache", job)
                self.assertIn("CARGO_HOME: /ci-cache/cargo", job)
        self.assertIn("RUSTUP_HOME: /ci-cache/rustup", package_jobs()["build-deb"])
        self.assertIn("CARGO_HOME='$cache/home'", package_jobs()["build"])

    def test_only_hosted_master_and_nightly_save_the_build_cache(self):
        job = jobs()["build"]
        save = job[job.index("Save build cache"):]
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
                # The self-hosted runner keeps the cache on its own disk.
                self.assertFalse(evaluate(condition, event_name=event, ref=ref, runner="self-hosted"))


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
