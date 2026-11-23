#!/usr/bin/env python3
"""factory/remote/oby-remote-gate's (its --mac mode too), setup.sh --pins', setup.sh's builder user's,
setup-mac.sh's and tighten.sh's own tests, with no VM and no Mac worker.

Each RemoteGate test makes an origin repository whose scripts/oby-check.sh is a stand-in, and a
root with repo.git cloned from it, as setup.sh makes /srv/oby. Each Tighten test runs the function
setup.sh runs last on the host against a temporary home, root and gate, with this checkout's real
scripts/oby-check.sh. The SetupMac tests run setup-mac.sh with a stand-in ssh that keeps the script
it sends, and run that script against a temporary home.

Run from the repository root: python3 factory/tests/test_remote_gate.py
"""

import fcntl
import importlib.machinery
import importlib.util
import json
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
SETUP_MAC = FACTORY / "remote" / "setup-mac.sh"
TIGHTEN = FACTORY / "remote" / "tighten.sh"
CHECK = FACTORY.parent / "scripts" / "oby-check.sh"
CI = FACTORY.parent / ".github" / "workflows" / "ci.yml"
MODEL = FACTORY.parent / "docs" / "models" / "job_race.qnt"
QUINT = FACTORY / "remote" / "quint"

# The stand-in check: the plan prints $FAKE/groups' groups; a run records its slot, CARGO_TARGET_DIR,
# OBY_CHECK_ONLY, its pid and a background child's (and in $FAKE/env its OBY_BUILD_JOBS, OBY_CHECK_CACHE
# and PATH), waits for $FAKE/release or $FAKE/release-<slot>, and exits with $FAKE/exit-<group>'s code
# (0 without one). Each run, the plan's too, records its OBY_CHECK_SHARD (or `unset`) in $FAKE/shards.
FAKE_CHECK = """#!/bin/sh
echo "${OBY_CHECK_ONLY:-plan} ${OBY_CHECK_SHARD-unset}" >>"$FAKE/shards"
if [ -n "$OBY_CHECK_PLAN" ]; then
    for group in $(cat "$FAKE/groups"); do echo "oby-check: group $group"; done
    echo "oby-check: planned, nothing run"
    exit 0
fi
slot=$(basename "$(dirname "$PWD")")
echo "$OBY_CHECK_ONLY ${CARGO_NET_OFFLINE:-unset} ${npm_config_offline:-unset}" >>"$FAKE/offline"
echo "$OBY_CHECK_ONLY|$OBY_BUILD_JOBS|$OBY_CHECK_CACHE|$PATH" >>"$FAKE/env"
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
        (bin_dir / "npm").write_text(
            f'#!/bin/sh\necho "npm $*" >>"{self.fake}/npm"\n'
            f'echo "${{npm_config_offline:-unset}}" >>"{self.fake}/npm-offline"\n'
        )
        (bin_dir / "npm").chmod(0o755)
        # The host's own environment, as ssh gives it: none of the check's offline settings, which
        # the check that runs these tests has set (REMOTE-9 sets them for the groups alone).
        offline = {"CARGO_NET_OFFLINE", "npm_config_offline"}
        self.env = {k: v for k, v in os.environ.items() if not k.startswith("OBY_") and k not in offline}
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

    def start(self, commit=None, base=None, mac=None, counts=(), shard=()):
        """oby-remote-gate, or with mac (a list of groups) oby-remote-gate --mac; shard, its words after the counts."""
        args = [*counts, *shard, commit or self.commit, base or "origin/dev"]
        proc = subprocess.Popen(
            [str(GATE), *(["--mac", *args, *mac] if mac is not None else args)],
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

    def test_remote_9_groups_run_offline(self):
        self.groups("rust", "electron", "scan")
        (self.fake / "release").touch()
        gate = self.start(base=self.base)
        self.assertEqual(gate.finish(), 0, gate.output())
        self.assertEqual(
            (self.fake / "offline").read_text().splitlines(),
            ["rust true true", "electron true true", "scan true true"],
        )
        # npm ci, before the electron group, still has the network.
        self.assertEqual((self.fake / "npm-offline").read_text(), "unset\n")

    def test_remote_32_gate_locks_are_the_owners_alone(self):
        # The locks A00-31 left at 0644, which another user could open and hold; slot-1's is held, so the gate
        # opens it, takes slot-2 and makes its lock.
        (self.root / "slot-1").mkdir()
        for lock in (self.root / "repo.lock", self.root / "slot-1" / "lock"):
            lock.touch()
            lock.chmod(0o644)
        held = os.open(self.root / "slot-1" / "lock", os.O_RDWR)
        try:
            fcntl.flock(held, fcntl.LOCK_EX | fcntl.LOCK_NB)
            (self.fake / "release").touch()
            gate = self.start()
            self.assertEqual(gate.slot(), 2)
            self.assertEqual(gate.finish(), 0, gate.output())
        finally:
            os.close(held)
        for lock in (self.root / "repo.lock", self.root / "slot-1" / "lock", self.root / "slot-2" / "lock"):
            self.assertEqual(lock.stat().st_mode & 0o777, 0o600, lock)


    # ---- REMOTE-28: --slots and --builder-slots ------------------------------------------------

    def test_remote_28_slots_set_the_slot_count_and_the_core_share(self):
        self.groups("rust")
        counts = ("--slots", "2", "--builder-slots", "2")
        two = [self.start(counts=counts) for _ in range(2)]
        self.wait_for_runs(2)
        self.assertEqual(sorted(gate.slot() for gate in two), [1, 2])
        third = self.start(counts=counts)
        third.wait_for(r"^oby-remote-gate: all 2 slots are busy; waiting$")
        self.assertFalse((self.root / "slot-3").exists(), "a third gate slot was made")
        jobs = str(max(1, (os.cpu_count() or 1) // 4))
        self.assertEqual({line.split("|")[1] for line in (self.fake / "env").read_text().splitlines()}, {jobs})
        (self.fake / "release").touch()
        for gate in [*two, third]:
            self.assertEqual(gate.finish(), 0, gate.output())
        for counts, said in ((("--slots", "0", "--builder-slots", "2"), "--slots takes 1 to 64"),
                             (("--slots", "4", "--builder-slots", "65"), "--slots takes 1 to 64"),
                             (("--slots", "4"), "--slots and --builder-slots each take a number"),
                             (("--slots", "x", "--builder-slots", "2"), "--slots and --builder-slots each take")):
            gate = self.start(counts=counts)
            self.assertEqual(gate.finish(), 2, gate.output())
            self.assertTrue(any(line.startswith(f"oby-remote-gate: {said}") for line in gate.lines), gate.output())

    # ---- REMOTE-17: --mac --------------------------------------------------------------------

    def test_remote_17_mac_runs_only_the_groups_given(self):
        # The plan names other groups: --mac reads none, and runs these, in this order.
        self.groups("rust", "electron", "scan")
        (self.fake / "release").touch()
        gate = self.start(base=self.base, mac=["swift", "factory", "rust"])
        self.assertEqual(gate.finish(), 0, gate.output())
        runs = self.runs()
        self.assertEqual([run[2] for run in runs], ["swift", "factory", "rust"])
        self.assertEqual({run[1] for run in runs}, {str(self.root / "slot-1" / "target")})
        self.assertIn("oby-remote-gate: groups swift factory rust", gate.lines)
        self.assertRegex(gate.lines[-1], r"^oby-remote-gate: passed in slot 1 after \d+s$")
        self.assertEqual((self.fake / "offline").read_text().splitlines(),
                         ["swift true true", "factory true true", "rust true true"])
        jobs = str(max(1, (os.cpu_count() or 1) // 2))
        home = Path.home()
        for line in (self.fake / "env").read_text().splitlines():
            group, build_jobs, cache, path = line.split("|", 3)
            self.assertEqual((build_jobs, cache), (jobs, str(self.root / "slot-1" / "cache")), group)
            self.assertTrue(path.startswith(f"{home}/.cargo/bin:/opt/homebrew/bin:/usr/local/bin:"), path)
        self.assertFalse((self.root / "slot-3").exists(), "--mac made a third slot")

    def test_remote_17_mac_both_slots_busy_ends_at_once(self):
        held = []
        for n in (1, 2):
            (self.root / f"slot-{n}").mkdir()
            fd = os.open(self.root / f"slot-{n}" / "lock", os.O_RDWR | os.O_CREAT, 0o644)
            fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
            held.append(fd)
        # An origin that would fail any fetch, so a fetch would show.
        self.env["OBY_REMOTE_URL"] = str(self.dir / "no-origin")
        try:
            started = time.monotonic()
            gate = self.start(mac=["rust"])
            self.assertEqual(gate.finish(seconds=10), 2, gate.output())
            self.assertLess(time.monotonic() - started, 5)
        finally:
            for fd in held:
                os.close(fd)
        self.assertEqual(gate.lines, ["oby-remote-gate: both slots are busy"])
        self.assertEqual(self.runs(), [])
        self.assertFalse((self.root / "slot-3").exists(), "--mac looked past two slots")

    def test_remote_17_mac_refuses_a_commit_not_on_origin(self):
        missing = "0123456789abcdef0123456789abcdef01234567"
        (self.fake / "release").touch()
        gate = self.start(commit=missing, mac=["rust", "swift"])
        self.assertEqual(gate.finish(), 2, gate.output())
        self.assertIn(f"oby-remote-gate: {missing} isn't on origin", gate.lines)
        self.assertEqual(self.runs(), [])

    def test_remote_17_mac_dropped_connection_frees_the_slot(self):
        gate = self.start(mac=["rust"])
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
        again = self.start(mac=["rust"])
        self.assertEqual(again.slot(), slot)
        self.assertEqual(again.finish(), 0, again.output())

    def test_remote_17_mac_refuses_an_unknown_group(self):
        (self.fake / "release").touch()
        for groups, said in ((["rust", "electron"], "--mac runs only rust, swift, factory, not electron"),
                             (["scan"], "--mac runs only rust, swift, factory, not scan"),
                             ([], "--mac takes a commit, a base and one or more groups")):
            gate = self.start(mac=groups)
            self.assertEqual(gate.finish(), 2, gate.output())
            self.assertIn(f"oby-remote-gate: {said}", gate.lines)
        self.assertEqual(self.runs(), [])
        self.assertFalse((self.root / "slot-1").exists(), "a usage error took a slot")

    # ---- REMOTE-61: --shard -------------------------------------------------------------------

    def shards(self):
        """Each run's group (`plan` for the plan's) and the OBY_CHECK_SHARD it got, or `unset`."""
        path = self.fake / "shards"
        return path.read_text().splitlines() if path.exists() else []

    def test_remote_61_a_later_shard_runs_only_rust_with_its_shard(self):
        self.groups("rust", "electron", "scan")
        (self.fake / "release").touch()
        gate = self.start(base=self.base, shard=("--shard", "2/3"))
        self.assertEqual(gate.finish(), 0, gate.output())
        self.assertEqual([run[2] for run in self.runs()], ["rust"])
        self.assertEqual(self.shards(), ["plan unset", "rust 2/3"])
        self.assertEqual(gate.lines[gate.lines.index(f"oby-remote-gate: slot {gate.slot()}") + 1],
                         "oby-remote-gate: shard 2/3")
        self.assertIn("oby-remote-gate: groups rust", gate.lines)
        self.assertFalse((self.fake / "npm").exists(), "a later shard ran electron's npm ci")
        self.assertRegex(gate.lines[-1], r"^oby-remote-gate: passed in slot 1 after \d+s$")

    def test_remote_61_shard_1_runs_every_linux_group_and_only_rust_gets_the_shard(self):
        self.groups("rust", "electron", "scan")
        (self.fake / "release").touch()
        gate = self.start(base=self.base, counts=("--slots", "4", "--builder-slots", "0"), shard=("--shard", "1/3"))
        self.assertEqual(gate.finish(), 0, gate.output())
        self.assertEqual([run[2] for run in self.runs()], ["rust", "electron", "scan"])
        self.assertEqual(self.shards(), ["plan unset", "rust 1/3", "electron unset", "scan unset"])
        self.assertIn("oby-remote-gate: shard 1/3", gate.lines)

    def test_remote_61_a_later_shard_with_no_rust_planned_runs_nothing_and_passes(self):
        self.groups("electron", "scan")
        gate = self.start(shard=("--shard", "3/3"))
        self.assertEqual(gate.finish(), 0, gate.output())
        self.assertEqual(self.runs(), [])
        self.assertIn("oby-remote-gate: shard 3/3 runs only rust, which isn't planned: nothing to run here", gate.lines)
        self.assertRegex(gate.lines[-1], r"^oby-remote-gate: passed in slot 1 after \d+s$")

    def test_remote_61_mac_gives_the_shard_to_rust_only(self):
        (self.fake / "release").touch()
        gate = self.start(base=self.base, mac=["rust", "factory"], shard=("--shard", "1/2"))
        self.assertEqual(gate.finish(), 0, gate.output())
        self.assertEqual([run[2] for run in self.runs()], ["rust", "factory"])
        self.assertEqual(self.shards(), ["rust 1/2", "factory unset"])
        self.assertIn("oby-remote-gate: shard 1/2", gate.lines)

    def test_remote_61_an_invalid_shard_is_a_usage_error(self):
        (self.fake / "release").touch()
        for shard in (("--shard", "0/2"), ("--shard", "3/2"), ("--shard", "1/65"), ("--shard", "01/2"),
                      ("--shard", "a/b"), ("--shard", "1"), ("--shard",)):
            for mac in (None, ["rust"]):
                gate = self.start(mac=mac, shard=shard)
                self.assertEqual(gate.finish(), 2, gate.output())
                if len(shard) == 2:
                    self.assertIn("oby-remote-gate: --shard takes <k>/<n>, with 1 <= k <= n <= 64 and no leading zero",
                                  gate.lines)
        self.assertEqual((self.runs(), self.shards()), ([], []))
        self.assertFalse((self.root / "slot-1").exists(), "a usage error took a slot")

    def test_remote_61_without_shard_no_shard_reaches_the_check(self):
        self.groups("rust", "scan")
        self.env["OBY_CHECK_SHARD"] = "2/3"
        (self.fake / "release").touch()
        for mac in (None, ["rust"]):
            gate = self.start(base=self.base, mac=mac)
            self.assertEqual(gate.finish(), 0, gate.output())
            self.assertFalse(any("shard" in line for line in gate.lines), gate.output())
        self.assertEqual(self.shards(), ["plan unset", "rust unset", "scan unset", "rust unset"])


# The builder's cargo for REMOTE-34. Its first run leaves what a hostile run could: an ignored file in the worktree,
# strays in the slot and the builder root, a config.toml in its CARGO_HOME, a post-checkout hook with core.hooksPath
# and info/attributes in the clone, and a file in target. Its second records what is still there.
BUILDER_CARGO = """#!/usr/bin/env python3
import json, os, subprocess
from pathlib import Path
fake = Path(os.environ["FAKE"])
target = Path(os.environ["CARGO_TARGET_DIR"])
slot, cargo_home = target.parent, Path(os.environ.get("CARGO_HOME") or Path.home() / ".cargo")
root, worktree = slot.parent, slot / "worktree"
repo = root / "repo.git"
if not (fake / "first").exists():
    (fake / "first").touch()
    (worktree / "ignored").mkdir()
    (worktree / "ignored" / "left").write_text("left\\n")
    (slot / "stray").write_text("stray\\n")
    (root / "stray").write_text("stray\\n")
    cargo_home.mkdir(parents=True, exist_ok=True)
    (cargo_home / "config.toml").write_text("[build]\\n")
    hook = repo / "hooks" / "post-checkout"
    hook.parent.mkdir(exist_ok=True)
    hook.write_text(f"#!/bin/sh\\ntouch '{fake}/hook-ran'\\n")
    hook.chmod(0o755)
    subprocess.run(["git", "config", "--file", str(repo / "config"), "core.hooksPath", str(hook.parent)], check=True)
    (repo / "info").mkdir(exist_ok=True)
    (repo / "info" / "attributes").write_text("* filter=steer\\n")
    target.mkdir(parents=True, exist_ok=True)
    (target / "kept").write_text("kept\\n")
else:
    (fake / "second").write_text(json.dumps({
        "slot": sorted(p.name for p in slot.iterdir()), "root": sorted(p.name for p in root.iterdir()),
        "ignored": (worktree / "ignored").exists(), "cargo_home": str(cargo_home),
        "cargo_config": (cargo_home / "config.toml").exists(), "target": (target / "kept").exists(),
        "hook": (fake / "hook-ran").exists(), "config": (repo / "config").read_text(),
        "attributes": (repo / "info" / "attributes").exists()}))
"""


# A builder's program for REMOTE-56 and 57: it records its name and words in $FAKE/runs, then waits while $FAKE/hold
# exists.
RECORDER = """#!/usr/bin/env python3
import json, os, sys, time
from pathlib import Path
fake = Path(os.environ["FAKE"])
with open(fake / "runs", "a") as kept:
    kept.write(json.dumps([Path(sys.argv[0]).name, *sys.argv[1:]]) + "\\n")
while (fake / "hold").exists():
    time.sleep(0.05)
"""


class BuilderRun(unittest.TestCase):
    """REMOTE-34 and 45: oby-remote-gate --run against a temporary origin and builder root, with a stand-in cargo in a
    temporary home's ~/.cargo/bin."""

    def setUp(self):
        self.dir = Path(tempfile.mkdtemp()).resolve()
        self.fake = self.dir / "fake"
        self.fake.mkdir()
        origin = self.dir / "origin"
        origin.mkdir()
        git("init", "--quiet", "--initial-branch=dev", cwd=origin)
        git("config", "uploadpack.allowAnySHA1InWant", "true", cwd=origin)
        (origin / ".gitignore").write_text("ignored/\n")
        (origin / "core").mkdir()
        (origin / "core" / "a.txt").write_text("a\n")
        git("add", ".", cwd=origin)
        git("-c", "user.name=t", "-c", "user.email=t@t", "commit", "--quiet", "-m", "base", cwd=origin)
        self.base = git("rev-parse", "HEAD", cwd=origin)
        self.tree = git("rev-parse", "HEAD^{tree}", cwd=origin)
        self.root = self.dir / "root"
        self.root.mkdir()
        git("clone", "--quiet", "--bare", str(origin), str(self.root / "repo.git"), cwd=self.dir)
        home = self.dir / "home"
        self.tools = home / ".cargo" / "bin"
        cargo = self.tools / "cargo"
        cargo.parent.mkdir(parents=True)
        cargo.write_text(BUILDER_CARGO)
        cargo.chmod(0o755)
        self.env = {k: v for k, v in os.environ.items() if not k.startswith(("OBY_", "CARGO_"))}
        self.env.update(OBY_REMOTE_ROOT=str(self.root), OBY_REMOTE_URL=str(origin), FAKE=str(self.fake),
                        HOME=str(home), GIT_CONFIG_GLOBAL="/dev/null", GIT_CONFIG_NOSYSTEM="1")

    def tearDown(self):
        shutil.rmtree(self.dir)

    def start_run(self, *command, builders=1):
        proc = subprocess.Popen([str(GATE), "--run", "--slots", "4", "--builder-slots", str(builders), "--wait", "5",
                                 self.base, self.tree, ".", "--", *command],
                                env=self.env, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        proc.stdin.write(b"0\n")
        proc.stdin.flush()
        return proc

    def finish(self, proc):
        out = proc.stdout.read().decode()
        code = proc.wait(timeout=60)
        proc.stdin.close()
        proc.stdout.close()
        return code, out

    def builder_run(self, *command, builders=1):
        return self.finish(self.start_run(*(command or ("cargo", "test")), builders=builders))

    def recorded(self):
        """The words each RECORDER program ran with, its name first."""
        path = self.fake / "runs"
        return [json.loads(line) for line in path.read_text().splitlines()] if path.exists() else []

    def record(self, *names):
        """RECORDER as each program named in the builder's ~/.cargo/bin, with a java that runs nothing."""
        for name in names:
            (self.tools / name).write_text(RECORDER)
            (self.tools / name).chmod(0o755)
        (self.tools / "java").write_text("#!/bin/sh\nexit 0\n")
        (self.tools / "java").chmod(0o755)

    def test_remote_34_each_builder_run_starts_clean(self):
        for _ in range(2):
            code, out = self.builder_run()
            self.assertEqual(code, 0, out)
            self.assertIn("oby-remote-gate: running in builder slot 1", out)
        second = json.loads((self.fake / "second").read_text())
        slot = self.root / "builder-1"
        self.assertEqual(second["slot"], ["lock", "target", "worktree"], second)
        self.assertEqual(second["root"], ["builder-1", "repo.git", "repo.lock"], second)
        self.assertFalse(second["ignored"], "the first run's ignored file is still in the worktree")
        self.assertEqual(second["cargo_home"], str(slot / "cargo"))
        self.assertFalse(second["cargo_config"], "the first run's CARGO_HOME is still there")
        self.assertTrue(second["target"], "the slot's target was emptied")
        self.assertFalse(second["hook"], "the first run's post-checkout hook ran")
        self.assertNotIn("hooksPath", second["config"])
        self.assertIn("bare = true", second["config"])
        self.assertFalse(second["attributes"], "the first run's info/attributes is still there")

    def test_remote_56_each_builder_slot_has_its_own_apalache_port(self):
        loader = importlib.machinery.SourceFileLoader("oby_remote_gate", str(GATE))
        runner = importlib.util.module_from_spec(importlib.util.spec_from_loader(loader.name, loader))
        loader.exec_module(runner)
        for count in range(1, runner.MAX_SLOTS + 1):
            ports = [runner.apalache_port(slot) for slot in range(1, count + 1)]
            self.assertEqual(len(set(ports)), count, ports)
            self.assertNotIn(8822, ports)
        self.record("quint")
        for command in (["quint", "verify", "--max-steps=12", "x.qnt"], ["quint", "compile", "x.qnt"],
                        [str(self.tools / "quint"), "verify", "x.qnt"]):
            code, out = self.builder_run(*command)
            self.assertEqual(code, 0, out)
            self.assertIn("oby-remote-gate: running in builder slot 1", out)
        self.assertEqual(self.recorded(), [["quint", "verify", "--server-endpoint=localhost:8823", "--max-steps=12",
                                            "x.qnt"],
                                           ["quint", "compile", "--server-endpoint=localhost:8823", "x.qnt"],
                                           ["quint", "verify", "--server-endpoint=localhost:8823", "x.qnt"]])

    def test_remote_56_two_quint_runs_at_once_get_different_ports(self):
        self.record("quint")
        (self.fake / "hold").touch()
        runs = [self.start_run("quint", "verify", "x.qnt", builders=2) for _ in range(2)]
        try:
            deadline = time.monotonic() + 30
            while len(self.recorded()) < 2 and time.monotonic() < deadline:
                time.sleep(0.05)
            held = self.recorded()
        finally:
            (self.fake / "hold").unlink()
        ended = [self.finish(run) for run in runs]
        for code, out in ended:
            self.assertEqual(code, 0, out)
        self.assertEqual(sorted(held), [["quint", "verify", "--server-endpoint=localhost:8823", "x.qnt"],
                                        ["quint", "verify", "--server-endpoint=localhost:8824", "x.qnt"]], ended)
        self.assertEqual(sorted(re.search(r"running in builder slot (\d)", out).group(1) for _, out in ended),
                         ["1", "2"])

    def test_remote_57_a_quint_run_naming_its_own_server_is_refused(self):
        self.record("quint")
        for words in (["verify", "--server-endpoint=localhost:8822", "x.qnt"],
                      ["verify", "x.qnt", "--server-endpoint", "localhost:8824"],
                      ["compile", "--serverEndpoint=localhost:9000", "x.qnt"],
                      ["run", "--serverEndpoint", "localhost:8825", "x.qnt"]):
            code, out = self.builder_run("quint", *words)
            self.assertEqual(code, 2, out)
            named = next(word for word in words if word.startswith("--server"))
            self.assertIn(f"oby-remote-gate: a quint run in a builder slot can't name its own Apalache server: {named}",
                          out.splitlines())
            self.assertNotIn("running in builder slot", out)
        self.assertEqual(self.recorded(), [])

    def test_remote_57_a_quint_option_before_its_subcommand_is_refused(self):
        self.record("quint")
        for words in (["--max-steps", "3", "verify", "x.qnt"], ["--verbosity=3", "verify", "x.qnt"]):
            code, out = self.builder_run("quint", *words)
            self.assertEqual(code, 2, out)
            self.assertIn(f"oby-remote-gate: a quint run in a builder slot takes its subcommand first: {words[0]}",
                          out.splitlines())
            self.assertNotIn("running in builder slot", out)
        self.assertEqual(self.recorded(), [])
        # An option alone still runs.
        for word in ("--version", "--help"):
            code, out = self.builder_run("quint", word)
            self.assertEqual(code, 0, out)
        self.assertEqual(self.recorded(), [["quint", "--version"], ["quint", "--help"]])

    def test_remote_57_other_commands_are_unchanged(self):
        self.record("quint", "cargo")
        commands = (["quint", "run", "--max-steps=5", "x.qnt"], ["quint", "typecheck", "x.qnt"],
                    ["quint", "test", "--main=x", "x.qnt"], ["quint", "--version"], ["cargo", "test"],
                    ["cargo", "test", "--", "--server-endpoint=localhost:8822"])
        for command in commands:
            code, out = self.builder_run(*command)
            self.assertEqual(code, 0, out)
            self.assertIn("oby-remote-gate: running in builder slot 1", out)
        self.assertEqual(self.recorded(), [list(command) for command in commands])

    def test_remote_45_the_vms_builder_clone_config_is_unchanged(self):
        # git clone --bare wrote this Mac's own settings into the clone; on the VM the reset writes setup.sh's text.
        (self.fake / "first").touch()
        code, out = self.builder_run()
        self.assertEqual(code, 0, out)
        config = "[core]\n\trepositoryformatversion = 0\n\tfilemode = true\n\tbare = true\n[gc]\n\tauto = 0\n"
        self.assertEqual(json.loads((self.fake / "second").read_text())["config"], config)
        self.assertEqual((self.root / "repo.git" / "config").read_text(), config)


class SetupBuilder(unittest.TestCase):
    """REMOTE-30: setup.sh's host script sets up oby-builder with no sudo. A stand-in ssh keeps the script setup.sh
    sends; its builder function runs against stand-in sudo, id and getent, which record and run nothing, and stand-in
    curl and sha256sum, which record their calls in the same log and download nothing."""

    def setUp(self):
        self.dir = Path(tempfile.mkdtemp())
        self.bin = self.dir / "bin"
        self.bin.mkdir()
        self.sent = self.dir / "sent.sh"
        self.tool("ssh", f'if [ "$2 $3" = "bash -s" ]; then cat >"{self.sent}"; else cat >/dev/null; fi')
        self.log = self.dir / "sudo.log"
        # sudo: one line per call; `sudo -n -l -U` prints $dir/rights; `find ... ! -user root` prints a path when
        # $dir/not-root exists; the builder's proof (`bash` as the builder) fails when $dir/proof-fails exists, its
        # Quint proof (`bwrap` as the builder) when $dir/quint-fails does; `test` as the builder finds nothing but the
        # `-w <path>` in $dir/writable.
        self.tool("sudo", f'printf "%s\\n" "$*" | tr "\\n" " " >>"{self.log}"; echo >>"{self.log}"\n'
                          f'if [ "$2 $3" = "-l -U" ]; then cat "{self.dir}/rights"; fi\n'
                          f'case "$*" in *"! -user root"*) if [ -e "{self.dir}/not-root" ]; then echo "$3"; fi;; esac\n'
                          f'if [ "$2 $5" = "-u bash" ] && [ -e "{self.dir}/proof-fails" ]; then exit 1; fi\n'
                          f'if [ "$2 $5" = "-u bwrap" ] && [ -e "{self.dir}/quint-fails" ]; then exit 1; fi\n'
                          f'[ "$5" != test ] || [ "$6 $7" = "$(cat "{self.dir}/writable" 2>/dev/null)" ]')
        self.tool("id", "exit 1")
        self.tool("java", 'echo \'openjdk version "21.0.9" 2025-10-21\' >&2')
        # curl writes an empty file where -o says; sha256sum keeps what it is asked to check in $dir/sums and fails
        # when $dir/bad-sum exists.
        self.tool("curl", f'echo "curl $*" >>"{self.log}"\n: >"$2"')
        self.tool("sha256sum", f'echo "sha256sum $*" >>"{self.log}"\ncat >>"{self.dir}/sums"\n'
                               f'[ ! -e "{self.dir}/bad-sum" ]')
        self.tool("getent", f'echo "oby-builder:x:1001:1001::{self.dir}/builder-home:/bin/bash"')
        self.rights("User oby-builder is not allowed to run sudo on oby-gate.")
        done = subprocess.run([str(SETUP)], env=dict(os.environ, PATH=f"{self.bin}{os.pathsep}{os.environ['PATH']}"),
                              capture_output=True, text=True, timeout=60)
        self.assertEqual(done.returncode, 0, done.stderr)
        self.assertEqual(subprocess.run(["bash", "-n", str(self.sent)]).returncode, 0)
        # Everything the script defines, without its last line, which runs setup.
        text = self.sent.read_text()
        self.assertTrue(text.endswith("\nsetup </dev/null\n"), text[-200:])
        self.functions = self.dir / "functions.sh"
        self.functions.write_text(text.removesuffix("setup </dev/null\n"))

    def tearDown(self):
        shutil.rmtree(self.dir)

    def tool(self, name, body):
        path = self.bin / name
        path.write_text(f"#!/bin/sh\n{body}\n")
        path.chmod(0o755)

    def rights(self, text):
        (self.dir / "rights").write_text(text + "\n")

    def builder(self, *quint):
        """The host script's builder function; with a prefix and a link, Quint's in place of /usr/local's."""
        return subprocess.run(["bash", "-c", '. "$0"; if (($#)); then QUINT_PREFIX=$1 QUINT_LINK=$2; fi; builder',
                               str(self.functions), *map(str, quint)], cwd=self.dir,
                              env=dict(os.environ, PATH=f"{self.bin}{os.pathsep}{os.environ['PATH']}",
                                       HOME=str(self.dir)),
                              stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, timeout=60)

    def calls(self):
        return [line.strip() for line in self.log.read_text().splitlines()] if self.log.exists() else []

    def test_remote_30_setup_makes_a_builder_user_without_sudo(self):
        self.assertIn("    builder\n}", self.sent.read_text())
        done = self.builder()
        self.assertEqual(done.returncode, 0, done.stdout)
        calls = self.calls()
        self.assertEqual(calls[:2], ["-n useradd --create-home --shell /bin/bash oby-builder",
                                     "-n -l -U oby-builder"])
        self.assertFalse([c for c in calls if re.search(r"usermod|gpasswd|adduser|sudoers|visudo| -G ", c)], calls)
        home = self.dir / "builder-home"
        self.assertIn(f"-n install -o root -g root -m 644 {self.dir}/.ssh/authorized_keys "
                      f"{home}/.ssh/authorized_keys", calls)
        self.assertIn("-n install -d -o oby-builder -g oby-builder -m 755 /srv/oby-builder", calls)
        self.assertIn("-n -u oby-builder -H git clone --bare --quiet https://github.com/obi-ai-bot/oby.git "
                      "/srv/oby-builder/repo.git", calls)
        self.assertIn("-n -u oby-builder -H git -C /srv/oby-builder/repo.git config gc.auto 0", calls)
        rust = [c for c in calls if "rustup toolchain install" in c]
        self.assertEqual(len(rust), 1, calls)
        self.assertTrue(rust[0].startswith(f"-n env RUSTUP_HOME={home}/.rustup CARGO_HOME={home}/.cargo bash -c "),
                        rust[0])
        self.assertTrue(rust[0].endswith(" _ " + re.search(r"^RUST=(\S+)$", self.sent.read_text(), re.M).group(1)),
                        rust[0])
        tightened = [c for c in calls if "tighten_writable" in c]
        self.assertEqual(len(tightened), 1, calls)
        self.assertTrue(tightened[0].startswith("-n bash -c "), tightened[0])
        self.assertTrue(tightened[0].endswith(f" _ {home} /srv/oby-builder"), tightened[0])
        self.assertFalse([c for c in calls if "/srv/oby " in c + " " or "/srv/oby/" in c], calls)

    def test_remote_30_a_builder_user_with_sudo_stops_setup(self):
        self.rights("User oby-builder may run the following commands on oby-gate:\n    (ALL) NOPASSWD: ALL")
        done = self.builder()
        self.assertEqual(done.returncode, 1, done.stdout)
        self.assertIn("setup.sh: oby-builder may run commands with sudo, and must not:", done.stdout)
        self.assertEqual(self.calls()[-1], "-n -l -U oby-builder", "setup went on after the check")

    def test_remote_32_setup_closes_the_gate_root(self):
        # A sudo that runs what it is given, but install's -o/-g, which need root: the root already exists.
        self.tool("sudo", f'echo "$*" >>"{self.log}"\nshift\nif [ "$1" = install ]; then exit 0; fi\nexec "$@"')
        root = self.dir / "srv-oby"
        root.mkdir(mode=0o755)
        root.chmod(0o755)
        done = subprocess.run(["bash", "-c", '. "$0"; gate_root "$1" "$2"', str(self.functions), str(root), "oby"],
                              env=dict(os.environ, PATH=f"{self.bin}{os.pathsep}{os.environ['PATH']}"),
                              stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, timeout=60)
        self.assertEqual(done.returncode, 0, done.stdout)
        self.assertEqual(root.stat().st_mode & 0o777, 0o700, self.calls())
        self.assertIn(f"-n install -d -o oby -g oby {root}", self.calls())
        self.assertIn('    gate_root "$root" "$user"\n', self.sent.read_text())

    def test_remote_33_setup_leaves_the_builder_no_key_it_can_change(self):
        done = self.builder()
        self.assertEqual(done.returncode, 0, done.stdout)
        calls = self.calls()
        home = self.dir / "builder-home"
        self.assertIn(f"-n chown root:root {home}", calls)
        self.assertIn(f"-n chmod 755 {home}", calls)
        made = calls.index(f"-n install -d -o root -g root -m 755 {home}/.ssh")
        self.assertLess(calls.index(f"-n rm -rf {home}/.ssh"), made, calls)
        self.assertIn(f"-n install -o root -g root -m 644 {self.dir}/.ssh/authorized_keys {home}/.ssh/authorized_keys",
                      calls)
        self.assertIn(f"-n find {home} -mindepth 1 -maxdepth 1 ! -name .ssh ! -name .cargo ! -name .rustup "
                      "! -name .kani ! -name .quint -exec rm -rf {} +", calls)
        self.assertFalse([c for c in calls if "-o oby-builder" in c and str(home) in c], calls)
        # As the builder: only checks, its clone in /srv/oby-builder, and the toolchain run from a scratch folder.
        for call in (c for c in calls if c.startswith("-n -u oby-builder -H ")):
            words = call.removeprefix("-n -u oby-builder -H ")
            if words.startswith("test "):
                continue
            self.assertNotIn(str(home), words, call)
            self.assertFalse(re.search(r"rustup|curl|chmod|tighten|install", words), call)
            self.assertTrue(words.startswith(("git clone ", "git -C /srv/oby-builder/repo.git ", "bash -c set -euo",
                                              "bwrap --unshare-net ")), call)
        proof = [c for c in calls if c.startswith("-n -u oby-builder -H bash -c ") and "cargo kani --version" in c]
        self.assertEqual(len(proof), 1, calls)
        self.assertIn('export CARGO_HOME="$scratch/cargo"', proof[0])
        for word in ("cargo --version", "rustc --version", "mktemp -d"):
            self.assertIn(word, proof[0])
        checked = [c.removeprefix("-n -u oby-builder -H test -w ") for c in calls if " test -w " in c]
        kani = f"{home}/.kani/kani-" + re.search(r"^KANI=(\S+)$", self.sent.read_text(), re.M).group(1)
        self.assertEqual(checked, [str(home), f"{home}/.ssh", f"{home}/.ssh/authorized_keys", f"{home}/.cargo",
                                   f"{home}/.cargo/bin", f"{home}/.rustup", f"{home}/.kani", kani, f"{kani}/bin",
                                   f"{home}/.quint"])
        self.assertIn("setup.sh: oby-builder is set up", done.stdout)

    def test_remote_33_setup_stops_when_the_builder_can_still_write_its_logins(self):
        home = self.dir / "builder-home"
        (self.dir / "writable").write_text(f"-w {home}/.ssh/authorized_keys\n")
        done = self.builder()
        self.assertNotEqual(done.returncode, 0, done.stdout)
        self.assertIn(f"setup.sh: oby-builder can still write {home}/.ssh/authorized_keys", done.stdout.splitlines())
        self.assertNotIn("setup.sh: oby-builder is set up", done.stdout)

    def test_remote_34_setup_installs_the_builders_toolchain_as_root(self):
        done = self.builder()
        self.assertEqual(done.returncode, 0, done.stdout)
        home = self.dir / "builder-home"
        calls = self.calls()
        [rust] = [c for c in calls if "rustup toolchain install" in c]
        self.assertTrue(rust.startswith(f"-n env RUSTUP_HOME={home}/.rustup CARGO_HOME={home}/.cargo bash -c "), rust)
        self.assertNotIn("-u oby-builder", rust)
        self.assertIn("--no-modify-path", rust)
        self.assertIn("rustfmt,clippy", rust)
        self.assertFalse([c for c in calls if "-u oby-builder" in c and re.search(r"rustup |curl ", c)], calls)
        self.assertNotIn(f"-n rm -rf {home}/.cargo {home}/.rustup {home}/.kani {home}/.quint", calls)
        # A toolchain with anything not root's in it (A00-31's, the builder's own) goes before the install.
        (self.dir / "not-root").touch()
        self.log.unlink()
        done = self.builder()
        self.assertEqual(done.returncode, 0, done.stdout)
        calls = self.calls()
        removed = calls.index(f"-n rm -rf {home}/.cargo {home}/.rustup {home}/.kani {home}/.quint")
        self.assertLess(removed, next(i for i, c in enumerate(calls) if "rustup toolchain install" in c), calls)
        self.assertIn(f"removing {home}/.cargo, {home}/.rustup, {home}/.kani and {home}/.quint: not all root's",
                      done.stdout)

    def test_remote_47_setup_installs_kani_for_the_builder_as_root(self):
        done = self.builder()
        self.assertEqual(done.returncode, 0, done.stdout)
        home = self.dir / "builder-home"
        calls = self.calls()
        kani = re.search(r"^KANI=(\S+)$", self.sent.read_text(), re.M).group(1)
        [install] = [c for c in calls if "kani-verifier" in c]
        self.assertTrue(install.startswith(f"-n env RUSTUP_HOME={home}/.rustup CARGO_HOME={home}/.cargo "
                                           f"KANI_HOME={home}/.kani bash -c "), install)
        self.assertIn("cargo install --locked kani-verifier --version", install)
        self.assertIn("cargo kani setup", install)
        self.assertTrue(install.endswith(f" _ {kani}"), install)
        self.assertFalse([c for c in calls if "-u oby-builder" in c and re.search(r"kani-verifier|kani setup", c)],
                         calls)
        rust = next(i for i, c in enumerate(calls) if "rustup toolchain install" in c)
        installed = calls.index(install)
        given = calls.index(f"-n chown -R -h root:root {home}/.kani")
        tightened = next(i for i, c in enumerate(calls) if "tighten_writable" in c)
        self.assertLess(rust, installed, calls)
        self.assertLess(installed, given, calls)
        self.assertLess(given, tightened, calls)
        [proof] = [c for c in calls if c.startswith("-n -u oby-builder -H bash -c ") and "cargo kani --version" in c]
        for word in ("cargo kani --version", "CARGO_NET_OFFLINE=true", "VERIFICATION:- SUCCESSFUL", "#[kani::proof]",
                     "kani::any()"):
            self.assertIn(word, proof)
        self.assertTrue(proof.endswith(f" _ {kani}"), proof)
        self.assertIn(f"setup.sh: oby-builder is set up: no sudo, /srv/oby-builder, rust ", done.stdout)
        self.assertIn(f", kani {kani}, its home, keys and toolchain root's", done.stdout)

    def test_remote_47_setup_stops_when_kani_fails_for_the_builder(self):
        (self.dir / "proof-fails").touch()
        done = self.builder()
        self.assertNotEqual(done.returncode, 0, done.stdout)
        self.assertNotIn("setup.sh: oby-builder is set up", done.stdout)
        calls = self.calls()
        # It stopped at the builder's proof of the Kani root installed, not before.
        self.assertEqual(len([c for c in calls if "cargo install --locked kani-verifier" in c]), 1, calls)
        self.assertIn("cargo kani --version", calls[-1])
        self.assertFalse([c for c in calls if " test -w " in c], "setup went on after the proof failed")

    def test_mutants_11_setup_installs_cargo_mutants_for_the_builder_as_root(self):
        self.assertIn("\nMUTANTS=27.1.0\n", self.sent.read_text())
        done = self.builder()
        self.assertEqual(done.returncode, 0, done.stdout)
        home = self.dir / "builder-home"
        calls = self.calls()
        [install] = [c for c in calls if "cargo install --locked cargo-mutants --version" in c]
        self.assertTrue(install.startswith(f"-n env RUSTUP_HOME={home}/.rustup CARGO_HOME={home}/.cargo bash -c "),
                        install)
        self.assertTrue(install.endswith(" _ 27.1.0"), install)
        self.assertIn('grep -qx "cargo-mutants v$1:"', install)
        self.assertFalse([c for c in calls if "-u oby-builder" in c and re.search(r"install.*cargo-mutants", c)],
                         calls)
        rust = next(i for i, c in enumerate(calls) if "rustup toolchain install" in c)
        tightened = next(i for i, c in enumerate(calls) if "tighten_writable" in c)
        self.assertLess(rust, calls.index(install), calls)
        self.assertLess(calls.index(install), tightened, calls)
        [check] = [c for c in calls if c.startswith("-n -u oby-builder -H bash -c set -euo")
                   and "cargo mutants --version" in c]
        self.assertIn('export CARGO_HOME="$scratch/cargo"', check)
        self.assertIn('== "cargo-mutants $1"', check)
        self.assertTrue(check.endswith(" _ 27.1.0"), check)
        self.assertIn("setup.sh: oby-builder is set up", done.stdout)

    def test_mutants_11_setup_stops_when_cargo_mutants_fails_for_the_builder(self):
        # A sudo that fails only the builder's cargo-mutants check, and is the stand-in above for everything else.
        (self.bin / "sudo").rename(self.bin / "stand-in-sudo")
        self.tool("sudo", f'case "$*" in *"-u oby-builder -H bash -c "*"cargo mutants --version"*)\n'
                          f'    printf "%s\\n" "$*" | tr "\\n" " " >>"{self.log}"; echo >>"{self.log}"; exit 1;;\n'
                          f'esac\nexec "{self.bin}/stand-in-sudo" "$@"')
        done = self.builder()
        self.assertNotEqual(done.returncode, 0, done.stdout)
        self.assertNotIn("setup.sh: oby-builder is set up", done.stdout)
        calls = self.calls()
        self.assertIn("cargo mutants --version", calls[-1])
        self.assertEqual(len([c for c in calls if "cargo kani --version" in c]), 1, calls)
        self.assertFalse([c for c in calls if " test -w " in c], "setup went on after the check failed")

    def test_remote_49_setup_installs_quint_for_the_builder_as_root(self):
        done = self.builder()
        self.assertEqual(done.returncode, 0, done.stdout)
        home = self.dir / "builder-home"
        calls = self.calls()
        quint = re.search(r"^QUINT=(\S+)$", self.sent.read_text(), re.M).group(1)
        self.assertEqual(quint, "0.33.1")
        [install] = [c for c in calls if " npm " in c]
        self.assertTrue(install.endswith(f" npm ci --ignore-scripts --prefix /usr/local/lib/quint/{quint}"), install)
        self.assertTrue(install.startswith("-n env npm_config_cache="), install)
        self.assertNotIn("-u oby-builder", install)
        cache = install.removeprefix("-n env npm_config_cache=").split()[0]
        self.assertFalse(cache.startswith((str(home), str(self.dir / ".npm"))), install)
        self.assertIn(f"-n rm -rf {cache}", calls)
        self.assertIn(f"-n ln -sfn /usr/local/lib/quint/{quint}/node_modules/.bin/quint /usr/local/bin/quint", calls)
        # Apalache and the evaluator: fetched by the ssh user, checked against setup.sh's SHA-256s, unpacked by root.
        sent = self.sent.read_text()
        pins = {name: re.search(rf"^{name}=(\S+)$", sent, re.M).group(1)
                for name in ("QUINT_APALACHE", "QUINT_APALACHE_SHA", "QUINT_EVALUATOR", "QUINT_EVALUATOR_SHA")}
        self.assertEqual((pins["QUINT_APALACHE"], pins["QUINT_EVALUATOR"]), ("0.62.4", "v0.7.1"))
        fetched = [c for c in calls if c.startswith("curl ")]
        self.assertEqual([c.split()[-1] for c in fetched], [
            "https://github.com/apalache-mc/apalache/releases/download/v0.62.4/apalache.tgz",
            "https://github.com/quint-co/quint/releases/download/evaluator%2Fv0.7.1/"
            "quint_evaluator-x86_64-unknown-linux-gnu.tar.gz"])
        archives = [c.split()[-2] for c in fetched]
        self.assertEqual((self.dir / "sums").read_text().splitlines(),
                         [f"{pins['QUINT_APALACHE_SHA']}  {archives[0]}", f"{pins['QUINT_EVALUATOR_SHA']}  {archives[1]}"])
        for name in ("QUINT_APALACHE_SHA", "QUINT_EVALUATOR_SHA"):
            self.assertRegex(pins[name], r"^[0-9a-f]{64}$")
        checked_sums = calls.index("sha256sum -c -")
        apalache, evaluator = f"{home}/.quint/apalache-dist-0.62.4", f"{home}/.quint/rust-evaluator-v0.7.1"
        unpacked = [calls.index(f"-n tar -xzf {archives[0]} --no-same-owner -C {apalache} apalache"),
                    calls.index(f"-n tar -xzf {archives[1]} --no-same-owner -C {evaluator} quint_evaluator")]
        made = calls.index(f"-n install -d -o root -g root -m 755 {home}/.quint {apalache} {evaluator}")
        self.assertLess(max(calls.index(c) for c in fetched), checked_sums, calls)
        self.assertLess(checked_sums, calls.index(f"-n rm -rf {home}/.quint"), calls)
        self.assertLess(calls.index(f"-n rm -rf {home}/.quint"), made, calls)
        self.assertLess(made, min(unpacked), calls)
        # Root only installs and unpacks: no Quint, Node or Java, and no program from .quint, runs as root.
        root = [c for c in calls if c.startswith("-n ") and not c.startswith("-n -u oby-builder ")]
        for call in root:
            self.assertNotRegex(call, r"\bquint (run|verify|typecheck|test|repl|compile|docs|parse|--version)\b", call)
            self.assertNotIn("bwrap", call)
            self.assertNotRegex(call, rf"(^|\s)HOME={re.escape(str(home))}(\s|$)", call)
            if " bash -c " in call:
                self.assertNotIn("quint", call)
            words = [w for w in call.split()[1:] if w != "env" and "=" not in w]
            self.assertNotIn(words[0].rsplit("/", 1)[-1], ("quint", "node", "npx", "java", "apalache-mc",
                                                            "quint_evaluator", "cli.js"), call)
        self.assertFalse([c for c in calls if "-u oby-builder" in c and "npm" in c], calls)
        kani = next(i for i, c in enumerate(calls) if "kani-verifier" in c)
        installed = calls.index(install)
        given = calls.index(f"-n chown -R -h root:root {home}/.quint")
        closed = calls.index(f"-n chmod -R go-w {home}/.quint")
        tightened = next(i for i, c in enumerate(calls) if "tighten_writable" in c)
        self.assertLess(kani, installed, calls)
        self.assertLess(installed, made, calls)
        self.assertLess(max(unpacked), given, calls)
        self.assertLess(given, closed, calls)
        self.assertLess(closed, tightened, calls)
        self.assertIn(f"-n find {home} -mindepth 1 -maxdepth 1 ! -name .ssh ! -name .cargo ! -name .rustup "
                      "! -name .kani ! -name .quint -exec rm -rf {} +", calls)
        # The builder's proof, Quint's one run: no network, its own process space, / read-only and a scratch /tmp.
        [proof] = [c for c in calls if c.startswith("-n -u oby-builder -H bwrap ")]
        self.assertLess(tightened, calls.index(proof), calls)
        for word in ("--unshare-net", "--unshare-pid", "--die-with-parent", "--ro-bind / /", "--tmpfs /tmp",
                     "java -version", "quint --version", "quint typecheck", "quint run --invariant=",
                     "quint verify --max-steps=3 --invariant="):
            self.assertIn(word, proof)
        self.assertNotIn("--bind / /", proof.replace("--ro-bind / /", ""))
        self.assertTrue(proof.split(" _ ", 1)[1].startswith(f"{quint} "), proof)
        checked = [c.removeprefix("-n -u oby-builder -H test -w ") for c in calls if " test -w " in c]
        self.assertIn(f"{home}/.quint", checked)
        self.assertLess(calls.index(proof), calls.index(f"-n -u oby-builder -H test -w {home}"), calls)
        lines = done.stdout.splitlines()
        said = lines.index(f"setup.sh: oby-builder has quint {quint} and java 21.0.9")
        self.assertTrue(lines[said + 1].startswith("setup.sh: oby-builder is set up: no sudo"), done.stdout)

    def test_remote_49_setup_stops_when_a_quint_download_fails_its_checksum(self):
        (self.dir / "bad-sum").touch()
        done = self.builder()
        self.assertNotEqual(done.returncode, 0, done.stdout)
        calls = self.calls()
        self.assertEqual(calls[-1], "sha256sum -c -", calls)
        self.assertNotIn("setup.sh: oby-builder is set up", done.stdout)

    def test_remote_49_setup_stops_when_quint_fails_for_the_builder(self):
        (self.dir / "quint-fails").touch()
        done = self.builder()
        self.assertNotEqual(done.returncode, 0, done.stdout)
        self.assertNotIn("setup.sh: oby-builder is set up", done.stdout)
        self.assertNotIn("setup.sh: oby-builder has quint", done.stdout)
        calls = self.calls()
        self.assertTrue(calls[-1].startswith("-n -u oby-builder -H bwrap --unshare-net "), calls[-1])
        self.assertIn("quint verify", calls[-1])
        self.assertFalse([c for c in calls if " test -w " in c], "setup went on after the Quint proof failed")

    def test_remote_49_setup_stops_when_the_builder_can_write_its_quint_folder(self):
        home = self.dir / "builder-home"
        (self.dir / "writable").write_text(f"-w {home}/.quint\n")
        done = self.builder()
        self.assertNotEqual(done.returncode, 0, done.stdout)
        self.assertIn(f"setup.sh: oby-builder can still write {home}/.quint", done.stdout.splitlines())
        self.assertNotIn("setup.sh: oby-builder is set up", done.stdout)

    def test_remote_49_setup_checks_every_folder_in_the_quint_folder(self):
        home = self.dir / "builder-home"
        for name in ("apalache-dist-0.62.4", "rust-evaluator-v0.7.1"):
            (home / ".quint" / name).mkdir(parents=True)
        (home / ".quint" / "a-file").touch()
        (self.dir / "writable").write_text(f"-w {home}/.quint/rust-evaluator-v0.7.1\n")
        done = self.builder()
        self.assertNotEqual(done.returncode, 0, done.stdout)
        checked = [c.removeprefix("-n -u oby-builder -H test -w ") for c in self.calls() if " test -w " in c]
        self.assertEqual(checked[-3:], [f"{home}/.quint", f"{home}/.quint/apalache-dist-0.62.4",
                                        f"{home}/.quint/rust-evaluator-v0.7.1"])
        self.assertIn(f"setup.sh: oby-builder can still write {home}/.quint/rust-evaluator-v0.7.1",
                      done.stdout.splitlines())

    def test_remote_53_setup_installs_quint_from_the_lockfile_with_npm_ci(self):
        # The two files travel in the one host script, byte for byte as committed.
        given = self.dir / "given"
        given.mkdir()
        subprocess.run(["bash", "-c", '. "$0"; printf "%s\\n" "$QUINT_PACKAGE" >"$1"; printf "%s\\n" "$QUINT_LOCK" >"$2"',
                        str(self.functions), str(given / "package.json"), str(given / "package-lock.json")], check=True)
        for name in ("package.json", "package-lock.json"):
            self.assertEqual((given / name).read_bytes(), (QUINT / name).read_bytes(), name)
        prefix = "/usr/local/lib/quint/0.33.1"
        self.assertIn(f"\nQUINT_PREFIX={prefix}\nQUINT_LINK=/usr/local/bin/quint\n", self.sent.read_text())
        # A sudo that keeps the files it installs into a folder, and is the stand-in above for everything else.
        installed = self.dir / "installed"
        installed.mkdir()
        (self.bin / "sudo").rename(self.bin / "stand-in-sudo")
        self.tool("sudo", f'case "$*" in "-n install -o root -g root -m 644 "*) cp "$9" "${{10}}" "{installed}";; esac\n'
                          f'exec "{self.bin}/stand-in-sudo" "$@"')
        done = self.builder()
        self.assertEqual(done.returncode, 0, done.stdout)
        calls = self.calls()
        removed = calls.index(f"-n rm -rf {prefix}")
        made = calls.index(f"-n install -d -o root -g root -m 755 {prefix}")
        [written] = [c for c in calls if c.startswith("-n install -o root -g root -m 644 ") and c.endswith(f" {prefix}")]
        self.assertEqual([Path(w).name for w in written.split()[-3:-1]], ["package.json", "package-lock.json"], written)
        for name in ("package.json", "package-lock.json"):
            self.assertEqual((installed / name).read_bytes(), (QUINT / name).read_bytes(), name)
        [ci] = [c for c in calls if "npm" in c.split()]
        cache = ci.removeprefix("-n env npm_config_cache=").split()[0]
        self.assertEqual(ci, f"-n env npm_config_cache={cache} npm ci --ignore-scripts --prefix {prefix}")
        given_to_root = calls.index(f"-n chown -R -h root:root {prefix}")
        linked = calls.index(f"-n ln -sfn {prefix}/node_modules/.bin/quint /usr/local/bin/quint")
        order = [removed, made, calls.index(written), calls.index(ci), calls.index(f"-n rm -rf {cache}"), given_to_root,
                 linked]
        self.assertEqual(order, sorted(order), calls)
        self.assertFalse([c for c in calls if "npm install" in c or ("-u oby-builder" in c and "npm" in c)], calls)
        self.assertNotIn("npm install", SETUP.read_text())

    def quint_install(self, lock):
        """Quint at its pin in a prefix of the test's, a link to it and the downloads in the builder's .quint: (the
        prefix, the link). lock is the prefix's lockfile, or None for an install from before it (A00-40's, with
        npm install -g into lib/node_modules)."""
        prefix = self.dir.resolve() / "lib-quint" / "0.33.1"
        modules = prefix / ("node_modules" if lock is not None else "lib/node_modules")
        quint = modules / "@informalsystems" / "quint"
        (quint / "dist" / "src").mkdir(parents=True)
        (quint / "dist" / "src" / "cli.js").write_text("#!/usr/bin/env node\n")
        (quint / "package.json").write_text('{\n  "name": "@informalsystems/quint",\n  "version": "0.33.1",\n'
                                            '  "main": "dist/src/index.js"\n}\n')
        if lock is None:
            program = prefix / "bin" / "quint"
            program.parent.mkdir()
            program.symlink_to("../lib/node_modules/@informalsystems/quint/dist/src/cli.js")
        else:
            program = modules / ".bin" / "quint"
            program.parent.mkdir()
            program.symlink_to("../@informalsystems/quint/dist/src/cli.js")
            (prefix / "package-lock.json").write_bytes(lock)
        link = self.dir.resolve() / "bin-quint"
        link.symlink_to(program)
        home = self.dir / "builder-home"
        for tool in ("apalache-dist-0.62.4/apalache/bin/apalache-mc", "rust-evaluator-v0.7.1/quint_evaluator"):
            path = home / ".quint" / tool
            path.parent.mkdir(parents=True)
            path.write_text("#!/bin/sh\n")
            path.chmod(0o755)
        return prefix, link

    def test_remote_55_an_unpinned_install_is_installed_afresh(self):
        prefix, link = self.quint_install(None)
        done = self.builder(prefix, link)
        self.assertEqual(done.returncode, 0, done.stdout)
        self.assertIn("setup.sh: installing quint 0.33.1", done.stdout.splitlines())
        calls = self.calls()
        [ci] = [c for c in calls if "npm" in c.split()]
        self.assertTrue(ci.endswith(f" npm ci --ignore-scripts --prefix {prefix}"), ci)
        self.assertLess(calls.index(f"-n rm -rf {prefix}"), calls.index(ci), calls)
        self.assertLess(calls.index(ci), calls.index(f"-n ln -sfn {prefix}/node_modules/.bin/quint {link}"), calls)

    def test_remote_55_an_install_from_this_lockfile_is_left_as_it_is(self):
        lock = (QUINT / "package-lock.json").read_bytes()
        prefix, link = self.quint_install(lock)
        done = self.builder(prefix, link)
        self.assertEqual(done.returncode, 0, done.stdout)
        self.assertIn("setup.sh: quint 0.33.1 already installed", done.stdout.splitlines())
        calls = self.calls()
        self.assertFalse([c for c in calls if "npm" in c or c == f"-n rm -rf {prefix}"], calls)
        # A lockfile one byte apart from the committed one is installed afresh.
        (prefix / "package-lock.json").write_bytes(lock + b"\n")
        self.log.unlink()
        done = self.builder(prefix, link)
        self.assertEqual(done.returncode, 0, done.stdout)
        self.assertIn("setup.sh: installing quint 0.33.1", done.stdout.splitlines())
        self.assertTrue([c for c in self.calls() if c.endswith(f" npm ci --ignore-scripts --prefix {prefix}")])


class QuintLock(unittest.TestCase):
    """REMOTE-53 and 54: Quint's committed npm tree, and setup.sh stopping before any ssh call when it isn't at
    setup.sh's pin."""

    def test_remote_53_the_committed_lockfile_pins_every_package(self):
        package = json.loads((QUINT / "package.json").read_text())
        self.assertEqual(package, {"name": "oby-quint", "private": True,
                                   "dependencies": {"@informalsystems/quint": "0.33.1"}})
        lock = json.loads((QUINT / "package-lock.json").read_text())
        self.assertEqual(lock["lockfileVersion"], 3)
        self.assertEqual(lock["packages"][""]["dependencies"], {"@informalsystems/quint": "0.33.1"})
        self.assertEqual(lock["packages"]["node_modules/@informalsystems/quint"]["version"], "0.33.1")
        packages = {name: entry for name, entry in lock["packages"].items() if name}
        self.assertGreater(len(packages), 1)
        for name, entry in packages.items():
            self.assertTrue(entry.get("integrity", "").startswith("sha512-"), name)
            self.assertTrue(entry.get("resolved", "").startswith("https://registry.npmjs.org/"), name)
        tracked = git("ls-files", "--", "factory/remote/quint", cwd=FACTORY.parent).splitlines()
        self.assertEqual(sorted(tracked), ["factory/remote/quint/package-lock.json", "factory/remote/quint/package.json"])

    def setup_copy(self, change):
        """setup.sh and setup.sh --pins run from a copy of factory/remote/ (ci.yml where it looks), its quint folder
        changed by change, with a stand-in ssh that records every call: (the run, the --pins run, ssh's calls)."""
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            shutil.copytree(FACTORY / "remote", root / "factory" / "remote")
            (root / ".github" / "workflows").mkdir(parents=True)
            shutil.copy(CI, root / ".github" / "workflows" / "ci.yml")
            quint = root / "factory" / "remote" / "quint"
            change(quint)
            ssh = root / "bin" / "ssh"
            ssh.parent.mkdir()
            ssh.write_text(f'#!/bin/sh\necho "ssh $1" >>"{root}/ssh.log"\ncat >/dev/null\n')
            ssh.chmod(0o755)
            env = dict(os.environ, PATH=f"{ssh.parent}{os.pathsep}{os.environ['PATH']}")
            setup = root / "factory" / "remote" / "setup.sh"
            done = subprocess.run([str(setup)], env=env, capture_output=True, text=True, timeout=60)
            pins = subprocess.run([str(setup), "--pins"], env=env, capture_output=True, text=True, timeout=60)
            log = root / "ssh.log"
            return done, pins, (log.read_text().splitlines() if log.exists() else []), quint

    def lock_at(self, root=None, locked=None, package=None):
        """A change that sets the lockfile's root dependency, its locked Quint or package.json's to a version."""
        def change(quint):
            lock = json.loads((quint / "package-lock.json").read_text())
            lock["packages"][""]["dependencies"]["@informalsystems/quint"] = root or "0.33.1"
            lock["packages"]["node_modules/@informalsystems/quint"]["version"] = locked or "0.33.1"
            (quint / "package-lock.json").write_text(json.dumps(lock, indent=2) + "\n")
            wanted = json.loads((quint / "package.json").read_text())
            wanted["dependencies"]["@informalsystems/quint"] = package or "0.33.1"
            (quint / "package.json").write_text(json.dumps(wanted, indent=2) + "\n")
        return change

    def test_remote_54_a_lockfile_that_disagrees_with_the_pin_stops_setup(self):
        pins = subprocess.run([str(SETUP), "--pins"], capture_output=True, text=True, timeout=30).stdout
        for change, said in (
                (self.lock_at(root="0.33.0", locked="0.33.0"), "package.json wants 0.33.1, package-lock.json wants "
                                                               "0.33.0 and locks 0.33.0"),
                (self.lock_at(locked="0.33.0"), "package.json wants 0.33.1, package-lock.json wants 0.33.1 and locks "
                                                "0.33.0"),
                (self.lock_at(root="0.33.0"), "package.json wants 0.33.1, package-lock.json wants 0.33.0 and locks "
                                              "0.33.1"),
                (self.lock_at(package="^0.33.1"), "package.json wants ^0.33.1, package-lock.json wants 0.33.1 and "
                                                  "locks 0.33.1")):
            done, listed, ssh, quint = self.setup_copy(change)
            self.assertEqual(done.returncode, 1, done.stdout + done.stderr)
            self.assertIn(f"setup.sh: {quint} isn't at quint's pin 0.33.1: {said}", done.stderr.splitlines())
            self.assertEqual(ssh, [], "setup reached the host")
            self.assertEqual((listed.returncode, listed.stdout), (0, pins), listed.stderr)
        # The committed files, unchanged, pass it and reach ssh.
        done, _, ssh, _ = self.setup_copy(lambda quint: None)
        self.assertEqual(done.returncode, 0, done.stderr)
        self.assertEqual(ssh, ["ssh oby-gate", "ssh oby-gate"])

    def test_remote_54_a_missing_lockfile_stops_setup(self):
        for name in ("package-lock.json", "package.json"):
            done, listed, ssh, quint = self.setup_copy(lambda folder: (folder / name).unlink())
            self.assertEqual(done.returncode, 1, done.stdout + done.stderr)
            self.assertIn(f"setup.sh: {quint / name} is missing: quint 0.33.1 installs only from it",
                          done.stderr.splitlines())
            self.assertEqual(ssh, [], "setup reached the host")
            self.assertEqual(listed.returncode, 0, listed.stderr)


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
            # A second GITLEAKS_VERSION that says another version than ci.yml's one.
            ci = Path(folder) / "ci.yml"
            ci.write_text(text + "\n  GITLEAKS_VERSION: 0.0.1\n")
            done = self.pins(ci)
            self.assertNotEqual(done.returncode, 0, done.stdout)
            self.assertIn("GITLEAKS_VERSION disagree", done.stderr)

    def test_remote_12_setup_installs_ci_yml_s_apt_packages(self):
        # ci.yml stays the one place the list is written: every word after `apt-get install` but its flags
        wanted = {
            word
            for line in CI.read_text().splitlines()
            if "apt-get install" in line
            for word in line.split("apt-get install", 1)[1].split()
            if not word.startswith("-")
        }
        self.assertTrue(wanted, "ci.yml installs nothing with apt-get install")
        arrays = re.findall(r"^\s*local packages=\(([^)]*)\)", SETUP.read_text(), re.MULTILINE)
        self.assertEqual(len(arrays), 1, f"setup.sh needs exactly one `local packages=(...)`: {arrays}")
        missing = sorted(wanted - set(arrays[0].split()))
        self.assertEqual(missing, [], f"ci.yml installs {missing} with apt-get, and setup.sh doesn't")


class SeedModel(unittest.TestCase):
    """REMOTE-52: the seed design model says what code it models and what a model can't check."""

    def test_remote_52_the_seed_model_names_what_it_models(self):
        text = MODEL.read_text()
        self.assertRegex(text, r"(?m)^module job_race_dev \{$")
        self.assertRegex(text, r"(?m)^\s*val noLostRetry = ")
        self.assertRegex(text, r"(?m)^\s*val noRetriedJobEverFinishes = ")
        header = text.split("module job_race_dev", 1)[0]
        self.assertTrue(header.strip() and all(line.startswith("//") for line in header.strip().splitlines()), header)
        for word in ("core/crates/obid/src/jobs.rs", "work()", "_ if stopped", "core/crates/obid/src/job/lifecycle.rs",
                     "NextRound", "end_done", "TASKS-22", "JOBSTATE-18", "12 steps", "violated",
                     "the design, not that jobs.rs implements it"):
            self.assertIn(word, " ".join(line.removeprefix("//").strip() for line in header.splitlines()), word)
        self.assertNotRegex(header, r"\.rs:\d", "the header names code by line number")


class Tighten(unittest.TestCase):
    """REMOTE-11: setup leaves nothing on the check's path writable by group or others."""

    def setUp(self):
        self.dir = Path(tempfile.mkdtemp())
        self.home = self.dir / "home"
        self.bin = self.home / ".cargo" / "bin"
        self.bin.mkdir(parents=True)
        self.marker = self.dir / "ran"
        (self.bin / "oby-check-blok").write_text(f'#!/bin/sh\necho "$*" >"{self.marker}"\nexit 0\n')
        (self.bin / "oby-check-blok-nodes").write_text("#!/bin/sh\nexit 0\n")
        (self.home / ".rustup" / "toolchains").mkdir(parents=True)
        self.root = self.dir / "root"
        for n in range(1, 5):
            (self.root / f"slot-{n}").mkdir(parents=True)
        self.gate = self.dir / "oby-remote-gate"
        self.gate.write_text("#!/bin/sh\n")
        os.chmod(self.gate, 0o755)
        # setup.sh's copy is origin/dev's, extracted into a folder that isn't a checkout.
        self.script = self.dir / "vet" / "scripts" / "oby-check.sh"
        self.script.parent.mkdir(parents=True)
        shutil.copy(CHECK, self.script)
        self.scratch = self.dir / "scratch"
        self.scratch.mkdir()
        self.paths = [
            self.home / ".cargo",
            self.bin,
            self.bin / "oby-check-blok",
            self.bin / "oby-check-blok-nodes",
            self.home / ".rustup" / "toolchains",
            *(self.root / f"slot-{n}" for n in range(1, 5)),
        ]
        for path in self.paths:
            os.chmod(path, 0o775)
        self.env = {k: v for k, v in os.environ.items() if not k.startswith("OBY_CHECK_")}

    def tearDown(self):
        shutil.rmtree(self.dir)

    def tighten(self):
        args = [self.home, self.root, self.gate, self.script, self.scratch]
        # From the home, where ssh starts setup on the host.
        return subprocess.run(
            ["bash", "-c", '. "$0"; tighten "$@"', str(TIGHTEN), *map(str, args)],
            cwd=self.home,
            env=self.env,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            timeout=60,
        )

    def test_remote_11_setup_tightens_the_checks_path(self):
        done = self.tighten()
        self.assertEqual(done.returncode, 0, done.stdout)
        for path in [*self.paths, self.home / ".rustup", self.root, self.gate]:
            self.assertEqual(path.stat().st_mode & 0o777, 0o755, f"{path}\n{done.stdout}")
        self.assertTrue(self.marker.exists(), done.stdout)
        self.assertIn("setup.sh: removed group and other write from", done.stdout)
        self.assertIn("setup.sh: oby-check-blok passes scripts/oby-check.sh's own vetting", done.stdout)
        lines = done.stdout.splitlines()
        self.assertIn("setup.sh: modes:", lines)
        self.assertTrue(
            any(line.startswith("-rwxr-xr-x") and line.endswith(f"{self.bin}/oby-check-blok") for line in lines),
            done.stdout,
        )
        again = self.tighten()
        self.assertEqual(again.returncode, 0, again.stdout)
        self.assertIn("setup.sh: modes already right", again.stdout.splitlines())

    def test_remote_11_a_path_that_cant_be_tightened_stops_setup(self):
        stand_in = self.dir / "bin"
        stand_in.mkdir()
        (stand_in / "chmod").write_text("#!/bin/sh\nexit 1\n")
        os.chmod(stand_in / "chmod", 0o755)
        self.env["PATH"] = f"{stand_in}{os.pathsep}{self.env['PATH']}"
        done = self.tighten()
        self.assertNotEqual(done.returncode, 0, done.stdout)
        lines = done.stdout.splitlines()
        self.assertIn(f"setup.sh: still writable by others: {self.bin}", lines)
        self.assertIn(f"setup.sh: still writable by others: {self.bin}/oby-check-blok", lines)
        self.assertEqual((self.bin / "oby-check-blok").stat().st_mode & 0o777, 0o775)
        self.assertFalse(self.marker.exists(), done.stdout)

    def test_remote_11_setup_applies_the_checks_own_rule(self):
        (self.bin / "oby-check-blok-nodes").unlink()
        done = self.tighten()
        self.assertNotEqual(done.returncode, 0, done.stdout)
        self.assertIn("oby-check: failed: oby-check-blok (no oby-check-blok-nodes beside", done.stdout)
        self.assertFalse(self.marker.exists(), done.stdout)

    def test_remote_18_tighten_covers_a_two_slot_root(self):
        # The Mac worker's root: two slots, as setup-mac.sh makes it.
        for n in (3, 4):
            (self.root / f"slot-{n}").rmdir()
            self.paths.remove(self.root / f"slot-{n}")
        os.chmod(self.root, 0o777)
        done = self.tighten()
        self.assertEqual(done.returncode, 0, done.stdout)
        self.assertNotIn("No such file", done.stdout)
        for path in [*self.paths, self.home / ".rustup", self.root, self.gate]:
            self.assertEqual(path.stat().st_mode & 0o777, 0o755, f"{path}\n{done.stdout}")
        lines = done.stdout.splitlines()
        for n in (1, 2):
            self.assertTrue(any(line.startswith("drwxr-xr-x") and line.endswith(f"{self.root}/slot-{n}")
                                for line in lines), done.stdout)
        self.assertFalse(any(line.endswith(("/slot-3", "/slot-4", "/slot-*")) for line in lines), done.stdout)
        # Under macOS's bash 3.2 too, which runs it on the Mac worker.
        if Path("/bin/bash").exists():
            os.chmod(self.root / "slot-2", 0o777)
            done = subprocess.run(
                ["/bin/bash", "-c", 'set -euo pipefail; . "$0"; tighten "$@"', str(TIGHTEN),
                 *map(str, [self.home, self.root, self.gate, self.script, self.scratch])],
                cwd=self.home, env=self.env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, timeout=60)
            self.assertEqual(done.returncode, 0, done.stdout)
            self.assertEqual((self.root / "slot-2").stat().st_mode & 0o777, 0o755, done.stdout)


class SetupMac(unittest.TestCase):
    """REMOTE-18 and MUTANTS-17: setup-mac.sh, with a stand-in ssh that keeps what it sends and runs nothing."""

    def setUp(self):
        self.dir = Path(tempfile.mkdtemp())
        self.bin = self.dir / "bin"
        self.bin.mkdir()
        self.sent = self.dir / "sent.sh"
        self.tool("ssh", f'echo "$*" >"{self.dir}/ssh-args"\ncat >"{self.sent}"')
        # This Mac's versions, as setup-mac.sh reads them.
        self.tool("rustc", 'echo "rustc 1.99.1 (the Mac)"')
        self.tool("xcodebuild", 'echo "Xcode 27.2"; echo "Build version 27A1"')
        self.tool("node", "echo v24.0.0")
        self.tool("gitleaks", "echo 8.30.1")
        self.env = dict(os.environ, PATH=f"{self.bin}{os.pathsep}{os.environ['PATH']}")

    def tearDown(self):
        shutil.rmtree(self.dir)

    def tool(self, name, body, folder=None):
        path = (folder or self.bin) / name
        path.write_text(f"#!/bin/sh\n{body}\n")
        path.chmod(0o755)

    def setup_mac(self, *args):
        return subprocess.run([str(SETUP_MAC), *args], env=self.env, capture_output=True, text=True, timeout=60)

    def test_remote_18_setup_mac_uses_no_sudo_and_builds_from_origin_dev(self):
        done = self.setup_mac()
        self.assertEqual(done.returncode, 0, done.stderr)
        # Through bash -s, never the host's login shell, to oby-mac-pro by default.
        self.assertEqual((self.dir / "ssh-args").read_text(), "oby-mac-pro bash -s\n")
        sent = self.sent.read_text()
        self.assertEqual(subprocess.run(["bash", "-n", str(self.sent)]).returncode, 0)
        self.assertNotRegex(sent, r"\bsudo\b")
        self.assertNotRegex(SETUP_MAC.read_text(), r"\bsudo\b")
        # oby-check-blok is built from the archive of origin/dev's commit in the clone, never a checked one.
        self.assertIn('dev=$(git -C "$root/repo.git" rev-parse refs/remotes/origin/dev)', sent)
        self.assertIn('git -C "$root/repo.git" archive "$dev" tools/oby-check-blok | tar -x -C "$tmp"', sent)
        self.assertIn('cargo install --locked --quiet --path "$tmp/tools/oby-check-blok"', sent)
        # Beside it, only install_mutants' calls (MUTANTS-17).
        installs = [found.strip() for found in re.findall(r"cargo install[^)|\n]*", sent)]
        self.assertEqual(sorted(installs), sorted(['cargo install --locked --quiet --path "$tmp/tools/oby-check-blok"',
                                                   "cargo install --list",
                                                   'cargo install --locked cargo-mutants --version "$MUTANTS"']), sent)
        # The one tighten.sh, run last with origin/dev's own check script on the two-slot root.
        self.assertIn(TIGHTEN.read_text(), sent)
        self.assertIn('tighten "$HOME" "$root" "$root/bin/oby-remote-gate" "$tmp/vet/scripts/oby-check.sh" "$tmp"',
                      sent)
        self.assertIn('mkdir -p "$root/slot-1" "$root/slot-2"', sent)
        # This checkout's runner, sent whole.
        found = re.search(r"^GATE_B64=(\S+)$", sent, re.MULTILINE)
        decoded = subprocess.run(["base64", "--decode"], input=found.group(1).encode(), capture_output=True)
        self.assertEqual(decoded.stdout, GATE.read_bytes())
        self.assertEqual(self.setup_mac("another-mac").returncode, 0)
        self.assertEqual((self.dir / "ssh-args").read_text(), "another-mac bash -s\n")

    def test_remote_18_setup_mac_stops_on_a_rust_mismatch_before_writing(self):
        self.assertEqual(self.setup_mac().returncode, 0)
        # The host: its own rustc, and every tool there; its home gets nothing.
        host = self.dir / "host-bin"
        host.mkdir()
        self.tool("rustc", 'echo "rustc 1.98.0 (the host)"', host)
        for name in ("xcodebuild", "node", "gitleaks"):
            shutil.copy(self.bin / name, host / name)
        for name in ("git", "python3", "rustup", "cargo"):
            self.tool(name, f'echo "{name} ran: $*" >>"{self.dir}/ran"', host)
        self.tool("xcrun", 'echo "    iPhone 17 (0000) (Shutdown)"', host)
        home = self.dir / "home"
        home.mkdir()
        done = subprocess.run(["bash", str(self.sent)], capture_output=True, text=True, timeout=60,
                              env=dict(os.environ, HOME=str(home), PATH=f"{host}:/usr/bin:/bin"))
        self.assertEqual(done.returncode, 1, done.stdout + done.stderr)
        # Its rustc, or one Homebrew's folders hold, which come first there.
        self.assertRegex(done.stdout, r"setup-mac.sh: rust: this host has rustc .*, the Mac rustc 1.99.1 \(the Mac\)")
        self.assertIn("setup-mac.sh: nothing was written", done.stdout)
        self.assertEqual(list(home.iterdir()), [])
        self.assertFalse((self.dir / "ran").exists(), "a tool ran before the checks passed")


    def install_mutants(self):
        """The sent script's install_mutants, under macOS's bash 3.2 where it is, against a temporary home and a
        stand-in cargo: `install --list` prints $dir/installed; `install --locked ...` is recorded in $dir/installs,
        fails when $dir/install-fails exists, and otherwise writes its version into $dir/installed; `mutants
        --version` prints $dir/version when it exists, and otherwise the version $dir/installed holds."""
        if not self.sent.exists():
            self.assertEqual(self.setup_mac().returncode, 0)
        text = self.sent.read_text()
        self.assertTrue(text.endswith("\nsetup </dev/null\n"), text[-200:])
        functions = self.dir / "functions.sh"
        functions.write_text(text.removesuffix("setup </dev/null\n"))
        host = self.dir / "host-bin"
        host.mkdir(exist_ok=True)
        self.tool("cargo", f'''case "$1 $2" in
"install --list") cat "{self.dir}/installed" 2>/dev/null; exit 0;;
"install --locked") echo "$*" >>"{self.dir}/installs"
    [ ! -e "{self.dir}/install-fails" ] || exit 1
    echo "cargo-mutants v$5:" >"{self.dir}/installed";;
"mutants --version") if [ -e "{self.dir}/version" ]; then cat "{self.dir}/version"; exit 0; fi
    sed -n "s/^cargo-mutants v\\(.*\\):$/cargo-mutants \\1/p" "{self.dir}/installed" 2>/dev/null | grep . ||
        {{ echo "error: no such command: \\`mutants\\`" >&2; exit 101; }};;
*) exit 2;;
esac''', host)
        home = self.dir / "home"
        home.mkdir(exist_ok=True)
        bash = "/bin/bash" if Path("/bin/bash").exists() else "bash"
        return subprocess.run([bash, "-c", 'set -euo pipefail; . "$0"; install_mutants', str(functions)], cwd=home,
                              env=dict(os.environ, HOME=str(home), PATH=f"{host}:/usr/bin:/bin"),
                              stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, timeout=60)

    def installs(self):
        path = self.dir / "installs"
        return path.read_text().splitlines() if path.exists() else []

    def test_mutants_17_setup_mac_installs_cargo_mutants_once_at_the_pin(self):
        done = self.setup_mac()
        self.assertEqual(done.returncode, 0, done.stderr)
        sent = self.sent.read_text()
        self.assertIn("\nMUTANTS=27.1.0\n", sent)
        self.assertNotRegex(sent, r"\bsudo\b")
        # After the checks and oby-check-blok, before tighten, so tighten covers it and a failed check writes nothing.
        self.assertIn("\n    install_mutants\n", sent)
        call = sent.index("\n    install_mutants\n")
        self.assertLess(sent.index('say "nothing was written"'), call)
        self.assertLess(sent.index('cargo install --locked --quiet --path "$tmp/tools/oby-check-blok"'), call)
        self.assertLess(call, sent.index('tighten "$HOME"'))
        self.assertIn("\n    cargo mutants --version\n", sent[sent.index('say "versions:"'):])
        done = self.install_mutants()
        self.assertEqual(done.returncode, 0, done.stdout)
        self.assertEqual(self.installs(), ["install --locked cargo-mutants --version 27.1.0"])
        self.assertIn("setup-mac.sh: installing cargo-mutants 27.1.0", done.stdout.splitlines())
        again = self.install_mutants()
        self.assertEqual(again.returncode, 0, again.stdout)
        self.assertEqual(self.installs(), ["install --locked cargo-mutants --version 27.1.0"])
        self.assertIn("setup-mac.sh: cargo-mutants 27.1.0 already installed", again.stdout.splitlines())
        self.assertNotIn("installing", again.stdout)

    def test_mutants_17_setup_mac_replaces_another_version_with_the_pin(self):
        (self.dir / "installed").write_text("cargo-mutants v26.0.0:\n    cargo-mutants\n")
        done = self.install_mutants()
        self.assertEqual(done.returncode, 0, done.stdout)
        self.assertEqual(self.installs(), ["install --locked cargo-mutants --version 27.1.0"])
        self.assertIn("setup-mac.sh: installing cargo-mutants 27.1.0", done.stdout.splitlines())

    def test_mutants_17_setup_mac_stops_when_cargo_mutants_is_not_at_the_pin(self):
        (self.dir / "install-fails").touch()
        done = self.install_mutants()
        self.assertNotEqual(done.returncode, 0, done.stdout)
        self.assertEqual(self.installs(), ["install --locked cargo-mutants --version 27.1.0"])
        self.assertNotIn("already installed", done.stdout)
        (self.dir / "install-fails").unlink()
        (self.dir / "installed").write_text("cargo-mutants v27.1.0:\n    cargo-mutants\n")
        (self.dir / "version").write_text("cargo-mutants 26.0.0\n")
        done = self.install_mutants()
        self.assertNotEqual(done.returncode, 0, done.stdout)
        self.assertIn("setup-mac.sh: cargo mutants is not at 27.1.0: cargo-mutants 26.0.0", done.stdout.splitlines())

    def test_mutants_17_setup_and_setup_mac_read_the_pin_from_pins_sh(self):
        pins = FACTORY / "remote" / "pins.sh"
        [line] = [line for line in pins.read_text().splitlines() if line.startswith("mutants=")]
        value = line.removeprefix("mutants=")
        self.assertEqual(value, "27.1.0")
        for script in (SETUP, SETUP_MAC):
            text = script.read_text()
            self.assertIn('\n. "$here/pins.sh"\n', text, script)
            self.assertNotIn(value, text, script)
            self.assertNotRegex(text, r"(?m)^\s*mutants=", script)
        # A copy of factory/remote/ (ci.yml where setup.sh looks) whose pins.sh says another version: both scripts
        # send it, through a stand-in ssh that keeps each host script and reaches no host.
        root = self.dir / "copy"
        shutil.copytree(FACTORY / "remote", root / "factory" / "remote")
        (root / ".github" / "workflows").mkdir(parents=True)
        shutil.copy(CI, root / ".github" / "workflows" / "ci.yml")
        (root / "factory" / "remote" / "pins.sh").write_text(pins.read_text().replace(line, "mutants=9.9.9"))
        ssh = root / "bin"
        ssh.mkdir()
        self.tool("ssh", 'if [ "$2 $3" = "bash -s" ]; then cat >"$SENT"; else cat >/dev/null; fi', ssh)
        for name in ("setup.sh", "setup-mac.sh"):
            sent = self.dir / f"{name}.sent"
            done = subprocess.run([str(root / "factory" / "remote" / name)], capture_output=True, text=True, timeout=60,
                                  env=dict(self.env, SENT=str(sent), PATH=f"{ssh}{os.pathsep}{self.env['PATH']}"))
            self.assertEqual(done.returncode, 0, done.stderr)
            self.assertIn("\nMUTANTS=9.9.9\n", sent.read_text(), name)


if __name__ == "__main__":
    unittest.main(verbosity=2)
