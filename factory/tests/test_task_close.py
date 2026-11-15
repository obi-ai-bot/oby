#!/usr/bin/env python3
"""`job advance <job> gate` closes again a task that merging dev brought back (A00-99).

Run from the repository root: python3 factory/tests/test_task_close.py
Each test builds a throwaway bare `origin` and a job worktree cloned from it, beside a copy of factory/bin/job and
the real factory/config.json. The job is in `build` with a PR already, so the advance calls no `gh`. dev adds
factory/queue/tasks/A00-16.md after the branch began, and the branch merges dev, as a builder does.
"""

import json
import os
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

FACTORY = Path(__file__).resolve().parents[1]
JOB_ID = "001-a00-16-something"
KEY = "A00-16"
TODO = f"factory/queue/tasks/{KEY}.md"
DONE = f"factory/queue/done/{KEY}.md"
TASK = f"KEY: {KEY}\nTITLE: fix: something\nDEPENDS: none\n\nThe task as the orchestrator queued it.\n"
EARLIER = f"KEY: {KEY}\nTITLE: fix: something\nDEPENDS: none\n\nThe task as the builder copied it.\n"


class TaskClose(unittest.TestCase):
    def setUp(self):
        self.dir = Path(tempfile.mkdtemp()).resolve()
        self.repo = self.dir / "repo"
        (self.repo / "factory" / "bin").mkdir(parents=True)
        self.job = self.repo / "factory" / "bin" / "job"
        shutil.copy2(FACTORY / "bin" / "job", self.job)
        shutil.copy2(FACTORY / "config.json", self.repo / "factory" / "config.json")
        self.wt = self.dir / "worktree"
        self.branch = f"factory/{JOB_ID}"
        self.job_dir = self.repo / "factory" / "jobs" / JOB_ID
        (self.job_dir / "round-1").mkdir(parents=True)
        (self.job_dir / "job.json").write_text(json.dumps({
            "id": JOB_ID, "issue": KEY, "title": "fix: something", "milestone": "A00", "sprint": 0,
            "fixes": [], "pr": 7, "pr_url": "https://github.com/o/r/pull/7", "branch": self.branch,
            "worktree": str(self.wt), "stage": "build", "round": 1, "round_limit": 3,
            "intake": None, "agents": None, "gate": None, "reviews": {},
            "cost_usd": 0, "started": "2026-01-01T00:00:00Z", "ended": None,
        }, indent=2) + "\n")
        (self.job_dir / "build.md").write_text("STATUS: READY\n")
        (self.dir / "claude").mkdir()
        self.env = {
            **{k: v for k, v in os.environ.items() if not k.startswith("GIT_")},
            "GIT_CONFIG_GLOBAL": "/dev/null", "GIT_CONFIG_NOSYSTEM": "1",
            "GIT_AUTHOR_NAME": "test", "GIT_AUTHOR_EMAIL": "test@example.com",
            "GIT_COMMITTER_NAME": "test", "GIT_COMMITTER_EMAIL": "test@example.com",
            "CLAUDE_CONFIG_DIR": str(self.dir / "claude"),
        }
        origin = self.dir / "origin.git"
        self.git(self.dir, "init", "-q", "--bare", "-b", "dev", str(origin))
        self.git(self.dir, "clone", "-q", str(origin), str(self.wt))
        self.git(self.wt, "checkout", "-q", "-b", "dev")
        self.write("README.md", "dev\n")
        self.commit("chore: start")
        self.git(self.wt, "push", "-q", "origin", "dev")
        self.git(self.wt, "checkout", "-q", "-b", self.branch)

    def tearDown(self):
        shutil.rmtree(self.dir)

    def git(self, cwd, *args):
        return subprocess.run(["git", *args], cwd=cwd, env=self.env, capture_output=True, text=True,
                              check=True).stdout

    def write(self, path, text):
        (self.wt / path).parent.mkdir(parents=True, exist_ok=True)
        (self.wt / path).write_text(text)

    def commit(self, subject):
        self.git(self.wt, "add", "-A")
        self.git(self.wt, "commit", "-q", "-m", subject)

    def dev_queues_the_task_and_the_branch_merges_it(self):
        """dev adds tasks/<KEY>.md after the branch began; the branch merges dev and pushes."""
        self.git(self.wt, "push", "-q", "origin", self.branch)
        self.git(self.wt, "checkout", "-q", "dev")
        self.write(TODO, TASK)
        self.commit(f"chore(factory): queue {KEY}")
        self.git(self.wt, "push", "-q", "origin", "dev")
        self.git(self.wt, "checkout", "-q", self.branch)
        self.git(self.wt, "fetch", "-q", "origin")
        self.git(self.wt, "merge", "-q", "--no-edit", "origin/dev")

    def push(self):
        self.git(self.wt, "push", "-q", "origin", self.branch)

    def advance(self):
        return subprocess.run([str(self.job), "advance", JOB_ID, "gate"], cwd=self.repo, env=self.env,
                              capture_output=True, text=True, timeout=60)

    def in_head(self, path):
        return subprocess.run(["git", "cat-file", "-e", f"HEAD:{path}"], cwd=self.wt, env=self.env).returncode == 0

    def stage(self):
        return json.loads((self.job_dir / "job.json").read_text())["stage"]

    def test_taskclose_1_a_task_merging_dev_brought_back_is_closed_again(self):
        self.write("code.txt", "the change\n")
        self.write(DONE, EARLIER)
        self.commit("fix: something")
        self.dev_queues_the_task_and_the_branch_merges_it()
        self.push()
        self.assertTrue(self.in_head(TODO) and self.in_head(DONE))
        before = self.git(self.wt, "rev-parse", "HEAD").strip()

        res = self.advance()

        self.assertEqual(res.returncode, 0, res.stdout + res.stderr)
        self.assertEqual(self.stage(), "gate")
        self.assertEqual(self.git(self.wt, "rev-list", "--count", f"{before}..HEAD").strip(), "1")
        self.assertEqual(self.git(self.wt, "log", "-1", "--format=%s").strip(),
                         f"chore(factory): close {KEY} again after merging dev")
        self.assertFalse(self.in_head(TODO))
        self.assertEqual(self.git(self.wt, "show", f"HEAD:{DONE}"), TASK)
        self.assertEqual(self.git(self.wt, "ls-remote", "origin", self.branch).split()[0],
                         self.git(self.wt, "rev-parse", "HEAD").strip())
        self.assertEqual(self.git(self.wt, "status", "--porcelain"), "")

    def test_taskclose_1_an_unpushed_branch_is_closed_again_here_and_nothing_is_pushed(self):
        self.write("code.txt", "the change\n")
        self.write(DONE, EARLIER)
        self.commit("fix: something")
        self.dev_queues_the_task_and_the_branch_merges_it()  # the merge itself isn't pushed
        on_origin = self.git(self.wt, "ls-remote", "origin", self.branch).split()[0]

        res = self.advance()

        self.assertEqual(res.returncode, 1, res.stdout + res.stderr)
        self.assertIn("isn't pushed", res.stderr)
        self.assertEqual(self.stage(), "build")
        self.assertEqual(self.git(self.wt, "ls-remote", "origin", self.branch).split()[0], on_origin)
        self.assertEqual(self.git(self.wt, "log", "-1", "--format=%s").strip(),
                         f"chore(factory): close {KEY} again after merging dev")
        self.assertFalse(self.in_head(TODO))

    def assert_refused_as_today(self):
        self.push()
        before = self.git(self.wt, "rev-parse", "HEAD").strip()

        res = self.advance()

        self.assertEqual(res.returncode, 1, res.stdout + res.stderr)
        self.assertIn("the branch doesn't close its task", res.stderr)
        self.assertEqual(self.stage(), "build")
        self.assertEqual(self.git(self.wt, "rev-parse", "HEAD").strip(), before)
        self.assertTrue(self.in_head(TODO))

    def test_taskclose_2_a_branch_without_a_done_file_is_refused(self):
        self.write("code.txt", "the change\n")
        self.commit("fix: something")
        self.dev_queues_the_task_and_the_branch_merges_it()
        self.assert_refused_as_today()

    def test_taskclose_2_a_branch_whose_task_file_differs_from_the_base_is_refused(self):
        self.write("code.txt", "the change\n")
        self.write(DONE, EARLIER)
        self.commit("fix: something")
        self.dev_queues_the_task_and_the_branch_merges_it()
        self.write(TODO, TASK + "The builder's own line.\n")
        self.commit("docs: change the task")
        self.assert_refused_as_today()


if __name__ == "__main__":
    unittest.main(verbosity=2)
