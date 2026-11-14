#!/usr/bin/env python3
"""factory/bin/test-run's macOS-only runs on the Mac worker (REMOTE-35 to 42), with a fake `ssh`, never the Mac
worker, the VM or the network.

Run from the repository root: python3 factory/tests/test_test_run_mac_worker.py
The throwaway repository, the fake `az`, this Mac's stand-in cargo and swift and the VM's builder root are
test_test_run_remote.py's (RemoteHarness). Its fake `ssh` here also plays oby-mac-pro: as that Mac's sshd would,
it starts the command in the fake home, where oby-worker/bin/oby-remote-gate is the real runner, with
OBY_REMOTE_ROOT at the worker's local root (its gates' repo.git and slots, and builders/ with its own repo.git);
with $FAKE/mac-unreachable it exits 255 as ssh does when the host doesn't answer. The worker's cargo is the fake
home's stand-in, as the VM's is. setup-mac.sh's and tighten.sh's tests reuse test_remote_gate.py's set-ups.
"""

import fcntl
import json
import os
import random
import re
import shlex
import shutil
import signal
import string
import subprocess
import time
import unittest

import test_remote_gate
from test_test_run_remote import CORE_VARS, FAKE_SSH, OBY_REMOTE_GATE, RemoteHarness, gone

PRO = {"ssh_host": "oby-mac-pro", "builder_slots": 2}
SANDBOX = "core/crates/obid/src/sandbox.rs"

# oby-mac-pro, ahead of the harness's fake ssh starting the command.
WORKER = '''
def worker(argv):
    if argv[-2] != "oby-mac-pro":
        return {}
    if (fake / "mac-unreachable").exists():
        print("ssh: connect to host oby-mac-pro port 22: Operation timed out", file=sys.stderr)
        sys.exit(255)
    return {"cwd": os.environ["HOME"], "env": dict(os.environ, OBY_REMOTE_ROOT=os.environ["FAKE_MAC_ROOT"])}
'''
FAKE_MAC_SSH = FAKE_SSH.replace("signal.signal(signal.SIGUSR1", WORKER + "signal.signal(signal.SIGUSR1", 1).replace(
    "start_new_session=True)", "start_new_session=True, **worker(argv))", 1)


class TestRunMacWorker(RemoteHarness, unittest.TestCase):
    def setUp(self):
        super().setUp()
        self.assertIn("**worker(argv)", FAKE_MAC_SSH)
        self.tool(self.bin / "ssh", FAKE_MAC_SSH)
        runner = self.home / "oby-worker" / "bin" / "oby-remote-gate"
        runner.parent.mkdir(parents=True)
        runner.symlink_to(OBY_REMOTE_GATE)
        # The worker's root, as setup-mac.sh makes ~/oby-worker: the gates' clone, and the builders' own.
        self.mac = self.dir / "mac"
        self.builders = self.mac / "builders"
        self.builders.mkdir(parents=True)
        for clone in (self.mac / "repo.git", self.builders / "repo.git"):
            self.git(self.dir, "clone", "-q", "--bare", str(self.origin), str(clone))
        self.env["FAKE_MAC_ROOT"] = str(self.mac)
        self.worker(**PRO)

    def worker(self, **keys):
        self.env["FACTORY_MAC_WORKER"] = json.dumps(keys)

    def flagged(self, *argv):
        """test-run with these words before the command, from core, as start() runs it."""
        done = subprocess.run([str(self.worktree / "factory" / "bin" / "test-run"), *argv], cwd=self.worktree / "core",
                              env={**self.env, "CARGO_TARGET_DIR": str(self.worktree / "core" / "target")},
                              capture_output=True, text=True, timeout=120)
        return done.returncode, done.stdout, done.stderr

    def said(self, err):
        return [line for line in err.splitlines() if line.startswith("factory/bin/test-run: ")]

    def hold_builder(self, n):
        (self.builders / f"builder-{n}").mkdir(exist_ok=True)
        fd = os.open(self.builders / f"builder-{n}" / "lock", os.O_RDWR | os.O_CREAT, 0o644)
        fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
        self.held.append(fd)

    def assert_on_the_worker(self, call, tree_words=11):
        argv = call["argv"]
        self.assertEqual(argv[-2], "oby-mac-pro")
        self.assertNotIn("-l", argv)
        for option in ("BatchMode=yes", "ForwardAgent=no", "ConnectTimeout=10"):
            self.assertIn(option, argv)
        return shlex.split(argv[-1])[tree_words - 1]

    # ---- REMOTE-35 -------------------------------------------------------------------------------

    def test_remote_35_a_macos_only_run_lands_on_the_mac_worker(self):
        self.write(SANDBOX, "// changed, not committed\n")
        code, out, err, _ = self.run_test_run("cargo", "test", "-p", "obid", "--test", "one")
        self.assertEqual(code, 0, out + err)
        [call] = self.ssh_calls()
        tree = self.assert_on_the_worker(call)
        self.assertEqual(call["argv"][-1], shlex.join([
            "oby-worker/bin/oby-remote-gate", "--mac", "--run", "--slots", "2", "--builder-slots", "2", "--wait",
            "600", self.base, tree, "core", "--", "cargo", "test", "-p", "obid", "--test", "one"]))
        self.assertEqual(self.git(self.worktree, "show", f"{tree}:{SANDBOX}"), "// changed, not committed")
        self.assertEqual(self.said(err), ["factory/bin/test-run: to the Mac worker oby-mac-pro: the worktree's "
                                          f"changes touch macOS-only code: {SANDBOX}"])
        [run] = self.vm_runs()
        slot = self.builders / "builder-1"
        share = str(max(1, (os.cpu_count() or 1) // 4))
        self.assertEqual(run["args"], ["test", "-p", "obid", "--test", "one"])
        self.assertEqual((run["cwd"], run["CARGO_TARGET_DIR"]), (str(slot / "worktree" / "core"), str(slot / "target")))
        self.assertEqual(run["nice"], min(os.getpriority(os.PRIO_PROCESS, 0) + 15, 20))
        self.assertEqual(run["CARGO_NET_OFFLINE"], "true")
        self.assertEqual([run[k] for k in ("OBY_BUILD_JOBS", *CORE_VARS)], [share] * 4)
        self.assertIn("the VM's cargo ran", out)
        self.assertIn("oby-remote-gate: running in builder slot 1", out)
        self.assertRegex(out, r"oby-remote-gate: ended 0 in builder slot 1 after \d+s\n$")
        self.assertEqual(self.lines("mac-runs"), [])
        self.assertFalse(self.lock.exists(), "a run on the Mac worker took this Mac's lock")
        self.assertFalse(self.state.exists(), "a run on the Mac worker opened a use of the VM")
        self.assertEqual(self.lines("az.log"), [])
        self.assertFalse(list(self.root.glob("builder-*")), "the VM's builder root was used")

    def test_remote_35_a_macos_only_test_binary_lands_on_the_mac_worker(self):
        code, out, err, _ = self.run_test_run("cargo", "test", "-p", "obid", "--test", "sandbox")
        self.assertEqual(code, 0, out + err)
        [call] = self.ssh_calls()
        self.assert_on_the_worker(call)
        self.assertEqual(self.said(err), ["factory/bin/test-run: to the Mac worker oby-mac-pro: its test is "
                                          "macOS-only: core/crates/obid/tests/sandbox.rs"])
        [run] = self.vm_runs()
        self.assertEqual(run["cwd"], str(self.builders / "builder-1" / "worktree" / "core"))
        self.assertEqual(self.lines("mac-runs"), [])

    def test_remote_35_a_failure_on_the_mac_worker_isnt_run_again_here(self):
        self.write(SANDBOX, "// changed\n")
        (self.fake / "cargo-exit").write_text("3\n")
        code, out, err, _ = self.run_test_run("cargo", "test", "--test", "one")
        self.assertEqual(code, 3, out + err)
        self.assertEqual(len(self.run_calls()), 1)
        self.assertEqual(self.lines("mac-runs"), [])
        self.assertNotIn("this Mac", err)

    def test_remote_35_a_run_that_doesnt_need_macos_still_goes_to_the_vm(self):
        self.write("core/crates/obid/src/gateway.rs", "// changed\n")
        code, out, err, _ = self.run_test_run("cargo", "test", "--test", "one")
        self.assertEqual(code, 0, out + err)
        [call] = self.run_calls()
        self.assertEqual(call["argv"][-4:-1], ["-l", "oby-builder", "oby-gate"])
        self.assertNotIn("oby-mac-pro", json.dumps(self.ssh_calls()))
        self.assertEqual(shlex.split(call["argv"][-1])[:2], ["oby-remote-gate", "--run"])
        self.assertNotIn("Mac worker", err)
        [run] = self.vm_runs()
        self.assertEqual(run["cwd"], str(self.root / "builder-1" / "worktree" / "core"))
        self.assertFalse(list(self.builders.glob("builder-*")))

    # ---- REMOTE-36 -------------------------------------------------------------------------------

    def test_remote_36_two_builders_run_at_once_on_the_mac_worker_and_a_gate_still_gets_a_slot(self):
        (self.fake / "hold").touch()
        other = self.add_worktree("y")
        for tree in (self.worktree, other):
            self.write(SANDBOX, "// changed\n", worktree=tree)
        a = self.start("cargo", "test", "--test", "one")
        b = self.start("cargo", "test", "--test", "one", worktree=other)
        self.wait_for_vm_runs(2)
        targets = sorted(run["CARGO_TARGET_DIR"] for run in self.vm_runs())
        self.assertEqual(targets, [str(self.builders / f"builder-{n}" / "target") for n in (1, 2)])
        # Every builder slot is held: a gate's macOS part takes a gate slot at once.
        started = time.monotonic()
        gate = subprocess.Popen([str(OBY_REMOTE_GATE), "--mac", self.base, "origin/dev", "factory"],
                                env={**self.env, "OBY_REMOTE_ROOT": str(self.mac)}, cwd=self.home,
                                stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
        out = gate.stdout.read()
        self.assertEqual(gate.wait(timeout=30), 0, out)
        gate.stdin.close()
        gate.stdout.close()
        self.assertIn("oby-remote-gate: slot 1\n", out)
        self.assertNotIn("busy", out)
        self.assertLess(time.monotonic() - started, 30)
        self.assertEqual(sorted(p.name for p in self.mac.glob("builder-*")), [])
        self.assertEqual(sorted(p.name for p in self.builders.glob("slot-*")), [])
        self.assertEqual(sorted(p.name for p in self.builders.glob("builder-*")), ["builder-1", "builder-2"])
        (self.fake / "hold").unlink()
        for proc in (a, b):
            out, err = proc.communicate(timeout=60)
            self.assertEqual(proc.returncode, 0, out + err)

    def test_remote_36_each_mac_worker_builder_run_starts_clean(self):
        self.worker(ssh_host="oby-mac-pro", builder_slots=1)
        self.write(SANDBOX, "// changed\n")
        code, out, err, _ = self.run_test_run("cargo", "test", "--test", "one")
        self.assertEqual(code, 0, out + err)
        # What a first run could leave: files in the slot, its worktree (an ignored one too) and its target.
        slot = self.builders / "builder-1"
        for path in ("stray", "cargo/registry/crate", "worktree/core/ignored/left", "worktree/core/new.rs",
                     "target/kept"):
            self.put(slot / path, "left by the first run\n")
        (self.fake / "hold").touch()
        proc = self.start("cargo", "test", "--test", "one")
        self.wait_for_vm_runs(2)
        self.assertEqual(sorted(os.listdir(slot)), ["lock", "target", "worktree"])
        self.assertFalse((slot / "worktree" / "core" / "ignored" / "left").exists())
        self.assertFalse((slot / "worktree" / "core" / "new.rs").exists())
        self.assertTrue((slot / "target" / "kept").exists(), "the slot's target went")
        (self.fake / "hold").unlink()
        out, err = proc.communicate(timeout=60)
        self.assertEqual(proc.returncode, 0, out + err)

    # ---- REMOTE-37 -------------------------------------------------------------------------------

    def test_remote_37_an_ignored_file_is_never_sent_to_the_mac_worker(self):
        self.write("core/ignored/local.txt", "IGNORED-CONTENT-37\n")
        self.write(SANDBOX, "// changed\n")
        code, out, err, _ = self.run_test_run("cargo", "test", "--test", "one")
        self.assertEqual(code, 0, out + err)
        [call] = self.run_calls()
        tree = self.assert_on_the_worker(call)
        sent = self.stdin(call)
        self.assertIn(SANDBOX.encode(), sent)
        self.assertNotIn(b"IGNORED-CONTENT-37", sent)
        self.assertNotIn(b"ignored/", sent)
        files = self.git(self.worktree, "ls-tree", "-r", "--name-only", tree).splitlines()
        self.assertIn(SANDBOX, files)
        self.assertFalse([f for f in files if "ignored/" in f], files)

    def test_remote_37_a_secret_keeps_the_run_here_and_unsent(self):
        # Made here, so this file holds no token for the repository's own scan to find.
        pick = random.Random(37).choice
        token = "ghp" + "_" + "".join(pick(string.ascii_letters + string.digits) for _ in range(36))
        self.write(SANDBOX, "// changed\n")
        self.write("core/notes.txt", f'token = "{token}"\n')
        self.assert_on_this_mac(self.run_test_run("cargo", "test", "--test", "one"),
                                "on this Mac: the secret scan found a secret in the changes, or couldn't run "
                                "(gitleaks exit 1)")
        self.assertEqual(self.ssh_calls(), [])

    # ---- REMOTE-38 -------------------------------------------------------------------------------

    def test_remote_38_an_unreachable_mac_worker_falls_back(self):
        (self.fake / "mac-unreachable").touch()
        self.write(SANDBOX, "// changed\n")
        self.assert_on_this_mac(self.run_test_run("cargo", "test", "--test", "one"),
                                "fell back to this Mac: ssh oby-mac-pro ended 255 before the run started: ssh: "
                                "connect to host oby-mac-pro port 22: Operation timed out", tried=1)
        self.assertEqual([call["argv"][-2] for call in self.ssh_calls()], ["oby-mac-pro"])

    def test_remote_38_a_mac_worker_not_set_up_for_builders_falls_back(self):
        shutil.rmtree(self.builders / "repo.git")
        self.write(SANDBOX, "// changed\n")
        self.assert_on_this_mac(self.run_test_run("cargo", "test", "--test", "one"),
                                "fell back to this Mac: ssh oby-mac-pro ended 2 before the run started: "
                                f"oby-remote-gate: {self.builders / 'repo.git'} is missing: run "
                                "factory/remote/setup-mac.sh from the Mac first", tried=1)
        self.assertEqual(os.listdir(self.builders), [])
        self.assertFalse(list(self.mac.glob("builder-*")))

    def test_remote_38_busy_mac_worker_builder_slots_fall_back(self):
        self.worker(ssh_host="oby-mac-pro", builder_slots=1)
        self.hold_builder(1)
        self.write(SANDBOX, "// changed\n")
        result = self.run_test_run("cargo", "test", "--test", "one", FACTORY_BUILDER_WAIT_SECONDS="1")
        self.assert_on_this_mac(result, "fell back to this Mac: all builder slots were busy for 1s", tried=1)
        self.assertIn("oby-remote-gate: all builder slots are busy", result[1])
        self.assertFalse(list(self.mac.glob("slot-*")), "a builder took a gate's slot")

    # ---- REMOTE-39 -------------------------------------------------------------------------------

    def assert_stopped_on_the_worker(self, stop, expected):
        (self.fake / "hold").touch()
        self.write(SANDBOX, "// changed\n")
        proc = self.start("cargo", "test", "--test", "one")
        self.wait_for_vm_runs(1)
        [run] = self.vm_runs()
        [call] = self.run_calls()
        self.assertEqual(call["argv"][-2], "oby-mac-pro")
        stop(proc, call)
        out, err = proc.communicate(timeout=60)
        self.assertEqual(proc.returncode, expected, out + err)
        self.assertTrue(gone(run["pid"]), f"the run {run['pid']} is still going")
        self.assertTrue(gone(run["child"]), f"the run's child {run['child']} is still going")
        deadline = time.monotonic() + 15
        while True:
            fd = os.open(self.builders / "builder-1" / "lock", os.O_RDWR)
            try:
                fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
                break
            except BlockingIOError:
                self.assertLess(time.monotonic(), deadline, "builder-1 is still held")
                time.sleep(0.1)
            finally:
                os.close(fd)
        self.assertEqual(self.lines("mac-runs"), [], "a stopped run ran again here")
        self.assertFalse(self.state.exists(), "a run on the Mac worker opened a use of the VM")

    def test_remote_39_a_dropped_connection_frees_the_mac_workers_builder_slot(self):
        self.assert_stopped_on_the_worker(lambda proc, call: os.kill(call["pid"], signal.SIGUSR1), 255)

    def test_remote_39_sigterm_to_test_run_frees_the_mac_workers_builder_slot(self):
        self.assert_stopped_on_the_worker(lambda proc, call: proc.send_signal(signal.SIGTERM), 128 + signal.SIGTERM)

    # ---- REMOTE-40 -------------------------------------------------------------------------------

    def test_remote_40_without_mac_worker_builder_slots_nothing_changes(self):
        self.write(SANDBOX, "// changed\n")
        today = f"factory/bin/test-run: on this Mac: the worktree's changes touch macOS-only code: {SANDBOX}\n"
        # None: FACTORY_MAC_WORKER and FACTORY_REMOTE_GATE unset, so config.json's own settings are read (its
        # mac_worker has no builder_slots, its remote_gate has builder slots).
        for worker, vm in (({"ssh_host": "oby-mac-pro"}, True), ({"ssh_host": "oby-mac-pro", "builder_slots": 0}, True),
                           ({"ssh_host": "oby-mac-pro"}, False), (None, None)):
            for name in ("mac-runs", "ssh.log"):
                (self.fake / name).unlink(missing_ok=True)
            self.env.pop("FACTORY_MAC_WORKER", None)
            self.env.pop("FACTORY_REMOTE_GATE", None)
            if worker is not None:
                self.worker(**worker)
                self.settings(builder_slots=2 if vm else 0)
            code, out, err, pid = self.run_test_run("cargo", "test", "--test", "one")
            self.assertEqual((code, out, err), (0, "this Mac's cargo ran\n", today if vm is not False else ""),
                             (worker, vm))
            self.assertEqual(self.ssh_calls(), [], worker)
            [run] = self.lines("mac-runs")
            self.assertTrue(run.split(" | ", 1)[1].startswith(f"{pid} "), run)

    def test_remote_40_invalid_mac_worker_builder_slots_make_the_settings_invalid(self):
        self.write(SANDBOX, "// changed\n")
        read = ("import importlib.machinery, importlib.util, json, sys\n"
                "loader = importlib.machinery.SourceFileLoader('gate', sys.argv[1])\n"
                "gate = importlib.util.module_from_spec(importlib.util.spec_from_loader('gate', loader))\n"
                "loader.exec_module(gate)\n"
                "worker, why = gate.mac_worker_config()\n"
                "print(json.dumps([worker, why, gate.mac_worker_part(worker, why, 'c', 'origin/dev', ['rust'],"
                " ['rust'], [])]))\n")
        why = "FACTORY_MAC_WORKER's builder_slots must be a whole number from 0 to 64, when set"
        for value in ("2", -1, 65, True):
            self.worker(ssh_host="oby-mac-pro", builder_slots=value)
            (self.fake / "ssh.log").unlink(missing_ok=True)
            code, out, err, _ = self.run_test_run("cargo", "test", "--test", "one")
            self.assertEqual((code, out), (0, "this Mac's cargo ran\n"), err)
            self.assertEqual(self.ssh_calls(), [], value)
            self.assertNotIn("Mac worker", err)
            # The gate's macOS part, through mac_worker_part, falls back saying why.
            done = subprocess.run([shutil.which("python3"), "-c", read, str(self.main / "factory" / "bin" / "gate")],
                                  cwd=self.main, env=self.env, capture_output=True, text=True, timeout=60)
            self.assertEqual(done.returncode, 0, done.stderr)
            self.assertEqual(json.loads(done.stdout),
                             [None, why, [None, f"Fell back from the Mac worker to this Mac: {why}"]], value)
        # A valid count is the settings', and the gate's own call doesn't change.
        self.worker(ssh_host="oby-mac-pro")
        done = subprocess.run([shutil.which("python3"), "-c", read, str(self.main / "factory" / "bin" / "gate")],
                              cwd=self.main, env=self.env, capture_output=True, text=True, timeout=60)
        self.assertEqual(json.loads(done.stdout)[:2], [{"builder_slots": 0, "ssh_host": "oby-mac-pro"}, None])

    # ---- REMOTE-41 -------------------------------------------------------------------------------

    def test_remote_41_a_run_marked_here_stays_on_this_mac(self):
        for path in (SANDBOX, "core/crates/obid/src/gateway.rs"):
            self.write(path, "// changed\n")
            (self.fake / "mac-runs").unlink(missing_ok=True)
            code, out, err = self.flagged("--here", "--", "cargo", "test", "--test", "one")
            self.assertEqual((code, out), (0, "this Mac's cargo ran\n"), err)
            self.assertEqual(self.said(err), ["factory/bin/test-run: on this Mac: the run asked for this Mac (--here)"])
            self.assertEqual(self.ssh_calls(), [])
            self.assertTrue(self.lines("mac-runs")[0].startswith("cargo test --test one | "), self.lines("mac-runs"))
            self.assertTrue(self.lock.exists(), "a run marked --here took no lock")
        self.assertEqual(self.vm_runs(), [])
        # Any other word before -- is a usage error.
        code, out, err = self.flagged("--bogus", "--", "true")
        self.assertEqual(code, 2)
        self.assertIn("Usage:\n  factory/bin/test-run [--here] -- <command> [args...]", err)
        self.assertEqual(self.ssh_calls(), [])

    def test_remote_41_a_swift_run_stays_here_with_a_mac_worker(self):
        self.write(SANDBOX, "// changed\n")
        self.assert_on_this_mac(self.run_test_run("swift", "test", "--package-path", "mcp"),
                                "on this Mac: swift runs only on a Mac", tool="swift")
        self.assertEqual(self.ssh_calls(), [])


class SetupMacBuilders(unittest.TestCase):
    """REMOTE-42: setup-mac.sh's builders root, with test_remote_gate.py's stand-in ssh that keeps what it sends."""

    setUp = test_remote_gate.SetupMac.setUp
    tearDown = test_remote_gate.SetupMac.tearDown
    tool = test_remote_gate.SetupMac.tool
    setup_mac = test_remote_gate.SetupMac.setup_mac

    def test_remote_42_setup_mac_makes_the_builders_root_apart_from_the_gates(self):
        done = self.setup_mac()
        self.assertEqual(done.returncode, 0, done.stderr)
        sent = self.sent.read_text()
        self.assertEqual(subprocess.run(["bash", "-n", str(self.sent)]).returncode, 0)
        self.assertNotRegex(sent, r"\bsudo\b")
        # The builders' lines, run as the host would, against a local origin, twice: safe to repeat.
        block = re.search(r'^    mkdir -p "\$root/builders"\n.*?config gc\.auto 0\n', sent, re.MULTILINE | re.DOTALL)
        self.assertIsNotNone(block, sent)
        self.assertIn('git clone --bare --quiet "$url" "$root/builders/repo.git"', block.group(0))
        origin, root = self.dir / "origin.git", self.dir / "oby-worker"
        subprocess.run(["git", "init", "-q", "--bare", str(origin)], check=True)
        script = f'set -euo pipefail\numask 022\nsay() {{ echo "$*"; }}\nurl={origin}\nroot={root}\n{block.group(0)}'
        for _ in range(2):
            ran = subprocess.run(["bash", "-c", script], capture_output=True, text=True, timeout=60)
            self.assertEqual(ran.returncode, 0, ran.stderr)
        clone = root / "builders" / "repo.git"
        git = ["git", "--git-dir", str(clone)]
        self.assertEqual(subprocess.run([*git, "rev-parse", "--is-bare-repository"], capture_output=True,
                                        text=True).stdout.strip(), "true")
        self.assertEqual(subprocess.run([*git, "config", "gc.auto"], capture_output=True, text=True).stdout.strip(),
                         "0")
        self.assertEqual(os.listdir(root), ["builders"], "the builders' lines touched the gates' root")
        self.assertEqual((root / "builders").stat().st_mode & 0o777, 0o755)


class TightenBuilders(unittest.TestCase):
    """REMOTE-42: tighten.sh covers the Mac worker's builders folder and its builder slots."""

    setUp = test_remote_gate.Tighten.setUp
    tearDown = test_remote_gate.Tighten.tearDown
    tighten = test_remote_gate.Tighten.tighten

    def test_remote_42_tighten_covers_the_mac_workers_builder_slots(self):
        builders = self.root / "builders"
        (builders / "builder-1").mkdir(parents=True)
        for path in (builders, builders / "builder-1"):
            os.chmod(path, 0o777)
        done = self.tighten()
        self.assertEqual(done.returncode, 0, done.stdout)
        for path in (builders, builders / "builder-1"):
            self.assertEqual(path.stat().st_mode & 0o777, 0o755, f"{path}\n{done.stdout}")
        lines = done.stdout.splitlines()
        for path in (builders, builders / "builder-1"):
            self.assertTrue(any(line.startswith("drwxr-xr-x") and line.endswith(str(path)) for line in lines),
                            done.stdout)
        # One it can't tighten stops setup, named.
        os.chmod(builders / "builder-1", 0o777)
        stand_in = self.dir / "bin"
        stand_in.mkdir()
        (stand_in / "chmod").write_text("#!/bin/sh\nexit 1\n")
        os.chmod(stand_in / "chmod", 0o755)
        self.env["PATH"] = f"{stand_in}{os.pathsep}{self.env['PATH']}"
        done = self.tighten()
        self.assertNotEqual(done.returncode, 0, done.stdout)
        self.assertIn(f"setup.sh: still writable by others: {builders / 'builder-1'}", done.stdout.splitlines())


if __name__ == "__main__":
    unittest.main(verbosity=2)
