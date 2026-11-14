#!/usr/bin/env python3
"""factory/bin/test-run's own tests, with stand-in commands, never cargo.

Run from the repository root: python3 factory/tests/test_test_run.py
Run it directly, not through test-run: the priority test measures test-run's own nice.
"""

import fcntl
import importlib.machinery
import importlib.util
import os
import re
import shutil
import signal
import subprocess
import sys
import tempfile
import threading
import time
import unittest
from pathlib import Path

TEST_RUN = Path(__file__).resolve().parents[1] / "bin" / "test-run"
CORE_VARS = ("CARGO_BUILD_JOBS", "RUST_TEST_THREADS", "NEXTEST_TEST_THREADS")
STARTED = re.compile(r"\d{4}-\d\d-\d\dT\d\d:\d\d:\d\dZ")
# A stand-in cargo: `cargo sleep`, `cargo record <file>` (start and end times, a second apart),
# `cargo pid <file>` (its pid, then sleeps), `cargo test <file>` (writes a marker).
FAKE_CARGO = """#!/bin/sh
case "$1" in
sleep) exec sleep 300 ;;
record) python3 -c 'import sys, time; f = open(sys.argv[1], "w"); f.write(repr(time.time()) + " "); f.flush(); time.sleep(1); f.write(repr(time.time()))' "$2" ;;
pid) echo $$ > "$2"; exec sleep 300 ;;
test) echo ran > "$2" ;;
esac
"""


def load_test_run():
    loader = importlib.machinery.SourceFileLoader("test_run", str(TEST_RUN))
    spec = importlib.util.spec_from_loader("test_run", loader)
    module = importlib.util.module_from_spec(spec)
    sys.path.insert(0, str(TEST_RUN.parent))
    try:
        loader.exec_module(module)
    finally:
        sys.path.remove(str(TEST_RUN.parent))
    return module


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
        # The gate runs these tests with CARGO_TARGET_DIR at the shared folder, which test-run refuses.
        self.env = {k: v for k, v in os.environ.items() if k not in (*CORE_VARS, "CARGO_TARGET_DIR")}
        self.env["OBY_TEST_RUN_LOCK"] = str(self.lock)
        self.env["CARGO_TARGET_DIR_FACTORY"] = str(self.dir / "core-target")
        bin_dir = self.dir / "bin"
        bin_dir.mkdir()
        cargo = bin_dir / "cargo"
        cargo.write_text(FAKE_CARGO)
        cargo.chmod(0o755)
        self.env["PATH"] = f"{bin_dir}{os.pathsep}{self.env.get('PATH', '')}"
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
        a = self.start("cargo", "record", str(self.dir / "a"))
        self.wait_for_holder(a)
        b = self.start("cargo", "record", str(self.dir / "b"))
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
        proc = self.start("cargo", "pid", str(pidfile))
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
        proc = self.start("cargo", "sleep")
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

    def test_shared_cargo_folder_is_refused(self):
        shared = self.dir / "core-target"
        shared.mkdir()
        marker = self.dir / "marker"
        for target in (str(shared), f"{shared}/"):
            self.env["CARGO_TARGET_DIR"] = target
            code, _, err = self.run_test_run("cargo", "test", str(marker))
            self.assertEqual(code, 2, err)
            self.assertFalse(marker.exists(), f"cargo test ran with CARGO_TARGET_DIR={target}")
            self.assertIn("core/target", err)
            self.assertEqual(self.lock.read_text() if self.lock.exists() else "", "", "test-run wrote a holder line")
        other = self.dir / "own-target"
        other.mkdir()
        self.env["CARGO_TARGET_DIR"] = str(other)
        code, _, err = self.run_test_run("cargo", "test", str(marker))
        self.assertEqual(code, 0, err)
        self.assertTrue(marker.exists())

    def test_light_run_does_not_wait_for_the_lock(self):
        holder = self.start("cargo", "sleep")
        self.wait_for_holder(holder)
        light = self.start("python3", "-c", "print('ran')")
        out, err = light.communicate(timeout=15)
        self.assertEqual(light.returncode, 0, err)
        self.assertEqual(out.strip(), "ran")
        self.assertNotIn("waiting for it", err)
        self.assertTrue(self.lock.read_text().startswith(f"{holder.pid} "))

    def test_cargo_test_waits_for_the_lock(self):
        holder = self.start("cargo", "sleep")
        self.wait_for_holder(holder)
        marker = self.dir / "marker"
        waiter = self.start("cargo", "test", str(marker))
        time.sleep(1)
        self.assertFalse(marker.exists(), "cargo test ran while another run held the lock")
        self.assertIsNone(waiter.poll())
        holder.send_signal(signal.SIGTERM)
        holder.communicate(timeout=10)
        _, err = waiter.communicate(timeout=15)
        self.assertEqual(waiter.returncode, 0, err)
        self.assertIn("waiting for it", err)
        self.assertTrue(marker.exists())

    def test_long_wait_names_the_holder_and_since_when(self):
        self.env["OBY_TEST_RUN_REPORT_SECONDS"] = "1"
        holder = self.start("cargo", "sleep")
        self.wait_for_holder(holder)
        waiter = self.start("cargo", "sleep")
        err = []
        threading.Thread(target=lambda: err.extend(waiter.stderr), daemon=True).start()
        deadline = time.monotonic() + 6
        lines = []
        while len(lines) < 2 and time.monotonic() < deadline:
            time.sleep(0.05)
            lines = [line for line in list(err) if "still waiting for the lock" in line]
        waiter.send_signal(signal.SIGTERM)
        holder.send_signal(signal.SIGTERM)
        self.assertEqual(waiter.wait(timeout=10), 143)
        holder.wait(timeout=10)
        self.assertGreaterEqual(len(lines), 2, err)
        for line in lines:
            self.assertIn(f"({holder.pid} ", line)
            self.assertRegex(line, STARTED)


class Heavy(unittest.TestCase):
    def test_heavy_and_light_commands(self):
        heavy = load_test_run().heavy
        for command in (
            ["sh", "-c", "cargo test -p obid"],
            ["env", "X=1", "cargo", "test"],
            ["/Users/x/.cargo/bin/cargo", "clippy"],
            ["swift", "test", "--package-path", "mcp"],
            ["swift", "build"],
            ["xcodebuild", "-scheme", "ObyPhone", "test"],
            ["rustc", "--version"],
            ["cargo-nextest", "run"],
            ["sh", "-c", "true;nextest run"],
            ["scripts/oby-check.sh", "origin/dev"],
            ["oby-check-blok"],
        ):
            self.assertTrue(heavy(command), command)
        for command in (
            ["python3", "-m", "unittest"],
            ["npm", "test"],
            ["npx", "vitest"],
            ["npm", "run", "typecheck"],
            ["cat", "Cargo.toml"],
            ["swift", "--version"],
            ["sh", "-c", "test -f core/cargo-config"],
        ):
            self.assertFalse(heavy(command), command)


if __name__ == "__main__":
    unittest.main(verbosity=2)
