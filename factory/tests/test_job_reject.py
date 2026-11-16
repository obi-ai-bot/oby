#!/usr/bin/env python3
"""`job reject` at any stage but queued, with a stand-in `gh`, never GitHub (docs/contracts/factory.md, JOBREJECT).

Run from the repository root: python3 factory/tests/test_job_reject.py
Each test builds a throwaway repository holding copies of factory/bin/job, factory/bin/child_session.py and the
real factory/config.json, with one job in the stage the test sets and its task file in factory/queue/tasks. The
stand-in `gh` logs every call, answers the PR read (`api graphql`) from a JSON file each test writes, and fails any
call it doesn't expect. The repository has no `origin`, so the branch delete fails locally, under check=False.
"""

import json
import os
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

FACTORY = Path(__file__).resolve().parents[1]
SPRINT = json.loads((FACTORY / "config.json").read_text())["sprint"]
PR = 7
KEY = "A00-16"
REASON = "The owner wants this done another way."

STAND_IN_GH = """#!/bin/sh
printf '%s\\0' "$*" >> {log}
case "$*" in
    "api user --jq .login") echo owner ;;
    "api graphql "*)
        if [ -n "$READ_FAILS" ]; then echo "HTTP 502: Bad Gateway" >&2; exit 1; fi
        cat {graphql} ;;
    "pr comment {pr} "*) ;;
    "pr close {pr}") ;;
    "pr merge {pr} --disable-auto") ;;
    *) echo "stand-in gh: unexpected: $*" >&2; exit 1 ;;
esac
"""


def read(state="OPEN", queued=False, armed=False):
    """PR 7 as queue_state reads it."""
    return {"data": {"repository": {"pullRequest": {
        "state": state, "isDraft": False, "headRefOid": "b" * 40, "mergeStateStatus": "BLOCKED",
        "isInMergeQueue": queued, "autoMergeRequest": {"enabledAt": "2026-01-01T00:00:00Z"} if armed else None,
        "timelineItems": {"nodes": []},
    }}}}


class JobReject(unittest.TestCase):
    def setUp(self):
        self.dir = Path(tempfile.mkdtemp()).resolve()
        self.repo = self.dir / "repo"
        self.log = self.dir / "gh.log"
        bin_dir = self.repo / "factory" / "bin"
        bin_dir.mkdir(parents=True)
        for name in ("job", "child_session.py"):
            shutil.copy2(FACTORY / "bin" / name, bin_dir / name)
        shutil.copy2(FACTORY / "config.json", self.repo / "factory" / "config.json")
        self.task = self.repo / "factory" / "queue" / "tasks" / f"{KEY}.md"
        self.task.parent.mkdir(parents=True)
        self.task.write_text(f"KEY: {KEY}\nTITLE: fix: something\nSPRINT: {SPRINT}\n\nDo something.\n")
        self.graphql = self.dir / "graphql.json"
        path_dir = self.dir / "path"
        path_dir.mkdir()
        gh = path_dir / "gh"
        gh.write_text(STAND_IN_GH.format(log=self.log, pr=PR, graphql=self.graphql))
        gh.chmod(0o755)
        (self.dir / "claude").mkdir()
        self.env = {
            **{k: v for k, v in os.environ.items() if not k.startswith("GIT_")},
            "GIT_CONFIG_GLOBAL": "/dev/null", "GIT_CONFIG_NOSYSTEM": "1",
            "GIT_AUTHOR_NAME": "test", "GIT_AUTHOR_EMAIL": "test@example.com",
            "GIT_COMMITTER_NAME": "test", "GIT_COMMITTER_EMAIL": "test@example.com",
            "CLAUDE_CONFIG_DIR": str(self.dir / "claude"),
            "CARGO_TARGET_DIR_FACTORY": str(self.dir / "no-shared-cargo"),
            "PATH": f"{path_dir}{os.pathsep}{os.environ['PATH']}",
        }
        subprocess.run(["git", "init", "-q", "-b", "dev", str(self.repo)], env=self.env, check=True)

    def tearDown(self):
        shutil.rmtree(self.dir)

    def add_job(self, stage, pr=PR, n=1, **pr_read):
        """The one job for the task, in `stage`, with PR 7 (or none), whose PR read says what `pr_read` sets."""
        shutil.rmtree(self.repo / "factory" / "jobs", ignore_errors=True)
        self.job_id = f"00{n}-a00-16-something"
        self.job_dir = self.repo / "factory" / "jobs" / self.job_id
        (self.job_dir / "round-1").mkdir(parents=True)
        (self.job_dir / "job.json").write_text(json.dumps({
            "id": self.job_id, "issue": KEY, "title": "fix: something", "milestone": "A00", "sprint": 0,
            "fixes": [], "pr": pr, "pr_url": f"https://github.com/o/r/pull/{pr}" if pr else None,
            "branch": f"factory/{self.job_id}", "worktree": str(self.repo / ".worktrees" / self.job_id),
            "stage": stage, "round": 0 if stage in {"intake", "needs-info", "spec"} else 1, "round_limit": 3,
            "intake": None, "agents": {"reviewers": {"code": "sonnet"}}, "gate": None, "reviews": {},
            "cost_usd": 0, "started": "2026-01-01T00:00:00Z", "ended": None,
        }, indent=2) + "\n")
        (self.job_dir / "decision.md").write_text("ESCALATE\n\n## Why\nA person decides.\n")
        self.graphql.write_text(json.dumps(read(**pr_read)))
        self.log.unlink(missing_ok=True)
        self.before = (self.job_dir / "job.json").read_bytes()

    def job(self, *args, stdin=REASON, **env):
        return subprocess.run([str(self.repo / "factory" / "bin" / "job"), *args], cwd=self.repo,
                              env={**self.env, **env}, input=stdin, capture_output=True, text=True, timeout=60)

    def reject(self, stdin=REASON, **env):
        return self.job("reject", KEY, stdin=stdin, **env)

    def saved(self):
        return json.loads((self.job_dir / "job.json").read_text())

    def calls(self):
        return [c for c in self.log.read_text().split("\0") if c] if self.log.exists() else []

    def pr_calls(self, verb=None):
        return [c for c in self.calls() if c.startswith(f"pr {verb} " if verb else "pr ")]

    def assert_rejected(self, res, stage):
        self.assertEqual(res.returncode, 0, res.stdout + res.stderr)
        self.assertNotIn("stand-in gh: unexpected", res.stderr)
        rejection = (self.job_dir / "rejection.md").read_text()
        self.assertTrue(rejection.startswith("REJECTED\n\nBy @"), rejection)
        self.assertIn(f"while the job was {stage}.", rejection)
        self.assertIn(REASON, rejection)
        job = self.saved()
        self.assertEqual(job["stage"], "rejected")
        self.assertIsNotNone(job["ended"])
        self.assertTrue(self.task.exists(), "the task file stays in factory/queue/tasks")

    def assert_refused(self, res, *words):
        self.assertEqual(res.returncode, 1, res.stdout + res.stderr)
        self.assertTrue(res.stderr.startswith("job: "), res.stderr)
        for w in words:
            self.assertIn(w, res.stderr)
        self.assertEqual((self.job_dir / "job.json").read_bytes(), self.before)
        for verb in ("comment", "close", "merge"):
            self.assertEqual(self.pr_calls(verb), [], self.calls())

    # ---- JOBREJECT-1 -----------------------------------------------------------------------------

    def test_jobreject_1_a_running_job_with_an_open_pr_is_rejected(self):
        for n, stage in enumerate(("build", "gate", "prove", "review", "approve"), 1):
            with self.subTest(stage=stage):
                self.add_job(stage, n=n)
                res = self.reject()
                self.assert_rejected(res, stage)
                self.assertEqual(self.pr_calls("close"), [f"pr close {PR}"])
                comments = self.pr_calls("comment")
                self.assertEqual(len(comments), 1, comments)
                self.assertIn("### Factory: rejected", comments[0])
                self.assertIn(REASON, comments[0])
                self.assertEqual(self.pr_calls("merge"), [])
                self.assertEqual(len([c for c in self.calls() if c.startswith("api graphql ")]), 1)

    # ---- JOBREJECT-2 -----------------------------------------------------------------------------

    def test_jobreject_2_a_job_with_no_pr_makes_no_pr_call(self):
        for n, (stage, intake) in enumerate((("intake", None), ("intake", "ACCEPT"), ("spec", None)), 1):
            with self.subTest(stage=stage, intake=intake):
                self.add_job(stage, pr=None, n=n)
                if intake:
                    (self.job_dir / "intake.md").write_text(f"DECISION: {intake}\nLANE: dispatcher\n\n## Why\n")
                res = self.reject()
                self.assert_rejected(res, stage)
                self.assertEqual(self.pr_calls(), [], self.calls())

    def test_jobreject_2_armed_auto_merge_is_turned_off_before_the_close(self):
        self.add_job("merging", armed=True)
        res = self.reject()
        self.assert_rejected(res, "merging")
        calls = self.pr_calls()
        disable, close = f"pr merge {PR} --disable-auto", f"pr close {PR}"
        self.assertIn(disable, calls)
        self.assertEqual(calls.count(close), 1, calls)
        self.assertLess(calls.index(disable), calls.index(close))

    def test_jobreject_2_a_pr_closed_by_hand_gets_the_comment_and_no_second_close(self):
        self.add_job("needs-human", state="CLOSED")
        res = self.reject()
        self.assert_rejected(res, "needs-human")
        self.assertEqual(len(self.pr_calls("comment")), 1, self.calls())
        self.assertEqual(self.pr_calls("close"), [])

    # ---- JOBREJECT-3 -----------------------------------------------------------------------------

    def test_jobreject_3_a_pr_in_the_merge_queue_is_refused(self):
        for n, (stage, queued) in enumerate((("queued", False), ("merging", True)), 1):
            with self.subTest(stage=stage):
                self.add_job(stage, n=n, queued=queued, armed=queued)
                res = self.reject()
                self.assert_refused(res, "GitHub's merge queue", f"PR #{PR}")
                self.assertFalse((self.job_dir / "rejection.md").exists())

    # ---- JOBREJECT-4 -----------------------------------------------------------------------------

    def test_jobreject_4_a_finished_job_is_refused_and_unchanged(self):
        self.add_job("merged", n=1)
        self.assert_refused(self.reject(), "already finished")
        self.assertFalse((self.job_dir / "rejection.md").exists())
        self.add_job("rejected", n=2)
        rejection = b"REJECTED\n\nBy @owner while the job was needs-human.\n\nAn earlier reason.\n"
        (self.job_dir / "rejection.md").write_bytes(rejection)
        self.assert_refused(self.reject(), "already finished")
        self.assertEqual((self.job_dir / "rejection.md").read_bytes(), rejection)

    def test_jobreject_4_an_empty_reason_is_refused(self):
        self.add_job("build")
        self.assert_refused(self.reject(stdin="  \n"), "no reason on stdin")
        self.assertFalse((self.job_dir / "rejection.md").exists())

    def test_jobreject_4_a_merged_or_unreadable_pr_is_refused(self):
        self.add_job("needs-human", n=1, state="MERGED")
        self.assert_refused(self.reject(), "already merged", "job queue")
        self.assertFalse((self.job_dir / "rejection.md").exists())
        self.add_job("review", n=2)
        self.assert_refused(self.reject(READ_FAILS="1"), f"couldn't read PR #{PR}", "HTTP 502")
        self.assertFalse((self.job_dir / "rejection.md").exists())

    # ---- JOBREJECT-5 -----------------------------------------------------------------------------

    def test_jobreject_5_advance_keeps_todays_rules(self):
        self.add_job("build")
        res = self.job("advance", self.job_id, "rejected")
        self.assertEqual(res.returncode, 1, res.stdout + res.stderr)
        self.assertIn("build → rejected is not a legal transition", res.stderr)
        self.assertEqual((self.job_dir / "job.json").read_bytes(), self.before)
        (self.job_dir / "rejection.md").write_text(f"REJECTED\n\nBy @owner while the job was build.\n\n{REASON}\n")
        res = self.job("advance", self.job_id, "rejected")
        self.assertEqual(res.returncode, 1, res.stdout + res.stderr)
        self.assertIn("build → rejected is not a legal transition", res.stderr)
        self.assertEqual((self.job_dir / "job.json").read_bytes(), self.before)
        self.assertEqual(self.pr_calls(), [], self.calls())

    def test_jobreject_5_advance_from_intake_still_needs_intakes_reject(self):
        self.add_job("intake", pr=None)
        (self.job_dir / "intake.md").write_text("DECISION: ACCEPT\nLANE: dispatcher\n\n## Why\n")
        res = self.job("advance", self.job_id, "rejected")
        self.assertEqual(res.returncode, 1, res.stdout + res.stderr)
        self.assertIn("intake.md says DECISION: ACCEPT, not REJECT", res.stderr)
        self.assertEqual((self.job_dir / "job.json").read_bytes(), self.before)

    def test_jobreject_5_job_queue_skips_a_job_rejected_from_build(self):
        self.add_job("build")
        self.assert_rejected(self.reject(), "build")
        res = self.job("queue", stdin="")
        self.assertEqual(res.returncode, 0, res.stdout + res.stderr)
        rows = [line.split("\t") for line in res.stdout.splitlines() if f"\t{KEY}\t" in line]
        self.assertTrue(rows, res.stdout)
        self.assertEqual(rows[0][0], "skip", rows)
        self.assertNotIn("resume", [r[0] for r in rows])


if __name__ == "__main__":
    unittest.main(verbosity=2)
