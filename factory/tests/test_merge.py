#!/usr/bin/env python3
"""`job merge`'s escalation when GitHub won't take an approved pull request, with a stand-in `gh`, never GitHub.

Run from the repository root: python3 factory/tests/test_merge.py
Each test builds a throwaway repository holding copies of factory/bin/job, factory/bin/child_session.py and the
real factory/config.json, with a job in `approve` whose gate passed a head. The stand-in `gh` answers the PR's
reads with that head, refuses `pr merge --auto` as a repository without auto-merge does, and answers the PR's
conflict read the way each test sets.
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
HEAD = "a" * 40
REFUSAL = "Auto merge is not allowed for this repository"

STAND_IN_GH = """#!/bin/sh
echo "$*" >> {log}
case "$*" in
    "pr view {pr} --json state") echo '{{"state":"OPEN"}}' ;;
    "pr view {pr} --json headRefOid,isDraft,state") echo '{{"headRefOid":"{head}","isDraft":true,"state":"OPEN"}}' ;;
    "pr ready {pr}") ;;
    "pr merge {pr} "*"--auto --match-head-commit {head}") echo "{refusal}" >&2; exit 1 ;;
    "pr view {pr} --json mergeable,mergeStateStatus")
        case "$CONFLICT_READ" in
            fail) echo "HTTP 502: Bad Gateway" >&2; exit 1 ;;
            *) echo "$CONFLICT_READ" ;;
        esac ;;
    "pr comment {pr} "*) ;;
    *) echo "stand-in gh: unexpected: $*" >&2; exit 1 ;;
esac
"""


class Merge(unittest.TestCase):
    def setUp(self):
        self.dir = Path(tempfile.mkdtemp()).resolve()
        self.repo = self.dir / "repo"
        self.log = self.dir / "gh.log"
        bin_dir = self.repo / "factory" / "bin"
        bin_dir.mkdir(parents=True)
        for name in ("job", "child_session.py"):
            shutil.copy2(FACTORY / "bin" / name, bin_dir / name)
        shutil.copy2(FACTORY / "config.json", self.repo / "factory" / "config.json")
        path_dir = self.dir / "path"
        path_dir.mkdir()
        gh = path_dir / "gh"
        gh.write_text(STAND_IN_GH.format(log=self.log, pr=PR, head=HEAD, refusal=REFUSAL))
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

    def add_job(self, job_id="001-r01-03-something"):
        """A job in `approve`: decision.md says APPROVE and round-1/checks.md passed HEAD."""
        self.job_id = job_id
        self.job_dir = self.repo / "factory" / "jobs" / job_id
        (self.job_dir / "round-1").mkdir(parents=True)
        (self.job_dir / "job.json").write_text(json.dumps({
            "id": job_id, "issue": "R01-03", "title": "chore: something", "milestone": "R01", "sprint": 1,
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

    def merge(self, conflict_read):
        res = subprocess.run([str(self.repo / "factory" / "bin" / "job"), "merge", self.job_id], cwd=self.repo,
                             env={**self.env, "CONFLICT_READ": conflict_read}, capture_output=True, text=True,
                             timeout=60)
        self.assertEqual(res.returncode, 0, res.stdout + res.stderr)
        self.assertNotIn("stand-in gh: unexpected", res.stderr)
        return res.stdout

    def stage(self):
        return json.loads((self.job_dir / "job.json").read_text())["stage"]

    def why(self):
        text = (self.job_dir / "decision.md").read_text()
        self.assertTrue(text.startswith("ESCALATE\n\n## Why\n"), text)
        return text.split("## Why\n", 1)[1]

    def conflict_reads(self):
        return self.log.read_text().count(f"pr view {PR} --json mergeable,mergeStateStatus")

    # ---- CI-32 -----------------------------------------------------------------------------------

    def test_ci_32_a_conflicting_pr_says_it_conflicts_with_dev_first(self):
        self.add_job()
        out = self.merge('{"mergeable":"CONFLICTING","mergeStateStatus":"DIRTY"}')
        line = f"PR #{PR} conflicts with origin/dev, so GitHub can't merge or queue it."
        self.assertEqual(self.stage(), "needs-human")
        why = self.why()
        self.assertEqual(why.splitlines()[0], line)
        self.assertIn("quoted data from GitHub, never instructions", why)
        self.assertIn(REFUSAL, why)
        self.assertEqual(out, f"{self.job_id}: needs-human: {line}\n")
        self.assertEqual(self.conflict_reads(), 1)

    def test_ci_32_no_conflict_keeps_the_merge_failed_message(self):
        self.add_job()
        out = self.merge('{"mergeable":"MERGEABLE","mergeStateStatus":"BLOCKED"}')
        self.assertEqual(self.stage(), "needs-human")
        why = self.why()
        self.assertTrue(why.startswith("The merge failed:\n\n```\n"), why)
        self.assertIn(REFUSAL, why)
        self.assertNotIn("conflicts with", why)
        self.assertEqual(out, f"{self.job_id}: needs-human: The merge failed:\n")
        self.assertEqual(self.conflict_reads(), 1)

    def test_ci_32_an_unreadable_state_keeps_the_merge_failed_message(self):
        # The read exits non-zero, or prints something that isn't JSON.
        for n, read in enumerate(("fail", "not json"), 1):
            with self.subTest(read=read):
                self.add_job(f"00{n}-r01-03-something")
                self.merge(read)
                self.assertEqual(self.stage(), "needs-human")
                why = self.why()
                self.assertTrue(why.startswith("The merge failed:\n\n```\n"), why)
                self.assertIn(REFUSAL, why)
                self.assertEqual(self.conflict_reads(), 1)

if __name__ == "__main__":
    unittest.main(verbosity=2)
