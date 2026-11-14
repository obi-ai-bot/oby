#!/usr/bin/env python3
"""factory/bin/check-lock's own tests, with stand-in checks, never oby-check.sh.

Run from the repository root: python3 factory/tests/test_check_lock.py
Each test runs check-lock in a temporary git folder with a lock file of its own, never the gate's.
Only test_second_worktree_builds_its_own_code runs real cargo, offline, with its build folder in that
temporary folder: only Cargo itself shows its own freshness rule.
"""

import fcntl
import os
import shutil
import signal
import subprocess
import sys
import tempfile
import time
import unittest
from pathlib import Path

BIN = Path(__file__).resolve().parents[1] / "bin"
CHECK_LOCK = BIN / "check-lock"

# Runs check-lock in this process, with child_session's Popen wrapped so that this process gets
# SIGTERM right after the command has started (it has written its pid), before check-lock knows it.
STOP_AT_START = """
import os, runpy, signal, sys, time
from pathlib import Path
sys.path.insert(0, sys.argv[1])
import child_session
real = child_session.Popen
pidfile = Path(sys.argv[2])
def popen(*args, **kwargs):
    proc = real(*args, **kwargs)
    deadline = time.monotonic() + 5
    while not (pidfile.exists() and pidfile.read_text().strip()) and time.monotonic() < deadline:
        time.sleep(0.01)
    os.kill(os.getpid(), signal.SIGTERM)
    return proc
child_session.Popen = popen
sys.argv = sys.argv[3:]
runpy.run_path(sys.argv[0], run_name="__main__")
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


class CheckLock(unittest.TestCase):
    def setUp(self):
        self.dir = Path(tempfile.mkdtemp())
        self.repo = self.dir / "repo"
        (self.repo / "core").mkdir(parents=True)
        (self.repo / "core" / "lib.rs").write_text("")
        subprocess.run(["git", "init", "-q", str(self.repo)], check=True)
        subprocess.run(["git", "-C", str(self.repo), "add", "core"], check=True)
        self.lock = self.dir / "locks" / ".factory-check.lock"
        self.lock.parent.mkdir()
        self.started = []
        self.pids = []

    def tearDown(self):
        for proc in self.started:
            if proc.poll() is None:
                proc.kill()
                proc.wait()
        for pid in self.pids:
            try:
                os.kill(pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
        for proc in self.started:
            proc.stdout.close()
            proc.stderr.close()
        shutil.rmtree(self.dir)

    def start(self, *command, driver=None):
        argv = [str(CHECK_LOCK), str(self.lock), *command]
        if driver:
            argv = [sys.executable, "-c", driver[0], *driver[1:], *argv]
        proc = subprocess.Popen(argv, cwd=self.repo, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        self.started.append(proc)
        return proc

    def run_check_lock(self, *command):
        proc = self.start(*command)
        out, err = proc.communicate(timeout=30)
        return proc.returncode, out, err

    def read_pid(self, pidfile):
        deadline = time.monotonic() + 5
        while not (pidfile.exists() and pidfile.read_text().strip()) and time.monotonic() < deadline:
            time.sleep(0.02)
        pid = int(pidfile.read_text())
        self.pids.append(pid)
        return pid

    def test_passed_check_passes_100_of_100(self):
        # A check that exits 0 at once while a process it started is ending: check-lock stops its
        # leftovers just as the group empties, when macOS answers EPERM (about 1 run in 3 here).
        for run in range(1, 101):
            code, _, err = self.run_check_lock("sh", "-c", "true & exit 0")
            self.assertEqual(code, 0, f"run {run} of 100 exited {code}:\n{err}")

    def test_exit_codes_pass_through(self):
        for want in (0, 1, 2):
            code, _, err = self.run_check_lock("sh", "-c", f"exit {want}")
            self.assertEqual(code, want, err)

    def test_stop_at_start_leaves_nothing_running(self):
        pidfile = self.dir / "check.pid"
        proc = self.start(
            "sh", "-c", f"echo $$ > {pidfile}; exec sleep 300",
            driver=(STOP_AT_START, str(BIN), str(pidfile)),
        )
        # Waits on the process, not its pipes: a check left running would hold them open.
        code = proc.wait(timeout=30)
        pid = self.read_pid(pidfile)
        self.assertEqual(code, 143)
        self.assertTrue(gone(pid), f"the check {pid} is still running after check-lock was stopped at its start")

    def test_leftover_process_is_stopped(self):
        code, out, err = self.run_check_lock("sh", "-c", "sleep 300 >/dev/null 2>&1 & echo $!")
        self.assertEqual(code, 0, err)
        pid = int(out.strip())
        self.pids.append(pid)
        self.assertTrue(gone(pid), f"sleep {pid} is still running after check-lock ended")

    def test_stop_while_running(self):
        pidfile = self.dir / "check.pid"
        proc = self.start("sh", "-c", f"echo $$ > {pidfile}; exec sleep 300")
        pid = self.read_pid(pidfile)
        proc.send_signal(signal.SIGTERM)
        self.assertEqual(proc.wait(timeout=10), 143)
        self.assertTrue(gone(pid), f"the check {pid} is still running after check-lock was stopped")
        fd = os.open(self.lock, os.O_RDWR)
        try:
            fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
        finally:
            os.close(fd)

    def test_second_worktree_builds_its_own_code(self):
        # Cargo gives a workspace crate the same build record in every worktree laid out alike, and
        # finds it fresh when no source file is newer than that build: B's older files would run A's
        # test binary, with B's own test missing, unless check-lock gives them the current time.
        shared = self.dir / "core-target"
        env = dict(os.environ, CARGO_TARGET_DIR=str(shared), CARGO_NET_OFFLINE="true")
        lib = "pub fn one() -> u32 {\n    1\n}\n\n#[test]\nfn in_both() {\n    assert_eq!(one(), 1);\n}\n"
        worktrees = {}
        for name, source in (("a", lib), ("b", lib + "\n#[test]\nfn only_in_b() {\n    assert_eq!(one() + 1, 2);\n}\n")):
            worktree = self.dir / name
            (worktree / "core" / "src").mkdir(parents=True)
            (worktree / "core" / "Cargo.toml").write_text('[package]\nname = "stand_in"\nversion = "0.1.0"\nedition = "2021"\n')
            (worktree / "core" / "src" / "lib.rs").write_text(source)
            subprocess.run(["git", "init", "-q", str(worktree)], check=True)
            subprocess.run(["git", "-C", str(worktree), "add", "core"], check=True)
            worktrees[name] = worktree

        def check(worktree, *filter):
            command = [str(CHECK_LOCK), str(self.lock), "cargo", "test", "--manifest-path", "core/Cargo.toml", *filter]
            return subprocess.run(command, cwd=worktree, env=env, capture_output=True, text=True, timeout=300)

        a = check(worktrees["a"])
        self.assertEqual(a.returncode, 0, a.stderr)
        hour_ago = time.time() - 3600
        for folder, _, files in os.walk(worktrees["b"]):
            for file in files:
                os.utime(Path(folder) / file, (hour_ago, hour_ago))
        b = check(worktrees["b"], "only_in_b")
        self.assertEqual(b.returncode, 0, b.stderr)
        self.assertIn("test only_in_b ... ok", b.stdout, b.stdout + b.stderr)
        self.assertIn("1 passed", b.stdout, b.stdout + b.stderr)


if __name__ == "__main__":
    unittest.main(verbosity=2)
