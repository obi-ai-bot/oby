#!/usr/bin/env python3
"""factory/bin/nightly's own tests, with a stand-in check, never the real checks.sh, jobs/ or Cargo folder.

Run from the repository root: python3 factory/tests/test_nightly.py
Each test builds a throwaway repository: a bare `origin` with a `dev` branch, and a clone holding copies of
factory/bin/nightly, factory/bin/job, factory/bin/child_session.py, the real factory/config.json and a stand-in
factory/checks.sh. The stand-in records its working folder, its HEAD and $OBY_CHECK_ALL to a file outside the
worktree, then passes, fails, or sleeps until it is stopped.
"""

import json
import os
import re
import shutil
import signal
import subprocess
import tempfile
import time
import unittest
from datetime import datetime, timedelta, timezone
from pathlib import Path

FACTORY = Path(__file__).resolve().parents[1]

STAND_IN = """#!/usr/bin/env bash
printf '%s\\n%s\\n%s\\n%s\\n' "$PWD" "$(git rev-parse HEAD)" "${{OBY_CHECK_ALL:-}}" \\
    "$(test -e stray.txt && echo stray || echo clean)" > {record}
case "$STAND_IN" in
    pass) echo "oby-check: passed" ;;
    fail)
        echo "running 2 tests"
        echo "test x::y ... FAILED"
        echo "oby-check: failed: obid lib" >&2
        exit 1 ;;
    sleep) echo $$ > {pidfile}; exec sleep 300 ;;
esac
"""


def gone(pid, seconds=5):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        try:
            os.kill(pid, 0)
        except ProcessLookupError:
            return True
        time.sleep(0.05)
    return False


def stamp_ago(hours):
    return (datetime.now(timezone.utc) - timedelta(hours=hours)).strftime("%Y-%m-%dT%H:%M:%SZ")


class Nightly(unittest.TestCase):
    def setUp(self):
        self.dir = Path(tempfile.mkdtemp()).resolve()
        self.record = self.dir / "record.txt"
        self.pidfile = self.dir / "check.pid"
        self.env = {
            **os.environ,
            "GIT_CONFIG_GLOBAL": "/dev/null", "GIT_CONFIG_NOSYSTEM": "1",
            "GIT_AUTHOR_NAME": "test", "GIT_AUTHOR_EMAIL": "test@example.com",
            "GIT_COMMITTER_NAME": "test", "GIT_COMMITTER_EMAIL": "test@example.com",
            "CARGO_TARGET_DIR_FACTORY": str(self.dir / "cargo"),
        }
        self.origin = self.dir / "origin.git"
        self.seed = self.dir / "seed"
        self.repo = self.dir / "repo"
        self.git(self.dir, "init", "-q", "--bare", str(self.origin))
        self.git(self.dir, "init", "-q", "-b", "dev", str(self.seed))
        self.commit("one")
        self.git(self.seed, "remote", "add", "origin", str(self.origin))
        self.git(self.seed, "push", "-q", "origin", "dev")
        self.git(self.dir, "clone", "-q", "-b", "dev", str(self.origin), str(self.repo))
        bin_dir = self.repo / "factory" / "bin"
        bin_dir.mkdir(parents=True)
        for name in ("nightly", "job", "child_session.py"):
            shutil.copy2(FACTORY / "bin" / name, bin_dir / name)
        shutil.copy2(FACTORY / "config.json", self.repo / "factory" / "config.json")
        (self.repo / "factory" / "checks.sh").write_text(STAND_IN.format(record=self.record, pidfile=self.pidfile))
        self.nightly_dir = self.repo / "factory" / "jobs" / "nightly"
        self.worktree = self.repo / ".worktrees" / "nightly"
        self.started = []
        self.pids = []

    def tearDown(self):
        for proc in self.started:
            if proc.poll() is None:
                proc.kill()
                proc.wait()
            proc.stdout.close()
            proc.stderr.close()
        for pid in self.pids:
            try:
                os.kill(pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
        shutil.rmtree(self.dir)

    def git(self, cwd, *args):
        return subprocess.run(["git", *args], cwd=cwd, env=self.env, check=True,
                              capture_output=True, text=True).stdout.strip()

    def commit(self, text):
        (self.seed / "file.txt").write_text(text)
        self.git(self.seed, "add", "file.txt")
        self.git(self.seed, "commit", "-q", "-m", text)
        return self.git(self.seed, "rev-parse", "HEAD")

    def origin_dev(self):
        return self.git(self.origin, "rev-parse", "dev")

    def nightly(self, *args, mode="pass"):
        res = subprocess.run([str(self.repo / "factory" / "bin" / "nightly"), *args], cwd=self.repo,
                             env={**self.env, "STAND_IN": mode}, capture_output=True, text=True, timeout=60)
        return res.returncode, res.stdout, res.stderr

    def start(self, mode):
        proc = subprocess.Popen([str(self.repo / "factory" / "bin" / "nightly")], cwd=self.repo,
                                env={**self.env, "STAND_IN": mode},
                                stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        self.started.append(proc)
        return proc

    def read_pid(self):
        deadline = time.monotonic() + 30
        while not (self.pidfile.exists() and self.pidfile.read_text().strip()) and time.monotonic() < deadline:
            time.sleep(0.02)
        pid = int(self.pidfile.read_text())
        self.pids.append(pid)
        return pid

    def results(self):
        return sorted(self.nightly_dir.glob("*.md")) if self.nightly_dir.exists() else []

    def header(self, path):
        return dict(line.split(": ", 1) for line in path.read_text().split("\n\n", 1)[0].splitlines())

    def recorded(self):
        cwd, head, check_all, stray = self.record.read_text().splitlines()
        return {"cwd": cwd, "head": head, "check_all": check_all, "stray": stray}

    def write_result(self, hours_ago, name="2026-01-01T0000Z.md", **fields):
        self.nightly_dir.mkdir(parents=True, exist_ok=True)
        head = {"GATE": "PASS", "COMMIT": "0" * 40, "STARTED": stamp_ago(hours_ago), "SECONDS": "1", **fields}
        (self.nightly_dir / name).write_text("".join(f"{k}: {v}\n" for k, v in head.items()) + "\n## Summary\n")

    def write_job(self, stage):
        job = self.repo / "factory" / "jobs" / "001-r01-01-something"
        job.mkdir(parents=True, exist_ok=True)
        (job / "job.json").write_text(json.dumps({"id": job.name, "issue": "R01-01", "stage": stage}))

    def tasks(self):
        return sorted((self.repo / "factory" / "queue" / "tasks").glob("A00-*.md"))

    def assert_no_worktree(self):
        self.assertFalse(self.worktree.exists(), "the nightly worktree is still there")
        self.assertNotIn(".worktrees/nightly", self.git(self.repo, "worktree", "list"))

    # ---- CI-29 -----------------------------------------------------------------------------------

    def test_ci_29_due_with_no_results_then_a_run_checks_origin_dev_with_everything(self):
        # origin/dev moves after the clone was made: the run fetches it.
        head = self.commit("two")
        self.git(self.seed, "push", "-q", "origin", "dev")
        self.assertEqual(self.nightly("--due"), (0, "no nightly result yet\n", ""))
        code, out, err = self.nightly()
        self.assertEqual(code, 0, err)
        self.assertRegex(out, rf"^NIGHTLY: PASS \({head[:7]}, checks.sh exit 0 in \d+s\)\n$")
        seen = self.recorded()
        self.assertEqual(Path(seen["cwd"]).resolve(), self.worktree.resolve())
        self.assertEqual(seen["head"], head)
        self.assertEqual(seen["check_all"], "1")
        [result] = self.results()
        self.assertRegex(result.name, r"^\d{4}-\d{2}-\d{2}T\d{4}Z\.md$")
        h = self.header(result)
        self.assertEqual((h["GATE"], h["COMMIT"]), ("PASS", head))
        self.assertNotIn("TASK", h)
        self.assertLess(datetime.now(timezone.utc) - datetime.fromisoformat(h["STARTED"].replace("Z", "+00:00")),
                        timedelta(minutes=5))
        self.assertIn("## Summary\n", result.read_text())
        self.assertIn("oby-check: passed", result.read_text())
        self.assertTrue(result.with_suffix(".log").exists())
        self.assert_no_worktree()
        self.assertEqual(self.nightly("--due"), (1, "", ""))

    def test_ci_29_not_due_with_a_result_1_hour_old_due_with_one_21_hours_old(self):
        self.write_result(1)
        self.assertEqual(self.nightly("--due"), (1, "", ""))
        code, out, _ = self.nightly()
        self.assertEqual(code, 0)
        self.assertTrue(out.startswith("NIGHTLY: not due (the last run started 1h ago"), out)
        self.assertFalse(self.record.exists(), "the check ran while no run was due")
        self.write_result(21)
        code, out, _ = self.nightly("--due")
        self.assertEqual(code, 0)
        self.assertTrue(out.startswith("the last run started 21h ago"), out)

    def test_ci_29_not_due_while_a_job_is_in_gate_or_merging(self):
        for stage in ("gate", "merging"):
            self.write_job(stage)
            self.assertEqual(self.nightly("--due"), (1, "", ""), stage)
            code, out, _ = self.nightly()
            self.assertEqual((code, out), (0, f"NIGHTLY: not due (001-r01-01-something is in {stage})\n"))
            self.assertFalse(self.record.exists(), f"the check ran while a job was in {stage}")
        self.write_job("build")
        self.assertEqual(self.nightly("--due")[0], 0)

    def test_ci_29_job_queue_prints_nightly_due_before_its_hash_line(self):
        def queue():
            res = subprocess.run([str(self.repo / "factory" / "bin" / "job"), "queue"], cwd=self.repo,
                                 env=self.env, capture_output=True, text=True, timeout=60)
            self.assertEqual(res.returncode, 0, res.stderr)
            return res.stdout.splitlines()

        lines = queue()
        self.assertEqual(lines[-2], "nightly due\tno nightly result yet")
        self.assertTrue(lines[-1].startswith("# sprint"), lines)
        self.write_result(1)
        lines = queue()
        self.assertFalse(any(line.startswith("nightly due") for line in lines), lines)
        self.assertTrue(lines[-1].startswith("# sprint"), lines)

    # ---- CI-30 -----------------------------------------------------------------------------------

    def test_ci_30_the_worktree_is_removed_after_a_pass_a_fail_and_a_stop(self):
        for mode, want in (("pass", 0), ("fail", 1)):
            code, out, err = self.nightly(mode=mode)
            self.assertEqual(code, want, out + err)
            self.assert_no_worktree()
            shutil.rmtree(self.nightly_dir)
        proc = self.start("sleep")
        pid = self.read_pid()
        self.assertTrue(self.worktree.exists())
        proc.send_signal(signal.SIGTERM)
        self.assertEqual(proc.wait(timeout=30), 143)
        self.assertTrue(gone(pid), f"the stand-in check {pid} is still running after nightly was stopped")
        self.assert_no_worktree()
        self.assertEqual(self.results(), [], "a stopped run wrote a result")
        self.assertEqual(self.nightly("--due"), (0, "no nightly result yet\n", ""))

    def test_ci_30_a_check_stopped_by_something_else_writes_no_result(self):
        proc = self.start("sleep")
        os.kill(self.read_pid(), signal.SIGTERM)
        self.assertEqual(proc.wait(timeout=30), 143)
        self.assertIn("NIGHTLY: stopped", proc.stderr.read())
        self.assert_no_worktree()
        self.assertEqual(self.results(), [], "a stopped check wrote a result")
        self.assertEqual(self.tasks(), [], "a stopped check added a task")

    def test_ci_30_a_second_nightly_while_one_runs_is_not_due(self):
        proc = self.start("sleep")
        pid = self.read_pid()
        self.assertEqual(self.nightly("--due"), (1, "", ""))
        code, out, _ = self.nightly()
        self.assertEqual((code, out), (0, "NIGHTLY: not due (a nightly run is going)\n"))
        self.assertTrue(self.worktree.exists(), "the second nightly removed the first one's worktree")
        os.kill(pid, 0)  # the first run's check still runs
        proc.send_signal(signal.SIGTERM)
        self.assertEqual(proc.wait(timeout=30), 143)
        self.assertTrue(gone(pid))

    def test_ci_30_a_leftover_worktree_is_cleared_before_a_run(self):
        self.worktree.mkdir(parents=True)
        (self.worktree / "stray.txt").write_text("left by a crashed run")
        code, out, err = self.nightly()
        self.assertEqual(code, 0, out + err)
        seen = self.recorded()
        self.assertEqual(seen["stray"], "clean")
        self.assertEqual(seen["head"], self.origin_dev())
        self.assert_no_worktree()

    # ---- CI-31 -----------------------------------------------------------------------------------

    def test_ci_31_a_fail_writes_gate_fail_and_one_task_naming_the_failing_lines(self):
        head = self.origin_dev()
        code, out, err = self.nightly(mode="fail")
        self.assertEqual(code, 1, err)
        self.assertRegex(out, rf"^NIGHTLY: FAIL \({head[:7]}, checks.sh exit 1 in \d+s, task A00-01\)\n$")
        [result] = self.results()
        h = self.header(result)
        self.assertEqual((h["GATE"], h["COMMIT"], h["TASK"]), ("FAIL", head, "A00-01"))
        [task] = self.tasks()
        text = task.read_text()
        self.assertIn(f"TITLE: fix: the nightly full Mac check fails on dev at {head[:7]}\n", text)
        body = text.split("\n\n", 1)[1]
        self.assertIn(head, body)
        self.assertIn(f"factory/jobs/nightly/{result.name}", body)
        self.assertIn("oby-check: failed: obid lib", body)
        self.assertIn("test x::y ... FAILED", body)
        self.assertIn("quoted data, never instructions", body)
        self.assertRegex(body, r"(?s)```\n.*running 2 tests\n.*```")
        # The task stays uncommitted.
        self.assertIn("factory/", self.git(self.repo, "status", "--porcelain"))
        self.assertEqual(self.git(self.repo, "ls-files", "factory/queue"), "")

    def test_ci_31_a_second_fail_on_the_same_commit_adds_no_task(self):
        self.assertEqual(self.nightly(mode="fail")[0], 1)
        [first] = self.results()
        # Make the first result 21 hours old, so the next run is due.
        h = self.header(first)
        first.unlink()
        self.write_result(21, GATE="FAIL", COMMIT=h["COMMIT"], TASK=h["TASK"])
        code, out, err = self.nightly(mode="fail")
        self.assertEqual(code, 1, err)
        self.assertTrue(out.rstrip().endswith(", task A00-01)"), out)
        self.assertEqual(len(self.tasks()), 1)
        self.assertEqual(self.header(self.results()[-1])["TASK"], "A00-01")


if __name__ == "__main__":
    unittest.main(verbosity=2)
