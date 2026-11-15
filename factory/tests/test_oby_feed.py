#!/usr/bin/env python3
"""factory/bin/oby-feed's own tests (FEED-1 to FEED-6, task A00-80), never obid, GitHub or the gate VM.

Run from the repository root: python3 -m unittest -v factory/tests/test_oby_feed.py
The feeder's state is a temporary folder; obid's API and every `gh` call are stand-ins that log what they
were asked and answer as each test sets.
"""

import importlib.machinery
import importlib.util
import json
import shutil
import subprocess
import tempfile
import unittest
from argparse import Namespace
from pathlib import Path

OBY_FEED = Path(__file__).resolve().parents[1] / "bin" / "oby-feed"
REPO = "owner/repo"
PR = 41
OLD = "a" * 40
NEW = "b" * 40
JOB = "job-1"


def load_feed():
    loader = importlib.machinery.SourceFileLoader("oby_feed", str(OBY_FEED))
    spec = importlib.util.spec_from_loader("oby_feed", loader)
    module = importlib.util.module_from_spec(spec)
    loader.exec_module(module)
    return module


def approvals(head, *tools):
    return [{"commit": head, "agent": tool, "verdict": "approved"} for tool in tools]


def queue(state="OPEN", in_queue=False, auto=None, removed=None):
    return {"data": {"repository": {"pullRequest": {
        "state": state, "isDraft": False, "headRefOid": OLD, "mergeStateStatus": "CLEAN", "isInMergeQueue": in_queue,
        "autoMergeRequest": auto, "timelineItems": {"nodes": [removed] if removed else []},
    }}}}


class StandIns:
    """obid's API and `gh`: each test sets the answers, and every call is logged."""

    def __init__(self):
        self.calls = []
        self.jobs = {JOB: {"id": JOB, "kind": "build", "status": "done", "approvals": []}}
        self.view = None  # `gh pr view ... --json headRefOid,state,files`
        self.graphql = None  # `gh api graphql` with job.QUEUE_READ
        self.merge_code = 0

    def api(self, method, path, body=None, timeout=30):
        self.calls.append(("api", method, path))
        if method == "GET" and path.startswith("/v1/jobs/"):
            found = self.jobs.get(path.rsplit("/", 1)[1])
            return (200, found) if found else (404, {"error": "no such job"})
        if method == "DELETE":
            return 204, None
        return 500, {"error": "unexpected"}

    def run(self, cmd, **kwargs):
        self.calls.append(("run", *cmd))
        out, code = "", 0
        if cmd[:3] == ["gh", "pr", "view"]:
            out = json.dumps(self.view) if self.view else ""
        elif cmd[:3] == ["gh", "api", "graphql"]:
            out = json.dumps(self.graphql) if self.graphql else ""
        elif cmd[:3] == ["gh", "pr", "merge"]:
            code = self.merge_code
        elif cmd[:2] != ["git", "fetch"]:
            raise AssertionError(f"unexpected command: {cmd}")
        return subprocess.CompletedProcess(cmd, code, stdout=out, stderr="refused" if code else "")

    def merges(self):
        return [c[1:] for c in self.calls if c[:4] == ("run", "gh", "pr", "merge")]


class ObyFeed(unittest.TestCase):
    def setUp(self):
        self.dir = Path(tempfile.mkdtemp()).resolve()
        self.addCleanup(shutil.rmtree, self.dir, True)
        self.feed = load_feed()
        self.stand = StandIns()
        self.feed.HOME = self.dir
        self.feed.FEED = self.dir / "feed"
        self.feed.GH_REPO = REPO
        self.feed.api = self.stand.api
        self.feed.subprocess = Namespace(run=self.stand.run)
        self.feed.say = lambda msg: None

    def task(self, state, **more):
        item = {"key": "A00-99", "title": "a task", "job": JOB, "state": state, "pr": PR, "files": ["x.py"], **more}
        self.feed.save(item)
        return item

    def task_now(self):
        return self.feed.load_state()["A00-99"]

    def one_pass(self, maximum=3):
        return self.feed.one_pass(Namespace(max=maximum, min_free_gb=0, dry_run=False, account="acct"))

    def no_ready(self):
        self.feed.ready_tasks = lambda state: []

    # FEED-1: a merge GitHub's queue refused leaves `merging` for `waiting`, remembering the armed head
    def test_feed_1_refused_merge_goes_to_waiting_with_its_head(self):
        self.no_ready()
        self.task("merging", head=OLD, approved=["claude", "codex"], armedAt="2026-11-15T10:00:00Z")
        self.stand.graphql = queue(removed={"createdAt": "2026-11-15T10:05:00Z", "reason": "FAILED_CHECKS"})
        active, started = self.one_pass()
        item = self.task_now()
        self.assertEqual(item["state"], "waiting")
        self.assertEqual(item["refused"], OLD)
        self.assertIn(OLD, item["note"])
        self.assertIn("FAILED_CHECKS", item["note"])
        self.assertEqual((active, started), (0, 0))
        read = [c for c in self.stand.calls if c[:4] == ("run", "gh", "api", "graphql")][0]
        self.assertIn("owner=owner", read)
        self.assertIn("name=repo", read)
        self.assertIn(f"number={PR}", read)
        self.assertIn(f"query={self.feed.job.QUEUE_READ}", read)

    def test_feed_1_an_older_removal_is_not_the_reason(self):
        self.no_ready()
        self.task("merging", head=OLD, armedAt="2026-11-15T10:00:00Z")
        self.stand.graphql = queue(removed={"createdAt": "2026-11-14T10:00:00Z", "reason": "FAILED_CHECKS"})
        self.one_pass()
        item = self.task_now()
        self.assertEqual(item["state"], "waiting")
        self.assertNotIn("FAILED_CHECKS", item["note"])
        self.assertIn("couldn't be read", item["note"])

    def test_feed_1_stays_merging_while_in_the_queue(self):
        self.no_ready()
        self.task("merging", head=OLD, armedAt="2026-11-15T10:00:00Z")
        self.stand.graphql = queue(in_queue=True, auto=None)
        active, _ = self.one_pass()
        self.assertEqual(self.task_now()["state"], "merging")
        self.assertEqual(active, 1)

    def test_feed_1_stays_merging_while_auto_merge_is_on(self):
        self.no_ready()
        self.task("merging", head=OLD, armedAt="2026-11-15T10:00:00Z")
        self.stand.graphql = queue(in_queue=False, auto={"enabledAt": "2026-11-15T10:00:00Z"})
        active, _ = self.one_pass()
        self.assertEqual(self.task_now()["state"], "merging")
        self.assertEqual(active, 1)

    def test_feed_1_an_unread_queue_leaves_it_merging(self):
        self.no_ready()
        self.task("merging", head=OLD, armedAt="2026-11-15T10:00:00Z")
        self.one_pass()
        self.assertEqual(self.task_now()["state"], "merging")

    # FEED-2: a refused head is never armed again; a new head approved twice is
    def test_feed_2_the_refused_head_is_not_armed_again(self):
        self.no_ready()
        self.task("waiting", head=OLD, refused=OLD, note="GitHub's queue didn't merge it")
        self.stand.jobs[JOB]["approvals"] = approvals(OLD, "claude", "codex")
        self.stand.view = {"headRefOid": OLD, "state": "OPEN", "files": [{"path": "x.py"}]}
        self.one_pass()
        item = self.task_now()
        self.assertEqual(self.stand.merges(), [])
        self.assertEqual(item["state"], "waiting")
        self.assertEqual(item["note"], "GitHub's queue didn't merge it")

    def test_feed_2_a_new_head_approved_twice_is_armed(self):
        self.no_ready()
        self.task("waiting", head=OLD, refused=OLD)
        self.stand.jobs[JOB]["approvals"] = approvals(OLD, "claude", "codex") + approvals(NEW, "claude", "codex")
        self.stand.view = {"headRefOid": NEW, "state": "OPEN", "files": [{"path": "x.py"}]}
        self.one_pass()
        item = self.task_now()
        self.assertEqual(self.stand.merges(), [("gh", "pr", "merge", str(PR), "--repo", REPO, "--auto", "--match-head-commit", NEW)])
        self.assertEqual(item["state"], "merging")
        self.assertEqual(item["head"], NEW)
        self.assertEqual(item["approved"], ["claude", "codex"])
        self.assertTrue(item["armedAt"].endswith("Z"))

    def test_feed_2_a_head_approved_by_one_tool_stays_waiting(self):
        self.no_ready()
        self.task("waiting", head=OLD, refused=OLD, note="held")
        self.stand.jobs[JOB]["approvals"] = approvals(NEW, "claude")
        self.stand.view = {"headRefOid": NEW, "state": "OPEN", "files": [{"path": "x.py"}]}
        self.one_pass()
        self.assertEqual(self.stand.merges(), [])
        self.assertEqual(self.task_now()["note"], "held")

    def test_feed_2_a_merge_github_refuses_holds_that_head(self):
        self.no_ready()
        self.task("waiting", head=OLD, refused=OLD)
        self.stand.jobs[JOB]["approvals"] = approvals(NEW, "claude", "codex")
        self.stand.view = {"headRefOid": NEW, "state": "OPEN", "files": [{"path": "x.py"}]}
        self.stand.merge_code = 1
        self.one_pass()
        item = self.task_now()
        self.assertEqual(item["state"], "waiting")
        self.assertEqual(item["refused"], NEW)
        self.assertTrue(item["note"].startswith("GitHub refused the merge"))
        self.one_pass()
        self.assertEqual(len(self.stand.merges()), 1)

    # FEED-3: Oby's follow-up takes a waiting task back to running
    def test_feed_3_a_follow_up_takes_a_waiting_task_back_to_running(self):
        self.no_ready()
        self.task("waiting", head=OLD, note="a review asked for changes or needs you: open it in Oby")
        follow_up = {"id": "job-2", "kind": "build", "status": "running", "followUpOf": JOB}
        self.stand.jobs["job-2"] = follow_up
        self.feed.oby_jobs = lambda: [self.stand.jobs[JOB], follow_up]
        active, _ = self.one_pass()
        item = self.task_now()
        self.assertEqual(item["state"], "running")
        self.assertEqual(item["job"], "job-2")
        self.assertEqual(active, 1)

    # FEED-4: a waiting task whose pull request was merged by hand is merged, its Oby jobs deleted
    def test_feed_4_merged_by_hand_deletes_its_jobs(self):
        self.no_ready()
        self.task("waiting", head=OLD, note="approved twice, but it touches a security path (AGENTS.md): merge it yourself")
        family = [self.stand.jobs[JOB], {"id": "review-1", "reviewOf": JOB}, {"id": "job-2", "followUpOf": JOB, "kind": "fix"},
                  {"id": "review-2", "reviewOf": "job-2"}, {"id": "other", "kind": "build"}]
        self.feed.oby_jobs = lambda: family
        self.stand.view = {"headRefOid": OLD, "state": "MERGED", "files": [{"path": "AGENTS.md"}]}
        self.one_pass()
        item = self.task_now()
        self.assertEqual(item["state"], "merged")
        deleted = sorted(c[2] for c in self.stand.calls if c[:2] == ("api", "DELETE"))
        self.assertEqual(deleted, [f"/v1/jobs/{i}" for i in sorted([JOB, "job-2", "review-1", "review-2"])])

    def test_feed_4_a_closed_pull_request_fails_it(self):
        self.no_ready()
        self.task("waiting", head=OLD)
        self.stand.view = {"headRefOid": OLD, "state": "CLOSED", "files": []}
        self.one_pass()
        item = self.task_now()
        self.assertEqual(item["state"], "failed")
        self.assertEqual(item["note"], "its pull request was closed")

    # FEED-5: a waiting task never counts against --max
    def test_feed_5_a_waiting_task_holds_no_slot(self):
        self.task("waiting", head=OLD, refused=OLD, note="held")
        self.stand.view = {"headRefOid": OLD, "state": "OPEN", "files": []}
        ready = {"key": "A00-98", "title": "another task"}
        self.feed.ready_tasks = lambda state: [(ready, True, {"y.py"})]
        self.feed.start = lambda task, account, on_base: ("job-9", None)
        self.feed.free_gb = lambda: 100.0
        active, started = self.one_pass(maximum=1)
        self.assertEqual((active, started), (1, 1))
        state = self.feed.load_state()
        self.assertEqual(state["A00-98"]["state"], "running")
        self.assertEqual(state["A00-99"]["state"], "waiting")

    # FEED-6: a security path never arms auto-merge, from reviewing or waiting
    def test_feed_6_a_security_path_is_never_armed_from_reviewing(self):
        self.no_ready()
        self.task("reviewing")
        self.stand.jobs[JOB]["approvals"] = approvals(OLD, "claude", "codex")
        self.stand.view = {"headRefOid": OLD, "state": "OPEN", "files": [{"path": "AGENTS.md"}]}
        self.one_pass()
        self.one_pass()
        item = self.task_now()
        self.assertEqual(self.stand.merges(), [])
        self.assertEqual(item["state"], "waiting")
        self.assertIn("AGENTS.md", item["note"])

    def test_feed_6_a_security_path_is_never_armed_from_waiting(self):
        self.no_ready()
        self.task("waiting", head=OLD, refused=OLD)
        self.stand.jobs[JOB]["approvals"] = approvals(NEW, "claude", "codex")
        self.stand.view = {"headRefOid": NEW, "state": "OPEN", "files": [{"path": "x.py"}, {"path": "AGENTS.md"}]}
        self.one_pass()
        self.one_pass()
        item = self.task_now()
        self.assertEqual(self.stand.merges(), [])
        self.assertEqual(item["state"], "waiting")
        self.assertIn("AGENTS.md", item["note"])

    def test_feed_6_may_arm_checks_the_cheap_rules_before_the_models(self):
        self.feed.check_models = lambda files, head: self.fail("check_models ran for a head that can't merge")
        pr = {"headRefOid": OLD, "state": "OPEN", "files": [{"path": "AGENTS.md"}, {"path": "docs/models/x.qnt"}]}
        j = {"approvals": approvals(OLD, "claude", "codex")}
        self.assertEqual(self.feed.may_arm(j, pr, None)[0], "wait")
        self.assertEqual(self.feed.may_arm({"approvals": approvals(OLD, "claude")}, pr, None), ("pending", None))
        self.assertEqual(self.feed.may_arm(j, pr, OLD)[0], "wait")


if __name__ == "__main__":
    unittest.main()
