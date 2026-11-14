#!/usr/bin/env python3
"""`job merge` on a base with GitHub's merge queue, with a stand-in `gh`, never GitHub.

Run from the repository root: python3 factory/tests/test_merge_queue.py
Each test builds a throwaway repository holding copies of factory/bin/job, factory/bin/child_session.py and the
real factory/config.json, with a job in `approve` whose gate passed a head. The stand-in `gh` logs every call,
answers the base's rules read with or without a `merge_queue` rule (or fails it), answers `pr merge` the way
each test sets, and reads the PR back as queued at that head.
"""

import json
import os
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

FACTORY = Path(__file__).resolve().parents[1]
PR = 7
HEAD = "b" * 40
QUEUE_REFUSAL = "The merge strategy for dev is set by the merge queue"
OTHER_REFUSAL = "Pull request is not mergeable: the base branch policy prohibits the merge"

STAND_IN_GH = """#!/bin/sh
echo "$*" >> {log}
case "$*" in
    "api repos/{{owner}}/{{repo}}/rules/branches/dev")
        case "$RULES" in
            queue) echo '[{{"type":"pull_request"}},{{"type":"merge_queue","parameters":{{}}}}]' ;;
            none) echo '[{{"type":"pull_request"}},{{"type":"required_status_checks"}}]' ;;
            *) echo "HTTP 403: Resource not accessible by integration" >&2; exit 1 ;;
        esac ;;
    "pr view {pr} --json state") echo '{{"state":"OPEN"}}' ;;
    "pr view {pr} --json headRefOid,isDraft,state") echo '{{"headRefOid":"{head}","isDraft":true,"state":"OPEN"}}' ;;
    "pr ready {pr}") ;;
    "pr merge {pr} "*)
        case "$MERGE" in
            pass) ;;
            squash-refused)
                case "$*" in
                    *--squash*) echo "{queue_refusal}" >&2; exit 1 ;;
                esac ;;
            queue-refused) echo "{queue_refusal}" >&2; exit 1 ;;
            *) echo "{other_refusal}" >&2; exit 1 ;;
        esac ;;
    "pr view {pr} --json mergeable,mergeStateStatus") echo '{{"mergeable":"MERGEABLE","mergeStateStatus":"BLOCKED"}}' ;;
    "api graphql "*) cat {graphql} ;;
    "pr comment {pr} "*) ;;
    *) echo "stand-in gh: unexpected: $*" >&2; exit 1 ;;
esac
"""

QUEUED = {"data": {"repository": {"pullRequest": {
    "state": "OPEN", "isDraft": False, "headRefOid": HEAD, "mergeStateStatus": "BLOCKED", "isInMergeQueue": True,
    "autoMergeRequest": {"enabledAt": "2026-01-01T00:00:00Z"}, "timelineItems": {"nodes": []},
}}}}


class MergeQueue(unittest.TestCase):
    def setUp(self):
        self.dir = Path(tempfile.mkdtemp()).resolve()
        self.repo = self.dir / "repo"
        self.log = self.dir / "gh.log"
        bin_dir = self.repo / "factory" / "bin"
        bin_dir.mkdir(parents=True)
        for name in ("job", "child_session.py"):
            shutil.copy2(FACTORY / "bin" / name, bin_dir / name)
        shutil.copy2(FACTORY / "config.json", self.repo / "factory" / "config.json")
        graphql = self.dir / "graphql.json"
        graphql.write_text(json.dumps(QUEUED))
        path_dir = self.dir / "path"
        path_dir.mkdir()
        gh = path_dir / "gh"
        gh.write_text(STAND_IN_GH.format(log=self.log, pr=PR, head=HEAD, graphql=graphql,
                                         queue_refusal=QUEUE_REFUSAL, other_refusal=OTHER_REFUSAL))
        gh.chmod(0o755)
        (self.dir / "claude").mkdir()
        self.env = {
            **os.environ,
            "GIT_CONFIG_GLOBAL": "/dev/null", "GIT_CONFIG_NOSYSTEM": "1",
            "GIT_AUTHOR_NAME": "test", "GIT_AUTHOR_EMAIL": "test@example.com",
            "GIT_COMMITTER_NAME": "test", "GIT_COMMITTER_EMAIL": "test@example.com",
            "CLAUDE_CONFIG_DIR": str(self.dir / "claude"),
            "PATH": f"{path_dir}{os.pathsep}{os.environ['PATH']}",
        }
        subprocess.run(["git", "init", "-q", "-b", "dev", str(self.repo)], env=self.env, check=True)

    def add_job(self, job_id="001-a00-16-something"):
        """A job in `approve`: decision.md says APPROVE and round-1/checks.md passed HEAD."""
        self.job_id = job_id
        self.job_dir = self.repo / "factory" / "jobs" / job_id
        (self.job_dir / "round-1").mkdir(parents=True)
        (self.job_dir / "job.json").write_text(json.dumps({
            "id": job_id, "issue": "A00-16", "title": "fix: something", "milestone": "A00", "sprint": 0,
            "fixes": [], "pr": PR, "pr_url": f"https://github.com/o/r/pull/{PR}",
            "branch": f"factory/{job_id}", "worktree": str(self.repo / ".worktrees" / job_id),
            "stage": "approve", "round": 1, "round_limit": 3, "intake": None,
            "agents": {"reviewers": {"code": "sonnet"}}, "gate": "PASS", "reviews": {"code": "PASS"},
            "cost_usd": 0, "started": "2026-01-01T00:00:00Z", "ended": None,
        }, indent=2) + "\n")
        (self.job_dir / "decision.md").write_text("APPROVE\n\n## Why\nIt does what the spec says.\n")
        (self.job_dir / "round-1" / "checks.md").write_text(f"GATE: PASS\nCOMMIT: {HEAD}\n\n## Summary\n")
        (self.job_dir / "round-1" / "review-code.md").write_text("VERDICT: PASS\n")
        self.log.unlink(missing_ok=True)

    def tearDown(self):
        shutil.rmtree(self.dir)

    def merge(self, rules, merge):
        res = subprocess.run([str(self.repo / "factory" / "bin" / "job"), "merge", self.job_id], cwd=self.repo,
                             env={**self.env, "RULES": rules, "MERGE": merge}, capture_output=True, text=True,
                             timeout=60)
        self.assertEqual(res.returncode, 0, res.stdout + res.stderr)
        self.assertNotIn("stand-in gh: unexpected", res.stderr)
        return res.stdout

    def stage(self):
        return json.loads((self.job_dir / "job.json").read_text())["stage"]

    def merge_calls(self):
        return [line.split() for line in self.log.read_text().splitlines() if line.startswith(f"pr merge {PR} ")]

    def assert_pinned(self, call):
        self.assertEqual(call[-3:], ["--auto", "--match-head-commit", HEAD])

    def assert_no_method(self, call):
        for flag in ("--squash", "--merge", "--rebase"):
            self.assertNotIn(flag, call)

    # ---- CI-23 -----------------------------------------------------------------------------------

    def test_ci_23_a_base_with_a_merge_queue_gets_no_merge_method(self):
        self.add_job()
        self.merge("queue", "squash-refused")
        calls = self.merge_calls()
        self.assertEqual(len(calls), 1, calls)
        self.assertEqual(calls[0], ["pr", "merge", str(PR), "--auto", "--match-head-commit", HEAD])
        self.assert_no_method(calls[0])
        self.assertEqual(self.stage(), "queued")

    def test_ci_23_a_base_without_a_merge_queue_keeps_the_merge_method(self):
        self.add_job()
        self.merge("none", "pass")
        calls = self.merge_calls()
        self.assertEqual(calls, [["pr", "merge", str(PR), "--squash", "--auto", "--match-head-commit", HEAD]])
        self.assertEqual(self.stage(), "queued")

    def test_ci_23_a_merge_queue_refusal_is_retried_once_without_the_flag(self):
        for n, rules in enumerate(("none", "fail"), 1):
            with self.subTest(rules=rules):
                self.add_job(f"00{n}-a00-16-something")
                self.merge(rules, "squash-refused")
                calls = self.merge_calls()
                self.assertEqual(len(calls), 2, calls)
                self.assertIn("--squash", calls[0])
                self.assert_no_method(calls[1])
                for call in calls:
                    self.assert_pinned(call)
                self.assertEqual(self.stage(), "queued")
                self.assertTrue((self.job_dir / "decision.md").read_text().startswith("APPROVE\n"))

    def test_ci_23_a_second_merge_queue_refusal_escalates(self):
        self.add_job()
        out = self.merge("none", "queue-refused")
        calls = self.merge_calls()
        self.assertEqual(len(calls), 2, calls)
        self.assertIn("--squash", calls[0])
        self.assert_no_method(calls[1])
        self.assertEqual(self.stage(), "needs-human")
        self.assertEqual(out, f"{self.job_id}: needs-human: The merge failed:\n")
        why = (self.job_dir / "decision.md").read_text()
        self.assertTrue(why.startswith("ESCALATE\n\n## Why\nThe merge failed:\n"), why)
        self.assertIn(QUEUE_REFUSAL, why)

    def test_ci_23_another_refusal_is_not_retried(self):
        self.add_job()
        out = self.merge("none", "other")
        calls = self.merge_calls()
        self.assertEqual(len(calls), 1, calls)
        self.assertIn("--squash", calls[0])
        self.assertEqual(self.stage(), "needs-human")
        self.assertEqual(out, f"{self.job_id}: needs-human: The merge failed:\n")
        self.assertIn(OTHER_REFUSAL, (self.job_dir / "decision.md").read_text())


if __name__ == "__main__":
    unittest.main(verbosity=2)
