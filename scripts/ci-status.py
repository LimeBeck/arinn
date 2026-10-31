#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2026 Arinn contributors
#
# SPDX-License-Identifier: GPL-3.0-or-later

"""Keep a red master or nightly visible (ADR-0121, #614).

    python scripts/ci-status.py nightly   fail unless the latest finished
                                          nightly CI run on master passed
    python scripts/ci-status.py alert     open, update or close the ci-red
                                          issue for the finished CI run

Both read GITHUB_REPOSITORY and GH_TOKEN. `alert` reads the finished run from
the workflow_run event in GITHUB_EVENT_PATH. A failed master push or nightly
opens one issue per kind, later failures comment on it, and the next green
run of the same kind closes it: a green push says nothing about the rolling
dependencies only the nightly upgrades. A run older than the newest finished
run of its kind changes nothing.
"""

import json
import os
import sys
import urllib.error
import urllib.parse
import urllib.request

LABEL = "ci-red"
TITLES = {"push": "CI is red on master", "schedule": "Nightly CI is red"}
FAILED = {"failure", "timed_out", "startup_failure"}
BODY = (
    "CI failed on `master`. While this issue is open, merge only pull requests that fix it "
    "or change only documentation, and cut no release (ADR-0121). "
    "The next green run of the same kind closes it.\n\n"
)


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
            raise RuntimeError(f"GitHub {method} {path} failed ({error.code})") from None


def nightly_problem(api):
    """None when the latest finished nightly on master passed, else why not."""
    reply = api.request("GET", "/actions/workflows/ci.yml/runs"
                               "?branch=master&event=schedule&status=completed&per_page=1")
    runs = (reply or {}).get("workflow_runs", [])
    if not runs:
        return "no finished nightly CI run on master"
    run = runs[0]
    if run.get("conclusion") != "success":
        return (f"the latest nightly {run.get('html_url')} ({run.get('created_at')}) "
                f"ended with {run.get('conclusion')}")
    return None


def alert_action(run):
    """("report" | "close", title) for a run that says something about master."""
    title = TITLES.get(run.get("event"))
    if title is None or run.get("head_branch") != "master":
        return None
    if run.get("conclusion") in FAILED:
        return "report", title
    if run.get("conclusion") == "success":
        return "close", title
    # Cancelled and skipped runs say nothing about master.
    return None


def newer_decisive(api, run):
    """A finished run of the same kind on master, newer than `run`, that passed
    or failed. Pages through every run created since `run`, so any number of
    newer cancelled runs cannot hide it."""
    order = (run.get("created_at", ""), run.get("id", 0))
    created = urllib.parse.quote(">=" + run.get("created_at", ""), safe="")
    page = 1
    while True:
        reply = api.request("GET", "/actions/workflows/ci.yml/runs"
                                   f"?branch=master&event={run['event']}&status=completed"
                                   f"&created={created}&per_page=100&page={page}")
        runs = (reply or {}).get("workflow_runs", [])
        for item in runs:
            if (item.get("conclusion") in FAILED | {"success"}
                    and (item.get("created_at", ""), item.get("id", 0)) > order):
                return item
        if len(runs) < 100:
            return None
        page += 1


def alert(api, run):
    action = alert_action(run)
    if action is None:
        return "ignored"
    # Each master commit has its own CI concurrency group, so runs finish out
    # of order: an older commit's result must not open or close the issue
    # after a newer run of the same kind has reported.
    if newer_decisive(api, run) is not None:
        return "stale"
    kind, title = action
    issues = api.request("GET", f"/issues?state=open&labels={LABEL}&per_page=100") or []
    issue = next((item for item in issues
                  if item.get("title") == title and "pull_request" not in item), None)
    line = f"{run['html_url']} (`{run['head_sha'][:12]}`) ended with {run['conclusion']}."
    if kind == "report":
        if issue:
            api.request("POST", f"/issues/{issue['number']}/comments", {"body": "Still red: " + line})
            return "updated"
        api.request("POST", "/issues", {"title": title, "labels": [LABEL], "body": BODY + line})
        return "opened"
    if issue is None:
        return "green"
    api.request("POST", f"/issues/{issue['number']}/comments", {"body": "Green again: " + line})
    api.request("PATCH", f"/issues/{issue['number']}", {"state": "closed", "state_reason": "completed"})
    return "closed"


def main(argv):
    if len(argv) != 2 or argv[1] not in {"nightly", "alert"}:
        print(__doc__, file=sys.stderr)
        return 2
    repo = os.environ.get("GITHUB_REPOSITORY", "")
    token = os.environ.get("GH_TOKEN", "")
    if not repo or not token:
        print("GITHUB_REPOSITORY and GH_TOKEN are required", file=sys.stderr)
        return 2
    api = GitHub(repo, token)
    if argv[1] == "nightly":
        problem = nightly_problem(api)
        if problem:
            print(f"A red nightly blocks releases: {problem}", file=sys.stderr)
            return 1
        print("The latest nightly on master passed")
        return 0
    with open(os.environ["GITHUB_EVENT_PATH"]) as event:
        run = json.load(event)["workflow_run"]
    print(f"{run.get('event')} run {run.get('html_url')}: {alert(api, run)}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
