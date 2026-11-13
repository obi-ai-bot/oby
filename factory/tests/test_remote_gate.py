#!/usr/bin/env python3
"""factory/remote/oby-remote-gate's and setup.sh --pins' own tests, with no VM.

Each test makes an origin repository whose scripts/oby-check.sh is a stand-in, and a root
with repo.git cloned from it, as setup.sh makes /srv/oby.

Run from the repository root: python3 factory/tests/test_remote_gate.py
"""

import fcntl
import os
import re
import shutil
import signal
import subprocess
import tempfile
import threading
import time
import unittest
from pathlib import Path

FACTORY = Path(__file__).resolve().parents[1]
GATE = FACTORY / "remote" / "oby-remote-gate"
SETUP = FACTORY / "remote" / "setup.sh"
CI = FACTORY.parent / ".github" / "workflows" / "ci.yml"

# The stand-in check: the plan prints $FAKE/groups' groups; a run records its slot, CARGO_TARGET_DIR,
# OBY_CHECK_ONLY, its pid and a background child's, waits for $FAKE/release or $FAKE/release-<slot>,
# and exits with $FAKE/exit-<group>'s code (0 without one).
FAKE_CHECK = """#!/bin/sh
if [ -n "$OBY_CHECK_PLAN" ]; then
    for group in $(cat "$FAKE/groups"); do echo "oby-check: group $group"; done
    echo "oby-check: planned, nothing run"
    exit 0
fi
slot=$(basename "$(dirname "$PWD")")
sleep 300 >/dev/null 2>&1 &
echo "$slot $CARGO_TARGET_DIR $OBY_CHECK_ONLY $$ $!" >>"$FAKE/runs"
while [ ! -e "$FAKE/release" ] && [ ! -e "$FAKE/release-$slot" ]; do sleep 0.1; done
kill $! 2>/dev/null
exit $(cat "$FAKE/exit-$OBY_CHECK_ONLY" 2>/dev/null || echo 0)
"""


def git(*args, cwd):
    return subprocess.run(["git", *args], cwd=cwd, check=True, capture_output=True, text=True).stdout.strip()


def gone(pid, seconds=5):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        try:
            os.kill(pid, 0)
        except ProcessLookupError:
            return True
        time.sleep(0.05)
    return False


class Gate:
    """A running oby-remote-gate, its output gathered line by line as it comes."""

    def __init__(self, proc):
        self.proc = proc
        self.lines = []
        self.reader = threading.Thread(target=self.read, daemon=True)
        self.reader.start()

    def read(self):
        for line in self.proc.stdout:
            self.lines.append(line.rstrip("\n"))

    def output(self):
        return "\n".join(self.lines)

    def wait_for(self, pattern, seconds=20):
        deadline = time.monotonic() + seconds
        while time.monotonic() < deadline:
            for line in self.lines:
                found = re.search(pattern, line)
                if found:
                    return found
            if self.proc.poll() is not None and not self.reader.is_alive():
                break
            time.sleep(0.05)
        raise AssertionError(f"no line matching {pattern!r} in:\n{self.output()}")

    def slot(self):
        return int(self.wait_for(r"^oby-remote-gate: slot (\d)$").group(1))

    def finish(self, seconds=30):
        code = self.proc.wait(timeout=seconds)
        self.reader.join(timeout=5)
        return code


class RemoteGate(unittest.TestCase):
    def setUp(self):
        self.dir = Path(tempfile.mkdtemp())
        self.fake = self.dir / "fake"
        self.fake.mkdir()
        origin = self.dir / "origin"
        origin.mkdir()
        git("init", "--quiet", "--initial-branch=dev", cwd=origin)
        git("config", "uploadpack.allowAnySHA1InWant", "true", cwd=origin)
        script = origin / "scripts" / "oby-check.sh"
        script.parent.mkdir()
        script.write_text(FAKE_CHECK)
        script.chmod(0o755)
        (origin / "apps" / "electron").mkdir(parents=True)
        (origin / "apps" / "electron" / "package.json").write_text("{}\n")
        (origin / "core").mkdir()
        (origin / "core" / "a.txt").write_text("a\n")
        git("add", ".", cwd=origin)
        git("-c", "user.name=t", "-c", "user.email=t@t", "commit", "--quiet", "-m", "base", cwd=origin)
        self.base = git("rev-parse", "HEAD", cwd=origin)
        (origin / "core" / "a.txt").write_text("b\n")
        git("-c", "user.name=t", "-c", "user.email=t@t", "commit", "--quiet", "-am", "change", cwd=origin)
        self.commit = git("rev-parse", "HEAD", cwd=origin)
        self.root = self.dir / "root"
        self.root.mkdir()
        git("clone", "--quiet", "--bare", str(origin), str(self.root / "repo.git"), cwd=self.dir)
        # A stand-in npm, so the electron group needs no network.
        bin_dir = self.dir / "bin"
        bin_dir.mkdir()
        (bin_dir / "npm").write_text(f'#!/bin/sh\necho "npm $*" >>"{self.fake}/npm"\n')
        (bin_dir / "npm").chmod(0o755)
        self.env = {k: v for k, v in os.environ.items() if not k.startswith("OBY_")}
        self.env.update(
            OBY_REMOTE_ROOT=str(self.root),
            OBY_REMOTE_URL=str(origin),
            FAKE=str(self.fake),
            PATH=f"{bin_dir}{os.pathsep}{os.environ['PATH']}",
        )
        self.groups("rust", "scan")
        self.gates = []

    def tearDown(self):
        (self.fake / "release").touch()
        for gate in self.gates:
            if gate.proc.poll() is None:
                gate.proc.kill()
                gate.proc.wait()
            gate.reader.join(timeout=5)
            gate.proc.stdin.close()
            gate.proc.stdout.close()
        for run in self.runs():
            for pid in run[3:]:
                try:
                    os.kill(int(pid), signal.SIGKILL)
                except ProcessLookupError:
                    pass
        shutil.rmtree(self.dir)

    def groups(self, *names):
        (self.fake / "groups").write_text(" ".join(names) + "\n")

    def start(self, commit=None, base=None):
        proc = subprocess.Popen(
            [str(GATE), commit or self.commit, base or "origin/dev"],
            env=self.env,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
        )
        gate = Gate(proc)
        self.gates.append(gate)
        return gate

    def runs(self):
        path = self.fake / "runs"
        return [line.split() for line in path.read_text().splitlines()] if path.exists() else []

    def wait_for_runs(self, count, seconds=20):
        deadline = time.monotonic() + seconds
        while len(self.runs()) < count and time.monotonic() < deadline:
            time.sleep(0.05)
        self.assertEqual(len(self.runs()), count, self.runs())

    def test_remote_2_two_gates_use_different_slots(self):
        self.groups("rust")
        a = self.start()
        b = self.start()
        self.wait_for_runs(2)
        slots = {a.slot(), b.slot()}
        self.assertEqual(len(slots), 2, a.output() + "\n" + b.output())
        runs = self.runs()
        self.assertEqual({run[0] for run in runs}, {f"slot-{n}" for n in slots})
        self.assertEqual(
            {run[1] for run in runs}, {str(self.root / f"slot-{n}" / "target") for n in slots}, runs
        )
        (self.fake / "release").touch()
        self.assertEqual((a.finish(), b.finish()), (0, 0), a.output() + "\n" + b.output())

    def test_remote_2_fifth_waits_for_a_free_slot(self):
        self.groups("rust")
        four = [self.start() for _ in range(4)]
        self.wait_for_runs(4)
        self.assertEqual(sorted(gate.slot() for gate in four), [1, 2, 3, 4])
        fifth = self.start()
        fifth.wait_for(r"^oby-remote-gate: all four slots are busy; waiting$")
        time.sleep(1.5)
        self.assertEqual(len(self.runs()), 4, "the fifth ran while four slots were held")
        freed = four[2].slot()
        (self.fake / f"release-slot-{freed}").touch()
        self.assertEqual(four[2].finish(), 0, four[2].output())
        self.wait_for_runs(5)
        self.assertEqual(fifth.slot(), freed)
        self.assertEqual(self.runs()[4][0], f"slot-{freed}")
        self.assertEqual(fifth.finish(), 0, fifth.output())
        (self.fake / "release").touch()
        for gate in four:
            self.assertEqual(gate.finish(), 0, gate.output())

    def test_remote_7_dropped_connection_frees_the_slot(self):
        self.groups("rust")
        gate = self.start()
        self.wait_for_runs(1)
        slot = gate.slot()
        check, child = map(int, self.runs()[0][3:5])
        gate.proc.stdin.close()
        self.assertTrue(gone(check), f"the check {check} is still running after standard input closed")
        self.assertTrue(gone(child), f"the check's child {child} is still running after standard input closed")
        self.assertEqual(gate.finish(), 128 + signal.SIGHUP, gate.output())
        fd = os.open(self.root / f"slot-{slot}" / "lock", os.O_RDWR)
        try:
            fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
        finally:
            os.close(fd)
        (self.fake / "release").touch()
        again = self.start()
        self.assertEqual(again.slot(), slot)
        self.assertEqual(again.finish(), 0, again.output())

    def test_remote_8_commit_not_on_origin_runs_nothing(self):
        missing = "0123456789abcdef0123456789abcdef01234567"
        gate = self.start(commit=missing)
        self.assertEqual(gate.finish(), 2, gate.output())
        self.assertIn(f"oby-remote-gate: {missing} isn't on origin", gate.lines)
        self.assertEqual(self.runs(), [])

    def test_failing_group_fails_the_gate_and_the_others_still_run(self):
        self.groups("scan", "swift", "electron", "rust")
        (self.fake / "exit-rust").write_text("1\n")
        (self.fake / "release").touch()
        gate = self.start(base=self.base)
        self.assertEqual(gate.finish(), 1, gate.output())
        self.assertEqual([run[2] for run in self.runs()], ["rust", "electron", "scan"])
        self.assertIn("oby-remote-gate: swift is the Mac's: it doesn't run here", gate.lines)
        self.assertIn("oby-remote-gate: groups rust electron scan", gate.lines)
        self.assertEqual((self.fake / "npm").read_text(), "npm ci\n")
        self.assertRegex(gate.lines[-1], r"^oby-remote-gate: failed in slot 1 after \d+s$")


class Pins(unittest.TestCase):
    def pins(self, ci=None):
        return subprocess.run(
            [str(SETUP), "--pins", *([str(ci)] if ci else [])], capture_output=True, text=True, timeout=30
        )

    def test_setup_pins_are_ci_yml_s(self):
        text = CI.read_text()

        def value(name):
            found = set(re.findall(rf'^\s*{name}:\s*"?([^"\s#]+)"?', text, re.MULTILINE))
            self.assertEqual(len(found), 1, f"{name}: {found}")
            return found.pop()

        done = self.pins()
        self.assertEqual(done.returncode, 0, done.stderr)
        self.assertEqual(
            done.stdout.splitlines(),
            [
                f"rust {value('RUSTUP_TOOLCHAIN')}",
                f"node {value('node-version')}",
                f"gitleaks {value('GITLEAKS_VERSION')} {value('GITLEAKS_SHA256')}",
            ],
        )
        with tempfile.TemporaryDirectory() as folder:
            # The first of its GITLEAKS_VERSIONs changed, the others as they are.
            ci = Path(folder) / "ci.yml"
            ci.write_text(re.sub(r"GITLEAKS_VERSION: \S+", "GITLEAKS_VERSION: 0.0.1", text, count=1))
            done = self.pins(ci)
            self.assertNotEqual(done.returncode, 0, done.stdout)
            self.assertIn("GITLEAKS_VERSION disagree", done.stderr)


if __name__ == "__main__":
    unittest.main(verbosity=2)
