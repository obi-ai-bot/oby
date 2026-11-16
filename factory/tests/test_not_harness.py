#!/usr/bin/env python3
"""`job advance <job> spec` refuses an intake.md with no valid LANE: line, or an ACCEPT in the agent lane
(docs/contracts/factory.md, NOTHARNESS-1 to 3; task B00-26).

Run from the repository root: python3 factory/tests/test_not_harness.py
Each test builds a throwaway repository holding a copy of factory/bin/job and the real factory/config.json, cloned
from a bare `origin` it makes with `git init --bare` under its own temporary folder, holding `dev`: nothing reaches the
network. The job is at `intake`, routed (`agents` set), with the intake.md the test writes, and `job advance <job>
spec|rejected` runs as a subprocess with a sanitized git environment and an empty CLAUDE_CONFIG_DIR.
"""

import json
import os
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

FACTORY = Path(__file__).resolve().parents[1]
JOB_ID = "001-b00-26-every-task-is"
BRANCH = f"factory/{JOB_ID}"
HEAD = "DECISION: {decision}\nRISK: medium\nSURFACES: security\nRESCOPED: no\n"
WHY = "\n## Summary\n- It fits.\n\n## Why\n"
NO_LANE = "job: intake.md has no LANE: line (dispatcher, between-owners or agent)"


class NotHarness(unittest.TestCase):
    def setUp(self):
        self.dir = Path(tempfile.mkdtemp()).resolve()
        self.addCleanup(shutil.rmtree, self.dir, True)
        (self.dir / "claude").mkdir()
        self.env = {
            **{k: v for k, v in os.environ.items() if not k.startswith("GIT_")},
            "GIT_CONFIG_GLOBAL": "/dev/null", "GIT_CONFIG_NOSYSTEM": "1",
            "GIT_AUTHOR_NAME": "test", "GIT_AUTHOR_EMAIL": "test@example.com",
            "GIT_COMMITTER_NAME": "test", "GIT_COMMITTER_EMAIL": "test@example.com",
            "CLAUDE_CONFIG_DIR": str(self.dir / "claude"),
        }
        self.make("first")

    def make(self, name):
        """A repository cloned from a bare origin holding `dev`, beside a copy of factory/bin/job, with the job at
        `intake`."""
        self.origin = self.dir / name / "origin.git"
        self.repo = self.dir / name / "repo"
        self.git(self.dir, "init", "-q", "--bare", "-b", "dev", str(self.origin))
        self.git(self.dir, "clone", "-q", str(self.origin), str(self.repo))
        self.git(self.repo, "checkout", "-q", "-b", "dev")
        (self.repo / "README.md").write_text("dev\n")
        self.git(self.repo, "add", "README.md")
        self.git(self.repo, "commit", "-q", "-m", "chore: start")
        self.git(self.repo, "push", "-q", "origin", "dev")
        (self.repo / "factory" / "bin").mkdir(parents=True)
        self.job = self.repo / "factory" / "bin" / "job"
        shutil.copy2(FACTORY / "bin" / "job", self.job)
        shutil.copy2(FACTORY / "config.json", self.repo / "factory" / "config.json")
        self.job_dir = self.repo / "factory" / "jobs" / JOB_ID
        self.job_dir.mkdir(parents=True)
        self.wt = self.repo / ".worktrees" / JOB_ID
        (self.job_dir / "job.json").write_text(json.dumps({
            "id": JOB_ID, "issue": "B00-26", "title": "feat(factory): every task is checked", "milestone": "B00",
            "sprint": 0, "fixes": [], "pr": None, "pr_url": None, "branch": BRANCH, "worktree": str(self.wt),
            "stage": "intake", "round": 0, "round_limit": 3, "intake": None,
            "agents": {"reviewers": {"code": "sonnet", "security": "opus"}}, "gate": None, "reviews": {},
            "cost_usd": 0, "started": "2026-01-01T00:00:00Z", "ended": None,
        }, indent=2) + "\n")

    def git(self, cwd, *args):
        return subprocess.run(["git", *args], cwd=cwd, env=self.env, capture_output=True, text=True,
                              check=True).stdout

    def intake(self, text):
        (self.job_dir / "intake.md").write_text(text)

    def advance(self, stage):
        return subprocess.run([str(self.job), "advance", JOB_ID, stage], cwd=self.repo, env=self.env,
                              capture_output=True, text=True, timeout=60)

    def saved(self):
        return json.loads((self.job_dir / "job.json").read_text())

    def has_branch(self):
        return subprocess.run(["git", "rev-parse", "--verify", "--quiet", f"refs/heads/{BRANCH}"], cwd=self.repo,
                              env=self.env, capture_output=True).returncode == 0

    def assert_refused(self, said):
        """`advance <job> spec` exits 1 with `said` on stderr, job.json byte-identical, and no worktree or branch."""
        before = (self.job_dir / "job.json").read_bytes()
        res = self.advance("spec")
        self.assertEqual(res.returncode, 1, res.stdout + res.stderr)
        self.assertEqual(res.stderr.strip(), said)
        self.assertEqual((self.job_dir / "job.json").read_bytes(), before)
        self.assertEqual(self.saved()["stage"], "intake")
        self.assertFalse(self.wt.exists())
        self.assertFalse(self.has_branch())
        return res

    # NOTHARNESS-1: an intake.md with no valid LANE: line never reaches spec
    def test_notharness_1_advance_to_spec_refuses_an_intake_with_no_valid_lane(self):
        accept = HEAD.format(decision="ACCEPT")
        cases = {"no LANE line": (accept + WHY + "It is the factory's own.", NO_LANE),
                 "LANE only below the blank line": (accept + WHY + "LANE: dispatcher\nIt is the factory's own.",
                                                    NO_LANE),
                 "an empty LANE": (accept + "LANE:\n" + WHY + "It is the factory's own.", NO_LANE),
                 "LANE: harness": (accept + "LANE: harness\n" + WHY + "It is the factory's own.",
                                   "job: intake.md says LANE: harness, not dispatcher, between-owners or agent")}
        for case, (text, said) in cases.items():
            with self.subTest(case=case):
                self.intake(text)
                self.assert_refused(said)

    # NOTHARNESS-2: an ACCEPT in the agent lane never reaches spec; a REJECT in it is rejected as before
    def test_notharness_2_an_accept_in_the_agent_lane_never_reaches_spec(self):
        self.intake(HEAD.format(decision="ACCEPT") + "LANE: agent\n" + WHY + "It is the CLI's own.")
        res = self.assert_refused("job: intake.md accepts work in the agent lane: the agent CLIs' own work is REJECT "
                                  "(AGENTS.md, \"Dispatcher, not harness\")")
        self.assertIn("accepts work in the agent lane", res.stderr)

        self.intake(HEAD.format(decision="REJECT") + "LANE: agent\n" + WHY + "It is the CLI's own.")
        res = self.advance("rejected")
        self.assertEqual(res.returncode, 0, res.stdout + res.stderr)
        self.assertEqual(self.saved()["stage"], "rejected")
        self.assertEqual(self.saved()["intake"]["lane"], "agent")

    # NOTHARNESS-3: a valid lane reaches spec, with its worktree, and job.json records it
    def test_notharness_3_a_valid_lane_reaches_spec_and_is_recorded(self):
        for lane in ("dispatcher", "between-owners"):
            with self.subTest(lane=lane):
                self.make(lane)
                self.intake(HEAD.format(decision="ACCEPT") + f"LANE: {lane}\n" + WHY + "It is the dispatcher's.")
                res = self.advance("spec")
                self.assertEqual(res.returncode, 0, res.stdout + res.stderr)
                self.assertEqual(res.stdout.strip(), f"{JOB_ID}: spec (round 0)")
                job = self.saved()
                self.assertEqual(job["stage"], "spec")
                self.assertEqual(job["intake"], {"decision": "ACCEPT", "rescoped": False, "risk": "medium",
                                                 "surfaces": ["security"], "lane": lane})
                self.assertTrue(self.wt.is_dir())
                self.assertEqual(self.git(self.wt, "branch", "--show-current").strip(), BRANCH)
                self.assertEqual(self.git(self.wt, "rev-parse", "HEAD"),
                                 self.git(self.dir, "--git-dir", str(self.origin), "rev-parse", "dev"))


if __name__ == "__main__":
    unittest.main(verbosity=2)
