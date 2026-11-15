#!/usr/bin/env python3
"""A job's lock: reads take none, and a change holds it only while it runs (A00-99).

Run from the repository root: python3 factory/tests/test_job_locks.py
Each test builds a throwaway repository holding a copy of factory/bin/job and the real factory/config.json, with a
job in `approve` whose decision.md says ESCALATE and no PR, so `job advance <job> needs-human` needs no git or GitHub.
A second process imports the module as factory/bin/oby-feed does and stays running.
"""

import fcntl
import json
import os
import shutil
import subprocess
import sys
import tempfile
import time
import unittest
from pathlib import Path

FACTORY = Path(__file__).resolve().parents[1]
JOB_ID = "001-a00-16-something"

# The module imported as factory/bin/oby-feed imports it; `read` calls all_jobs(), `update` holds load_for_update
# until a line comes on stdin and then saves a field of its own. Either says `ready` and stays running.
HOLDER = """import importlib.machinery, importlib.util, sys
loader = importlib.machinery.SourceFileLoader("factory_job", sys.argv[1])
spec = importlib.util.spec_from_loader("factory_job", loader)
job = importlib.util.module_from_spec(spec)
loader.exec_module(job)
if sys.argv[2] == "read":
    job.all_jobs()
else:
    with job.load_for_update(sys.argv[3]) as j:
        print("locked", flush=True)
        sys.stdin.readline()
        j["note"] = "kept"
        job.save(j)
print("ready", flush=True)
sys.stdin.readline()
"""


class JobLocks(unittest.TestCase):
    def setUp(self):
        self.dir = Path(tempfile.mkdtemp()).resolve()
        self.repo = self.dir / "repo"
        (self.repo / "factory" / "bin").mkdir(parents=True)
        self.job = self.repo / "factory" / "bin" / "job"
        shutil.copy2(FACTORY / "bin" / "job", self.job)
        shutil.copy2(FACTORY / "config.json", self.repo / "factory" / "config.json")
        self.job_dir = self.repo / "factory" / "jobs" / JOB_ID
        (self.job_dir / "round-1").mkdir(parents=True)
        (self.job_dir / "job.json").write_text(json.dumps({
            "id": JOB_ID, "issue": "A00-16", "title": "fix: something", "milestone": "A00", "sprint": 0,
            "fixes": [], "pr": None, "pr_url": None, "branch": f"factory/{JOB_ID}",
            "worktree": str(self.repo / ".worktrees" / JOB_ID), "stage": "approve", "round": 1, "round_limit": 3,
            "intake": None, "agents": None, "gate": "PASS", "reviews": {},
            "cost_usd": 0, "started": "2026-01-01T00:00:00Z", "ended": None,
        }, indent=2) + "\n")
        (self.job_dir / "decision.md").write_text("ESCALATE\n\n## Why\nA person decides.\n")
        (self.dir / "claude").mkdir()
        path_dir = self.dir / "path"
        path_dir.mkdir()
        gh = path_dir / "gh"  # a stand-in that says every PR is merged
        gh.write_text('#!/bin/sh\ncase "$*" in\n    "pr view 7 --json state") echo \'{"state":"MERGED"}\' ;;\n'
                      '    *) echo "stand-in gh: unexpected: $*" >&2; exit 1 ;;\nesac\n')
        gh.chmod(0o755)
        self.env = {
            **{k: v for k, v in os.environ.items() if not k.startswith("GIT_")},
            "CLAUDE_CONFIG_DIR": str(self.dir / "claude"),
            "CARGO_TARGET_DIR_FACTORY": str(self.dir / "no-shared-cargo"),
            "PATH": f"{path_dir}{os.pathsep}{os.environ['PATH']}",
        }
        self.holders = []

    def tearDown(self):
        for p in self.holders:
            p.kill()
            p.communicate()
        shutil.rmtree(self.dir)

    def holder(self, *args):
        p = subprocess.Popen([sys.executable, "-c", HOLDER, str(self.job), *args], cwd=self.repo, env=self.env,
                             stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)
        self.holders.append(p)
        return p

    def advance(self, stage, **kw):
        return subprocess.Popen([str(self.job), "advance", JOB_ID, stage], cwd=self.repo, env=self.env,
                                stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, **kw)

    def saved(self):
        return json.loads((self.job_dir / "job.json").read_text())

    def assert_unlocked(self):
        with open(self.job_dir / ".lock", "w") as lock:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)  # raises BlockingIOError while anyone holds it

    def test_joblock_1_an_advance_completes_while_a_reader_keeps_running(self):
        reader = self.holder("read")
        self.assertEqual(reader.stdout.readline(), "ready\n")
        out, err = self.advance("needs-human").communicate(timeout=10)
        self.assertIn("needs-human", out, err)
        self.assertEqual(self.saved()["stage"], "needs-human")
        self.assertIsNone(reader.poll(), "the reader is still running")

    def test_joblock_2_two_changes_to_one_job_are_serialized(self):
        first = self.holder("update", JOB_ID)
        self.assertEqual(first.stdout.readline(), "locked\n")
        second = self.advance("needs-human")
        time.sleep(1)
        self.assertIsNone(second.poll(), "the advance waits for the change holding the job")
        first.stdin.write("\n")
        first.stdin.flush()
        self.assertEqual(first.stdout.readline(), "ready\n")
        out, err = second.communicate(timeout=10)
        self.assertEqual(second.returncode, 0, err)
        job = self.saved()
        self.assertEqual((job["stage"], job.get("note")), ("needs-human", "kept"))

    def test_joblock_2_job_queue_marking_a_merged_job_waits_for_a_change_and_keeps_it(self):
        job = self.saved()
        job["pr"] = 7
        (self.job_dir / "job.json").write_text(json.dumps(job, indent=2) + "\n")
        first = self.holder("update", JOB_ID)
        self.assertEqual(first.stdout.readline(), "locked\n")
        queue = subprocess.Popen([str(self.job), "queue"], cwd=self.repo, env=self.env,
                                 stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        time.sleep(1)
        self.assertIsNone(queue.poll(), "job queue waits for the change holding the job")
        first.stdin.write("\n")
        first.stdin.flush()
        self.assertEqual(first.stdout.readline(), "ready\n")
        out, err = queue.communicate(timeout=30)
        self.assertEqual(queue.returncode, 0, err)
        self.assertNotIn("stand-in gh: unexpected", err)
        job = self.saved()
        self.assertEqual((job["stage"], job.get("note")), ("merged", "kept"))

    def test_joblock_3_a_change_releases_its_lock_when_it_ends(self):
        with self.subTest("a process that goes on running"):
            p = self.holder("update", JOB_ID)
            self.assertEqual(p.stdout.readline(), "locked\n")
            with self.assertRaises(BlockingIOError):
                self.assert_unlocked()
            p.stdin.write("\n")
            p.stdin.flush()
            self.assertEqual(p.stdout.readline(), "ready\n")
            self.assertIsNone(p.poll())
            self.assert_unlocked()
        with self.subTest("a refusal"):
            refused = self.advance("build")
            refused.communicate(timeout=10)
            self.assertEqual(refused.returncode, 1)
            self.assert_unlocked()
        with self.subTest("a change that saved"):
            done = self.advance("needs-human")
            done.communicate(timeout=10)
            self.assertEqual(done.returncode, 0)
            self.assert_unlocked()


if __name__ == "__main__":
    unittest.main(verbosity=2)
