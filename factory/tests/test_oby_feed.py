#!/usr/bin/env python3
"""factory/bin/oby-feed's own tests (docs/contracts/factory.md, FEEDMODEL-1 to 5, FEEDALIVE-1 to 6, FEEDKANI-1 to 9 and
FEEDAPPROVE-1 to 8; task A00-80, FEED-1 to FEED-6), never obid, GitHub, ssh, Quint, Kani, the gate VM or the network.

FEED-1 to FEED-6: the feeder's state is a temporary folder; obid's API and every `gh` call are stand-ins that log
what they were asked and answer as each test sets.

FEEDAPPROVE-1 to 8: the stand-in obid answers `POST /v1/jobs` with an approver job, `approver-<n>`, or gives back the
one it made under the same clientRequestId (200), as obid's receipts do. The job's `startCommit` is the pull request's
head when it was asked, and its `status` and `summary` are the stand-in's `approver` until a test sets them; that is
`done` with `APPROVE`, so a test that merges after two approvals reaches the merge one pass later.

FEEDMODEL-1 to 4: check_models() runs with subprocess.run stubbed: `gh api` returns a model whose first line is a
`// verify:` line, the `ssh … cat >` and `ssh … rm -f` calls succeed, and each `ssh … quint verify … --invariant=<name> …`
returns the output and exit code the test gives that name, as Quint 0.33.1 printed them on the gate VM. Every output
that isn't empty starts with a stand-in for the VM's protobuf warning, a line that holds the word "error" without
starting with it; an empty one stands for a run that printed nothing at all (a timeout, a dropped connection).

FEEDKANI-1 to 9: check_kani() runs prove's own functions against a real bare origin and the feeder's clone of it, each
pull request's head pushed to origin only, with a stand-in test-run in a temporary prove.BIN that prints Kani's verdict
lines; `gh` and obid are the stand-ins above.

Run from the repository root: python3 -m unittest discover -s factory/tests -p 'test_oby_feed.py' -v
"""

import http.client
import importlib.machinery
import io
import importlib.util
import json
import os
import re
import shutil
import subprocess
import tempfile
import unittest
from argparse import Namespace
from pathlib import Path
from unittest import mock

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


feed = load_feed()

PATH = "docs/models/oby_factory.qnt"
HEAD = "0123456789abcdef0123456789abcdef01234567"
WARNING = "[libprotobuf WARNING] a stand-in for the VM's warning, which mentions an error mid-line"
HOLDS = ("The outcome is: NoError\n[ok] No violation found (12ms).\n", 0)
VIOLATED = ("The outcome is: Error\n[violation] Found an issue (12ms).\nerror: found a counterexample\n", 1)
VERIFY = re.compile(r"\bquint verify\b")


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
        self.merge_err = "refused"
        self.posts = []  # each `POST /v1/jobs` body
        self.post_answers = []  # answers to the next posts, in order: (code, body), or None for no answer
        self.approver = {"status": "done", "summary": "APPROVE\n1. met: the stand-in read it"}
        self.missed = None  # the feeder's OBID_MISSED, for a post with no answer
        self.off_base = set()  # task keys whose file origin/<base> doesn't have, as the approver step asks git

    def api(self, method, path, body=None, timeout=30):
        self.calls.append(("api", method, path))
        if method == "GET" and path.startswith("/v1/jobs/"):
            found = self.jobs.get(path.rsplit("/", 1)[1])
            return (200, found) if found else (404, {"error": "no such job"})
        if method == "DELETE":
            return 204, None
        if method == "POST" and path == "/v1/jobs":
            self.posts.append(json.dumps(body, sort_keys=True))
            if self.post_answers:
                answer = self.post_answers.pop(0)
                if answer is None:
                    self.missed.append(path)
                    return None, None
                return answer
            sent = body["job"]
            for j in self.jobs.values():
                if j.get("clientRequestId") == sent["clientRequestId"]:
                    return 200, j
            made = {"id": f"approver-{len([j for j in self.jobs if j.startswith('approver-')]) + 1}", "kind": "review",
                    "pr": sent["pr"], "clientRequestId": sent["clientRequestId"],
                    "startCommit": (self.view or {}).get("headRefOid"), **self.approver}
            self.jobs[made["id"]] = made
            return 201, made
        return 500, {"error": "unexpected"}

    def sent(self):
        """The `job` of each `POST /v1/jobs`."""
        return [json.loads(body)["job"] for body in self.posts]

    def run(self, cmd, **kwargs):
        self.calls.append(("run", *cmd))
        out, code = "", 0
        if cmd[:3] == ["gh", "pr", "view"]:
            out = json.dumps(self.view) if self.view else ""
        elif cmd[:3] == ["gh", "api", "graphql"]:
            out = json.dumps(self.graphql) if self.graphql else ""
        elif cmd[:3] == ["gh", "pr", "merge"]:
            code = self.merge_code
        elif cmd[:3] == ["git", "cat-file", "-e"]:  # the approver step's question: is the task file on origin/<base>?
            code = 1 if Path(cmd[3]).stem in self.off_base else 0
        elif cmd[:2] == ["git", "show"]:  # the Kani check's read of the task on origin/<base>: none there
            return subprocess.CompletedProcess(cmd, 128, stdout="", stderr="fatal: path does not exist")
        elif cmd[:2] != ["git", "fetch"]:
            raise AssertionError(f"unexpected command: {cmd}")
        return subprocess.CompletedProcess(cmd, code, stdout=out, stderr=self.merge_err if code else "")

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
        self.real_api = self.feed.api
        self.feed.api = self.stand.api
        self.stand.missed = self.feed.OBID_MISSED
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
        self.one_pass()  # its approver first, then the merge on its APPROVE
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
        self.one_pass()  # its approver first, then the merge on its APPROVE
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
        self.assertEqual(self.feed.may_arm(j, pr, None, "A00-99")[0], "wait")
        self.assertEqual(self.feed.may_arm({"approvals": approvals(OLD, "claude")}, pr, None, "A00-99"), ("pending", None))
        self.assertEqual(self.feed.may_arm(j, pr, OLD, "A00-99")[0], "wait")

    def main(self, *argv, passes=None):
        """Runs main() with `argv`; each sleep is logged and `passes(n)` is called before the n-th one, from 1,
        until it raises Stop."""
        sleeps = []

        def sleep(seconds):
            sleeps.append(seconds)
            if passes:
                passes(len(sleeps))

        with mock.patch.object(self.feed.sys, "argv", ["oby-feed", *argv]), mock.patch.object(self.feed.time, "sleep", sleep):
            try:
                self.feed.main()
            except Stop:
                pass
        return sleeps

    # FEEDALIVE-1: obid down is a skipped pass, never a crash
    def test_feedalive_1_obid_down_is_a_skipped_pass_with_one_line(self):
        feed = load_feed()
        feed.HOME = self.dir
        feed.FEED = self.dir / "feed"
        feed.subprocess = Namespace(run=self.stand.run)
        (self.dir / "home").mkdir()
        (self.dir / "home" / "api-token").write_text("token\n")
        feed.URL = "http://127.0.0.1:1"  # nothing listens on port 1
        self.assertEqual(feed.api("GET", "/v1/jobs", timeout=5), (None, None))
        said = []
        feed.say = said.append
        feed.ready_tasks = lambda state: self.fail("a pass with obid down went on")
        self.assertIsNone(feed.one_pass(Namespace(max=3, min_free_gb=0, dry_run=False, account="acct")))
        self.assertEqual(said, ["obid didn't answer: this pass is skipped, the next one tries again"])

    def obid(self, answer):
        """The real api() against a stand-in urlopen: `answer(method, url)`, or `answer(method, url, data)` when it
        takes the request's body too, returns the body's bytes, or raises."""
        with_data = answer.__code__.co_argcount == 3

        class Res:
            status = 200

            def __init__(self, req):
                self.req = req

            def read(self):
                args = (self.req.get_method(), self.req.full_url) + ((self.req.data,) if with_data else ())
                return answer(*args)

            def __enter__(self):
                return self

            def __exit__(self, *_):
                return False

        (self.dir / "home").mkdir(exist_ok=True)
        (self.dir / "home" / "api-token").write_text("token\n")
        self.feed.api = self.real_api
        return mock.patch.object(self.feed.urllib.request, "urlopen", lambda req, timeout: Res(req))

    def test_feedalive_1_obid_gone_mid_answer_is_no_answer(self):
        def answer(method, url):
            raise http.client.IncompleteRead(b"[{", 40)

        with self.obid(answer):
            self.assertEqual(self.feed.api("GET", "/v1/jobs"), (None, None))

    def test_feedalive_1_obid_gone_mid_pass_skips_the_pass(self):
        said = []
        self.feed.say = said.append
        self.task("running")

        def answer(method, url):
            if url.endswith("/v1/jobs?limit=200"):
                return b"[]"
            raise self.feed.urllib.error.URLError("Connection refused")

        ready = {"key": "A00-98", "title": "another task", "body": ""}
        self.feed.ready_tasks = lambda state: [(ready, True, {"y.py"})]
        self.feed.free_gb = lambda: 100.0
        with self.obid(answer):
            self.assertIsNone(self.one_pass())
        self.assertEqual(said[-1], "obid stopped answering mid-pass (/v1/jobs/job-1): the next pass tries again")
        self.assertEqual(self.task_now()["state"], "running")
        self.assertNotIn("A00-98", self.feed.load_state())
        self.assertFalse([line for line in said if "not started" in line], said)

    def test_feedalive_1_a_merge_seen_while_obid_goes_away_is_recorded_later(self):
        self.task("waiting", head=OLD)
        self.stand.view = {"headRefOid": OLD, "state": "MERGED", "files": []}
        self.no_ready()

        def answer(method, url):
            if url.endswith("/v1/jobs?limit=200"):
                return json.dumps([{"id": JOB}]).encode()
            if method == "GET" and url.endswith(f"/v1/jobs/{JOB}"):
                return json.dumps({"id": JOB, "status": "done"}).encode()
            raise self.feed.urllib.error.URLError("Connection refused")

        with self.obid(answer):
            self.assertIsNone(self.one_pass())
        self.assertEqual(self.task_now()["state"], "waiting")

    def test_feedalive_1_a_merge_whose_cleanup_obid_cut_short_is_finished_never_failed(self):
        """GitHub merged it; obid deletes the build job, then drops the review's delete. The next pass, with the build
        job gone (404), finishes deleting the review and records the merge: never `failed` for the missing job."""
        self.task("waiting", head=OLD)
        self.stand.view = {"headRefOid": OLD, "state": "MERGED", "files": []}
        self.no_ready()
        review = {"id": "rev-1", "kind": "review", "reviewOf": JOB}
        alive, calls, cut = {JOB, "rev-1"}, [], [True]

        def answer(method, url):
            path = url.split("127.0.0.1:7811", 1)[-1] if "7811" in url else url
            calls.append((method, path))
            if path.endswith("/v1/jobs?limit=200"):
                return json.dumps([j for j in ({"id": JOB, "kind": "build"}, review) if j["id"] in alive]).encode()
            jid = path.rsplit("/", 1)[1]
            if jid not in alive:
                raise self.feed.urllib.error.HTTPError(url, 404, "Not Found", {}, io.BytesIO(b'{"error": "no such job"}'))
            if method == "GET":
                return json.dumps({"id": jid, "status": "done"}).encode()
            if jid == "rev-1" and cut[0]:
                raise self.feed.urllib.error.URLError("Connection refused")
            alive.discard(jid)
            return b""

        with self.obid(answer):
            self.assertIsNone(self.one_pass())
            self.assertEqual(alive, {"rev-1"})
            self.assertNotEqual(self.task_now()["state"], "failed")
            cut[0] = False
            calls.clear()
            self.one_pass()
        item = self.task_now()
        self.assertEqual(item["state"], "merged")
        self.assertIn("2 Oby jobs", item["note"])
        self.assertEqual(alive, set())
        self.assertNotIn(("GET", f"/v1/jobs/{JOB}"), calls)

    def merged_family(self, alive, refuse=()):
        """A stand-in obid holding the jobs in `alive`: a DELETE of one it has removes it, of one it hasn't is a 404,
        of one in `refuse` is a 409. Returns the DELETE calls it saw."""
        deletes = []

        def api(method, path, body=None, timeout=30):
            jid = path.rsplit("/", 1)[1]
            if method == "GET":
                return (200, {"id": jid, "status": "done"}) if jid in alive else (404, {"error": "no such job"})
            if method == "DELETE":
                deletes.append(jid)
                if jid in refuse:
                    return 409, {"error": "it is still running"}
                if jid not in alive:
                    return 404, {"error": "no such job"}
                alive.discard(jid)
                return 204, None
            return 500, {"error": "unexpected"}

        self.feed.api = api
        return deletes

    def test_feedalive_1_a_merge_and_its_job_list_are_saved_before_any_job_is_deleted(self):
        """The feeder stops right after obid deleted the build job: the next pass, with the build job gone, still
        knows the merge and every job of the task, deletes the rest and records it merged, never failed."""
        self.no_ready()
        self.task("waiting", head=OLD)
        self.stand.view = {"headRefOid": OLD, "state": "MERGED", "files": []}
        alive = {JOB, "rev-1"}
        listed = lambda: [{"id": JOB, "kind": "build"}, {"id": "rev-1", "reviewOf": JOB}]
        self.feed.oby_jobs = lambda: [j for j in listed() if j["id"] in alive]
        deletes = self.merged_family(alive)
        api, on_disk = self.feed.api, []

        def stops_after_the_first_delete(method, path, body=None, timeout=30):
            if method == "DELETE":
                on_disk.append(self.task_now())
                if len(deletes) == 1:
                    raise Stop()
            return api(method, path, body, timeout)

        self.feed.api = stops_after_the_first_delete
        with self.assertRaises(Stop):
            self.one_pass()
        self.assertTrue(on_disk[0].get("mergeSeen"))
        self.assertEqual(on_disk[0]["cleanup"], sorted([JOB, "rev-1"]))
        self.assertEqual(alive, {"rev-1"})
        self.feed.api = api
        self.one_pass()
        item = self.task_now()
        self.assertEqual(item["state"], "merged")
        self.assertIn("2 Oby jobs", item["note"])
        self.assertEqual(alive, set())

    def test_feedalive_1_a_404_on_a_job_already_deleted_is_success(self):
        self.no_ready()
        self.task("waiting", head=OLD, mergeSeen=True, cleanup=[JOB, "rev-1"], left=[JOB, "rev-1"])
        self.feed.oby_jobs = lambda: [{"id": "rev-1", "reviewOf": JOB}]
        deletes = self.merged_family({"rev-1"})
        self.assertIsNotNone(self.one_pass())
        self.assertEqual(deletes, [JOB, "rev-1"])
        item = self.task_now()
        self.assertEqual(item["state"], "merged")
        self.assertIn("2 Oby jobs", item["note"])

    def test_feedalive_1_a_merged_task_deletes_from_its_saved_list_until_every_job_is_gone(self):
        """obid keeps a review it won't delete yet: the task stays merging with that job left, and later passes
        delete only what is left until it is gone, never recording it merged with a job behind."""
        self.no_ready()
        self.task("merging", head=OLD)
        self.stand.graphql = queue("MERGED")
        alive = {JOB, "rev-1"}
        self.feed.oby_jobs = lambda: [j for j in ({"id": JOB}, {"id": "rev-1", "reviewOf": JOB}) if j["id"] in alive]
        refuse = {"rev-1"}
        deletes = self.merged_family(alive, refuse)
        self.one_pass()
        item = self.task_now()
        self.assertEqual(item["state"], "merging")
        self.assertEqual(item["left"], ["rev-1"])
        self.assertEqual(alive, {"rev-1"})
        self.one_pass()
        self.assertEqual(self.task_now()["left"], ["rev-1"])
        refuse.clear()
        deletes.clear()
        self.one_pass()
        self.assertEqual(deletes, ["rev-1"])
        item = self.task_now()
        self.assertEqual(item["state"], "merged")
        self.assertIn("2 Oby jobs", item["note"])
        self.assertEqual(alive, set())

    def test_feedalive_1_a_merged_task_after_a_follow_up_deletes_its_first_build_job_too(self):
        self.no_ready()
        self.task("waiting", head=OLD, job="job-2")
        self.stand.view = {"headRefOid": OLD, "state": "MERGED", "files": []}
        alive = {JOB, "rev-1", "job-2", "rev-2"}
        self.feed.oby_jobs = lambda: [{"id": JOB, "kind": "build"}, {"id": "rev-1", "reviewOf": JOB},
                                      {"id": "job-2", "kind": "build", "followUpOf": JOB}, {"id": "rev-2", "reviewOf": "job-2"},
                                      {"id": "other", "kind": "build"}]
        deletes = self.merged_family(alive)
        self.one_pass()
        self.assertEqual(alive, set())
        self.assertNotIn("other", deletes)
        self.assertEqual(self.task_now()["state"], "merged")

    def two_waiting_and_one_ready(self):
        """Two waiting tasks and a ready one: what a pass looks at once a request went unanswered."""
        self.task("waiting", head=OLD)
        self.feed.save({"key": "A00-97", "title": "a third task", "job": JOB, "state": "waiting", "pr": PR, "head": OLD})
        ready = {"key": "A00-98", "title": "another task", "body": ""}
        self.feed.ready_tasks = lambda state: [(ready, True, {"y.py"})]
        self.feed.free_gb = lambda: 100.0
        started = []
        self.feed.start = lambda task, account, on_base: (started.append(task["key"]) or "job-9", None)
        return started

    def ask_approvers_first(self):
        """One pass that starts no new task, in which each head approved twice gets its approver, answering APPROVE:
        the next pass is the one that merges."""
        ready = self.feed.ready_tasks
        self.feed.ready_tasks = lambda state: []
        self.assertIsNotNone(self.one_pass())
        self.feed.ready_tasks = ready

    def test_feedalive_1_github_gone_mid_pass_follows_and_starts_nothing_more(self):
        started = self.two_waiting_and_one_ready()

        def run(cmd, **kwargs):
            if cmd[:3] == ["gh", "pr", "view"]:
                self.stand.calls.append(("run", *cmd))
                return subprocess.CompletedProcess(cmd, 1, "", "HTTP 502: Bad Gateway")
            return self.stand.run(cmd, **kwargs)

        self.feed.subprocess = Namespace(run=run)
        self.assertIsNone(self.one_pass())
        self.assertEqual(len([c for c in self.stand.calls if c[:4] == ("run", "gh", "pr", "view")]), 1)
        self.assertEqual(started, [])

    def test_feedalive_1_a_merge_github_didnt_answer_follows_and_starts_nothing_more(self):
        started = self.two_waiting_and_one_ready()
        self.stand.jobs[JOB]["approvals"] = approvals(OLD, "claude", "codex")
        self.stand.view = {"headRefOid": OLD, "state": "OPEN", "files": [{"path": "x.py"}]}
        self.stand.merge_code = 1
        self.stand.merge_err = "HTTP 502: Bad Gateway (https://api.github.com/graphql)"
        self.ask_approvers_first()
        self.assertIsNone(self.one_pass())
        self.assertEqual(len(self.stand.merges()), 1)
        self.assertEqual(started, [])

    def test_feedalive_1_a_merge_that_cant_reach_github_is_tried_again_never_refused(self):
        """`gh pr merge` failing on DNS is GitHub not answering: the pass stops, the head waits unrefused."""
        started = self.two_waiting_and_one_ready()
        self.stand.jobs[JOB]["approvals"] = approvals(OLD, "claude", "codex")
        self.stand.view = {"headRefOid": OLD, "state": "OPEN", "files": [{"path": "x.py"}]}
        self.stand.merge_code = 1
        self.stand.merge_err = 'Post "https://api.github.com/graphql": dial tcp: lookup api.github.com: no such host'
        self.ask_approvers_first()
        self.assertIsNone(self.one_pass())
        self.assertEqual(len(self.stand.merges()), 1)
        self.assertEqual(started, [])
        self.assertFalse(any("refused" in item for item in self.feed.load_state().values()))
        self.assertTrue(self.feed.GH_MISSED)

    def test_feedalive_1_a_model_github_didnt_give_is_never_read_as_removed(self):
        """`gh api` fetching a changed model fails without a 404: the pass stops at once, nothing merges unverified,
        nothing starts, the head waits unrefused; a 404 is still a removed model."""
        started = self.two_waiting_and_one_ready()
        self.stand.jobs[JOB]["approvals"] = approvals(OLD, "claude", "codex")
        self.stand.view = {"headRefOid": OLD, "state": "OPEN", "files": [{"path": PATH}]}
        self.feed.touches_security = lambda files: []
        fetches, said = [], ["HTTP 502: Bad Gateway (https://api.github.com/repos/owner/repo/contents/x)"]

        def run(cmd, **kwargs):
            if cmd[:2] == ["gh", "api"] and "contents" in cmd[2]:
                fetches.append(cmd)
                return subprocess.CompletedProcess(cmd, 1, "", said[0])
            if cmd[0] == "ssh":
                raise AssertionError("a model GitHub didn't give reached the gate VM")
            return self.stand.run(cmd, **kwargs)

        self.feed.subprocess = Namespace(run=run)
        config = dict(self.feed.job.CONFIG, remote_gate={"ssh_host": "oby-gate"})
        with mock.patch.object(self.feed.job, "CONFIG", config):
            self.assertIsNone(self.one_pass())
            self.assertEqual(len(fetches), 1)
            self.assertEqual(self.stand.merges(), [])
            self.assertEqual(started, [])
            items = self.feed.load_state().values()
            self.assertFalse(any("refused" in item for item in items))
            self.assertEqual(len([item for item in items if "502" in item.get("note", "")]), 1)
            self.assertEqual(self.feed.GATE_MISSED, [])
            said[0] = "gh: Not Found (HTTP 404)"
            self.assertIsNone(self.feed.check_models([PATH], OLD))

    def test_feedalive_1_obid_gone_mid_pass_follows_and_starts_nothing_more(self):
        started = self.two_waiting_and_one_ready()

        def api(method, path, body=None, timeout=30):
            self.stand.calls.append(("api", method, path))
            self.feed.OBID_MISSED.append(path)
            return None, None

        self.feed.api = api
        self.feed.oby_jobs = lambda: []
        self.assertIsNone(self.one_pass())
        self.assertEqual(len([c for c in self.stand.calls if c[:2] == ("api", "GET")]), 1)
        self.assertEqual(started, [])

    def test_feedalive_1_a_start_whose_answer_was_lost_is_retried_with_the_same_request_key(self):
        """obid makes the job but its answer is cut off: the next pass sends the same clientRequestId, so obid answers
        that job back (200) instead of making a second one, and the key is dropped once answered."""
        ready = {"key": "A00-98", "title": "another task", "body": ""}
        self.feed.ready_tasks = lambda state: [] if "A00-98" in state else [(ready, True, {"y.py"})]
        self.feed.free_gb = lambda: 100.0
        keys = []

        def answer(method, url, data):
            if url.endswith("/v1/jobs?limit=200"):
                return b"[]"
            keys.append(json.loads(data)["job"]["clientRequestId"])
            if len(keys) == 1:
                raise http.client.IncompleteRead(b'{"id": "jo', 20)
            return json.dumps({"id": "job-9"}).encode()

        clock = iter(range(1000, 2000, 60))
        with self.obid(answer), mock.patch.object(self.feed.time, "time", lambda: next(clock)):
            self.assertIsNone(self.one_pass())
            self.assertEqual(self.one_pass(), (1, 1))
        self.assertEqual(len(keys), 2)
        self.assertEqual(keys[0], keys[1])
        self.assertEqual(self.feed.load_state()["A00-98"]["job"], "job-9")
        self.assertFalse((self.feed.FEED / "A00-98.start").exists())

    def test_feedalive_1_a_lost_start_is_retried_with_its_saved_body_even_after_the_task_changed(self):
        """obid makes the job but its answer is lost; dev then retitles the task and the account changes. The next
        pass sends the very body it saved, so obid's key check (a changed body under one key is a 409) gives the job
        back instead of refusing it and letting a later pass make a second build."""
        task = {"key": "A00-98", "title": "another task", "body": ""}
        self.feed.ready_tasks = lambda state: [] if "A00-98" in state else [(dict(task), True, {"y.py"})]
        self.feed.free_gb = lambda: 100.0
        self.feed.oby_jobs = lambda: []
        bodies = []

        def api(method, path, body=None, timeout=30):
            bodies.append(body["job"])
            if len(bodies) == 1:
                self.feed.OBID_MISSED.append(path)
                return None, None
            return (201, {"id": "job-9"}) if body["job"] == bodies[0] else (409, {"error": "that key carried another job"})

        self.feed.api = api
        clock = iter(range(1000, 2000, 60))
        with mock.patch.object(self.feed.time, "time", lambda: next(clock)):
            self.assertIsNone(self.one_pass())
            task["title"] = "another task, retitled on dev"
            self.assertEqual(self.feed.one_pass(Namespace(max=3, min_free_gb=0, dry_run=False, account="other")), (1, 1))
        self.assertEqual(bodies[0], bodies[1])
        self.assertEqual(self.feed.load_state()["A00-98"]["job"], "job-9")
        self.assertFalse((self.feed.FEED / "A00-98.start").exists())

    def test_feedalive_1_a_lost_start_is_recorded_even_once_the_disk_is_full_or_the_task_left_the_sprint(self):
        """obid makes the job but its answer is lost; before the next pass its build fills the disk under the minimum,
        dev moves the task to a later sprint (it is no longer ready) or no slot is free. The next pass finds obid's job
        by the saved key in its job list, sends nothing, records the job with its files and counts it under way."""
        for change in ("disk full", "left the sprint", "no slot", "all three"):
            with self.subTest(change=change):
                self.feed.FEED = self.dir / f"feed-{change}"
                task = {"key": "A00-98", "title": "another task", "body": "### Files\n- factory/bin/y.py\n"}
                ready = [(task, True, {"factory/bin/y.py"})]
                self.feed.ready_tasks = lambda state: [] if "A00-98" in state else list(ready)
                disk = [100.0]
                self.feed.free_gb = lambda: disk[0]
                listed = []
                self.feed.oby_jobs = lambda: list(listed)
                bodies = []

                def api(method, path, body=None, timeout=30):
                    bodies.append(body["job"])
                    if len(bodies) == 1:
                        self.feed.OBID_MISSED.append(path)
                        return None, None
                    return 200, {"id": "job-9"}

                self.feed.api = api
                args = Namespace(max=3, min_free_gb=15, dry_run=False, account="acct")
                self.assertIsNone(self.feed.one_pass(args))
                listed.append({"id": "job-9", "kind": "build", "status": "running",
                               "clientRequestId": bodies[0]["clientRequestId"]})
                if change in ("disk full", "all three"):
                    disk[0] = 5.0
                if change in ("left the sprint", "all three"):
                    ready[:] = []
                if change in ("no slot", "all three"):
                    args.max = 0
                self.assertEqual(self.feed.one_pass(args), (1, 1))
                self.assertEqual(len(bodies), 1)
                item = self.feed.load_state()["A00-98"]
                self.assertEqual((item["job"], item["state"], item["files"]), ("job-9", "running", ["factory/bin/y.py"]))
                self.assertFalse((self.feed.FEED / "A00-98.start").exists())

    def saved_start(self, key, body=True):
        """A start an earlier pass saved and obid never answered: the whole body, or (an older feeder's) its key alone."""
        request_id = f"feed-{key}-1000"
        pending = self.feed.FEED / f"{key}.start"
        pending.parent.mkdir(parents=True, exist_ok=True)
        sent = {"repo": REPO, "kind": "build", "account": "acct", "title": f"{key} saved", "task": "saved",
                "clientRequestId": request_id}
        pending.write_text(json.dumps({"job": sent, "title": "saved", "files": [f"{key}.py"]}) if body else request_id)
        return request_id, sent

    def test_feedalive_1_a_key_saved_without_its_body_is_never_sent_under_a_body_built_anew(self):
        """An older feeder saved only the key. Its body is gone, so it is never sent again: obid's job under that key
        is the task's job; with none, obid never used the key and the task starts anew under a new one."""
        for made in (True, False):
            with self.subTest(made=made):
                self.feed.FEED = self.dir / f"feed-{made}"
                request_id, _ = self.saved_start("A00-98", body=False)
                task = {"key": "A00-98", "title": "another task", "body": ""}
                self.feed.ready_tasks = lambda state: [] if "A00-98" in state else [(task, True, {"y.py"})]
                self.feed.free_gb = lambda: 100.0
                self.feed.oby_jobs = lambda: [{"id": "job-7", "clientRequestId": request_id}] if made else []
                keys = []

                def api(method, path, body=None, timeout=30):
                    keys.append(body["job"]["clientRequestId"])
                    return 201, {"id": "job-9"}

                self.feed.api = api
                self.assertEqual(self.one_pass(), (1, 1))
                self.assertNotIn(request_id, keys)
                self.assertEqual(len(keys), 0 if made else 1)
                self.assertEqual(self.feed.load_state()["A00-98"]["job"], "job-7" if made else "job-9")
                self.assertFalse((self.feed.FEED / "A00-98.start").exists())

    def test_feedalive_1_a_start_left_unanswered_this_pass_holds_its_slot_and_files_at_once(self):
        """obid may have made a job it answered with a 503. With --max 1 and two ready tasks, the first start's 503
        holds the one slot, so the second isn't sent; and a saved start still waiting holds its files, so a ready task
        on the same file isn't sent while one on another file is."""
        for case in ("slot", "files"):
            with self.subTest(case=case):
                self.feed.FEED = self.dir / f"feed-{case}"
                if case == "files":
                    self.saved_start("A00-96")  # its files: A00-96.py
                tasks = [({"key": "A00-97", "title": "first", "body": ""}, True, {"A00-96.py" if case == "files" else "a.py"}),
                         ({"key": "A00-98", "title": "second", "body": ""}, True, {"b.py"})]
                self.feed.ready_tasks = lambda state: [t for t in tasks if t[0]["key"] not in state]
                self.feed.free_gb = lambda: 100.0
                self.feed.oby_jobs = lambda: []
                sent = []

                def api(method, path, body=None, timeout=30):
                    sent.append(body["job"]["clientRequestId"].rsplit("-", 1)[0])
                    if case == "slot" or sent[-1] == "feed-A00-96":
                        return 503, {"error": "restarting"}
                    return 201, {"id": f"job-{len(sent)}"}

                self.feed.api = api
                self.feed.one_pass(Namespace(max=1 if case == "slot" else 3, min_free_gb=0, dry_run=False,
                                             account="acct"))
                if case == "slot":
                    self.assertEqual(sent, ["feed-A00-97"])
                    self.assertTrue((self.feed.FEED / "A00-97.start").exists())
                else:
                    self.assertEqual(sent, ["feed-A00-96", "feed-A00-98"])
                self.assertNotIn("A00-97", self.feed.load_state())

    def test_feedalive_1_saved_starts_hold_slots_and_are_sent_again_only_into_free_ones(self):
        """Saved starts obid never made count against --max and the disk check: with one task under way and --max 2,
        only the first of two saved starts is sent again, exactly as saved; with no slot or the disk under the minimum,
        none is; and two obid answers with a 503 still hold both slots of --max 2, so no new task takes one."""
        for case in ("one slot", "no slot", "disk full", "unanswered"):
            with self.subTest(case=case):
                self.feed.FEED = self.dir / f"feed-{case}"
                if case != "unanswered":
                    self.task("running")
                    self.feed.follow = lambda item, jobs, account: False
                _, first = self.saved_start("A00-96")
                self.saved_start("A00-97")
                new = {"key": "A00-98", "title": "a new task", "body": ""}
                self.feed.ready_tasks = lambda state: [] if "A00-98" in state else [(new, True, {"y.py"})]
                self.feed.free_gb = lambda: 5.0 if case == "disk full" else 100.0
                self.feed.oby_jobs = lambda: []
                sent = []

                def api(method, path, body=None, timeout=30):
                    sent.append(body["job"])
                    return (503, {"error": "restarting"}) if case == "unanswered" else (201, {"id": f"job-{len(sent)}"})

                self.feed.api = api
                args = Namespace(max={"no slot": 1, "unanswered": 2}.get(case, 2), min_free_gb=15, dry_run=False,
                                 account="acct")
                self.feed.one_pass(args)
                state = self.feed.load_state()
                if case == "one slot":
                    self.assertEqual(sent, [first])
                    self.assertEqual(state["A00-96"]["job"], "job-1")
                    self.assertTrue((self.feed.FEED / "A00-97.start").exists())
                elif case == "unanswered":
                    self.assertEqual([s["clientRequestId"] for s in sent], ["feed-A00-96-1000", "feed-A00-97-1000"])
                else:
                    self.assertEqual(sent, [])
                    self.assertTrue((self.feed.FEED / "A00-96.start").exists())
                self.assertNotIn("A00-98", state)

    def test_feedalive_1_one_request_key_per_task_attempt_through_a_5xx_until_the_job_is_saved(self):
        """obid answers a start with a 503 while it restarts, then with the job: both posts carry the one key, which
        is still on disk when the job is saved and dropped only after; a 4xx refusal ends the attempt."""
        ready = {"key": "A00-98", "title": "another task", "body": ""}
        self.feed.ready_tasks = lambda state: [] if "A00-98" in state else [(ready, True, {"y.py"})]
        self.feed.free_gb = lambda: 100.0
        self.feed.oby_jobs = lambda: []
        pending = self.feed.FEED / "A00-98.start"
        keys, answers = [], [(503, {"error": "obid is restarting"}), (201, {"id": "job-9"}), (None, None)]

        def api(method, path, body=None, timeout=30):
            keys.append(body["job"]["clientRequestId"])
            return answers[len(keys) - 1]

        saved, real_save = [], self.feed.save

        def save(item):
            saved.append(pending.exists())
            real_save(item)

        self.feed.api, self.feed.save = api, save
        clock = iter(range(1000, 2000, 60))
        with mock.patch.object(self.feed.time, "time", lambda: next(clock)):
            self.assertEqual(self.one_pass(), (0, 0))
            self.assertTrue(pending.exists())
            self.assertEqual(self.one_pass(), (1, 1))
            self.assertEqual(len(keys), 2)
            self.assertEqual(keys[0], keys[1])
        self.assertEqual(saved, [True])
        self.assertFalse(pending.exists())
        self.assertEqual(self.feed.start({"key": "A00-96", "title": "t", "body": ""}, "acct", True)[0], None)
        self.assertTrue((self.feed.FEED / "A00-96.start").exists())  # no answer: kept
        self.feed.api = lambda method, path, body=None, timeout=30: (409, {"error": "that key carried another task"})
        self.assertEqual(self.feed.start({"key": "A00-96", "title": "t", "body": ""}, "acct", True)[0], None)
        self.assertFalse((self.feed.FEED / "A00-96.start").exists())

    def test_feedalive_1_an_interrupted_answer_never_ends_the_feeder(self):
        """A body cut off with no error, IncompleteRead and a reset mid-read: three skipped passes, never a crash."""
        cuts = iter([b'[{"id": "jo', http.client.IncompleteRead(b"[", 40), ConnectionResetError("reset by peer")])

        def answer(method, url):
            cut = next(cuts)
            if isinstance(cut, Exception):
                raise cut
            return cut

        def passes(n):
            if n == 3:
                raise Stop

        with self.obid(answer):
            self.assertEqual(self.main(passes=passes), [120, 240, 480])

    def test_feedalive_1_backoff_doubles_to_ten_minutes_and_resets(self):
        got = iter([None] * 5 + [(1, 0)])

        def pass_(args):
            nxt = next(got, Stop)
            if nxt is Stop:
                raise Stop
            return nxt

        self.feed.one_pass = pass_
        self.assertEqual(self.main(), [120, 240, 480, 600, 600, 60])

    # FEEDALIVE-2: an idle pass doesn't end the loop; --until-idle does
    def test_feedalive_2_the_loop_outlives_idle_passes_and_until_idle_stops_it(self):
        self.no_ready()
        self.feed.oby_jobs = lambda: []

        def passes(n):
            if n == 4:
                raise Stop

        self.assertEqual(self.main(passes=passes), [300, 300, 300, 300])
        self.assertEqual(self.main("--until-idle", passes=lambda n: self.fail("--until-idle went on")), [])

    def on_real_git(self, first_task=None):
        """A real bare origin, a seed clone that pushes to it and the feeder's own clone, made after the first push,
        which holds `first_task` as factory/queue/tasks/A00-98.md when given. The feeder's `git fetch`,
        `git ls-tree` and `git show` run for real. Returns `push(text)`, which commits A00-98.md on the seed and
        pushes it, the feeder's queue folder, and the list of keys it starts."""
        base = self.feed.job.BASE
        # GIT_DIR and the like, set when the tests run inside a worktree's hooks, would point git at that repository
        no_git_env = mock.patch.dict(os.environ, {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}, clear=True)
        no_git_env.start()
        self.addCleanup(no_git_env.stop)

        def git(cwd, *args):
            subprocess.run(["git", "-c", "user.name=t", "-c", "user.email=t@t", "-c", "commit.gpgsign=false", *args],
                           cwd=cwd, check=True, capture_output=True)

        origin, seed, repo = self.dir / "origin.git", self.dir / "seed", self.dir / "repo"
        git(self.dir, "init", "-q", "--bare", "-b", base, str(origin))
        git(self.dir, "init", "-q", "-b", base, str(seed))

        def push(text):
            (seed / "factory" / "queue" / "tasks").mkdir(parents=True, exist_ok=True)
            (seed / "factory" / "queue" / "tasks" / "A00-98.md").write_text(text)
            git(seed, "add", "factory")
            git(seed, "commit", "-q", "-m", "a task")
            git(seed, "push", "-q", str(origin), base)

        (seed / "README").write_text("x\n")
        git(seed, "add", "README")
        if first_task:
            push(first_task)
        else:
            git(seed, "commit", "-q", "-m", "first")
            git(seed, "push", "-q", str(origin), base)
        git(self.dir, "clone", "-q", "-b", base, str(origin), str(repo))
        self.seed_git = lambda *args: git(seed, *args)

        self.feed.oby_jobs = lambda: []
        self.feed.free_gb = lambda: 100.0
        self.feed.subprocess = subprocess
        self.feed.REPO = repo
        started = []
        self.feed.start = lambda task, account, on_base: (started.append(task["key"]) or "job-9", None)
        job = self.feed.job
        queue_dir = repo / "factory" / "queue"
        for patch in (mock.patch.object(job, "REPO", repo), mock.patch.object(job, "TASKS", queue_dir / "tasks"),
                      mock.patch.object(job, "DONE", queue_dir / "done"), mock.patch.object(job, "JOBS", repo / "factory" / "jobs"),
                      mock.patch.object(job.run, "__defaults__", (repo, True)), mock.patch.object(job, "SPRINT", 0)):
            patch.start()
            self.addCleanup(patch.stop)
        return push, queue_dir, started

    # FEEDALIVE-3: a task file ready after idle passes starts at the next pass
    def test_feedalive_3_a_task_ready_after_idle_passes_starts_at_the_next_pass(self):
        """The task file is pushed to a real origin's <base> after the feeder's clone: the checkout is never pulled."""
        push, queue_dir, started = self.on_real_git()
        text = "KEY: A00-98\nTITLE: another task\nSPRINT: 0\nORDER: 1\nDEPENDS: none\n\nWhat.\n\n### Files\n- y.py\n"

        def passes(n):
            if n == 3:
                push(text)
            if n == 4:
                raise Stop

        sleeps = self.main(passes=passes)
        self.assertEqual(started, ["A00-98"])
        self.assertFalse((queue_dir / "tasks" / "A00-98.md").exists())
        self.assertEqual(sleeps, [300, 300, 300, 60])
        self.assertEqual(self.feed.load_state()["A00-98"]["state"], "running")
        self.assertEqual(self.feed.load_state()["A00-98"]["files"], ["y.py"])

    def test_feedalive_3_a_task_dev_changed_is_read_as_dev_has_it(self):
        """The checkout holds the task in a later sprint; dev moves it to this one: it starts with dev's files."""
        later = "KEY: A00-98\nTITLE: another task\nSPRINT: 5\nORDER: 1\nDEPENDS: none\n\nWhat.\n\n### Files\n- x.py\n"
        push, queue_dir, started = self.on_real_git(first_task=later)

        def passes(n):
            if n == 2:
                push(later.replace("SPRINT: 5", "SPRINT: 0").replace("- x.py", "- y.py"))
            if n == 3:
                raise Stop

        self.assertEqual(self.main(passes=passes), [300, 300, 60])
        self.assertEqual(started, ["A00-98"])
        self.assertIn("SPRINT: 5", (queue_dir / "tasks" / "A00-98.md").read_text())
        self.assertEqual(self.feed.load_state()["A00-98"]["files"], ["y.py"])

    def test_feedalive_3_a_task_dev_has_never_falls_back_to_the_checkouts_stale_copy(self):
        """Dev's copy of a task the checkout holds ready turns unreadable: it waits, the stale copy never starts;
        once dev's text reads again, it starts with dev's files."""
        text = "KEY: A00-98\nTITLE: another task\nSPRINT: 0\nORDER: 1\nDEPENDS: none\n\nWhat.\n\n### Files\n- x.py\n"
        push, queue_dir, started = self.on_real_git(first_task=text)
        push(text.replace("KEY: A00-98", "KEY: A00-9X"))
        self.assertEqual(self.one_pass(), (0, 0))
        self.assertEqual(started, [])
        self.assertIn("KEY: A00-98", (queue_dir / "tasks" / "A00-98.md").read_text())
        push(text.replace("- x.py", "- y.py"))
        self.assertEqual(self.one_pass(), (1, 1))
        self.assertEqual(started, ["A00-98"])
        self.assertEqual(self.feed.load_state()["A00-98"]["files"], ["y.py"])

    def test_feedalive_3_a_task_dev_took_away_never_starts_from_the_checkout(self):
        """Dev deletes a task the checkout still holds ready: it never starts; a task file only the checkout ever had
        (written here, not yet committed) still does."""
        text = "KEY: A00-98\nTITLE: another task\nSPRINT: 0\nORDER: 1\nDEPENDS: none\n\nWhat.\n\n### Files\n- x.py\n"
        push, queue_dir, started = self.on_real_git(first_task=text)
        self.seed_git("rm", "-q", "factory/queue/tasks/A00-98.md")
        self.seed_git("commit", "-q", "-m", "drop A00-98")
        self.seed_git("push", "-q", str(self.dir / "origin.git"), self.feed.job.BASE)
        (queue_dir / "tasks" / "A00-97.md").write_text(text.replace("A00-98", "A00-97").replace("x.py", "z.py"))
        self.assertEqual(self.one_pass(), (1, 1))
        self.assertTrue((queue_dir / "tasks" / "A00-98.md").exists())
        self.assertEqual(started, ["A00-97"])

    # FEEDALIVE-4: a 502 from `gh pr merge` is tried again; GitHub refusing the merge holds the head
    def test_feedalive_4_a_502_is_tried_again_and_a_refusal_holds_the_head(self):
        self.no_ready()
        self.task("reviewing")
        self.stand.jobs[JOB]["approvals"] = approvals(OLD, "claude", "codex")
        self.stand.view = {"headRefOid": OLD, "state": "OPEN", "files": [{"path": "x.py"}]}
        self.stand.merge_code = 1
        self.stand.merge_err = "HTTP 502: Bad Gateway (https://api.github.com/graphql)"
        self.one_pass()  # its approver first, then the merge on its APPROVE
        self.one_pass()
        item = self.task_now()
        self.assertEqual(item["state"], "waiting")
        self.assertNotIn("refused", item)
        self.assertIn("502", item["note"])
        self.stand.merge_err = "GraphQL: Pull request is not mergeable (enablePullRequestAutoMerge)"
        self.one_pass()  # its approver's job, given back under the same key, then the merge
        self.one_pass()
        item = self.task_now()
        self.assertEqual(len(self.stand.merges()), 2)
        self.assertEqual(item["refused"], OLD)
        self.assertTrue(item["note"].startswith("GitHub refused the merge"))
        self.one_pass()
        self.assertEqual(len(self.stand.merges()), 2)

    def test_feedalive_4_a_passing_failure_is_tried_at_most_three_times_a_head(self):
        self.no_ready()
        self.task("waiting", head=OLD)
        self.stand.jobs[JOB]["approvals"] = approvals(OLD, "claude", "codex")
        self.stand.view = {"headRefOid": OLD, "state": "OPEN", "files": [{"path": "x.py"}]}
        self.stand.merge_code = 1
        self.stand.merge_err = "error connecting to api.github.com: connection reset"
        for _ in range(12):  # each try is a pass at the approver's job, given back under the same key, then the merge
            self.one_pass()
        self.assertEqual(len(self.stand.merges()), 4)
        self.assertEqual(self.task_now()["refused"], OLD)

    # FEEDALIVE-5: ssh failing to reach the gate VM leaves the head unrefused, tried again
    def test_feedalive_5_an_unreachable_gate_vm_leaves_the_head_unrefused(self):
        self.no_ready()
        self.task("reviewing")
        self.stand.jobs[JOB]["approvals"] = approvals(OLD, "claude", "codex")
        self.stand.view = {"headRefOid": OLD, "state": "OPEN", "files": [{"path": PATH}]}
        self.feed.touches_security = lambda files: []
        puts = []

        def run(cmd, **kwargs):
            if cmd[:2] == ["gh", "api"] and "contents" in cmd[2]:
                return subprocess.CompletedProcess(cmd, 0, "// verify: main=m invariants=safe\n", "")
            if cmd[0] == "ssh":
                puts.append(cmd)
                return subprocess.CompletedProcess(cmd, 255, "", "ssh: connect to host oby-gate port 22: Connection refused")
            return self.stand.run(cmd, **kwargs)

        self.feed.subprocess = Namespace(run=run)
        config = dict(self.feed.job.CONFIG, remote_gate={"ssh_host": "oby-gate"})
        with mock.patch.object(self.feed.job, "CONFIG", config):
            self.one_pass()
            item = self.task_now()
            self.assertEqual(item["state"], "waiting")
            self.assertNotIn("refused", item)
            self.assertIn("Connection refused", item["note"])
            self.one_pass()
        self.assertEqual(len(puts), 2)
        self.assertNotIn("refused", self.task_now())
        self.assertEqual(self.stand.merges(), [])

    def test_feedalive_5_a_dropped_gate_vm_is_a_skipped_pass_and_never_holds_the_head(self):
        """ssh drops at every pass: each pass is skipped, starts nothing, and the head is never held for it."""
        self.task("waiting", head=OLD)
        self.stand.jobs[JOB]["approvals"] = approvals(OLD, "claude", "codex")
        self.stand.view = {"headRefOid": OLD, "state": "OPEN", "files": [{"path": PATH}]}
        self.feed.touches_security = lambda files: []
        ready = {"key": "A00-98", "title": "another task", "body": ""}
        self.feed.ready_tasks = lambda state: [(ready, True, {"y.py"})]
        self.feed.free_gb = lambda: 100.0
        started = []
        self.feed.start = lambda task, account, on_base: (started.append(task["key"]) or "job-9", None)

        def run(cmd, **kwargs):
            if cmd[:2] == ["gh", "api"] and "contents" in cmd[2]:
                return subprocess.CompletedProcess(cmd, 0, "// verify: main=m invariants=safe\n", "")
            if cmd[0] == "ssh":
                return subprocess.CompletedProcess(cmd, 255, "", "Connection to oby-gate closed by remote host.")
            return self.stand.run(cmd, **kwargs)

        self.feed.subprocess = Namespace(run=run)
        config = dict(self.feed.job.CONFIG, remote_gate={"ssh_host": "oby-gate"})
        with mock.patch.object(self.feed.job, "CONFIG", config):
            for _ in range(5):
                self.assertIsNone(self.one_pass())
        item = self.task_now()
        self.assertEqual(item["state"], "waiting")
        self.assertNotIn("refused", item)
        self.assertEqual(started, [])
        self.assertEqual(self.stand.merges(), [])

    def test_feedalive_5_a_dropped_verify_run_is_unreached_and_a_failing_invariant_holds(self):
        j = {"approvals": approvals(OLD, "claude", "codex")}
        pr = {"headRefOid": OLD, "files": [{"path": PATH}]}
        self.feed.touches_security = lambda files: []
        self.feed.check_models = lambda files, head: self.feed.Unreached("gave no verdict: it printed nothing (exit 255)")
        self.assertEqual(self.feed.may_arm(j, pr, None, "A00-99")[0], "retry")
        self.feed.check_models = lambda files, head: f"{PATH}: the invariant safe does not hold"
        self.assertEqual(self.feed.may_arm(j, pr, None, "A00-99")[0], "wait")

    def test_feedalive_5_a_connection_dropped_after_some_output_is_unreached(self):
        def run(cmd, **_):
            if cmd[0] == "gh":
                return subprocess.CompletedProcess(cmd, 0, "// verify: main=m invariants=safe\n", "")
            if VERIFY.search(cmd[-1]):
                return subprocess.CompletedProcess(cmd, 255, "Compiling the model...\n", "Connection to oby-gate closed by remote host.")
            return subprocess.CompletedProcess(cmd, 0, "", "")

        config = dict(self.feed.job.CONFIG, remote_gate={"ssh_host": "oby-gate"})
        with mock.patch.object(self.feed.job, "CONFIG", config):
            self.feed.subprocess = Namespace(run=run)
            got = self.feed.check_models([PATH], HEAD)
        self.assertIsInstance(got, self.feed.Unreached)
        self.assertIn("gave no verdict", got)

    def test_feedalive_5_a_verdict_printed_before_ssh_dropped_is_no_verdict_for_both_kinds(self):
        """ssh prints Quint's verdict lines, then exits 255: neither a witness passes nor an invariant is held; both
        are the gate VM unreached, tried again at the next pass."""
        for kind, out in (("witnesses", VIOLATED[0]), ("invariants", VIOLATED[0]), ("witnesses", HOLDS[0]),
                          ("invariants", HOLDS[0])):
            def run(cmd, **_):
                if cmd[0] == "gh":
                    return subprocess.CompletedProcess(cmd, 0, f"// verify: main=m {kind}=safe\n", "")
                if VERIFY.search(cmd[-1]):
                    return subprocess.CompletedProcess(cmd, 255, out, "Connection to oby-gate closed by remote host.")
                return subprocess.CompletedProcess(cmd, 0, "", "")

            config = dict(self.feed.job.CONFIG, remote_gate={"ssh_host": "oby-gate"})
            with mock.patch.object(self.feed.job, "CONFIG", config):
                self.feed.subprocess = Namespace(run=run)
                got = self.feed.check_models([PATH], HEAD)
            self.assertIsInstance(got, self.feed.Unreached, (kind, out))
            self.assertIn("gave no verdict: ssh dropped (exit 255)", got)
            self.assertNotIn("does not hold", got)
            self.assertNotIn("was not violated", got)

    def test_feedalive_5_a_violation_printed_before_ssh_dropped_skips_the_pass_and_never_holds_the_head(self):
        """A whole pass: Quint's violation of an invariant printed, then ssh exits 255. The pass is skipped and the
        head stays unrefused; the next pass, whose run holds, arms it."""
        self.task("waiting", head=OLD)
        self.stand.jobs[JOB]["approvals"] = approvals(OLD, "claude", "codex")
        self.stand.view = {"headRefOid": OLD, "state": "OPEN", "files": [{"path": PATH}]}
        self.feed.touches_security = lambda files: []
        self.no_ready()
        verdict = [(VIOLATED[0], 255)]

        def run(cmd, **kwargs):
            if cmd[:2] == ["gh", "api"] and "contents" in cmd[2]:
                return subprocess.CompletedProcess(cmd, 0, "// verify: main=m invariants=safe\n", "")
            if cmd[0] == "ssh":
                out, code = verdict[0] if VERIFY.search(cmd[-1]) else ("", 0)
                return subprocess.CompletedProcess(cmd, code, out, "Connection to oby-gate closed by remote host." if code else "")
            return self.stand.run(cmd, **kwargs)

        self.feed.subprocess = Namespace(run=run)
        config = dict(self.feed.job.CONFIG, remote_gate={"ssh_host": "oby-gate"})
        with mock.patch.object(self.feed.job, "CONFIG", config):
            self.assertIsNone(self.one_pass())
            item = self.task_now()
            self.assertEqual(item["state"], "waiting")
            self.assertNotIn("refused", item)
            self.assertNotIn("does not hold", item["note"])
            self.assertEqual(self.stand.merges(), [])
            verdict[0] = HOLDS
            self.assertIsNotNone(self.one_pass())  # its approver first, then the merge on its APPROVE
            self.assertIsNotNone(self.one_pass())
        self.assertEqual(self.task_now()["state"], "merging")
        self.assertEqual(len(self.stand.merges()), 1)

    # FEEDALIVE-6: --account any names no account; the default names deskree-team
    def test_feedalive_6_account_any_sends_no_account_and_the_default_sends_deskree_team(self):
        self.feed.oby_jobs = lambda: []
        self.feed.free_gb = lambda: 100.0
        task = {"key": "A00-98", "title": "another task", "body": ""}
        self.feed.ready_tasks = lambda state: [] if "A00-98" in state else [(task, True, set())]
        sent = []
        self.feed.api = lambda method, path, body=None, timeout=30: (sent.append(body["job"]), (201, {"id": "job-9"}))[1]
        self.main("--once", "--account", "any")
        self.assertNotIn("account", sent[0])
        self.feed.FEED = self.dir / "feed-2"
        self.main("--once")
        self.assertEqual(sent[1]["account"], "deskree-team")

    def approved_twice(self, state, head=OLD, files=("x.py",), **more):
        """A task whose pull request's head `head` two tools approved."""
        self.no_ready()
        self.task(state, **more)
        self.stand.jobs[JOB]["approvals"] += approvals(head, "claude", "codex")
        self.stand.view = {"headRefOid": head, "state": "OPEN", "files": [{"path": f} for f in files]}

    def approving(self, head=OLD, status="done", summary="APPROVE\n1. met: it does", start=None, view=None):
        """A task whose approver job `approver-1`, asked about `head`, is `status` with `summary`, having read `start`."""
        self.no_ready()
        self.task("approving", head=head, approver="approver-1", approverHead=head, approved=["claude", "codex"],
                  approvers=["approver-1"])
        self.stand.jobs[JOB]["approvals"] = approvals(head, "claude", "codex")
        self.stand.jobs["approver-1"] = {"id": "approver-1", "kind": "review", "pr": PR, "status": status,
                                         "summary": summary, "startCommit": head if start is None else start}
        self.stand.view = {"headRefOid": view or head, "state": "OPEN", "files": [{"path": "x.py"}]}

    # FEEDAPPROVE-1: a head approved twice starts its approver instead of the merge
    def test_feedapprove_1_a_reviewing_head_approved_twice_starts_the_approver_not_the_merge(self):
        self.approved_twice("reviewing")
        self.stand.approver = {"status": "running", "summary": None}
        self.one_pass()
        self.assertEqual(len(self.stand.posts), 1)
        sent = self.stand.sent()[0]
        self.assertEqual({k: sent[k] for k in ("repo", "kind", "pr", "base", "rounds", "title", "clientRequestId", "account")},
                         {"repo": REPO, "kind": "review", "pr": PR, "base": self.feed.job.BASE, "rounds": 1,
                          "title": "A00-99 approver", "clientRequestId": f"feed-approve-A00-99-{OLD[:12]}", "account": "acct"})
        for absent in ("check", "branch", "model", "agent"):
            self.assertNotIn(absent, sent)
        item = self.task_now()
        self.assertEqual((item["state"], item["approver"], item["approverHead"], item["approved"], item["approvers"]),
                         ("approving", "approver-1", OLD, ["claude", "codex"], ["approver-1"]))
        self.assertEqual(self.stand.merges(), [])
        self.one_pass()
        self.assertEqual(len(self.stand.posts), 1)
        self.assertEqual(self.stand.merges(), [])
        self.assertEqual(self.task_now()["state"], "approving")

    def test_feedapprove_1_a_waiting_task_with_a_new_head_approved_twice_starts_the_approver(self):
        self.approved_twice("waiting", head=NEW, refused=OLD)
        self.stand.approver = {"status": "running", "summary": None}
        self.one_pass()
        self.assertEqual([s["clientRequestId"] for s in self.stand.sent()], [f"feed-approve-A00-99-{NEW[:12]}"])
        item = self.task_now()
        self.assertEqual((item["state"], item["approverHead"], item["approved"]), ("approving", NEW, ["claude", "codex"]))
        self.assertEqual(self.stand.merges(), [])

    def test_feedapprove_1_account_any_names_no_account_on_the_approver(self):
        self.approved_twice("reviewing")
        self.feed.one_pass(Namespace(max=3, min_free_gb=0, dry_run=False, account="any"))
        self.assertNotIn("account", self.stand.sent()[0])

    def test_feedapprove_1_the_brief_names_the_head_the_base_definition_and_the_task_file(self):
        item = {"key": "A00-99", "title": "t" * 300, "pr": PR}
        text = self.feed.approver_brief(item, OLD)
        self.assertEqual(text, self.feed.approver_brief(dict(item), OLD))
        self.assertLessEqual(len(text), 2000)
        for said in (OLD, f"#{PR}", f"git show origin/{self.feed.job.BASE}:.claude/agents/approver.md",
                     f"git show origin/{self.feed.job.BASE}:factory/queue/tasks/A00-99.md", "APPROVE", "ESCALATE",
                     f"git diff origin/{self.feed.job.BASE}...HEAD"):
            self.assertIn(said, text)
        self.assertNotIn("factory/queue/done/", text)  # never the pull request's own copy, which it may have changed
        self.assertLessEqual(len(self.feed.approver_brief(dict(item, title="t" * 5000), OLD)), 2000)

    def test_feedapprove_1_a_task_whose_file_isnt_on_the_base_is_held_with_no_approver(self):
        self.approved_twice("reviewing")
        self.stand.off_base = {"A00-99"}
        self.one_pass()
        self.one_pass()
        item = self.task_now()
        self.assertEqual((item["state"], item["refused"]), ("waiting", OLD))
        self.assertEqual(item["note"], f"approved twice, but its task file isn't on origin/{self.feed.job.BASE}, so its "
                                       "approver has no acceptance list to read: merge it yourself")
        self.assertEqual((self.stand.posts, self.stand.merges()), ([], []))

    # FEEDAPPROVE-2: APPROVE arms the exact head
    def test_feedapprove_2_approve_arms_the_head_the_approver_read(self):
        self.approving(summary="  APPROVE  \n1. met: it does\n2. met: it says so")
        self.one_pass()
        self.assertEqual(self.stand.merges(), [("gh", "pr", "merge", str(PR), "--repo", REPO, "--auto", "--match-head-commit", OLD)])
        item = self.task_now()
        self.assertEqual((item["state"], item["head"], item["approved"]), ("merging", OLD, ["claude", "codex"]))
        self.one_pass()
        self.assertEqual(len(self.stand.merges()), 1)

    def test_feedapprove_2_an_approve_is_not_armed_when_may_arm_no_longer_arms_the_head(self):
        """While the approver ran, a review asked for changes, a review needs the owner, a model check started to fail
        or the approvals went: its APPROVE arms nothing, and the task is back in the usual flow, `waiting`."""
        cases = {"changes": (lambda j: j["approvals"].append({"commit": OLD, "agent": "cursor", "verdict": "changes"}),
                             "a review asked for changes or needs you: open it in Oby", False),
                 "needs you": (lambda j: j.update(reviewNeedsYou=True),
                               "a review asked for changes or needs you: open it in Oby", False),
                 "model": (None, f"approved twice, but {PATH}: the invariant safe does not hold", True),
                 "approvals gone": (lambda j: j.update(approvals=[]),
                                    f"the approver approved {OLD[:12]}, but it no longer has two approvals: waiting for them", False)}
        for case, (change, note, held) in cases.items():
            with self.subTest(case=case):
                self.feed.FEED = self.dir / f"feed-{case}"
                self.stand = StandIns()
                self.feed.api = self.stand.api
                self.feed.subprocess = Namespace(run=self.stand.run)
                self.approving()
                if change:
                    change(self.stand.jobs[JOB])
                self.feed.check_models = (lambda files, head: f"{PATH}: the invariant safe does not hold") if case == "model" else (lambda files, head: None)
                self.one_pass()
                item = self.task_now()
                self.assertEqual((item["state"], item["note"]), ("waiting", note))
                self.assertEqual(item.get("refused"), OLD if held else None)
                self.one_pass()
                self.assertEqual(self.stand.merges(), [])

    # FEEDAPPROVE-3: ESCALATE waits for the owner with its reasons
    def test_feedapprove_3_escalate_waits_with_its_reasons_and_never_merges_that_head(self):
        reasons = ["1. met: the feeder asks its approver first", "",
                   "It adds words a person sees that no Sangam screen shows: \"Ask the owner\".",
                   "It changes an AGENTS.md rule (Software factory).",
                   "Decide: whether the factory may merge this by itself."]
        self.approving(summary="ESCALATE\n" + "\n".join(reasons))
        self.one_pass()
        item = self.task_now()
        self.assertEqual((item["state"], item["refused"]), ("waiting", OLD))
        self.assertEqual(item["escalation"], "\n".join(reasons))
        self.assertEqual(item["note"], "approved twice, but the approver escalated it to you: " + reasons[0])
        self.one_pass()
        self.one_pass()
        self.assertEqual(self.stand.merges(), [])
        self.assertEqual(self.stand.posts, [])
        self.feed.free_gb = lambda: 100.0
        out = io.StringIO()
        with mock.patch("sys.stdout", out):
            self.feed.status()
        lines = out.getvalue().splitlines()
        self.assertTrue(lines[0].startswith("waiting    A00-99"), lines)
        self.assertEqual(lines[1:1 + len(reasons)], ["  " + line for line in reasons])
        self.assertEqual(lines[-1], "# 1 waiting; 100 GB free")
        self.stand.jobs[JOB]["approvals"] += approvals(NEW, "claude", "codex")
        self.stand.view = {"headRefOid": NEW, "state": "OPEN", "files": [{"path": "x.py"}]}
        self.stand.approver = {"status": "running", "summary": None}
        self.one_pass()
        self.assertEqual([s["clientRequestId"] for s in self.stand.sent()], [f"feed-approve-A00-99-{NEW[:12]}"])
        item = self.task_now()
        self.assertEqual((item["state"], item["approverHead"], item["approvers"]), ("approving", NEW, ["approver-1", "approver-2"]))
        self.assertEqual(self.stand.merges(), [])

    def test_feedapprove_3_status_strips_control_characters_from_the_approvers_words(self):
        self.approving(summary="ESCALATE\n\x1b[2JIt \x07changes a lane.\x1b]0;owned\x07\n\x9bthe end")
        self.one_pass()
        self.feed.free_gb = lambda: 100.0
        out = io.StringIO()
        with mock.patch("sys.stdout", out):
            self.feed.status()
        printed = out.getvalue()
        self.assertFalse(re.search(r"[\x00-\x09\x0b-\x1f\x7f-\x9f]", printed), repr(printed))
        self.assertIn("  [2JIt changes a lane.]0;owned\n  the end\n", printed)

    def held_head_comes_back(self, summary):
        """OLD's approver answers `summary`; NEW is approved and GitHub refuses its merge, so `refused` is NEW; then OLD
        is pushed back, its two approvals still there and obid's receipt for its approver gone. Returns the approver keys
        sent and the heads merged."""
        self.approving(head=OLD, summary=summary)
        self.one_pass()
        self.assertEqual(self.task_now()["held"], [OLD])
        self.stand.jobs[JOB]["approvals"] += approvals(NEW, "claude", "codex")
        self.stand.view = {"headRefOid": NEW, "state": "OPEN", "files": [{"path": "x.py"}]}
        self.stand.merge_code = 1
        self.one_pass()
        self.one_pass()
        item = self.task_now()
        self.assertEqual((item["state"], item["refused"], item["held"]), ("waiting", NEW, [OLD]))
        self.stand.view = {"headRefOid": OLD, "state": "OPEN", "files": [{"path": "x.py"}]}
        self.one_pass()
        self.one_pass()
        self.assertEqual(self.task_now()["state"], "waiting")
        return [s["clientRequestId"] for s in self.stand.sent()], [m[-1] for m in self.stand.merges()]

    def test_feedapprove_3_an_escalated_head_that_comes_back_is_never_asked_again(self):
        self.assertEqual(self.held_head_comes_back("ESCALATE\nIt changes a lane."),
                         ([f"feed-approve-A00-99-{NEW[:12]}"], [NEW]))

    # FEEDAPPROVE-4: a malformed answer or a broken approver never merges
    def test_feedapprove_4_a_malformed_answer_or_a_broken_approver_holds_the_head(self):
        cases = [("done", first, f'its answer\'s first line is "{first}"')
                 for first in ("Approve", "**APPROVE**", "APPROVE.", "STATUS: READY", "")]
        cases += [(status, "APPROVE", f"its job ended {status}") for status in ("failed", "stopped", "interrupted")]
        cases += [("gone", "APPROVE", "its job is gone")]
        for status, first, why in cases:
            with self.subTest(status=status, first=first):
                self.feed.FEED = self.dir / f"feed-{status}-{len(first)}-{first[:2]}"
                self.stand = StandIns()
                self.stand.missed = self.feed.OBID_MISSED
                self.feed.api = self.stand.api
                self.feed.subprocess = Namespace(run=self.stand.run)
                self.approving(status=status, summary=first + ("\n1. met: it does" if first else ""))
                if status == "gone":
                    del self.stand.jobs["approver-1"]
                self.one_pass()
                item = self.task_now()
                self.assertEqual((item["state"], item["refused"]), ("waiting", OLD))
                self.assertEqual(item["note"], f"approved twice, but the approver gave no verdict: {why}: merge it yourself or push a new head")
                self.one_pass()
                self.assertEqual(self.stand.merges(), [])
                self.assertEqual(self.stand.posts, [])

    def test_feedapprove_4_a_head_with_no_verdict_that_comes_back_is_never_asked_again(self):
        self.assertEqual(self.held_head_comes_back("Approve"), ([f"feed-approve-A00-99-{NEW[:12]}"], [NEW]))

    # FEEDAPPROVE-5: a moved head never merges on an old verdict
    def test_feedapprove_5_an_approve_for_a_head_that_moved_never_merges(self):
        self.approving(head=OLD, view=NEW)
        self.one_pass()
        item = self.task_now()
        self.assertEqual(item["state"], "waiting")
        self.assertNotIn("refused", item)
        self.assertTrue(item["note"].startswith("the head moved while the approver read it"), item["note"])
        self.one_pass()  # approvals of OLD only
        self.assertEqual((self.stand.posts, self.stand.merges()), ([], []))
        self.stand.jobs[JOB]["approvals"] += approvals(NEW, "claude", "codex")
        self.stand.approver = {"status": "running", "summary": None}
        self.one_pass()
        self.assertEqual([s["clientRequestId"] for s in self.stand.sent()], [f"feed-approve-A00-99-{NEW[:12]}"])
        self.assertEqual(self.task_now()["approverHead"], NEW)
        self.one_pass()
        self.assertEqual(self.stand.merges(), [])
        self.stand.jobs["approver-2"].update(status="done", summary="APPROVE\n1. met: it does")
        self.one_pass()
        self.assertEqual(self.stand.merges(), [("gh", "pr", "merge", str(PR), "--repo", REPO, "--auto", "--match-head-commit", NEW)])

    def test_feedapprove_5_an_approver_that_read_another_head_never_merges(self):
        self.approving(head=OLD, start=NEW)
        self.one_pass()
        item = self.task_now()
        self.assertEqual(item["state"], "waiting")
        self.assertNotIn("refused", item)
        self.assertTrue(item["note"].startswith("the head moved while the approver read it"), item["note"])
        self.assertEqual(self.stand.merges(), [])

    # FEEDAPPROVE-6: the existing holds come first
    def test_feedapprove_6_a_security_path_a_failing_model_one_tool_or_a_refused_head_starts_no_approver(self):
        for case in ("security", "model", "one tool", "refused"):
            with self.subTest(case=case):
                self.feed.FEED = self.dir / f"feed-{case}"
                self.stand = StandIns()
                self.stand.missed = self.feed.OBID_MISSED
                self.feed.api = self.stand.api
                files = {"security": ["x.py", "AGENTS.md"], "model": [PATH]}.get(case, ["x.py"])
                self.approved_twice("reviewing", files=files, **({"refused": OLD} if case == "refused" else {}))
                if case == "one tool":
                    self.stand.jobs[JOB]["approvals"] = approvals(OLD, "claude")

                def run(cmd, **kwargs):
                    if cmd[:2] == ["gh", "api"] and "contents" in cmd[2]:
                        return subprocess.CompletedProcess(cmd, 0, "// verify: main=m invariants=safe\n", "")
                    if cmd[0] == "ssh":
                        return subprocess.CompletedProcess(cmd, *(VIOLATED[::-1] if VERIFY.search(cmd[-1]) else (0, "")), "")
                    return self.stand.run(cmd, **kwargs)

                self.feed.subprocess = Namespace(run=run)
                config = dict(self.feed.job.CONFIG, remote_gate={"ssh_host": "oby-gate"})
                with mock.patch.object(self.feed.job, "CONFIG", config):
                    self.one_pass()
                    self.one_pass()
                self.assertEqual((self.stand.posts, self.stand.merges()), ([], []))
                item = self.task_now()
                if case == "security":
                    self.assertEqual(item["note"], "approved twice, but it touches a security path (AGENTS.md): merge it yourself")
                if case == "model":
                    self.assertEqual(item["note"], f"approved twice, but {PATH}: the invariant safe does not hold")
                self.assertEqual(item["state"], "reviewing" if case == "one tool" else "waiting")

    # FEEDAPPROVE-7: approver jobs are deleted with the task, and kept while it matters
    def test_feedapprove_7_a_merged_task_deletes_its_approver_jobs_with_the_rest(self):
        self.approving(head=NEW)
        item = self.task_now()
        item["approvers"] = ["approver-0", "approver-1"]
        self.feed.save(item)
        self.stand.view = {"headRefOid": NEW, "state": "MERGED", "files": []}
        self.feed.oby_jobs = lambda: [{"id": JOB, "kind": "build"}, {"id": "review-1", "reviewOf": JOB},
                                      {"id": "approver-0", "kind": "review"}, {"id": "approver-1", "kind": "review"},
                                      {"id": "other", "kind": "build"}]
        cleanup, api = [], self.feed.api

        def first_delete(method, path, body=None, timeout=30):
            if method == "DELETE" and not cleanup:
                cleanup.append(self.task_now()["cleanup"])
            return api(method, path, body, timeout)

        self.feed.api = first_delete
        self.one_pass()
        self.assertEqual(cleanup, [sorted([JOB, "review-1", "approver-0", "approver-1"])])
        deleted = sorted(c[2] for c in self.stand.calls if c[:2] == ("api", "DELETE"))
        self.assertEqual(deleted, [f"/v1/jobs/{i}" for i in sorted([JOB, "review-1", "approver-0", "approver-1"])])
        self.assertEqual(self.task_now()["state"], "merged")

    def test_feedapprove_7_clean_keeps_the_approvers_of_approving_and_waiting_tasks(self):
        self.task("approving", approver="approver-1", approverHead=OLD, approvers=["approver-1"])
        self.feed.save({"key": "A00-97", "title": "another", "job": "job-3", "state": "waiting", "pr": 42, "head": OLD,
                        "refused": OLD, "escalation": "It changes a lane.", "approver": "approver-2", "approvers": ["approver-2"]})
        self.feed.save({"key": "A00-96", "title": "merged", "job": "job-4", "state": "merged", "pr": 43,
                        "approvers": ["approver-3"]})
        self.feed.oby_jobs = lambda: [{"id": i, "kind": k, "status": "done"} for i, k in
                                      ((JOB, "build"), ("job-3", "build"), ("approver-1", "review"),
                                       ("approver-2", "review"), ("approver-3", "review"))]
        self.feed.free_gb = lambda: 100.0
        self.feed.clean()
        deleted = sorted(c[2] for c in self.stand.calls if c[:2] == ("api", "DELETE"))
        self.assertEqual(deleted, ["/v1/jobs/approver-3"])

    # FEEDAPPROVE-8: obid not starting the approver never merges
    def test_feedapprove_8_an_approver_start_with_no_answer_is_sent_again_unchanged(self):
        self.approved_twice("reviewing")
        self.stand.post_answers = [None]
        self.assertIsNone(self.one_pass())
        self.assertEqual(self.task_now()["state"], "reviewing")
        self.one_pass()
        self.assertEqual(len(self.stand.posts), 2)
        self.assertEqual(self.stand.posts[0], self.stand.posts[1])
        self.assertEqual(self.stand.sent()[0]["clientRequestId"], f"feed-approve-A00-99-{OLD[:12]}")
        item = self.task_now()
        self.assertEqual((item["state"], item["approver"]), ("approving", "approver-1"))
        self.assertEqual(self.stand.merges(), [])

    def test_feedapprove_8_a_5xx_keeps_the_task_and_a_4xx_holds_the_head(self):
        self.approved_twice("reviewing")
        self.stand.post_answers = [(503, {"error": "obid is restarting"})]
        self.one_pass()
        item = self.task_now()
        self.assertEqual(item["state"], "reviewing")
        self.assertTrue(item["note"].endswith("tried again at the next pass"), item["note"])
        self.stand.post_answers = [(422, {"error": "pr: no such pull request"})]
        self.one_pass()
        item = self.task_now()
        self.assertEqual((item["state"], item["refused"]), ("waiting", OLD))
        self.assertTrue(item["note"].startswith("approved twice, but obid didn't start the approver: "), item["note"])
        self.one_pass()
        self.assertEqual(len(self.stand.posts), 2)
        self.assertEqual(self.stand.merges(), [])


KEY = "A00-99"
PROOFS = f"docs/proofs/{KEY}.md"
LIB = "core/crates/demo/src/lib.rs"
HARNESS = "add_never_panics"
KANI_RUN = ["cargo", "kani", "-p", "demo", "--tests", "--harness", HARNESS]
BASE_LIB = "pub fn add(a: u8, b: u8) -> u8 {\n    a.saturating_add(b)\n}\n"
PROOF_MODULE = """
#[cfg(test)]
mod proofs {
    #![cfg(kani)]

    #[kani::proof]
    fn add_never_panics() {
        let a: u8 = kani::any();
        let b: u8 = kani::any();
        let _ = super::add(a, b);
    }
}
"""
PROOF_ENTRY = """# Proofs of A00-99

## Proofs

### PROOF-1
- Property: add never panics for any two bytes.
- Scenario: DEMO-1
- Crate: `demo`
- Harness: `add_never_panics`
- Assumes: none
- Unwind: none
- Broken file: `core/crates/demo/src/lib.rs`
- Broken old: `{old}`
- Broken new: `a.saturating_add(b) // BROKEN`
"""
# The gate VM's stand-in: logs each command, then prints VERIFICATION:- FAILED when a `.rs` file of the tree it runs
# in holds BROKEN, else SUCCESSFUL; STAND_IN makes it always fail, always succeed (vacuous), sleep, or fall back to
# this Mac and then succeed.
KANI_STAND_IN = """#!/usr/bin/env python3
import json, os, sys, time
from pathlib import Path
with open(os.environ["STAND_IN_LOG"], "a") as f:
    f.write(json.dumps(sys.argv[2:]) + "\\n")
mode = os.environ.get("STAND_IN", "kani")
if mode == "sleep":
    time.sleep(30)
if mode == "fallback":
    print("factory/bin/test-run: fell back to this Mac: ssh ended 255 before the run started", file=sys.stderr)
    print("VERIFICATION:- SUCCESSFUL")
    sys.exit(0)
broken = any("BROKEN" in p.read_text() for p in Path.cwd().rglob("*.rs"))
if mode == "failing" or (broken and mode != "vacuous"):
    print("VERIFICATION:- FAILED")
    sys.exit(1)
print("VERIFICATION:- SUCCESSFUL")
"""


class FeedKani(unittest.TestCase):
    """FEEDKANI-1 to 9: a real bare origin and the feeder's clone of it, each pull request's head pushed to origin
    only, and a stand-in test-run in a temporary prove.BIN; `gh` and obid are StandIns. No cargo, Kani or ssh."""

    def setUp(self):
        self.dir = Path(tempfile.mkdtemp()).resolve()
        self.addCleanup(shutil.rmtree, self.dir, True)
        # GIT_DIR and the like, set when the tests run inside a worktree's hooks, would point git at that repository
        env = {k: v for k, v in os.environ.items() if not k.startswith("GIT_") and k != "STAND_IN"}
        no_git_env = mock.patch.dict(os.environ, dict(env, STAND_IN_LOG=str(self.dir / "test-run.log")), clear=True)
        no_git_env.start()
        self.addCleanup(no_git_env.stop)
        self.feed = load_feed()
        self.stand = StandIns()
        self.feed.HOME = self.dir
        self.feed.FEED = self.dir / "feed"
        self.feed.GH_REPO = REPO
        self.feed.api = self.stand.api
        self.feed.say = lambda msg: None
        self.feed.ready_tasks = lambda state: []
        self.feed.touches_security = lambda files: []  # merge.rs isn't in this throwaway checkout
        base = self.feed.job.BASE
        origin, self.seed, self.repo = self.dir / "origin.git", self.dir / "seed", self.dir / "repo"
        self.git(self.dir, "init", "-q", "--bare", "-b", base, str(origin))
        self.git(self.dir, "init", "-q", "-b", base, str(self.seed))
        (self.seed / LIB).parent.mkdir(parents=True)
        (self.seed / LIB).write_text(BASE_LIB)
        self.git(self.seed, "add", LIB)
        self.git(self.seed, "commit", "-q", "-m", "base")
        self.git(self.seed, "remote", "add", "origin", str(origin))
        self.git(self.seed, "push", "-q", "origin", base)
        self.git(self.dir, "clone", "-q", "-b", base, str(origin), str(self.repo))
        bin_dir = self.dir / "bin"
        bin_dir.mkdir()
        (bin_dir / "test-run").write_text(KANI_STAND_IN)
        (bin_dir / "test-run").chmod(0o755)
        self.feed.prove.BIN = bin_dir
        self.feed.REPO = self.repo
        job = self.feed.job
        for patch in (mock.patch.object(job, "REPO", self.repo),
                      mock.patch.object(job, "TASKS", self.repo / "factory" / "queue" / "tasks")):
            patch.start()
            self.addCleanup(patch.stop)
        self.feed.subprocess = Namespace(run=self.git_or_gh)

    def git_or_gh(self, cmd, **kwargs):
        """git runs for real in the throwaway repositories, logged; `gh` goes to the stand-in."""
        if cmd[0] == "git":
            self.stand.calls.append(("run", *cmd))
            return subprocess.run(cmd, **kwargs)
        return self.stand.run(cmd, **kwargs)

    def git(self, cwd, *args):
        return subprocess.run(["git", "-c", "user.name=t", "-c", "user.email=t@t", "-c", "commit.gpgsign=false", *args],
                              cwd=cwd, check=True, capture_output=True, text=True).stdout

    def base_task(self, formal):
        """Commits factory/queue/tasks/A00-99.md, whose `### Formal verification` says `formal`, on origin's base."""
        base = self.feed.job.BASE
        self.git(self.seed, "checkout", "-q", base)
        path = self.seed / "factory" / "queue" / "tasks" / f"{KEY}.md"
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(f"KEY: {KEY}\nTITLE: a task\n\n## What\nA rule.\n\n### Formal verification\n{formal}\n\n### Files\n- x.py\n")
        self.git(self.seed, "add", "factory")
        self.git(self.seed, "commit", "-q", "-m", "a task")
        self.git(self.seed, "push", "-q", "origin", base)

    def head(self, files):
        """Commits `files` (path: text) on a branch from origin's base, pushes it to origin only, and returns its sha."""
        self.git(self.seed, "checkout", "-q", "-B", "pr", self.feed.job.BASE)
        for path, text in files.items():
            (self.seed / path).parent.mkdir(parents=True, exist_ok=True)
            (self.seed / path).write_text(text)
        self.git(self.seed, "add", *files)
        self.git(self.seed, "commit", "-q", "-m", "the pull request")
        self.git(self.seed, "push", "-q", "-f", "origin", "pr")
        return self.git(self.seed, "rev-parse", "HEAD").strip()

    def review(self, head, files, state="reviewing"):
        """One pass over the task, `state`, its pull request at `head` changing `files` and approved twice by two
        tools; the test-run log is emptied first. Returns the pass's result and the task."""
        self.feed.save({"key": KEY, "title": "a task", "job": JOB, "state": state, "pr": PR, "files": ["x.py"]})
        (self.dir / "test-run.log").unlink(missing_ok=True)
        self.stand.jobs[JOB]["approvals"] = approvals(head, "claude", "codex")
        self.stand.view = {"headRefOid": head, "state": "OPEN", "files": [{"path": f} for f in files]}
        return self.one_pass(), self.feed.load_state()[KEY]

    def one_pass(self):
        return self.feed.one_pass(Namespace(max=3, min_free_gb=0, dry_run=False, account="acct"))

    def kani_calls(self):
        log = self.dir / "test-run.log"
        return [json.loads(line) for line in log.read_text().splitlines()] if log.exists() else []

    def proved_head(self, old="a.saturating_add(b)", lib=PROOF_MODULE, proofs=None):
        """A head adding the harness and docs/proofs/A00-99.md: (its sha, the pull request's files)."""
        files = {LIB: BASE_LIB + lib, PROOFS: PROOF_ENTRY.format(old=old) if proofs is None else proofs}
        return self.head(files), list(files)

    def assert_held(self, item, head, *said):
        self.assertEqual(item["state"], "waiting")
        self.assertEqual(item.get("refused"), head)
        self.assertTrue(item["note"].startswith(self.feed.HELD), item["note"])
        for words in said:
            self.assertIn(words, item["note"])

    def assert_no_scratch_copy(self):
        listed = self.git(self.repo, "worktree", "list", "--porcelain")
        self.assertEqual([line for line in listed.splitlines() if line.startswith("worktree ")],
                         [f"worktree {self.repo}"], listed)

    # FEEDKANI-1: a passing harness whose broken rule turns it red arms the merge
    def test_feedkani_1_passing_harness_and_red_broken_rule_arm_the_merge(self):
        self.base_task("A Kani harness over the pure function add: it never panics.")  # the approver reads it there
        head, files = self.proved_head()
        got, item = self.review(head, files)
        self.assertEqual(got, (1, 0))
        self.assertEqual(self.kani_calls(), [KANI_RUN, KANI_RUN])
        self.assertEqual(item["state"], "approving")  # its approver first, then the merge on its APPROVE
        self.one_pass()  # may_arm() again before the merge: the harness and its broken rule run once more
        item = self.feed.load_state()[KEY]
        self.assertEqual(self.stand.merges(), [("gh", "pr", "merge", str(PR), "--repo", REPO, "--auto", "--match-head-commit", head)])
        self.assertEqual(item["state"], "merging")
        self.assertEqual(self.kani_calls(), [KANI_RUN] * 4)
        self.assert_no_scratch_copy()
        self.assertEqual(self.git(self.repo, "show", f"{head}:{LIB}"), BASE_LIB + PROOF_MODULE)
        self.assertIn("VERIFICATION:- FAILED", (self.feed.FEED / f"{KEY}.proofs.log").read_text())

    # FEEDKANI-2: a failing harness holds the head, naming its entry and harness
    def test_feedkani_2_failing_harness_is_held_and_named(self):
        os.environ["STAND_IN"] = "failing"
        head, files = self.proved_head()
        _, item = self.review(head, files)
        self.assertEqual(self.stand.merges(), [])
        self.assert_held(item, head, HARNESS, "FAILED")
        self.assertTrue(item["note"].startswith(f"approved twice, but {PROOFS}: PROOF-1"), item["note"])
        self.assertEqual(self.kani_calls(), [KANI_RUN])
        self.assert_no_scratch_copy()

    # FEEDKANI-3: a broken rule that leaves the harness green, or that doesn't apply, holds the head
    def test_feedkani_3_broken_rule_still_green_is_held_as_vacuous(self):
        os.environ["STAND_IN"] = "vacuous"
        head, files = self.proved_head()
        _, item = self.review(head, files)
        self.assertEqual(self.stand.merges(), [])
        self.assert_held(item, head, f"{PROOFS}: PROOF-1", HARNESS, "vacuous")
        self.assertEqual(self.kani_calls(), [KANI_RUN, KANI_RUN])

    def test_feedkani_3_broken_rule_that_does_not_apply_is_held(self):
        head, files = self.proved_head(old="a.wrapping_add(b)")
        _, item = self.review(head, files)
        self.assertEqual(self.stand.merges(), [])
        self.assert_held(item, head, f"{PROOFS}: PROOF-1", HARNESS, "DID NOT APPLY")
        self.assertEqual(self.kani_calls(), [KANI_RUN])

    # FEEDKANI-4: a run past proofs.timeout_s holds the head in advisory mode as in blocking mode
    def test_feedkani_4_timeout_holds_whatever_the_mode(self):
        os.environ["STAND_IN"] = "sleep"
        head, files = self.proved_head()
        for mode in ("advisory", "blocking"):
            with self.subTest(mode=mode), mock.patch.dict(self.feed.prove.JOB.CONFIG, {"proofs": {"mode": mode, "timeout_s": 1}}):
                _, item = self.review(head, files)
                self.assert_held(item, head, f"{PROOFS}: PROOF-1", HARNESS, "TIMEOUT")
                self.assertEqual(self.stand.merges(), [])
                self.assertEqual(self.kani_calls(), [KANI_RUN])
        self.assert_no_scratch_copy()

    # FEEDKANI-5: a forbidden construct or a malformed entry holds the head before anything runs
    def test_feedkani_5_forbidden_construct_or_malformed_entry_is_held_before_any_run(self):
        stubbed = PROOF_MODULE.replace("    #[kani::proof]\n", "    #[kani::proof]\n    #[kani::stub(core::cmp::max, core::cmp::min)]\n")
        line = next(n for n, text in enumerate((BASE_LIB + stubbed).splitlines(), 1) if "kani::stub" in text)
        no_crate = "".join(text for text in PROOF_ENTRY.format(old="a.saturating_add(b)").splitlines(True)
                           if not text.startswith("- Crate:"))
        cases = (({"lib": stubbed}, ("forbidden construct:", f"{LIB}:{line}: kani::stub")),
                 ({"proofs": no_crate}, (f"{PROOFS}: PROOF-1: Crate is missing",)),
                 ({"proofs": "# Proofs of A00-99\n\nNone here.\n"}, (f"{KEY}.md has no `## Proofs` section",)))
        for how, said in cases:
            with self.subTest(said=said[0]):
                head, files = self.proved_head(**how)
                _, item = self.review(head, files)
                self.assert_held(item, head, *said)
                self.assertEqual(self.kani_calls(), [])
                self.assertEqual(self.stand.merges(), [])

    # FEEDKANI-6: a new harness, or a task that asks for Kani, with no proofs file waits, held, and nothing runs
    def test_feedkani_6_new_harness_or_task_asking_for_kani_without_the_file_waits(self):
        head = self.head({LIB: BASE_LIB + PROOF_MODULE})
        _, item = self.review(head, [LIB])
        self.assert_held(item, head, f"{PROOFS} is missing", "#[kani::proof]", LIB)
        self.base_task("A Kani harness over the pure function add: it never panics. The harness must turn red when "
                       "saturating_add is swapped for a plain +.")
        head = self.head({"x.py": "x = 1\n"})
        _, item = self.review(head, ["x.py"])
        self.assert_held(item, head, f"{PROOFS} is missing", "asks for a Kani proof")
        self.assertEqual(self.kani_calls(), [])
        self.assertEqual(self.stand.merges(), [])

    # FEEDKANI-7: nothing to prove arms as before, with no Kani run and, with no .rs file, no fetch of the head
    def test_feedkani_7_nothing_to_prove_arms_with_no_kani_run(self):
        self.base_task("None: no rule decides an outcome here, so no Kani harness.")
        head = self.head({LIB: BASE_LIB.replace("saturating_add", "wrapping_add")})
        self.review(head, [LIB])
        self.one_pass()  # its approver first, then the merge on its APPROVE
        self.assertEqual(self.feed.load_state()[KEY]["state"], "merging")
        self.assertEqual(self.kani_calls(), [])
        head = self.head({"x.py": "x = 1\n"})
        self.review(head, ["x.py"])
        self.one_pass()
        self.assertEqual(self.feed.load_state()[KEY]["state"], "merging")
        self.assertEqual(len(self.stand.merges()), 2)
        self.assertEqual(self.kani_calls(), [])
        self.assertNotIn(("run", "git", "fetch", "-q", "origin", head), self.stand.calls)

    # FEEDKANI-8: a run that fell back to this Mac, a head not fetched or a prove exit is retried, never passed
    def test_feedkani_8_fallback_to_this_mac_is_retried_never_passed(self):
        os.environ["STAND_IN"] = "fallback"
        head, files = self.proved_head()
        got, item = self.review(head, files)
        self.assertIsNone(got)
        self.assertEqual(item["state"], "waiting")
        self.assertNotIn("refused", item)
        self.assertTrue(item["note"].endswith("tried again at the next pass (1 of 3)"), item["note"])
        self.assertIn("fell back to this Mac", item["note"])
        for n in (2, 3):
            self.assertIsNone(self.one_pass())
            item = self.feed.load_state()[KEY]
            self.assertNotIn("refused", item)
            self.assertTrue(item["note"].endswith(f"tried again at the next pass ({n} of 3)"), item["note"])
        self.assertIsNone(self.one_pass())
        item = self.feed.load_state()[KEY]
        self.assertEqual(item["refused"], head)
        self.assertTrue(item["note"].endswith("held after 3 tries"), item["note"])
        self.assertEqual(self.stand.merges(), [])
        self.assert_no_scratch_copy()

    def test_feedkani_8_head_not_fetched_or_prove_exit_is_retried(self):
        missing = "f" * 40  # never pushed: the fetch fails
        got, item = self.review(missing, [PROOFS])
        self.assertIsNone(got)
        self.assertEqual(item["state"], "waiting")
        self.assertNotIn("refused", item)
        self.assertIn("wasn't fetched", item["note"])
        self.assertTrue(item["note"].endswith("tried again at the next pass (1 of 3)"), item["note"])
        head, files = self.proved_head()
        with mock.patch.dict(self.feed.prove.JOB.CONFIG, {"proofs": {"timeout_s": 0}}):  # settings() exits 2
            got, item = self.review(head, files)
        self.assertIsNone(got)
        self.assertEqual(item["state"], "waiting")
        self.assertNotIn("refused", item)
        self.assertIn("factory/bin/prove exited 2", item["note"])
        self.assertTrue(item["note"].endswith("tried again at the next pass (1 of 3)"), item["note"])
        self.assertEqual(self.kani_calls(), [])
        self.assertEqual(self.stand.merges(), [])

    # FEEDKANI-9: the brief asks for docs/proofs/<KEY>.md only when the task's Formal verification asks for Kani
    def test_feedkani_9_brief_asks_for_the_proofs_file_only_when_the_task_asks_for_kani(self):
        def task(formal):
            return {"key": KEY, "title": "a task", "body": f"## What\nA rule.\n\n### Formal verification\n{formal}\n\n### Files\n- x.py\n"}

        for on_base in (True, False):
            text = self.feed.brief(task("A Kani harness over the pure function add: it never panics."), on_base)
            self.assertIn(PROOFS, text)
            self.assertIn("## Proofs", text)
            for formal in ("None of its own: no Kani harness, the rule is tested with unit tests.",
                           "A Quint model docs/models/x.qnt with\n// verify: main=x steps=4 invariants=safe"):
                text = self.feed.brief(task(formal), on_base)
                self.assertNotIn(PROOFS, text)
                self.assertNotIn("## Proofs", text)


class Stop(Exception):
    """Ends a test's main() loop."""


class CheckModels(unittest.TestCase):
    def check(self, line, runs):
        """check_models() over one model whose first line is `line`; `runs` maps a check's name to its output and
        exit code. Returns check_models()'s result and the `quint verify` commands it sent."""
        verifies = []

        def run(cmd, **_):
            if cmd[0] == "gh":
                return subprocess.CompletedProcess(cmd, 0, f"{line}\nmodule oby_factory {{}}\n", "")
            remote = cmd[-1]
            if VERIFY.search(remote):
                verifies.append(remote)
                out, code = runs[re.search(r"--invariant=(\w+)", remote).group(1)]
                return subprocess.CompletedProcess(cmd, code, f"{WARNING}\n{out}" if out else "", "")
            return subprocess.CompletedProcess(cmd, 0, "", "")

        config = dict(feed.job.CONFIG, remote_gate={"ssh_host": "oby-gate"})
        with mock.patch.object(feed.subprocess, "run", side_effect=run), mock.patch.object(feed.job, "CONFIG", config):
            return feed.check_models([PATH], HEAD), verifies

    def assert_own_ports(self, verifies):
        """Every `quint verify` carries --server-endpoint right after `verify`, on a port of its own above 8886."""
        ports = []
        for remote in verifies:
            m = re.search(r"\bquint verify --server-endpoint=localhost:(\d+) ", remote)
            self.assertIsNotNone(m, remote)
            ports.append(int(m.group(1)))
        self.assertEqual(len(ports), len(set(ports)), ports)
        self.assertTrue(all(port > 8886 for port in ports), ports)

    def assert_no_verdict(self, got, check, name, main, detail):
        self.assertIsNotNone(got)
        self.assertTrue(got.startswith(f"{PATH}: the {check} {name} (main={main}) gave no verdict: "), got)
        self.assertIn(detail, got)
        self.assertNotIn("does not hold", got)
        self.assertNotIn("was not violated", got)

    def test_feedmodel_1_holding_invariant_and_violated_witness_pass_on_ports_of_their_own(self):
        got, verifies = self.check("// verify: main=oby_factory steps=4 invariants=safe witnesses=neverMerges",
                                   {"safe": HOLDS, "neverMerges": VIOLATED})
        self.assertIsNone(got)
        self.assertEqual(len(verifies), 2)
        self.assert_own_ports(verifies)

    def test_feedmodel_2_misspelt_witness_gives_no_verdict_naming_the_error_line(self):
        out = ("error: [QNT404] Name 'neverMergesByItslf' not found\nerror: name resolution failed\n", 1)
        got, verifies = self.check("// verify: main=oby_factory steps=4 witnesses=neverMergesByItslf",
                                   {"neverMergesByItslf": out})
        self.assert_no_verdict(got, "witness", "neverMergesByItslf", "oby_factory",
                               "error: [QNT404] Name 'neverMergesByItslf' not found")
        self.assertIn("docs/models/oby_factory.qnt", got)
        self.assert_own_ports(verifies)

    def test_feedmodel_2_misspelt_main_gives_no_verdict_on_an_invariant(self):
        got, verifies = self.check("// verify: main=oby_factry steps=4 invariants=safe",
                                   {"safe": ("error: module oby_factry does not exist\n", 1)})
        self.assert_no_verdict(got, "invariant", "safe", "oby_factry", "error: module oby_factry does not exist")
        self.assert_own_ports(verifies)

    def test_feedmodel_3_port_clash_timeout_and_dropped_connection_give_no_verdict(self):
        cases = (
            (("Failed to bind to address 0.0.0.0:8822: Address already in use\n", 1), "Address already in use"),
            (("", 124), "it timed out after 900s"),
            (("", 255), "it printed nothing (exit 255)"),
        )
        for run, detail in cases:
            for kind, check in (("invariants", "invariant"), ("witnesses", "witness")):
                with self.subTest(detail=detail, check=check):
                    got, verifies = self.check(f"// verify: main=oby_factory steps=4 {kind}=broken", {"broken": run})
                    self.assert_no_verdict(got, check, "broken", "oby_factory", detail)
                    self.assert_own_ports(verifies)

    def test_feedmodel_4_holding_witness_and_violated_invariant_keep_todays_words(self):
        got, verifies = self.check("// verify: main=oby_factory steps=4 witnesses=neverFails",
                                   {"neverFails": ("[ok] No violation found (12ms).\n", 0)})
        self.assertEqual(got, f"{PATH}: the witness neverFails was not violated, so the model may prove nothing")
        self.assert_own_ports(verifies)
        got, verifies = self.check("// verify: main=oby_factory steps=4 invariants=safe",
                                   {"safe": ("[violation] Found an issue (12ms).\nerror: found a counterexample\n", 1)})
        self.assertEqual(got, f"{PATH}: the invariant safe does not hold")
        self.assert_own_ports(verifies)

    def test_feedmodel_5_model_name_with_shell_characters_never_reaches_the_vm_and_keeps_the_pr_waiting(self):
        bad = "docs/models/a;touch x.qnt"
        sent = []

        def run(cmd, **_):
            sent.append(cmd)
            return subprocess.CompletedProcess(cmd, 0, "// verify: main=a invariants=safe\n" if cmd[0] == "gh" else HOLDS[0], "")

        item = {"key": "A00-95", "job": "j1", "state": "reviewing", "pr": 7}
        pr = {"state": "OPEN", "headRefOid": HEAD, "files": [{"path": PATH}, {"path": bad}]}
        approvals = [{"commit": HEAD, "verdict": "approved", "agent": tool} for tool in ("claude", "codex")]
        config = dict(feed.job.CONFIG, remote_gate={"ssh_host": "oby-gate"})
        with mock.patch.object(feed.subprocess, "run", side_effect=run), mock.patch.object(feed.job, "CONFIG", config), \
                mock.patch.object(feed, "api", return_value=(200, {"approvals": approvals})), \
                mock.patch.object(feed, "gh_json", return_value=pr), mock.patch.object(feed, "touches_security", return_value=[]):
            self.assertTrue(feed.follow(item, [], "acct"))
        self.assertEqual(item["state"], "waiting")
        self.assertIn(f"{bad} gave no verdict", item["note"])
        self.assertFalse([cmd for cmd in sent if "touch x" in " ".join(cmd) and cmd[0] == "ssh"], sent)
        self.assertFalse([cmd for cmd in sent if cmd[:3] == ["gh", "pr", "merge"]], sent)
        self.assertTrue([cmd for cmd in sent if cmd[0] == "ssh" and VERIFY.search(cmd[-1]) and "oby_factory.qnt" in cmd[-1]], sent)

    def test_feedmodel_5_remote_paths_are_shell_quoted_even_without_the_name_check(self):
        sent = []

        def run(cmd, **_):
            sent.append(cmd)
            return subprocess.CompletedProcess(cmd, 0, "// verify: main=a invariants=safe\n" if cmd[0] == "gh" else HOLDS[0], "")

        config = dict(feed.job.CONFIG, remote_gate={"ssh_host": "oby-gate"})
        with mock.patch.object(feed, "MODEL_NAME", re.compile(r".+")), mock.patch.object(feed.job, "CONFIG", config), \
                mock.patch.object(feed.subprocess, "run", side_effect=run):
            self.assertIsNone(feed.check_models(["docs/models/a;touch x.qnt"], HEAD))
        remote = [cmd[-1] for cmd in sent if cmd[0] == "ssh"]
        self.assertEqual(len(remote), 3, remote)
        for command in remote:
            self.assertIn(f"'/tmp/oby-feed-{HEAD[:12]}-a;touch x.qnt'", command)


if __name__ == "__main__":
    unittest.main()
