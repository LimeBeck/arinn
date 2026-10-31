# SPDX-FileCopyrightText: 2026 Arinn contributors
#
# SPDX-License-Identifier: GPL-3.0-or-later

"""A red master or nightly stays visible and blocks releases (#614, ADR-0121)."""

import importlib.util
import pathlib
import re
import unittest
import urllib.parse

from test_ci_build_scope import jobs

ROOT = pathlib.Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "scripts" / "ci-status.py"
spec = importlib.util.spec_from_file_location("ci_status", SCRIPT)
ci_status = importlib.util.module_from_spec(spec)
spec.loader.exec_module(ci_status)
WORKFLOWS = ROOT / ".github" / "workflows"
ALERT = (WORKFLOWS / "ci-alert.yml").read_text()
RELEASE = (WORKFLOWS / "release.yml").read_text()
LABELS = (ROOT / ".github" / "labels.yml").read_text()


class FakeGitHub:
    def __init__(self, runs=None, issues=None):
        self.runs = runs or []
        self.issues = issues or []
        self.calls = []

    def request(self, method, path, body=None):
        self.calls.append((method, path, body))
        if method == "GET" and path.startswith("/actions/workflows/ci.yml/runs?"):
            # Like the REST API: newest first, `created=>=` filter, paged.
            query = urllib.parse.parse_qs(path.split("?", 1)[1])
            runs = [item for item in self.runs if item["event"] == query["event"][0]]
            if "created" in query:
                since = query["created"][0].removeprefix(">=")
                runs = [item for item in runs if item["created_at"] >= since]
            runs.sort(key=lambda item: (item["created_at"], item["id"]), reverse=True)
            size = int(query.get("per_page", ["30"])[0])
            page = int(query.get("page", ["1"])[0])
            return {"workflow_runs": runs[(page - 1) * size:page * size]}
        if method == "GET" and path.startswith("/actions/runs/"):
            run_id = int(path.rsplit("/", 1)[1])
            return next((item for item in self.runs if item["id"] == run_id), {})
        if method == "GET" and path.startswith("/issues?"):
            return self.issues
        return {}

    def writes(self):
        return [(method, path, body) for method, path, body in self.calls if method != "GET"]


def run(event="push", conclusion="failure", branch="master", run_id=1, created="2026-10-30T03:17:00Z"):
    return {"id": run_id, "event": event, "conclusion": conclusion, "head_branch": branch,
            "html_url": "https://github.com/example/arinn/actions/runs/1",
            "head_sha": "0123456789abcdef0123456789abcdef01234567",
            "created_at": created}


def issue(number, title, label="ci-red"):
    return {"number": number, "title": title, "labels": [{"name": label}]}


class NightlyTest(unittest.TestCase):
    def test_latest_finished_nightly_on_master_is_queried(self):
        api = FakeGitHub(runs=[run("schedule", "success")])
        self.assertIsNone(ci_status.nightly_problem(api))
        (method, path, _), = api.calls
        self.assertEqual(method, "GET")
        for part in ("branch=master", "event=schedule", "status=completed", "per_page=1"):
            self.assertIn(part, path)

    def test_a_red_or_missing_nightly_blocks_the_release(self):
        for conclusion in ("failure", "cancelled", "timed_out"):
            with self.subTest(conclusion=conclusion):
                problem = ci_status.nightly_problem(FakeGitHub(runs=[run("schedule", conclusion)]))
                self.assertIn(conclusion, problem)
        self.assertIn("no finished nightly", ci_status.nightly_problem(FakeGitHub()))


class AlertTest(unittest.TestCase):
    def test_first_failure_opens_one_labelled_issue(self):
        api = FakeGitHub()
        self.assertEqual(ci_status.alert(api, run("push")), "opened")
        (method, path, body), = api.writes()
        self.assertEqual((method, path), ("POST", "/issues"))
        self.assertEqual(body["title"], "CI is red on master")
        self.assertEqual(body["labels"], ["ci-red"])
        self.assertIn("merge only pull requests that fix it", body["body"])

    def test_later_failures_comment_on_the_open_issue(self):
        api = FakeGitHub(issues=[issue(7, "CI is red on master")])
        self.assertEqual(ci_status.alert(api, run("push", "timed_out")), "updated")
        (method, path, body), = api.writes()
        self.assertEqual((method, path), ("POST", "/issues/7/comments"))
        self.assertIn("timed_out", body["body"])

    def test_green_run_closes_only_its_own_kind(self):
        # A green push says nothing about the dependencies the nightly upgrades.
        nightly = issue(8, "Nightly CI is red")
        api = FakeGitHub(issues=[nightly])
        self.assertEqual(ci_status.alert(api, run("push", "success")), "green")
        self.assertEqual(api.writes(), [])
        self.assertEqual(ci_status.alert(api, run("schedule", "success")), "closed")
        self.assertEqual([(m, p) for m, p, _ in api.writes()],
                         [("POST", "/issues/8/comments"), ("PATCH", "/issues/8")])
        self.assertEqual(api.writes()[-1][2]["state"], "closed")

    def test_failed_nightly_opens_its_own_issue_beside_a_master_one(self):
        api = FakeGitHub(issues=[issue(7, "CI is red on master")])
        self.assertEqual(ci_status.alert(api, run("schedule")), "opened")
        self.assertEqual(api.writes()[0][2]["title"], "Nightly CI is red")

    def test_pull_requests_and_indecisive_runs_are_ignored(self):
        api = FakeGitHub()
        for given in (run("pull_request"), run("workflow_dispatch"), run("push", branch="feature"),
                      run("push", "cancelled"), run("schedule", "skipped")):
            with self.subTest(given=given):
                self.assertEqual(ci_status.alert(api, given), "ignored")
        self.assertEqual(api.calls, [])

    def test_pull_requests_are_not_mistaken_for_the_issue(self):
        pull = {**issue(9, "CI is red on master"), "pull_request": {}}
        api = FakeGitHub(issues=[pull])
        self.assertEqual(ci_status.alert(api, run("push")), "opened")


class OrderTest(unittest.TestCase):
    def test_an_older_green_run_does_not_close_a_newer_failure(self):
        # Master commits finish out of order (one CI group per commit).
        older = run("push", "success", run_id=1, created="2026-10-30T10:00:00Z")
        newer = run("push", "failure", run_id=2, created="2026-10-30T11:00:00Z")
        api = FakeGitHub(runs=[newer, older], issues=[issue(7, "CI is red on master")])
        self.assertEqual(ci_status.alert(api, older), "stale")
        self.assertEqual(api.writes(), [])

    def test_an_older_failure_does_not_reopen_after_a_newer_green_run(self):
        older = run("push", "failure", run_id=1, created="2026-10-30T10:00:00Z")
        newer = run("push", "success", run_id=2, created="2026-10-30T11:00:00Z")
        api = FakeGitHub(runs=[newer, older])
        self.assertEqual(ci_status.alert(api, older), "stale")
        self.assertEqual(api.writes(), [])

    def test_many_newer_cancelled_runs_do_not_hide_a_newer_failure(self):
        # A newer failure behind more than a page of cancelled runs (#621 review).
        older = run("push", "success", run_id=1, created="2026-10-30T10:00:00Z")
        newer = run("push", "failure", run_id=2, created="2026-10-30T11:00:00Z")
        cancelled = [run("push", "cancelled", run_id=100 + index,
                         created=f"2026-10-30T12:{index // 60:02d}:{index % 60:02d}Z")
                     for index in range(150)]
        api = FakeGitHub(runs=[*cancelled, newer, older], issues=[issue(7, "CI is red on master")])
        self.assertEqual(ci_status.alert(api, older), "stale")
        self.assertEqual(api.writes(), [])
        pages = [path for method, path, _ in api.calls if "/actions/workflows/" in path]
        self.assertEqual(len(pages), 2)
        self.assertIn("created=%3E%3D2026-10-30T10%3A00%3A00Z", pages[0])

    def test_a_delayed_earlier_attempt_does_not_override_its_re_run(self):
        # A failed run re-run green keeps its ID; the failure report arrives late.
        first = {**run("push", "failure", run_id=5), "run_attempt": 1}
        rerun = {**run("push", "success", run_id=5), "run_attempt": 2}
        api = FakeGitHub(runs=[rerun])
        self.assertEqual(ci_status.alert(api, rerun), "green")
        self.assertEqual(ci_status.alert(api, first), "stale")
        self.assertEqual(api.writes(), [])

    def test_newer_cancelled_runs_and_other_kinds_do_not_make_a_run_stale(self):
        current = run("push", "failure", run_id=1, created="2026-10-30T10:00:00Z")
        cancelled = run("push", "cancelled", run_id=3, created="2026-10-30T12:00:00Z")
        nightly = run("schedule", "success", run_id=2, created="2026-10-30T11:00:00Z")
        api = FakeGitHub(runs=[cancelled, nightly, current])
        self.assertEqual(ci_status.alert(api, current), "opened")


class WorkflowTest(unittest.TestCase):
    def test_alert_runs_after_master_ci_with_issue_access_only(self):
        self.assertRegex(ALERT, r"workflow_run:\n\s+workflows: \[CI\]\n\s+types: \[completed\]\n"
                                r"\s+branches: \[master\]")
        alert = jobs(ALERT)["alert"]
        self.assertIn("github.event.workflow_run.event == 'push'", alert)
        self.assertIn("github.event.workflow_run.event == 'schedule'", alert)
        self.assertIn("issues: write", alert)
        # alert() lists CI runs to skip stale completions.
        self.assertIn("actions: read", alert)
        self.assertNotIn("contents: write", ALERT)
        # The master copy of the script runs, never code from the finished run.
        self.assertIn("ref: master", alert)
        self.assertNotIn("head_sha", ALERT)
        self.assertIn("queue: max", alert)
        self.assertIn("run: python scripts/ci-status.py alert", alert)

    def test_release_checks_the_nightly_before_full_ci(self):
        check = jobs(RELEASE)["check"]
        self.assertIn("actions: read", check)
        self.assertIn("run: python scripts/ci-status.py nightly", check)
        self.assertRegex(jobs(RELEASE)["ci"], re.compile(r"^\s+needs: check$", re.M))

    def test_label_is_declared(self):
        self.assertIn("- name: ci-red\n", LABELS)
        self.assertEqual(ci_status.LABEL, "ci-red")


if __name__ == "__main__":
    unittest.main()
