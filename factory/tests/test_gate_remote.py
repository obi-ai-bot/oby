#!/usr/bin/env python3
"""factory/bin/gate's remote path (REMOTE-1, 3, 4, 5, 6, 10, 13 to 16, 19 to 21, 28, 43, 44, 62 to 65), with a fake `az`,
`ssh` and IP lookup, never the VM, its network rule, the Mac worker or the network.

Run from the repository root: python3 factory/tests/test_gate_remote.py
Each test builds a throwaway repository: a bare `origin` with a `dev` branch whose scripts/oby-check.sh is a
stand-in, a clone holding copies of factory/bin/gate and factory/bin/job, the real factory/config.json and a
stand-in factory/checks.sh, and a job whose worktree is another clone on a pushed branch. The fake `az` and
`ssh` come first on PATH: each records its arguments and acts from files the test writes. This Mac's IP is read
from a file:// URL (FACTORY_REMOTE_IP_URL).
"""

import hashlib
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
SHARDS = json.dumps({"linux": 3, "mac": 2})
CHECK = "/usr/bin/shasum -a 256 oby-worker/bin/oby-remote-gate"  # the gate's check of the Mac worker's runner
JOB_ID = "001-a00-01-something"
OLD_IP, NEW_IP = "8.8.4.4", "1.1.1.1"  # this Mac's IP before and after it changes: global, but no one's home

# The stand-in check: its plan prints $FAKE/groups' groups and exits with $FAKE/plan-exit's code.
FAKE_CHECK = """#!/bin/sh
if [ -n "$OBY_CHECK_PLAN" ]; then
    for group in $(cat "$FAKE/groups"); do echo "oby-check: group $group"; done
    exit $(cat "$FAKE/plan-exit" 2>/dev/null || echo 0)
fi
exit 0
"""

# The stand-in factory/checks.sh: records $OBY_CHECK_ONLY (or `all`) and its $OBY_CHECK_SHARD when set, prints
# $FAKE/mac-says-<it>[-<k>-<n>] (with $FAKE/mac-garbles-<it>[-<k>-<n>], a byte that isn't UTF-8) and exits with $FAKE/mac-exit-<it>-<k>-<n>'s code, else $FAKE/mac-exit-<it>'s.
STAND_IN = """#!/usr/bin/env bash
part="${OBY_CHECK_ONLY:-all}"
shard="${OBY_CHECK_SHARD:+-${OBY_CHECK_SHARD/\\//-}}"
echo "$part${OBY_CHECK_SHARD:+ $OBY_CHECK_SHARD}" >>"$FAKE/mac-runs"
echo "the Mac's $part output"
cat "$FAKE/mac-says-$part$shard" 2>/dev/null
[[ ! -e "$FAKE/mac-garbles-$part$shard" ]] || printf '\\377\\n'
exit $(cat "$FAKE/mac-exit-$part$shard" 2>/dev/null || cat "$FAKE/mac-exit-$part" 2>/dev/null || echo 0)
"""

# get-instance-view prints $FAKE/states' first line and drops it while more follow; start and deallocate exit
# with $FAKE/az-<action>-exit's code. `network nsg rule show` prints $FAKE/rule-source as the rule's source and
# `update` writes its new source there; each exits with $FAKE/az-rule-<action>-exit's code first.
FAKE_AZ = """#!/usr/bin/env python3
import json, os, sys
from pathlib import Path
fake = Path(os.environ["FAKE"])
with open(fake / "az.log", "a") as log:
    log.write(json.dumps(sys.argv[1:]) + "\\n")
if sys.argv[1] == "network":
    action = sys.argv[4]
    code = fake / f"az-rule-{action}-exit"
    if code.exists():
        sys.exit(int(code.read_text()))
    if action == "show":
        print(json.dumps({"name": "ssh-from-owner", "sourceAddressPrefix": (fake / "rule-source").read_text().strip(),
                          "sourceAddressPrefixes": []}))
    else:
        new = sys.argv[sys.argv.index("--source-address-prefixes") + 1]
        (fake / "rule-source").write_text(new + "\\n")
    sys.exit(0)
if sys.argv[2] == "get-instance-view":
    states = (fake / "states").read_text().splitlines()
    print(states[0])
    if len(states) > 1:
        (fake / "states").write_text("\\n".join(states[1:]) + "\\n")
    sys.exit(0)
code = fake / f"az-{sys.argv[2]}-exit"
sys.exit(int(code.read_text()) if code.exists() else 0)
"""

# oby-gate: `ssh ... true` exits 255 while $FAKE/rule-source isn't $FAKE/ip (as /32 or bare), else with
# $FAKE/ssh-reach's code; the remote gate copies $FAKE_STATE to $FAKE/state-during-run, prints a slot and exits with
# $FAKE/remote-exit's. oby-mac-pro: `true` exits with $FAKE/pro-reach's code; a command starting /usr/bin/shasum
# runs through sh in $FAKE/pro-home (its cwd and HOME), its output and exit code passed through; with
# $FAKE/pro-busy its gate says both slots are busy and exits 2, else it prints slot 1 and exits with
# $FAKE/pro-exit's code. A call with `--shard <k>/<n>` reads <name>-<k>-<n> before each such file (remote-exit,
# pro-exit, pro-busy), prints slot k, its shard and $FAKE/<remote|pro>-says-<k>-<n>, and with
# $FAKE/<remote|pro>-silent-<k>-<n> leaves out its verdict line. A VM shard with $FAKE/vm-waits-<k>-<n> (seconds)
# waits until the Mac worker's call has started, and fails if it hasn't by then. Each gate call writes `start` and
# `end` with its host and shard to $FAKE/events.
FAKE_SSH = """#!/usr/bin/env python3
import json, os, shutil, stat, subprocess, sys, time
from pathlib import Path
fake = Path(os.environ["FAKE"])
with open(fake / "ssh.log", "a") as log:
    log.write(json.dumps({"argv": sys.argv[1:], "stdin_pipe": stat.S_ISFIFO(os.fstat(0).st_mode)}) + "\\n")
words = sys.argv[-1].split()
shard = words[words.index("--shard") + 1] if "--shard" in words else None
def own(name):
    mine = f"{name}-{shard.replace('/', '-')}" if shard else name
    return mine if (fake / mine).exists() else name
def read(name, default):
    return int((fake / own(name)).read_text()) if (fake / own(name)).exists() else default
def event(what, host):
    with open(fake / "events", "a") as events:
        events.write(f"{what} {host} {shard or '-'}\\n")
def run(host, slot, code):
    event("start", host)
    print(f"oby-remote-gate: slot {shard.split('/')[0] if shard else slot}")
    if shard:
        print(f"oby-remote-gate: shard {shard}")
    print("the Pro's output" if host == "pro" else "the VM's output")
    if shard and (fake / f"{host}-says-{shard.replace('/', '-')}").exists():
        print((fake / f"{host}-says-{shard.replace('/', '-')}").read_text().rstrip())
    if code in (0, 1, 2) and not (shard and (fake / f"{host}-silent-{shard.replace('/', '-')}").exists()):
        verdict = ["passed", "failed", "couldn't judge"][code]
        print(f"oby-remote-gate: {verdict} in slot {shard.split('/')[0] if shard else slot} after 1s")
    sys.stdout.flush()
    event("end", host)
    sys.exit(code)
if sys.argv[-2] == "oby-mac-pro":
    if sys.argv[-1] == "true":
        sys.exit(read("pro-reach", 0))
    if sys.argv[-1].startswith("/usr/bin/shasum"):
        home = fake / "pro-home"
        sys.exit(subprocess.run(["sh", "-c", sys.argv[-1]], cwd=home, env=dict(os.environ, HOME=str(home))).returncode)
    if (fake / own("pro-busy")).exists():
        print("oby-remote-gate: both slots are busy")
        sys.exit(2)
    run("pro", 1, read("pro-exit", 0))
if sys.argv[-1] == "true":
    ip = (fake / "ip").read_text().strip() if (fake / "ip").exists() else None
    if (fake / "rule-source").read_text().strip().removesuffix("/32") != ip:
        sys.exit(255)
    sys.exit(read("ssh-reach", 0))
shutil.copy(os.environ["FAKE_STATE"], fake / "state-during-run")
waits = fake / f"vm-waits-{shard.replace('/', '-')}" if shard else None
if waits and waits.exists():
    deadline = time.monotonic() + float(waits.read_text())
    while not any(line.startswith("start pro") for line in
                  ((fake / "events").read_text().splitlines() if (fake / "events").exists() else [])):
        if time.monotonic() > deadline:
            print(f"the VM's shard {shard} waited for the Mac worker's call, which never came")
            print(f"oby-remote-gate: failed in slot 1 after 1s")
            sys.exit(1)
        time.sleep(0.05)
run("remote", 2, read("remote-exit", 0))
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
                    if not k.startswith(("OBY_", "FACTORY_REMOTE_")) and k not in ("FACTORY_MAC_WORKER",
                                                                                  "FACTORY_GATE_SHARDS")}
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
        # The runner the gate checks the Mac worker's against (REMOTE-43), and the worker's copy of it.
        self.runner = self.repo / "factory" / "remote" / "oby-remote-gate"
        self.pro_runner = self.fake / "pro-home" / "oby-worker" / "bin" / "oby-remote-gate"
        for copy in (self.runner, self.pro_runner):
            copy.parent.mkdir(parents=True)
            shutil.copy2(FACTORY / "remote" / "oby-remote-gate", copy)
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
        # This Mac's IP, read from a file: no test reaches the network. By default it is the rule's source.
        self.env.update(FACTORY_REMOTE_IP_URL=(self.fake / "ip").as_uri(), FAKE_STATE=str(self.state))
        self.rule_source(f"{OLD_IP}/32")
        self.ip(OLD_IP)

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

    def rule_source(self, source):
        (self.fake / "rule-source").write_text(source + "\n")

    def ip(self, answer):
        (self.fake / "ip").write_text(answer + "\n")

    def run_bin(self, name, *args, remote=True, **env):
        # Off is FACTORY_REMOTE_GATE=null, so the test doesn't depend on config.json's own remote_gate; the Mac
        # worker and the shards are off (FACTORY_MAC_WORKER=null, FACTORY_GATE_SHARDS=null) unless a test gives them,
        # or None to leave them unset.
        env = {**self.env, "FACTORY_REMOTE_GATE": REMOTE if remote else "null", "FACTORY_MAC_WORKER": "null",
               "FACTORY_GATE_SHARDS": "null", **env}
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

    def updates(self):
        return [call for call in self.az_calls() if call[:4] == ["network", "nsg", "rule", "update"]]

    def fallback(self):
        return self.checks().split("\nFell back to the Mac: ", 1)[1].split("\n", 1)[0]

    def forget(self):
        """A fresh start for the fakes' logs, between gates in one test."""
        for name in ("az.log", "ssh.log", "mac-runs", "events"):
            (self.fake / name).unlink(missing_ok=True)

    def assert_fell_back(self, code, out, err):
        self.assertEqual(code, 0, out + err)
        self.assertEqual((self.header()["GATE"], self.header()["HOST"]), ("PASS", "mac"))
        self.assertEqual(self.lines("mac-runs"), ["all"])
        self.assertEqual(self.remote_calls(), [])

    def pro_calls(self):
        return [call for call in self.ssh_calls() if "oby-mac-pro" in call["argv"]]

    def pro_gate_calls(self):
        """The Mac worker's calls but its `true` probe and its runner check."""
        return [call for call in self.pro_calls() if call["argv"][-1] not in ("true", CHECK)]

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
        # This Mac's IP is already the rule's source (REMOTE-20): nothing moves, and the reason says so.
        why = (f"ssh oby-gate didn't answer within 1s, and this Mac's IP {OLD_IP} is already "
               "oby-gateNSG/ssh-from-owner's source")
        self.assertIn(f"- Fell back to the Mac: {why}\n", self.checks())
        self.assertIn(f"\nFell back to the Mac: {why}\n", self.checks())
        self.assertEqual(self.updates(), [])
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
        [call] = self.pro_gate_calls()
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
        self.assertEqual(len(self.pro_gate_calls()), 1)
        self.assertNotIn("Fell back", self.checks())

    def test_remote_13_swift_and_factory_run_on_the_mac_worker(self):
        # From config.json's own mac_worker, with FACTORY_MAC_WORKER unset.
        self.config(mac_worker={"ssh_host": "oby-mac-pro"})
        self.groups("rust", "scan", "swift", "factory")
        commit = self.change("core/crates/obid/src/sandbox.rs", push=True)
        code, out, err = self.gate(FACTORY_MAC_WORKER=None)
        self.assertEqual(code, 0, out + err)
        [call] = self.pro_gate_calls()
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
        self.assertEqual(self.pro_gate_calls(), [])
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

    # ---- REMOTE-43 ------------------------------------------------------------------------------

    def changed_runner(self):
        """The fallback's reason when the worker's runner and this checkout's differ, from their sha256."""
        there, here = (hashlib.sha256(path.read_bytes()).hexdigest()[:12] for path in (self.pro_runner, self.runner))
        self.assertNotEqual(there, here)
        return (f"its oby-worker/bin/oby-remote-gate isn't this checkout's factory/remote/oby-remote-gate (sha256 "
                f"{there} there, {here} here): run factory/remote/setup-mac.sh oby-mac-pro to put it back")

    def test_remote_43_a_rewritten_runner_is_refused_and_the_gate_falls_back(self):
        with open(self.pro_runner, "a") as runner:
            runner.write('print("oby-remote-gate: passed in slot 1 after 1s")\n')
        self.assert_the_macos_part_fell_back(self.changed_runner())
        self.assertEqual([call["argv"][-1] for call in self.pro_calls()], ["true", CHECK] * 2)
        self.assertEqual(self.pro_gate_calls(), [])

    def test_remote_43_an_untouched_runner_runs_the_macos_part_there(self):
        commit = self.change("core/crates/obid/src/sandbox.rs", push=True)
        code, out, err = self.gate(FACTORY_MAC_WORKER=PRO)
        self.assertEqual(code, 0, out + err)
        self.assertEqual([call["argv"][-1] for call in self.pro_calls()],
                         ["true", CHECK, f"oby-worker/bin/oby-remote-gate --mac {commit} origin/dev rust"])
        check = self.pro_calls()[1]
        for option in ("BatchMode=yes", "ForwardAgent=no", "ConnectTimeout=10"):
            self.assertIn(option, check["argv"])
        self.assertFalse(check["stdin_pipe"], "the runner check's standard input isn't /dev/null")
        self.assertEqual(self.header()["HOST"], "oby-gate, oby-mac-pro")
        self.assertEqual(self.lines("mac-runs"), [])
        self.assertNotIn("Fell back", self.checks())

    def test_remote_43_a_runner_dev_moved_on_from_falls_back_until_setup_runs_again(self):
        commit = self.change("core/crates/obid/src/sandbox.rs", push=True)
        with open(self.runner, "a") as runner:
            runner.write("# dev moved on\n")
        code, out, err = self.gate(FACTORY_MAC_WORKER=PRO)
        self.assertEqual(code, 0, out + err)
        self.assertEqual(self.header()["HOST"], "oby-gate, mac")
        self.assertIn(f"\n- Fell back from oby-mac-pro to this Mac: {self.changed_runner()}\n", self.checks())
        self.assertEqual(self.pro_gate_calls(), [])
        self.assertEqual(self.lines("mac-runs"), ["rust"])
        # setup-mac.sh puts this checkout's runner there: the part runs there again.
        shutil.copy2(self.runner, self.pro_runner)
        self.forget()
        code, out, err = self.gate(FACTORY_MAC_WORKER=PRO)
        self.assertEqual(code, 0, out + err)
        self.assertEqual([call["argv"][-1] for call in self.pro_gate_calls()],
                         [f"oby-worker/bin/oby-remote-gate --mac {commit} origin/dev rust"])
        self.assertEqual(self.header()["HOST"], "oby-gate, oby-mac-pro")
        self.assertEqual(self.lines("mac-runs"), [])

    # ---- REMOTE-44 ------------------------------------------------------------------------------

    def test_remote_44_an_unreadable_runner_falls_back_to_this_mac(self):
        self.pro_runner.unlink()
        self.assert_the_macos_part_fell_back("couldn't read the sha256 of its oby-worker/bin/oby-remote-gate (ssh "
                                             "ended 1): run factory/remote/setup-mac.sh oby-mac-pro to put it back")
        self.assertEqual(self.pro_gate_calls(), [])

    def test_remote_44_no_runner_in_this_checkout_falls_back_to_this_mac(self):
        self.runner.unlink()
        self.assert_the_macos_part_fell_back("this checkout has no factory/remote/oby-remote-gate to check its runner "
                                             "against")
        self.assertEqual(self.pro_gate_calls(), [])

    # ---- REMOTE-5 --------------------------------------------------------------------------------

    def write_state(self, minutes):
        self.state.parent.mkdir(parents=True, exist_ok=True)
        self.state.write_text(json.dumps({"used": ago(minutes)}))

    def deallocations(self):
        return [call for call in self.az_calls() if call[1] == "deallocate"]

    def test_remote_5_deallocates_only_after_30_idle_minutes(self):
        # An old state.json, with `used` only: no use open. A job's stage counts for nothing.
        self.write_job("merging")
        self.write_state(29)
        self.assertEqual(self.run_bin("gate", "--idle"), (0, "", ""))
        self.assertEqual(self.az_calls(), [])
        self.write_state(31)
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

    def test_remote_5_a_mac_only_gate_doesnt_keep_the_vm_up(self):
        # A job gating on the Mac (stage `gate`) with no use of the VM open.
        self.write_job("gate")
        self.write_state(31)
        code, out, err = self.run_bin("gate", "--idle")
        self.assertEqual(code, 0, err)
        self.assertTrue(out.startswith("gate --idle: deallocating oby-gate"), out)
        self.assertEqual(self.deallocations(),
                         [["vm", "deallocate", "--resource-group", "oby-factory", "--name", "oby-gate", "--no-wait"]])

    # ---- REMOTE-19 -------------------------------------------------------------------------------

    def test_remote_19_a_changed_ip_moves_the_rule_and_the_gate_uses_the_vm(self):
        self.ip(NEW_IP)  # the rule still allows OLD_IP/32, so SSH fails until it moves
        code, out, err = self.gate(FACTORY_REMOTE_REACH_SECONDS="1")
        self.assertEqual(code, 0, out + err)
        self.assertEqual((self.header()["GATE"], self.header()["HOST"]), ("PASS", "oby-gate"))
        self.assertEqual(self.updates(), [["network", "nsg", "rule", "update", "--resource-group", "oby-factory",
                                           "--nsg-name", "oby-gateNSG", "--name", "ssh-from-owner",
                                           "--source-address-prefixes", f"{NEW_IP}/32"]])
        self.assertIn(["network", "nsg", "rule", "show", "--resource-group", "oby-factory", "--nsg-name",
                       "oby-gateNSG", "--name", "ssh-from-owner", "--output", "json"], self.az_calls())
        self.assertEqual([call[:4] for call in self.az_calls() if call[0] == "network"],
                         [["network", "nsg", "rule", "show"], ["network", "nsg", "rule", "update"]])
        where = self.checks().split("## Where it ran\n", 1)[1].split("\n\n", 1)[0]
        self.assertIn(f"- Moved oby-gateNSG/ssh-from-owner's source from {OLD_IP}/32 to {NEW_IP}/32: this Mac's IP "
                      "changed; the VM answered", where)
        self.assertNotIn("Fell back", self.checks())
        self.assertEqual(self.lines("mac-runs"), [], "the Mac ran a part with nothing for it")
        self.assertEqual(len(self.remote_calls()), 1)

    def test_remote_19_a_moved_rule_that_still_cant_reach_falls_back(self):
        self.ip(NEW_IP)
        (self.fake / "ssh-reach").write_text("255\n")
        self.assert_fell_back(*self.gate(FACTORY_REMOTE_REACH_SECONDS="1"))
        self.assertEqual(len(self.updates()), 1)
        self.assertIn(f"- Moved oby-gateNSG/ssh-from-owner's source from {OLD_IP}/32 to {NEW_IP}/32: this Mac's IP "
                      "changed; ssh still didn't answer within 1s", self.checks())
        self.assertEqual(self.fallback(), "ssh oby-gate didn't answer within 1s, nor within 1s after moving "
                                          f"oby-gateNSG/ssh-from-owner's source to {NEW_IP}/32")

    def test_remote_19_nsg_and_rule_names_come_from_the_settings(self):
        self.ip(NEW_IP)
        names = {**json.loads(REMOTE), "nsg": "other-nsg", "nsg_rule": "ssh-x"}
        code, out, err = self.gate(FACTORY_REMOTE_GATE=json.dumps(names), FACTORY_REMOTE_REACH_SECONDS="1")
        self.assertEqual(code, 0, out + err)
        self.assertEqual(self.header()["HOST"], "oby-gate")
        self.assertEqual(self.updates(), [["network", "nsg", "rule", "update", "--resource-group", "oby-factory",
                                           "--nsg-name", "other-nsg", "--name", "ssh-x",
                                           "--source-address-prefixes", f"{NEW_IP}/32"]])
        self.assertIn("other-nsg/ssh-x's source", self.checks())
        # An invalid name makes the settings invalid: the whole check runs on the Mac, with no az or ssh.
        self.forget()
        self.rule_source(f"{OLD_IP}/32")
        self.assert_fell_back(*self.gate(FACTORY_REMOTE_GATE=json.dumps({**names, "nsg": "bad name"})))
        self.assertEqual((self.az_calls(), self.ssh_calls()), ([], []))
        self.assertTrue(self.fallback().startswith("FACTORY_REMOTE_GATE needs ssh_host, resource_group, vm"),
                        self.fallback())

    # ---- REMOTE-20 -------------------------------------------------------------------------------

    def test_remote_20_an_unchanged_ip_falls_back_without_an_update(self):
        (self.fake / "ssh-reach").write_text("255\n")
        for source in (f"{OLD_IP}/32", OLD_IP):
            self.forget()
            self.rule_source(source)
            self.assert_fell_back(*self.gate(FACTORY_REMOTE_REACH_SECONDS="1"))
            self.assertEqual(self.updates(), [], source)
            self.assertTrue(self.fallback().startswith("ssh oby-gate didn't answer within 1s"), self.fallback())
            self.assertIn(f"this Mac's IP {OLD_IP} is already oby-gateNSG/ssh-from-owner's source", self.fallback())
            self.assertNotIn("Moved", self.checks())

    def test_remote_20_a_refused_or_malformed_ip_answer_updates_nothing(self):
        for answer in (None, "<html><body>1.1.1.1</body></html>", "::1", "10.0.0.5", "127.0.0.1", "0.0.0.0",
                       "224.0.0.1", "1.2.3.4\n5.6.7.8", "1.2.3.4/24"):
            self.forget()
            if answer is None:  # refused: nothing to read
                (self.fake / "ip").unlink(missing_ok=True)
            else:
                self.ip(answer)
            self.assert_fell_back(*self.gate(FACTORY_REMOTE_REACH_SECONDS="1"))
            self.assertEqual([call for call in self.az_calls() if call[0] == "network"], [], answer)
            self.assertTrue(self.fallback().startswith("ssh oby-gate didn't answer within 1s, and this Mac's public "
                                                       "IP couldn't be read"), self.fallback())
            self.assertEqual((self.fake / "rule-source").read_text(), f"{OLD_IP}/32\n")

    def test_remote_20_az_failing_on_the_rule_falls_back(self):
        self.ip(NEW_IP)
        for action in ("show", "update"):
            self.forget()
            exit_file = self.fake / f"az-rule-{action}-exit"
            exit_file.write_text("1\n")
            self.assert_fell_back(*self.gate(FACTORY_REMOTE_REACH_SECONDS="1"))
            verb = "read" if action == "show" else "update"
            self.assertIn(f"az couldn't {verb} oby-gateNSG/ssh-from-owner", self.fallback())
            self.assertEqual([call[3] for call in self.az_calls() if call[0] == "network"],
                             ["show"] if action == "show" else ["show", "update"])
            self.assertEqual((self.fake / "rule-source").read_text(), f"{OLD_IP}/32\n")
            self.assertNotIn("Moved", self.checks())
            exit_file.unlink()

    # ---- REMOTE-21 -------------------------------------------------------------------------------

    def test_remote_21_a_gate_records_its_vm_use(self):
        before = ago(0)
        code, out, err = self.gate()
        self.assertEqual(code, 0, out + err)
        during = json.loads((self.fake / "state-during-run").read_text())
        [use] = during["open"].values()
        self.assertIsInstance(use["pid"], int)
        self.assertGreaterEqual(use["started"], before)
        after = json.loads(self.state.read_text())
        self.assertEqual(after["open"], {})
        self.assertGreaterEqual(after["used"], before)

    def write_uses(self, used_minutes, **uses):
        self.state.parent.mkdir(parents=True, exist_ok=True)
        self.state.write_text(json.dumps({"used": ago(used_minutes), "open": {
            use: {"pid": pid, "started": ago(minutes)} for use, (pid, minutes) in uses.items()}}))

    def a_process(self, ended):
        proc = subprocess.Popen(["sleep", "60"] if not ended else ["true"])
        if ended:
            proc.wait()
        else:
            self.addCleanup(proc.wait)
            self.addCleanup(proc.kill)
        return proc.pid

    def test_remote_21_an_open_vm_use_keeps_the_vm_up(self):
        self.write_job("review")
        self.write_uses(31, live=(self.a_process(ended=False), 31))
        self.assertEqual(self.run_bin("gate", "--idle"), (0, "", ""))
        self.assertEqual(self.az_calls(), [])
        self.assertIn("live", json.loads(self.state.read_text())["open"])

    def test_remote_21_a_dead_gates_open_use_stops_counting(self):
        self.write_job("review")
        dead = self.a_process(ended=True)
        # It started 31 minutes ago: it counts as ended then, so the VM is deallocated once.
        self.write_uses(31, dead=(dead, 31))
        code, out, err = self.run_bin("gate", "--idle")
        self.assertEqual(code, 0, err)
        self.assertTrue(out.startswith("gate --idle: deallocating oby-gate"), out)
        self.assertEqual(len(self.deallocations()), 1)
        self.assertEqual(json.loads(self.state.read_text())["open"], {})
        # It started 5 minutes ago: it counts as ended then, which isn't 30 minutes yet.
        self.write_uses(31, dead=(dead, 5))
        started = json.loads(self.state.read_text())["open"]["dead"]["started"]
        self.assertEqual(self.run_bin("gate", "--idle"), (0, "", ""))
        self.assertEqual(len(self.deallocations()), 1)
        state = json.loads(self.state.read_text())
        self.assertEqual((state["open"], state["used"]), ({}, started))

    # ---- REMOTE-28 -------------------------------------------------------------------------------

    def test_remote_28_slot_counts_reach_the_vm_only_when_set(self):
        commit = self.git(self.worktree, "rev-parse", "HEAD")
        code, out, err = self.gate()
        self.assertEqual(code, 0, out + err)
        [call] = self.remote_calls()
        self.assertEqual(call["argv"][-2:], ["oby-gate", f"oby-remote-gate {commit} origin/dev"])
        for keys, words in (({"slots": 6}, "--slots 6 --builder-slots 0"),
                            ({"builder_slots": 8}, "--slots 4 --builder-slots 8"),
                            ({"slots": 4, "builder_slots": 8, "builder_user": "oby-builder"},
                             "--slots 4 --builder-slots 8")):
            self.forget()
            code, out, err = self.gate(FACTORY_REMOTE_GATE=json.dumps({**json.loads(REMOTE), **keys}))
            self.assertEqual(code, 0, out + err)
            [call] = self.remote_calls()
            self.assertEqual(call["argv"][-1], f"oby-remote-gate {words} {commit} origin/dev")
        # An invalid count or user makes the settings invalid: the whole check runs on the Mac, with no az or ssh.
        for keys in ({"slots": 0}, {"slots": 65}, {"slots": "4"}, {"slots": True}, {"builder_slots": -1},
                     {"builder_slots": 65}, {"builder_slots": 1.5}, {"builder_user": "bad name"},
                     {"builder_user": 7}):
            self.forget()
            self.assert_fell_back(*self.gate(FACTORY_REMOTE_GATE=json.dumps({**json.loads(REMOTE), **keys})))
            self.assertEqual((self.az_calls(), self.ssh_calls()), ([], []), keys)
            self.assertTrue(self.fallback().startswith("FACTORY_REMOTE_GATE's slots must be a whole number from 1 "
                                                       "to 64"), self.fallback())

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

    # ---- REMOTE-62 to 65: shards ---------------------------------------------------------------

    SANDBOX_ONLY = "(the diff touches macOS-only code: core/crates/obid/src/sandbox.rs)"

    def sharded(self, **env):
        """A gate whose diff touches macOS-only code, with the Mac worker and three Linux and two macOS shards."""
        commit = self.change("core/crates/obid/src/sandbox.rs", push=True)
        return commit, self.gate(**{"FACTORY_MAC_WORKER": PRO, "FACTORY_GATE_SHARDS": SHARDS, **env})

    def where(self):
        return self.checks().split("## Where it ran\n", 1)[1].split("\n\n", 1)[0].splitlines()

    def section(self, heading):
        """The log's section under `==== <heading> ====`, up to the next heading."""
        log = (self.job_dir / "round-1" / "checks.log").read_text()
        self.assertIn(f"==== {heading} ====\n", log)
        return log.split(f"==== {heading} ====\n", 1)[1].split("\n==== ", 1)[0]

    def events(self):
        return self.lines("events")

    def test_remote_62_the_shards_spread_over_the_three_hosts_and_start_at_once(self):
        # Each VM shard waits for the Mac worker's call to start, and fails if it doesn't within 30 seconds.
        for k in (1, 2, 3):
            (self.fake / f"vm-waits-{k}-3").write_text("30\n")
        commit, (code, out, err) = self.sharded()
        self.assertEqual(code, 0, out + err + self.checks())
        vm = [call["argv"][-1] for call in self.remote_calls() if call["argv"][-2] == "oby-gate"]
        self.assertEqual(sorted(vm), [f"oby-remote-gate --shard {k}/3 {commit} origin/dev" for k in (1, 2, 3)])
        self.assertTrue(all(call["stdin_pipe"] for call in self.remote_calls() if call["argv"][-2] == "oby-gate"))
        self.assertEqual([call["argv"][-1] for call in self.pro_gate_calls()],
                         [f"oby-worker/bin/oby-remote-gate --mac --shard 1/2 {commit} origin/dev rust"])
        self.assertEqual(self.lines("mac-runs"), ["rust 2/2"])
        # The Mac worker's call started before any VM shard ended.
        events = self.events()
        self.assertLess(events.index("start pro 1/2"), min(events.index(f"end remote {k}/3") for k in (1, 2, 3)), events)
        h = self.header()
        self.assertEqual((h["GATE"], h["HOST"]), ("PASS", "oby-gate, oby-mac-pro, mac"))
        where = self.where()
        for i, line in enumerate((
                "oby-gate: rust shard 1/3, scan in slot 1: passed with exit 0 in",
                "oby-gate: rust shard 2/3 in slot 2: passed with exit 0 in",
                "oby-gate: rust shard 3/3 in slot 3: passed with exit 0 in",
                f"oby-mac-pro: rust shard 1/2 {self.SANDBOX_ONLY} in slot 1: passed with exit 0 in",
                f"mac: rust shard 2/2 {self.SANDBOX_ONLY}: passed with exit 0 in")):
            self.assertTrue(where[i].startswith(f"- {line}"), where)
        self.assertEqual(len(where), 5, where)
        self.assertIn("\n- Sharded: obid's test binaries in 3 shards on Linux and 2 on macOS\n", self.checks())
        self.assertRegex(self.checks(), r"\n## Time\n- Wall clock: (\d+)s\n- oby-gate: \d+s over 3 calls\n"
                                        r"- oby-mac-pro: \d+s\n- mac: \d+s\n\n## Protected paths\n")
        wall = re.search(r"- Wall clock: (\d+)s", self.checks()).group(1)
        self.assertIn(f"\n## checks.sh\nExit 0 in {wall}s.", self.checks())
        self.assertTrue(out.startswith(f"GATE: PASS ({JOB_ID} round 1, on oby-gate, oby-mac-pro, mac, exit 0 in {wall}s"),
                        out)
        # The log keeps the expected order, each part under its own heading.
        log = (self.job_dir / "round-1" / "checks.log").read_text()
        headings = re.findall(r"^==== (.*) ====$", log, re.M)
        self.assertEqual(headings, ["oby-gate: rust shard 1/3, scan", "oby-gate: rust shard 2/3",
                                    "oby-gate: rust shard 3/3", f"oby-mac-pro: rust shard 1/2 {self.SANDBOX_ONLY}",
                                    f"mac: rust shard 2/2 {self.SANDBOX_ONLY}"])
        self.assertIn("oby-remote-gate: shard 2/3", self.section("oby-gate: rust shard 2/3"))
        # One use of the VM, open during the calls and closed after the last.
        [use] = json.loads((self.fake / "state-during-run").read_text())["open"].values()
        self.assertEqual(json.loads(self.state.read_text())["open"], {})

    def test_remote_62_a_vm_shard_that_waits_for_the_mac_worker_in_vain_fails(self):
        # The wait above can fail: with no Mac worker call at all, the waiting shard gives up and fails the gate.
        (self.fake / "vm-waits-2-3").write_text("2\n")
        _, (code, out, err) = self.sharded(FACTORY_MAC_WORKER="null")
        self.assertEqual(code, 1, out + err)
        self.assertIn("- oby-gate: rust shard 2/3: failed with exit 1 in", self.checks())
        self.assertIn("waited for the Mac worker's call, which never came", self.section("oby-gate: rust shard 2/3"))

    def test_remote_62_without_a_mac_worker_every_macos_shard_runs_here(self):
        self.groups("rust", "scan", "swift", "factory")
        commit, (code, out, err) = self.sharded(FACTORY_MAC_WORKER="null")
        self.assertEqual(code, 0, out + err)
        self.assertEqual(self.pro_calls(), [])
        self.assertEqual(self.lines("mac-runs"), ["rust 1/2", "swift", "factory", "rust 2/2"])
        self.assertEqual(self.header()["HOST"], "oby-gate, mac")
        self.assertEqual([line.removeprefix("- mac: ").rsplit(": passed", 1)[0] for line in self.where()[3:]],
                         [f"rust shard 1/2 {self.SANDBOX_ONLY}", "swift", "factory", f"rust shard 2/2 {self.SANDBOX_ONLY}"])
        self.assertRegex(self.checks(), r"\n## Time\n- Wall clock: \d+s\n- oby-gate: \d+s over 3 calls\n- mac: \d+s\n")

    def test_remote_62_swift_and_factory_go_with_the_mac_workers_first_shard(self):
        # From config.json's own gate_shards, with FACTORY_GATE_SHARDS unset.
        self.config(gate_shards={"linux": 2, "mac": 3})
        self.groups("rust", "electron", "scan", "swift", "factory")
        commit, (code, out, err) = self.sharded(FACTORY_GATE_SHARDS=None)
        self.assertEqual(code, 0, out + err)
        self.assertEqual(sorted(call["argv"][-1] for call in self.pro_gate_calls()),
                         [f"oby-worker/bin/oby-remote-gate --mac --shard 1/3 {commit} origin/dev rust swift factory",
                          f"oby-worker/bin/oby-remote-gate --mac --shard 2/3 {commit} origin/dev rust"])
        self.assertEqual(self.lines("mac-runs"), ["rust 3/3"])
        self.assertIn("- oby-gate: rust shard 1/2, electron, scan in slot 1: passed", self.checks())
        self.assertIn(f"- oby-mac-pro: rust shard 1/3 {self.SANDBOX_ONLY}, swift, factory in slot 1: passed",
                      self.checks())
        self.assertIn("\n- Sharded: obid's test binaries in 2 shards on Linux and 3 on macOS\n", self.checks())

    def test_remote_62_one_macos_shard_or_no_macos_rust_is_todays_single_macos_part(self):
        self.groups("rust", "scan", "factory")
        commit, (code, out, err) = self.sharded(FACTORY_GATE_SHARDS=json.dumps({"linux": 2, "mac": 1}))
        self.assertEqual(code, 0, out + err)
        self.assertEqual([call["argv"][-1] for call in self.pro_gate_calls()],
                         [f"oby-worker/bin/oby-remote-gate --mac {commit} origin/dev rust factory"])
        self.forget()
        self.git(self.worktree, "reset", "-q", "--hard", "HEAD~1")
        self.git(self.worktree, "push", "-q", "--force", "origin", "factory/x")
        code, out, err = self.gate(FACTORY_MAC_WORKER="null", FACTORY_GATE_SHARDS=SHARDS)
        self.assertEqual(code, 0, out + err)
        self.assertEqual(self.lines("mac-runs"), ["factory"])
        self.assertIn("\n- mac: factory: passed with exit 0 in", self.checks())

    def test_remote_62_no_rust_planned_on_linux_is_one_vm_call(self):
        self.groups("electron", "scan")
        commit, (code, out, err) = self.sharded()
        self.assertEqual(code, 0, out + err)
        self.assertEqual([call["argv"][-1] for call in self.remote_calls()],
                         [f"oby-remote-gate {commit} origin/dev"])
        self.assertIn("- oby-gate: electron, scan in slot 2: passed", self.checks())
        self.assertRegex(self.checks(), r"- oby-gate: \d+s over 1 call\n")

    def test_remote_63_one_shard_failing_fails_the_gate_and_names_the_test(self):
        commit = self.change("core/crates/obid/src/sandbox.rs", push=True)
        cases = [(f"remote-exit-{k}-3", f"remote-says-{k}-3", f"oby-gate: rust shard {k}/3" + (", scan" if k == 1 else ""),
                  f" in slot {k}: failed with exit 1") for k in (1, 2, 3)]
        cases += [("pro-exit-1-2", "pro-says-1-2", f"oby-mac-pro: rust shard 1/2 {self.SANDBOX_ONLY}",
                   " in slot 1: failed with exit 1"),
                  ("mac-exit-rust-2-2", "mac-says-rust-2-2", f"mac: rust shard 2/2 {self.SANDBOX_ONLY}",
                   ": failed with exit 1")]
        for exit_file, says, heading, line in cases:
            self.forget()
            test = f"tests/{exit_file.replace('-', '_')}.rs"
            (self.fake / exit_file).write_text("1\n")
            (self.fake / says).write_text(f"oby-check: {test} failed\n")
            code, out, err = self.gate(FACTORY_MAC_WORKER=PRO, FACTORY_GATE_SHARDS=SHARDS)
            self.assertEqual(code, 1, heading + out + err)
            self.assertEqual(self.header()["GATE"], "FAIL", heading)
            self.assertIn(f"\n- {heading}{line} in", self.checks())
            self.assertIn(f"oby-check: {test} failed", self.section(heading))
            # Every other part still ran to its end and passed.
            self.assertEqual(sum(": passed with exit 0 in" in where for where in self.where()), 4, self.where())
            (self.fake / exit_file).unlink()
            (self.fake / says).unlink()
        self.forget()
        code, out, err = self.gate(FACTORY_MAC_WORKER=PRO, FACTORY_GATE_SHARDS=SHARDS)
        self.assertEqual((code, self.header()["GATE"]), (0, "PASS"), out + err)
        self.assertEqual(len(self.remote_calls()), 3 + 2, self.remote_calls())  # three VM calls, the check and the call
        self.assertIn(commit, self.header()["COMMIT"])

    def test_remote_64_a_vm_shard_that_ends_without_its_verdict_line_didnt_report(self):
        for code_text in ("0", "1"):
            self.forget()
            (self.fake / "remote-silent-2-3").touch()
            (self.fake / "remote-exit-2-3").write_text(code_text + "\n")
            _, (code, out, err) = self.sharded()
            self.assertEqual(code, 1, out + err)
            self.assertEqual(self.header()["GATE"], "FAIL")
            self.assertIn(f"\n- oby-gate: rust shard 2/3: didn't report: it ended {code_text} without its verdict "
                          "line\n", self.checks())
            self.assertNotIn("rust 2/3", self.lines("mac-runs"), "a shard that didn't report ran again here")

    def test_remote_64_a_vm_shard_that_couldnt_judge_runs_on_this_mac(self):
        (self.fake / "remote-exit-3-3").write_text("2\n")
        _, (code, out, err) = self.sharded()
        self.assertEqual(code, 0, out + err)
        self.assertEqual(sorted(self.lines("mac-runs")), ["rust 2/2", "rust 3/3"])
        self.assertIn("\n- Fell back from oby-gate to this Mac for rust shard 3/3: oby-remote-gate couldn't judge "
                      "(exit 2)\n", self.checks())
        self.assertIn("\n- mac: rust shard 3/3: passed with exit 0 in", self.checks())
        self.assertEqual(self.header()["HOST"], "oby-gate, oby-mac-pro, mac")
        log = (self.job_dir / "round-1" / "checks.log").read_text()
        self.assertIn("==== oby-gate: its output before the fallback, not part of the verdict ====\n"
                      "oby-remote-gate: slot 3\noby-remote-gate: shard 3/3", log)
        self.assertRegex(self.checks(), r"- oby-gate: \d+s over 2 calls\n")
        # Its run here is the verdict.
        self.forget()
        (self.fake / "mac-exit-rust-3-3").write_text("1\n")
        code, out, err = self.gate(FACTORY_MAC_WORKER=PRO, FACTORY_GATE_SHARDS=SHARDS)
        self.assertEqual(code, 1, out + err)
        self.assertIn("\n- mac: rust shard 3/3: failed with exit 1 in", self.checks())

    def test_remote_64_a_vm_shard_that_was_cut_off_runs_on_this_mac(self):
        (self.fake / "remote-exit-1-3").write_text("255\n")
        _, (code, out, err) = self.sharded()
        self.assertEqual(code, 0, out + err)
        self.assertEqual(self.lines("mac-runs"), ["rust 1/3", "scan", "rust 2/2"] if self.lines("mac-runs")[0] == "rust 1/3"
                         else ["rust 2/2", "rust 1/3", "scan"])
        self.assertIn("Fell back from oby-gate to this Mac for rust shard 1/3: oby-remote-gate was cut off (exit 255)",
                      self.checks())
        self.assertIn("\n- mac: rust shard 1/3, scan: passed with exit 0 in", self.checks())

    def test_remote_64_a_busy_mac_worker_sends_its_shard_here(self):
        (self.fake / "pro-busy").touch()
        _, (code, out, err) = self.sharded()
        self.assertEqual(code, 0, out + err)
        self.assertEqual(sorted(self.lines("mac-runs")), ["rust 1/2", "rust 2/2"])
        line = "Fell back from oby-mac-pro to this Mac for rust shard 1/2: both of its slots are busy"
        self.assertIn(f"\n- {line}\n", self.checks())
        self.assertIn(f"\n\n{line}\n", self.checks())
        self.assertIn(f"\n- mac: rust shard 1/2 {self.SANDBOX_ONLY}: passed with exit 0 in", self.checks())
        self.assertEqual(self.header()["HOST"], "oby-gate, mac")

    def test_remote_64_invalid_mac_worker_settings_send_its_shard_here(self):
        _, (code, out, err) = self.sharded(FACTORY_MAC_WORKER="{oby-mac-pro")
        self.assertEqual(code, 0, out + err)
        self.assertEqual(self.pro_calls(), [])
        self.assertEqual(sorted(self.lines("mac-runs")), ["rust 1/2", "rust 2/2"])
        self.assertIn("\n- Fell back from the Mac worker to this Mac for rust shard 1/2: FACTORY_MAC_WORKER isn't JSON\n",
                      self.checks())

    def test_remote_64_an_unreachable_vm_sends_every_vm_shard_here(self):
        (self.fake / "ssh-reach").write_text("255\n")
        _, (code, out, err) = self.sharded(FACTORY_REMOTE_REACH_SECONDS="1")
        self.assertEqual(code, 0, out + err)
        self.assertEqual([call for call in self.remote_calls() if call["argv"][-2] == "oby-gate"], [])
        runs = self.lines("mac-runs")
        self.assertEqual([run for run in runs if run != "rust 2/2"], ["rust 1/3", "scan", "rust 2/3", "rust 3/3"])
        why = (f"ssh oby-gate didn't answer within 1s, and this Mac's IP {OLD_IP} is already "
               "oby-gateNSG/ssh-from-owner's source")
        self.assertEqual(self.checks().count(f"- Fell back from oby-gate to this Mac: {why}\n"), 1)
        self.assertNotIn("for rust shard", self.checks())
        self.assertEqual(self.header()["HOST"], "oby-mac-pro, mac")
        self.assertEqual(json.loads(self.state.read_text())["open"], {})

    def test_remote_64_a_part_whose_run_raised_is_a_fail(self):
        # A run here whose output isn't UTF-8 raises in its part's thread: that part didn't report.
        (self.fake / "mac-garbles-rust-2-3").touch()
        (self.fake / "mac-garbles-rust-2-2").touch()
        (self.fake / "remote-exit-2-3").write_text("2\n")
        _, (code, out, err) = self.sharded()
        self.assertEqual(code, 1, out + err)
        self.assertRegex(self.checks(), r"\n- mac: rust shard 2/3: didn't report: its run raised \w+")
        self.assertRegex(self.checks(), r"\n- mac: rust shard 2/2 \(.*\): didn't report: its run raised \w+")

    def test_remote_65_without_the_key_nothing_changes(self):
        self.config(gate_shards=None)
        for value in (None, "null"):
            self.forget()
            commit, (code, out, err) = self.sharded(FACTORY_GATE_SHARDS=value)
            self.assertEqual(code, 0, out + err)
            self.assertFalse([call for call in self.ssh_calls() if "--shard" in call["argv"][-1]], self.ssh_calls())
            self.assertEqual([call["argv"][-1] for call in self.remote_calls() if call["argv"][-2] == "oby-gate"],
                             [f"oby-remote-gate {commit} origin/dev"])
            # The Mac worker's call starts only after the VM's ended.
            events = self.events()
            self.assertLess(events.index("end remote -"), events.index("start pro -"), events)
            for absent in ("## Time", "Sharded:", "Ran unsharded:"):
                self.assertNotIn(absent, self.checks())
            self.assertEqual(self.header()["HOST"], "oby-gate, oby-mac-pro")
        # The key with remote_gate off: one checks.sh run, as before.
        self.forget()
        code, out, err = self.gate(remote=False, FACTORY_GATE_SHARDS=SHARDS)
        self.assertEqual(code, 0, out + err)
        self.assertEqual((self.ssh_calls(), self.lines("mac-runs")), ([], ["all"]))
        self.assertEqual(list(self.header()), ["GATE", "COMMIT", "CHECKS"])
        self.assertNotIn("Sharded", self.checks())

    def test_remote_65_invalid_shards_run_unsharded_and_say_why(self):
        needs = "needs linux, a whole number from 1 to 64, and mac, a whole number from 1 to 3"
        for value, why in (("{linux", "FACTORY_GATE_SHARDS isn't JSON"),
                           (json.dumps({"linux": 0, "mac": 2}), f"FACTORY_GATE_SHARDS {needs}"),
                           (json.dumps({"linux": 2, "mac": 4}), f"FACTORY_GATE_SHARDS {needs}"),
                           (json.dumps({"linux": 65, "mac": 1}), f"FACTORY_GATE_SHARDS {needs}"),
                           (json.dumps({"linux": True, "mac": 1}), f"FACTORY_GATE_SHARDS {needs}"),
                           (json.dumps({"linux": 2.0, "mac": 1}), f"FACTORY_GATE_SHARDS {needs}"),
                           (json.dumps({"linux": 2}), f"FACTORY_GATE_SHARDS {needs}"),
                           (json.dumps([2, 2]), f"FACTORY_GATE_SHARDS {needs}")):
            self.forget()
            _, (code, out, err) = self.sharded(FACTORY_GATE_SHARDS=value)
            self.assertEqual(code, 0, out + err)
            self.assertFalse([call for call in self.ssh_calls() if "--shard" in call["argv"][-1]], value)
            self.assertIn(f"\n- Ran unsharded: {why}\n", self.checks())
            self.assertIn(f"\n\nRan unsharded: {why}\n", self.checks().split("## Protected paths", 1)[0])
            self.assertNotIn("## Time", self.checks())
            self.assertEqual(self.header()["HOST"], "oby-gate, oby-mac-pro")
        # config.json's own key, invalid, names it.
        self.forget()
        self.config(gate_shards={"linux": 2, "mac": "2"})
        _, (code, out, err) = self.sharded(FACTORY_GATE_SHARDS=None)
        self.assertIn(f"\n- Ran unsharded: gate_shards {needs}\n", self.checks())

    def test_remote_65_the_whole_check_cases_stay_the_same_with_the_key(self):
        # A commit not on origin, and a change to the check's own files: the whole check here, no Mac worker call.
        self.change("core/crates/obid/src/sandbox.rs")  # not pushed
        code, out, err = self.gate(FACTORY_MAC_WORKER=PRO, FACTORY_GATE_SHARDS=SHARDS)
        self.assertEqual(code, 0, out + err)
        self.assertEqual((self.ssh_calls(), self.lines("mac-runs")), ([], ["all"]))
        self.assertIn("Fell back to the Mac: not on origin", self.checks())
        self.assertNotIn("## Time", self.checks())
        self.forget()
        self.change("scripts/oby-check-test.sh", push=True)
        code, out, err = self.gate(FACTORY_MAC_WORKER=PRO, FACTORY_GATE_SHARDS=SHARDS)
        self.assertEqual(code, 0, out + err)
        self.assertEqual((self.az_calls(), self.ssh_calls(), self.lines("mac-runs")), ([], [], ["all"]))
        self.assertNotIn("Sharded", self.checks())


if __name__ == "__main__":
    unittest.main(verbosity=2)
