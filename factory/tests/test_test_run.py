#!/usr/bin/env python3
"""factory/bin/test-run's own tests, with stand-in commands, never cargo.

Run from the repository root: python3 factory/tests/test_test_run.py
Run it directly, not through test-run: the priority test measures test-run's own nice.
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

TEST_RUN = Path(__file__).resolve().parents[1] / "bin" / "test-run"
CORE_VARS = ("CARGO_BUILD_JOBS", "RUST_TEST_THREADS", "NEXTEST_TEST_THREADS")


def gone(pid, seconds=5):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        try:
            os.kill(pid, 0)
        except ProcessLookupError:
            return True
        time.sleep(0.05)
    return False


class TestRun(unittest.TestCase):
    def setUp(self):
        self.dir = Path(tempfile.mkdtemp())
        self.lock = self.dir / "test-run.lock"
        self.env = {k: v for k, v in os.environ.items() if k not in CORE_VARS}
        self.env["OBY_TEST_RUN_LOCK"] = str(self.lock)
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
        shutil.rmtree(self.dir)

    def start(self, *command):
        proc = subprocess.Popen(
            [str(TEST_RUN), "--", *command],
            env=self.env,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )
        self.started.append(proc)
        return proc

    def run_test_run(self, *command):
        proc = self.start(*command)
        out, err = proc.communicate(timeout=30)
        return proc.returncode, out, err

    def wait_for_holder(self, proc):
        deadline = time.monotonic() + 5
        while time.monotonic() < deadline:
            if self.lock.exists() and self.lock.read_text().startswith(f"{proc.pid} "):
                return
            time.sleep(0.02)
        self.fail(f"test-run {proc.pid} never wrote its holder line")

    def test_overlapping_runs_take_turns(self):
        record = (
            "import sys, time; f = open(sys.argv[1], 'w'); f.write(repr(time.time()) + ' ');"
            " f.flush(); time.sleep(1); f.write(repr(time.time()))"
        )
        a = self.start("python3", "-c", record, str(self.dir / "a"))
        self.wait_for_holder(a)
        b = self.start("python3", "-c", record, str(self.dir / "b"))
        _, a_err = a.communicate(timeout=30)
        _, b_err = b.communicate(timeout=30)
        self.assertEqual((a.returncode, b.returncode), (0, 0), a_err + b_err)
        a_start, a_end = map(float, (self.dir / "a").read_text().split())
        b_start, b_end = map(float, (self.dir / "b").read_text().split())
        self.assertTrue(a_end <= b_start or b_end <= a_start, f"a {a_start}-{a_end} overlaps b {b_start}-{b_end}")
        waiter, holder = (b_err, a) if "waiting for it" in b_err else (a_err, b)
        self.assertIn("waiting for it", waiter)
        self.assertIn(f"({holder.pid} ", waiter)

    def test_exit_code_passes_through(self):
        self.assertEqual(self.run_test_run("sh", "-c", "exit 7")[0], 7)
        self.assertEqual(self.run_test_run("true")[0], 0)

    def test_leftover_process_is_stopped(self):
        code, out, err = self.run_test_run("sh", "-c", "sleep 300 >/dev/null 2>&1 & echo $!")
        self.assertEqual(code, 0, err)
        pid = int(out.strip())
        self.pids.append(pid)
        self.assertTrue(gone(pid), f"sleep {pid} is still running after test-run ended")

    def test_lock_is_free_after_a_stop(self):
        pidfile = self.dir / "sleep.pid"
        proc = self.start("sh", "-c", f"echo $$ > {pidfile}; exec sleep 300")
        self.wait_for_holder(proc)
        deadline = time.monotonic() + 5
        while not (pidfile.exists() and pidfile.read_text().strip()) and time.monotonic() < deadline:
            time.sleep(0.02)
        sleep_pid = int(pidfile.read_text())
        self.pids.append(sleep_pid)
        proc.send_signal(signal.SIGTERM)
        self.assertEqual(proc.wait(timeout=10), 143)
        self.assertTrue(gone(sleep_pid), f"sleep {sleep_pid} is still running after test-run was stopped")
        fd = os.open(self.lock, os.O_RDWR)
        try:
            fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
        finally:
            os.close(fd)

    def test_half_the_cores_and_background_priority(self):
        show = (
            "import os; print(os.environ.get('CARGO_BUILD_JOBS'), os.environ.get('RUST_TEST_THREADS'),"
            " os.environ.get('NEXTEST_TEST_THREADS'), os.getpriority(os.PRIO_PROCESS, 0))"
        )
        code, out, err = self.run_test_run("python3", "-c", show)
        self.assertEqual(code, 0, err)
        half = str(max(1, os.cpu_count() // 2))
        priority = str(min(os.getpriority(os.PRIO_PROCESS, 0) + 15, 20))
        self.assertEqual(out.split(), [half, half, half, priority])

    def test_gate_lock_is_not_used(self):
        self.assertFalse("factory-check.lock" in TEST_RUN.read_text(), "test-run names the gate's lock")
        proc = self.start("sleep", "300")
        self.wait_for_holder(proc)
        gate_lock = self.dir / "core-target" / ".factory-check.lock"
        gate_lock.parent.mkdir()
        fd = os.open(gate_lock, os.O_RDWR | os.O_CREAT)
        try:
            fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
        finally:
            os.close(fd)
        proc.send_signal(signal.SIGTERM)
        proc.communicate(timeout=10)


if __name__ == "__main__":
    unittest.main(verbosity=2)
