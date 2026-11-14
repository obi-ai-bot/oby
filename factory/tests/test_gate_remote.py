#!/usr/bin/env python3
"""factory/bin/gate's remote path (REMOTE-1, 3, 4, 5, 6, 10, 13 to 16), with a fake `az` and `ssh`, never the
VM or the Mac worker.

Run from the repository root: python3 factory/tests/test_gate_remote.py
Each test builds a throwaway repository: a bare `origin` with a `dev` branch whose scripts/oby-check.sh is a
stand-in, a clone holding copies of factory/bin/gate and factory/bin/job, the real factory/config.json and a
stand-in factory/checks.sh, and a job whose worktree is another clone on a pushed branch. The fake `az` and
`ssh` come first on PATH: each records its arguments and acts from files the test writes.
"""

import json
import os
import re
import shlex
import shutil
import subprocess
import tempfile
import unittest
from datetime import datetime, timedelta, timezone
from pathlib import Path

FACTORY = Path(__file__).resolve().parents[1]
REMOTE = json.dumps({"ssh_host": "oby-gate", "resource_group": "oby-factory", "vm": "oby-gate"})
PRO = json.dumps({"ssh_host": "oby-mac-pro"})
JOB_ID = "001-a00-01-something"

# The stand-in check: its plan prints $FAKE/groups' groups and exits with $FAKE/plan-exit's code.
FAKE_CHECK = """#!/bin/sh
if [ -n "$OBY_CHECK_PLAN" ]; then
    for group in $(cat "$FAKE/groups"); do echo "oby-check: group $group"; done
    exit $(cat "$FAKE/plan-exit" 2>/dev/null || echo 0)
fi
exit 0
"""

# The stand-in factory/checks.sh: records $OBY_CHECK_ONLY (or `all`) and exits with $FAKE/mac-exit-<it>'s code.
STAND_IN = """#!/usr/bin/env bash
part="${OBY_CHECK_ONLY:-all}"
echo "$part" >>"$FAKE/mac-runs"
echo "the Mac's $part output"
exit $(cat "$FAKE/mac-exit-$part" 2>/dev/null || echo 0)
"""

# get-instance-view prints $FAKE/states' first line and drops it while more follow; start and deallocate exit
# with $FAKE/az-<action>-exit's code.
FAKE_AZ = """#!/usr/bin/env python3
import json, os, sys
from pathlib import Path
fake = Path(os.environ["FAKE"])
with open(fake / "az.log", "a") as log:
    log.write(json.dumps(sys.argv[1:]) + "\\n")
if sys.argv[2] == "get-instance-view":
    states = (fake / "states").read_text().splitlines()
    print(states[0])
    if len(states) > 1:
        (fake / "states").write_text("\\n".join(states[1:]) + "\\n")
    sys.exit(0)
code = fake / f"az-{sys.argv[2]}-exit"
sys.exit(int(code.read_text()) if code.exists() else 0)
"""

# oby-gate: `ssh ... true` exits with $FAKE/ssh-reach's code; the remote gate prints a slot and exits with
# $FAKE/remote-exit's. oby-mac-pro: `true` exits with $FAKE/pro-reach's code; with $FAKE/pro-busy its gate says
# both slots are busy and exits 2, else it prints slot 1 and exits with $FAKE/pro-exit's code.
FAKE_SSH = """#!/usr/bin/env python3
import json, os, stat, sys
from pathlib import Path
fake = Path(os.environ["FAKE"])
with open(fake / "ssh.log", "a") as log:
    log.write(json.dumps({"argv": sys.argv[1:], "stdin_pipe": stat.S_ISFIFO(os.fstat(0).st_mode)}) + "\\n")
def read(name, default):
    return int((fake / name).read_text()) if (fake / name).exists() else default
if sys.argv[-2] == "oby-mac-pro":
    if sys.argv[-1] == "true":
        sys.exit(read("pro-reach", 0))
    if (fake / "pro-busy").exists():
        print("oby-remote-gate: both slots are busy")
        sys.exit(2)
    code = read("pro-exit", 0)
    print("oby-remote-gate: slot 1")
    print("the Pro's output")
    if code in (0, 1, 2):
        verdict = ["passed", "failed", "couldn't judge"][code]
        print(f"oby-remote-gate: {verdict} in slot 1 after 1s")
    sys.exit(code)
if sys.argv[-1] == "true":
    sys.exit(read("ssh-reach", 0))
code = read("remote-exit", 0)
print("oby-remote-gate: slot 2")
print("the VM's output")
verdict = ["passed", "failed", "couldn't judge"][code]
print(f"oby-remote-gate: {verdict} in slot 2 after 1s")
sys.exit(code)
"""


def ago(minutes):
    return (datetime.now(timezone.utc) - timedelta(minutes=minutes)).strftime("%Y-%m-%dT%H:%M:%SZ")


class GateRemote(unittest.TestCase):
    def setUp(self):
        self.dir = Path(tempfile.mkdtemp()).resolve()
        self.fake = self.dir / "fake"
        self.fake.mkdir()
        bin_dir = self.dir / "bin"
        bin_dir.mkdir()
        for name, text in (("az", FAKE_AZ), ("ssh", FAKE_SSH)):
            (bin_dir / name).write_text(text)
            (bin_dir / name).chmod(0o755)
        self.env = {k: v for k, v in os.environ.items()
                    if not k.startswith(("OBY_", "FACTORY_REMOTE_")) and k != "FACTORY_MAC_WORKER"}
        self.env.update(
            GIT_CONFIG_GLOBAL="/dev/null", GIT_CONFIG_NOSYSTEM="1",
            GIT_AUTHOR_NAME="test", GIT_AUTHOR_EMAIL="test@example.com",
            GIT_COMMITTER_NAME="test", GIT_COMMITTER_EMAIL="test@example.com",
            CARGO_TARGET_DIR_FACTORY=str(self.dir / "cargo"),
            FAKE=str(self.fake),
            PATH=f"{bin_dir}{os.pathsep}{os.environ['PATH']}",
        )
        origin, seed = self.dir / "origin.git", self.dir / "seed"
        self.git(self.dir, "init", "-q", "--bare", str(origin))
        self.git(self.dir, "init", "-q", "-b", "dev", str(seed))
        check = seed / "scripts" / "oby-check.sh"
        check.parent.mkdir()
        check.write_text(FAKE_CHECK)
        check.chmod(0o755)
        self.git(seed, "add", ".")
        self.git(seed, "commit", "-q", "-m", "base")
        self.git(seed, "remote", "add", "origin", str(origin))
        self.git(seed, "push", "-q", "origin", "dev")
        # The main checkout: the gate, job, config.json and the stand-in checks.sh.
        self.repo = self.dir / "repo"
        self.git(self.dir, "clone", "-q", "-b", "dev", str(origin), str(self.repo))
        (self.repo / "factory" / "bin").mkdir(parents=True)
        for name in ("gate", "job"):
            shutil.copy2(FACTORY / "bin" / name, self.repo / "factory" / "bin" / name)
        shutil.copy2(FACTORY / "config.json", self.repo / "factory" / "config.json")
        (self.repo / "factory" / "checks.sh").write_text(STAND_IN)
        # The job's worktree, on a branch pushed to origin.
        self.worktree = self.dir / "worktree"
        self.git(self.dir, "clone", "-q", "-b", "dev", str(origin), str(self.worktree))
        self.git(self.worktree, "checkout", "-q", "-b", "factory/x")
        self.change("core/crates/obid/src/gateway.rs")
        self.git(self.worktree, "push", "-q", "origin", "factory/x")
        self.job_dir = self.repo / "factory" / "jobs" / JOB_ID
        (self.job_dir / "round-1").mkdir(parents=True)
        self.write_job("gate")
        self.state = self.repo / "factory" / "jobs" / "remote-gate" / "state.json"
        self.groups("rust", "scan")
        (self.fake / "states").write_text("PowerState/running\n")

    def tearDown(self):
        shutil.rmtree(self.dir)

    def git(self, cwd, *args):
        return subprocess.run(["git", *args], cwd=cwd, env=self.env, check=True,
                              capture_output=True, text=True).stdout.strip()

    def change(self, path, push=False, text="x\n"):
        file = self.worktree / path
        file.parent.mkdir(parents=True, exist_ok=True)
        file.write_text(file.read_text() + text if file.exists() else text)
        self.git(self.worktree, "add", path)
        self.git(self.worktree, "commit", "-q", "-m", f"change {path}")
        if push:
            self.git(self.worktree, "push", "-q", "origin", "factory/x")
        return self.git(self.worktree, "rev-parse", "HEAD")

    def write_job(self, stage):
        (self.job_dir / "job.json").write_text(json.dumps({
            "id": JOB_ID, "issue": "A00-01", "stage": stage, "round": 1, "pr": None,
            "worktree": str(self.worktree), "branch": "factory/x",
        }))

    def groups(self, *names):
        (self.fake / "groups").write_text(" ".join(names) + "\n")

    def run_bin(self, name, *args, remote=True, **env):
        # Off is FACTORY_REMOTE_GATE=null, so the test doesn't depend on config.json's own remote_gate; the Mac
        # worker is off (FACTORY_MAC_WORKER=null) unless a test gives it, or None to leave it unset.
        env = {**self.env, "FACTORY_REMOTE_GATE": REMOTE if remote else "null", "FACTORY_MAC_WORKER": "null", **env}
        env = {k: v for k, v in env.items() if v is not None}
        res = subprocess.run([str(self.repo / "factory" / "bin" / name), *args], cwd=self.repo, env=env,
                             capture_output=True, text=True, timeout=120)
        return res.returncode, res.stdout, res.stderr

    def gate(self, **kw):
        return self.run_bin("gate", JOB_ID, **kw)

    def checks(self):
        return (self.job_dir / "round-1" / "checks.md").read_text()

    def header(self):
        return dict(line.split(": ", 1) for line in self.checks().split("\n\n", 1)[0].splitlines())

    def lines(self, name):
        path = self.fake / name
        return path.read_text().splitlines() if path.exists() else []

    def az_calls(self):
        return [json.loads(line) for line in self.lines("az.log")]

    def ssh_calls(self):
        return [json.loads(line) for line in self.lines("ssh.log")]

    def remote_calls(self):
        return [call for call in self.ssh_calls() if call["argv"][-1] != "true"]

    def pro_calls(self):
        return [call for call in self.ssh_calls() if "oby-mac-pro" in call["argv"]]

    def config(self, **keys):
        """The test's config.json: the real one, with these keys set, or dropped when None."""
        path = self.repo / "factory" / "config.json"
        config = json.loads(path.read_text())
        for key, value in keys.items():
            config.pop(key, None) if value is None else config.update({key: value})
        path.write_text(json.dumps(config))

    # ---- REMOTE-1 --------------------------------------------------------------------------------

    def test_remote_1_a_deallocated_vm_is_started_and_runs_the_linux_groups(self):
        (self.fake / "states").write_text("PowerState/deallocated\n")
        commit = self.git(self.worktree, "rev-parse", "HEAD")
        code, out, err = self.gate()
        self.assertEqual(code, 0, out + err)
        self.assertIn(["vm", "start", "--resource-group", "oby-factory", "--name", "oby-gate", "--no-wait"],
                      self.az_calls())
        [call] = self.remote_calls()
        self.assertEqual(call["argv"][-2:], ["oby-gate", shlex.join(["oby-remote-gate", commit, "origin/dev"])])
        for option in ("BatchMode=yes", "ForwardAgent=no", "ConnectTimeout=10", "ServerAliveInterval=30",
                       "ServerAliveCountMax=4"):
            self.assertIn(option, call["argv"])
        self.assertTrue(call["stdin_pipe"], "the remote gate's standard input isn't a pipe")
        h = self.header()
        self.assertEqual((h["GATE"], h["COMMIT"], h["HOST"]), ("PASS", commit, "oby-gate"))
        self.assertIn("## Where it ran\n- oby-gate: rust, scan in slot 2: passed with exit 0 in", self.checks())
        self.assertIn("the VM's output", (self.job_dir / "round-1" / "checks.log").read_text())
        self.assertTrue(out.startswith(f"GATE: PASS ({JOB_ID} round 1, on oby-gate, exit 0"), out)
        self.assertEqual(self.lines("mac-runs"), [], "the Mac ran a part with nothing for it")
        self.assertIn("used", json.loads(self.state.read_text()))

    def test_remote_1_a_vm_failure_fails_the_gate_without_a_mac_retry(self):
        (self.fake / "remote-exit").write_text("1\n")
        code, out, err = self.gate()
        self.assertEqual(code, 1, out + err)
        self.assertEqual((self.header()["GATE"], self.header()["HOST"]), ("FAIL", "oby-gate"))
        self.assertEqual(self.lines("mac-runs"), [], "the Mac ran the check after the VM's failure")
        self.assertNotIn("Fell back", self.checks())
        self.assertEqual(len(self.remote_calls()), 1)

    # ---- REMOTE-3 --------------------------------------------------------------------------------

    def test_remote_3_an_unreachable_vm_falls_back_to_the_mac(self):
        (self.fake / "ssh-reach").write_text("255\n")
        code, out, err = self.gate(FACTORY_REMOTE_REACH_SECONDS="1")
        self.assertEqual(code, 0, out + err)
        self.assertEqual(self.lines("mac-runs"), ["all"])
        self.assertEqual(self.remote_calls(), [])
        self.assertGreater(len(self.ssh_calls()), 0)
        self.assertEqual((self.header()["GATE"], self.header()["HOST"]), ("PASS", "mac"))
        self.assertIn("- Fell back to the Mac: ssh oby-gate didn't answer within 1s\n", self.checks())
        self.assertIn("\nFell back to the Mac: ssh oby-gate didn't answer within 1s\n", self.checks())
        self.assertIn("fell back to the Mac", out)

    def test_remote_3_couldnt_judge_falls_back_to_the_mac(self):
        (self.fake / "remote-exit").write_text("2\n")
        code, out, err = self.gate()
        self.assertEqual(code, 0, out + err)
        self.assertEqual(self.lines("mac-runs"), ["all"])
        self.assertEqual((self.header()["GATE"], self.header()["HOST"]), ("PASS", "mac"))
        self.assertIn("Fell back to the Mac: oby-remote-gate couldn't judge (exit 2)", self.checks())
        log = (self.job_dir / "round-1" / "checks.log").read_text()
        self.assertIn("==== oby-gate: its output before the fallback, not part of the verdict ====\n"
                      "oby-remote-gate: slot 2\nthe VM's output", log)
        self.assertIn("the Mac's all output", log)

    def test_remote_3_a_commit_not_on_origin_runs_on_the_mac(self):
        self.change("core/crates/obid/src/gateway.rs")  # not pushed
        code, out, err = self.gate()
        self.assertEqual(code, 0, out + err)
        self.assertEqual((self.az_calls(), self.ssh_calls()), ([], []))
        self.assertEqual(self.lines("mac-runs"), ["all"])
        self.assertEqual(self.header()["HOST"], "mac")
        self.assertIn("Fell back to the Mac: not on origin", self.checks())
        self.assertFalse(self.state.exists(), "a gate that never used the VM stamped it")

    # ---- REMOTE-4 --------------------------------------------------------------------------------

    def test_remote_4_a_macos_only_path_also_runs_rust_on_the_mac(self):
        self.change("core/crates/obid/src/sandbox.rs", push=True)
        code, out, err = self.gate()
        self.assertEqual(code, 0, out + err)
        self.assertEqual(self.lines("mac-runs"), ["rust"])
        self.assertEqual((self.header()["GATE"], self.header()["HOST"]), ("PASS", "oby-gate, mac"))
        self.assertIn("- mac: rust (the diff touches macOS-only code: core/crates/obid/src/sandbox.rs): "
                      "passed with exit 0", self.checks())
        # Only the Mac's part fails: the gate fails.
        (self.fake / "mac-exit-rust").write_text("1\n")
        code, out, err = self.gate()
        self.assertEqual(code, 1, out + err)
        self.assertEqual(self.lines("mac-runs"), ["rust", "rust"])
        self.assertEqual((self.header()["GATE"], self.header()["HOST"]), ("FAIL", "oby-gate, mac"))
        # The VM's part fails: the Mac's isn't run.
        (self.fake / "remote-exit").write_text("1\n")
        code, out, err = self.gate()
        self.assertEqual(code, 1, out + err)
        self.assertEqual(self.lines("mac-runs"), ["rust", "rust"])
        self.assertIn("not run: the VM's part failed", self.checks())
        self.assertEqual(self.header()["HOST"], "oby-gate")

    def test_remote_4_macos_code_in_an_unlisted_file_also_runs_rust_on_the_mac(self):
        self.change("core/crates/obid/src/jobs.rs", push=True,
                    text='#[cfg(target_os = "macos")]\nfn keychain_only() {}\n')
        code, out, err = self.gate()
        self.assertEqual(code, 0, out + err)
        self.assertEqual(self.lines("mac-runs"), ["rust"])
        self.assertEqual(self.header()["HOST"], "oby-gate, mac")
        self.assertIn("(the diff touches macOS-only code: core/crates/obid/src/jobs.rs)", self.checks())

    def test_remote_4_a_deleted_file_with_macos_code_also_runs_rust_on_the_mac(self):
        # dev gains a file with a macOS-only test; the branch takes it, then deletes it.
        self.git(self.worktree, "checkout", "-q", "-b", "dev-next", "origin/dev")
        self.change("core/crates/obid/src/power.rs", text='#[cfg(all(test, target_os = "macos"))]\nmod tests {}\n')
        self.git(self.worktree, "push", "-q", "origin", "dev-next:dev")
        self.git(self.worktree, "checkout", "-q", "factory/x")
        self.git(self.worktree, "merge", "-q", "--no-edit", "origin/dev")
        self.git(self.worktree, "rm", "-q", "core/crates/obid/src/power.rs")
        self.git(self.worktree, "commit", "-q", "-m", "delete power.rs")
        self.git(self.worktree, "push", "-q", "origin", "factory/x")
        code, out, err = self.gate()
        self.assertEqual(code, 0, out + err)
        self.assertEqual(self.lines("mac-runs"), ["rust"])
        self.assertIn("(the diff touches macOS-only code: core/crates/obid/src/power.rs)", self.checks())

    def test_remote_4_a_listed_file_whose_macos_tests_live_elsewhere_runs_rust_on_the_mac(self):
        # accounts.rs names no macOS cfg; its Keychain tests are macOS-only in tests/accounts_api.rs.
        self.change("core/crates/obid/src/accounts.rs", push=True)
        code, out, err = self.gate()
        self.assertEqual(code, 0, out + err)
        self.assertEqual(self.lines("mac-runs"), ["rust"])
        self.assertIn("(the diff touches macOS-only code: core/crates/obid/src/accounts.rs)", self.checks())

    def test_remote_4_swift_runs_on_the_mac(self):
        self.groups("scan", "swift")
        code, out, err = self.gate()
        self.assertEqual(code, 0, out + err)
        self.assertEqual(self.lines("mac-runs"), ["swift"])
        self.assertEqual(self.header()["HOST"], "oby-gate, mac")
        self.assertIn("- oby-gate: scan in slot 2: passed", self.checks())

    def test_remote_4_factory_runs_on_the_mac(self):
        self.groups("scan", "factory")
        code, out, err = self.gate()
        self.assertEqual(code, 0, out + err)
        self.assertEqual(self.lines("mac-runs"), ["factory"])
        self.assertEqual(self.header()["HOST"], "oby-gate, mac")
        self.assertIn("- mac: factory: passed with exit 0", self.checks())

    def test_remote_4_a_factory_failure_on_the_mac_fails_the_gate(self):
        self.groups("scan", "factory")
        (self.fake / "mac-exit-factory").write_text("1\n")
        code, out, err = self.gate()
        self.assertEqual(code, 1, out + err)
        self.assertEqual((self.header()["GATE"], self.header()["HOST"]), ("FAIL", "oby-gate, mac"))

    def test_remote_4_factory_isnt_run_when_the_vms_part_failed(self):
        self.groups("scan", "factory")
        (self.fake / "remote-exit").write_text("1\n")
        code, out, err = self.gate()
        self.assertEqual(code, 1, out + err)
        self.assertEqual(self.lines("mac-runs"), [])
        self.assertIn("- mac: factory: not run: the VM's part failed", self.checks())

    # ---- REMOTE-10 -------------------------------------------------------------------------------

    def assert_the_whole_check_runs_on_the_mac(self, path):
        self.change(path, push=True)
        code, out, err = self.gate()
        self.assertEqual(code, 0, out + err)
        self.assertEqual(self.lines("mac-runs"), ["all"])
        self.assertEqual((self.az_calls(), self.ssh_calls()), ([], []))
        self.assertFalse(self.state.exists(), "a gate that never used the VM stamped it")
        self.assertEqual((self.header()["GATE"], self.header()["HOST"]), ("PASS", "mac"))
        self.assertIn(f"- mac: the whole check (the diff touches the check's own files: {path}): passed with exit 0",
                      self.checks())
        self.assertNotIn("Fell back", self.checks())
        # The Mac's run is the verdict.
        (self.fake / "mac-exit-all").write_text("1\n")
        code, out, err = self.gate()
        self.assertEqual(code, 1, out + err)
        self.assertEqual((self.header()["GATE"], self.header()["HOST"]), ("FAIL", "mac"))
        self.assertEqual((self.az_calls(), self.ssh_calls()), ([], []))

    def test_remote_10_a_change_to_the_checks_test_runs_the_whole_check_on_the_mac(self):
        self.assert_the_whole_check_runs_on_the_mac("scripts/oby-check-test.sh")

    def test_remote_10_a_change_under_oby_check_blok_runs_the_whole_check_on_the_mac(self):
        self.assert_the_whole_check_runs_on_the_mac("tools/oby-check-blok/src/main.rs")

    # ---- REMOTE-13 ------------------------------------------------------------------------------

    SANDBOX = "rust (the diff touches macOS-only code: core/crates/obid/src/sandbox.rs)"

    def test_remote_13_a_free_mac_worker_runs_the_macos_part(self):
        commit = self.change("core/crates/obid/src/sandbox.rs", push=True)
        code, out, err = self.gate(FACTORY_MAC_WORKER=PRO)
        self.assertEqual(code, 0, out + err)
        [call] = [call for call in self.pro_calls() if call["argv"][-1] != "true"]
        self.assertEqual(call["argv"][-2:],
                         ["oby-mac-pro", f"oby-worker/bin/oby-remote-gate --mac {commit} origin/dev rust"])
        for option in ("BatchMode=yes", "ForwardAgent=no", "ConnectTimeout=10"):
            self.assertIn(option, call["argv"])
        self.assertTrue(call["stdin_pipe"], "the Mac worker's standard input isn't a pipe")
        self.assertEqual(self.lines("mac-runs"), [], "this Mac ran a part the Mac worker had")
        self.assertFalse([c for c in self.az_calls() if "oby-mac-pro" in c], self.az_calls())
        self.assertEqual((self.header()["GATE"], self.header()["HOST"]), ("PASS", "oby-gate, oby-mac-pro"))
        self.assertIn(f"\n- oby-mac-pro: {self.SANDBOX} in slot 1: passed with exit 0 in", self.checks())
        self.assertNotIn("Fell back", self.checks())
        self.assertIn(f"==== oby-mac-pro: {self.SANDBOX} ====\noby-remote-gate: slot 1\nthe Pro's output",
                      (self.job_dir / "round-1" / "checks.log").read_text())
        self.assertTrue(out.startswith(f"GATE: PASS ({JOB_ID} round 1, on oby-gate, oby-mac-pro, exit 0"), out)

    def test_remote_13_a_mac_worker_failure_fails_the_gate_without_a_retry(self):
        self.change("core/crates/obid/src/sandbox.rs", push=True)
        (self.fake / "pro-exit").write_text("1\n")
        code, out, err = self.gate(FACTORY_MAC_WORKER=PRO)
        self.assertEqual(code, 1, out + err)
        self.assertEqual((self.header()["GATE"], self.header()["HOST"]), ("FAIL", "oby-gate, oby-mac-pro"))
        self.assertIn(f"- oby-mac-pro: {self.SANDBOX} in slot 1: failed with exit 1 in", self.checks())
        self.assertEqual(self.lines("mac-runs"), [], "this Mac ran the part again after the Mac worker's failure")
        self.assertEqual(len([call for call in self.pro_calls() if call["argv"][-1] != "true"]), 1)
        self.assertNotIn("Fell back", self.checks())

    def test_remote_13_swift_and_factory_run_on_the_mac_worker(self):
        # From config.json's own mac_worker, with FACTORY_MAC_WORKER unset.
        self.config(mac_worker={"ssh_host": "oby-mac-pro"})
        self.groups("rust", "scan", "swift", "factory")
        commit = self.change("core/crates/obid/src/sandbox.rs", push=True)
        code, out, err = self.gate(FACTORY_MAC_WORKER=None)
        self.assertEqual(code, 0, out + err)
        [call] = [call for call in self.pro_calls() if call["argv"][-1] != "true"]
        self.assertEqual(call["argv"][-1],
                         f"oby-worker/bin/oby-remote-gate --mac {commit} origin/dev rust swift factory")
        self.assertEqual(self.lines("mac-runs"), [])
        self.assertEqual(self.header()["HOST"], "oby-gate, oby-mac-pro")
        self.assertIn(f"- oby-mac-pro: {self.SANDBOX}, swift, factory in slot 1: passed with exit 0", self.checks())
        # Without rust's macOS-only reason, only swift and factory go.
        self.git(self.worktree, "reset", "-q", "--hard", "HEAD~1")
        commit = self.change("core/crates/obid/src/gateway.rs")
        self.git(self.worktree, "push", "-q", "--force", "origin", "factory/x")
        code, out, err = self.gate(FACTORY_MAC_WORKER=None)
        self.assertEqual(code, 0, out + err)
        self.assertEqual(self.pro_calls()[-1]["argv"][-1],
                         f"oby-worker/bin/oby-remote-gate --mac {commit} origin/dev swift factory")
        self.assertIn("- oby-mac-pro: swift, factory in slot 1: passed with exit 0", self.checks())

    # ---- REMOTE-14 ------------------------------------------------------------------------------

    def assert_the_macos_part_fell_back(self, why, output=None, **env):
        self.change("core/crates/obid/src/sandbox.rs", push=True)
        code, out, err = self.gate(FACTORY_MAC_WORKER=PRO, **env)
        self.assertEqual(code, 0, out + err)
        self.assertEqual(self.lines("mac-runs"), ["rust"])
        self.assertEqual((self.header()["GATE"], self.header()["HOST"]), ("PASS", "oby-gate, mac"))
        line = f"Fell back from oby-mac-pro to this Mac: {why}"
        self.assertIn(f"\n- {line}\n", self.checks())
        self.assertIn("## Where it ran\n- oby-gate: rust, scan in slot 2: passed with exit 0 in", self.checks())
        self.assertIn(f"\n- mac: {self.SANDBOX}: passed with exit 0 in", self.checks())
        self.assertIn(f"\n\n{line}\n", self.checks())
        self.assertIn(f", fell back from oby-mac-pro to this Mac: {why})", out)
        log = (self.job_dir / "round-1" / "checks.log").read_text()
        if output:
            self.assertIn("==== oby-mac-pro: its output before the fallback, not part of the verdict ====\n"
                          + output, log)
        self.assertIn("the Mac's rust output", log)
        # This Mac's run is the verdict.
        (self.fake / "mac-exit-rust").write_text("1\n")
        code, out, err = self.gate(FACTORY_MAC_WORKER=PRO, **env)
        self.assertEqual(code, 1, out + err)
        self.assertEqual((self.header()["GATE"], self.header()["HOST"]), ("FAIL", "oby-gate, mac"))

    def test_remote_14_an_unreachable_mac_worker_falls_back_to_this_mac(self):
        (self.fake / "pro-reach").write_text("255\n")
        self.assert_the_macos_part_fell_back("ssh oby-mac-pro didn't answer within 1s",
                                             FACTORY_REMOTE_REACH_SECONDS="1")
        self.assertEqual([call for call in self.pro_calls() if call["argv"][-1] != "true"], [])
        self.assertGreater(len(self.pro_calls()), 0)

    def test_remote_14_a_busy_mac_worker_falls_back_to_this_mac(self):
        (self.fake / "pro-busy").touch()
        self.assert_the_macos_part_fell_back("both of its slots are busy", "oby-remote-gate: both slots are busy")

    def test_remote_14_couldnt_judge_falls_back_to_this_mac(self):
        (self.fake / "pro-exit").write_text("2\n")
        self.assert_the_macos_part_fell_back("oby-remote-gate couldn't judge (exit 2)",
                                             "oby-remote-gate: slot 1\nthe Pro's output")

    def test_remote_14_cut_off_falls_back_to_this_mac(self):
        (self.fake / "pro-exit").write_text("255\n")
        self.assert_the_macos_part_fell_back("oby-remote-gate was cut off (exit 255)",
                                             "oby-remote-gate: slot 1\nthe Pro's output")

    # ---- REMOTE-15 ------------------------------------------------------------------------------

    def test_remote_15_the_mac_worker_isnt_used_when_the_vms_part_failed(self):
        self.change("core/crates/obid/src/sandbox.rs", push=True)
        (self.fake / "remote-exit").write_text("1\n")
        code, out, err = self.gate(FACTORY_MAC_WORKER=PRO)
        self.assertEqual(code, 1, out + err)
        self.assertEqual(self.pro_calls(), [])
        self.assertEqual(self.lines("mac-runs"), [])
        self.assertIn(f"- mac: {self.SANDBOX}: not run: the VM's part failed", self.checks())
        self.assertEqual(self.header()["HOST"], "oby-gate")

    def test_remote_15_a_commit_not_on_origin_never_reaches_the_mac_worker(self):
        self.change("core/crates/obid/src/sandbox.rs")  # not pushed
        code, out, err = self.gate(FACTORY_MAC_WORKER=PRO)
        self.assertEqual(code, 0, out + err)
        self.assertEqual(self.ssh_calls(), [])
        self.assertEqual(self.lines("mac-runs"), ["all"])
        self.assertIn("Fell back to the Mac: not on origin", self.checks())

    def test_remote_15_a_change_to_the_checks_own_files_never_reaches_the_mac_worker(self):
        self.change("core/crates/obid/src/sandbox.rs")
        self.change("scripts/oby-check-test.sh", push=True)
        code, out, err = self.gate(FACTORY_MAC_WORKER=PRO)
        self.assertEqual(code, 0, out + err)
        self.assertEqual((self.az_calls(), self.ssh_calls()), ([], []))
        self.assertEqual(self.lines("mac-runs"), ["all"])
        self.assertEqual(self.header()["HOST"], "mac")

    # ---- REMOTE-16 ------------------------------------------------------------------------------

    def test_remote_16_unset_mac_worker_is_unchanged(self):
        self.change("core/crates/obid/src/sandbox.rs", push=True)
        for value in (None, "null"):
            # config.json without mac_worker, then with it null; FACTORY_MAC_WORKER unset.
            self.config(mac_worker=None if value is None else json.loads(value))
            (self.fake / "mac-runs").unlink(missing_ok=True)
            code, out, err = self.gate(FACTORY_MAC_WORKER=None)
            self.assertEqual(code, 0, out + err)
            self.assertEqual(self.pro_calls(), [])
            self.assertEqual(self.lines("mac-runs"), ["rust"])
            self.assertEqual((self.header()["GATE"], self.header()["HOST"]), ("PASS", "oby-gate, mac"))
            self.assertNotIn("Fell back", self.checks())
            self.assertRegex(self.checks(), r"## Where it ran\n- oby-gate: rust, scan in slot 2: passed with exit 0 "
                             rf"in \d+s\n- mac: {re.escape(self.SANDBOX)}: passed with exit 0 in \d+s\n\n## Protected")
            self.assertRegex(out, rf"^GATE: PASS \({JOB_ID} round 1, on oby-gate, mac, exit 0 in \d+s\)\n$")

    def test_remote_16_invalid_mac_worker_settings_run_on_this_mac(self):
        self.change("core/crates/obid/src/sandbox.rs", push=True)
        for value, why in (("{oby-mac-pro", "FACTORY_MAC_WORKER isn't JSON"),
                           (json.dumps({"ssh_host": "-oProxyCommand=touch pwned"}),
                            "FACTORY_MAC_WORKER needs ssh_host, matching [A-Za-z0-9][A-Za-z0-9._-]*"),
                           (json.dumps(["oby-mac-pro"]),
                            "FACTORY_MAC_WORKER needs ssh_host, matching [A-Za-z0-9][A-Za-z0-9._-]*")):
            (self.fake / "mac-runs").unlink(missing_ok=True)
            code, out, err = self.gate(FACTORY_MAC_WORKER=value)
            self.assertEqual(code, 0, out + err)
            self.assertEqual(self.lines("mac-runs"), ["rust"])
            self.assertEqual(self.header()["HOST"], "oby-gate, mac")
            self.assertIn(f"\n- Fell back from the Mac worker to this Mac: {why}\n", self.checks())
            self.assertEqual([call["argv"][-2] for call in self.ssh_calls()], ["oby-gate"] * len(self.ssh_calls()))

    # ---- REMOTE-5 --------------------------------------------------------------------------------

    def write_state(self, minutes):
        self.state.parent.mkdir(parents=True, exist_ok=True)
        self.state.write_text(json.dumps({"used": ago(minutes)}))

    def deallocations(self):
        return [call for call in self.az_calls() if call[1] == "deallocate"]

    def test_remote_5_deallocates_only_after_30_idle_minutes(self):
        self.write_job("review")
        self.write_state(29)
        self.assertEqual(self.run_bin("gate", "--idle"), (0, "", ""))
        self.assertEqual(self.az_calls(), [])
        for stage in ("gate", "merging"):
            self.write_job(stage)
            self.write_state(31)
            self.assertEqual(self.run_bin("gate", "--idle"), (0, "", ""))
            self.assertEqual(self.az_calls(), [], stage)
        self.write_job("review")
        # az fails: the next run tries again.
        (self.fake / "az-deallocate-exit").write_text("1\n")
        code, out, _ = self.run_bin("gate", "--idle")
        self.assertEqual(code, 0)
        self.assertIn("couldn't deallocate oby-gate", out)
        self.assertNotIn("settled", json.loads(self.state.read_text()))
        (self.fake / "az-deallocate-exit").unlink()
        code, out, _ = self.run_bin("gate", "--idle")
        self.assertEqual(code, 0)
        self.assertTrue(out.startswith("gate --idle: deallocating oby-gate"), out)
        self.assertEqual(self.deallocations()[-1],
                         ["vm", "deallocate", "--resource-group", "oby-factory", "--name", "oby-gate", "--no-wait"])
        calls = len(self.az_calls())
        self.assertEqual(self.run_bin("gate", "--idle"), (0, "", ""))
        self.assertEqual(len(self.az_calls()), calls, "a settled stamp called az again")
        # A VM already deallocated is never deallocated.
        self.write_state(31)
        (self.fake / "states").write_text("PowerState/deallocated\n")
        self.assertEqual(self.run_bin("gate", "--idle"), (0, "", ""))
        self.assertEqual(len(self.deallocations()), 2)
        self.assertEqual(self.az_calls()[-1][1], "get-instance-view")
        state = json.loads(self.state.read_text())
        self.assertEqual(state["settled"], state["used"])

    def test_remote_5_job_queue_runs_the_idle_rule_and_keeps_its_rows(self):
        self.write_job("review")
        self.write_state(31)
        gate = self.repo / "factory" / "bin" / "gate"
        gate.rename(self.dir / "gate")
        code, before, err = self.run_bin("job", "queue")
        self.assertEqual(code, 0, err)
        self.assertEqual(self.az_calls(), [])
        (self.dir / "gate").rename(gate)
        code, after, err = self.run_bin("job", "queue")
        self.assertEqual(code, 0, err)
        self.assertEqual(after, before)
        self.assertTrue(after.splitlines()[-1].startswith("# sprint"), after)
        self.assertIn("gate --idle: deallocating oby-gate", err)
        self.assertEqual(len(self.deallocations()), 1)

    # ---- REMOTE-6 --------------------------------------------------------------------------------

    def test_remote_6_unset_remote_gate_is_unchanged(self):
        self.write_state(31)
        code, out, err = self.gate(remote=False)
        self.assertEqual(code, 0, out + err)
        self.assertEqual((self.az_calls(), self.ssh_calls()), ([], []))
        self.assertEqual(self.lines("mac-runs"), ["all"])
        self.assertEqual(list(self.header()), ["GATE", "COMMIT", "CHECKS"])
        self.assertNotIn("## Where it ran", self.checks())
        self.assertRegex(out, rf"^GATE: PASS \({JOB_ID} round 1, checks.sh exit 0 in \d+s\)\n$")
        self.write_job("review")
        self.assertEqual(self.run_bin("gate", "--idle", remote=False), (0, "", ""))
        self.assertEqual(self.az_calls(), [])


if __name__ == "__main__":
    unittest.main(verbosity=2)
