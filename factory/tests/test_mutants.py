#!/usr/bin/env python3
"""factory/bin/mutants (docs/contracts/factory.md, MUTANTS-1 to 9), with a stand-in test-run, never cargo,
cargo-mutants or a real host.

Run from the repository root: python3 -m unittest discover -s factory/tests -p 'test_mutants.py' -v
Each test builds a throwaway repository holding copies of factory/bin/mutants, factory/bin/prove, factory/bin/job and
factory/bin/child_session.py, a config.json the test writes, the crates core/crates/demo and core/crates/second, and
a job whose gate passed its branch's head. The stand-in test-run beside them records each call (its argv, cwd,
CARGO_TARGET_DIR, PID and the mutants.diff in its cwd), then prints the file STAND_IN_OUTPUT names and exits
STAND_IN_CODE; with STAND_IN=sleep it prints that file, flushed, then sleeps.
"""

import json
import os
import shutil
import signal
import subprocess
import tempfile
import time
import unittest
from pathlib import Path

FACTORY = Path(__file__).resolve().parents[1]
LIB = "core/crates/demo/src/lib.rs"
SECOND = "core/crates/second/src/lib.rs"
EXTRA = "core/crates/demo/src/extra.rs"

BASE_LIB = """pub fn big(a: u8) -> bool {
    a > 10
}
"""

SHAPE_LIB = """pub fn big(a: u8) -> bool {
    a > 10
}

pub fn small(a: u8) -> bool {
    a < 3
}

pub fn add(a: u8, b: u8) -> u8 {
    a.saturating_add(b)
}

pub fn spin(n: u32) -> u32 {
    let mut i = 0;
    while i < n {
        i += 1;
    }
    i
}
"""

EXCLUDED_LIB = """pub fn big(a: u8) -> bool {
    a > 10
}

#[cfg(kani)]
mod checks {
    fn helper(a: u8) -> bool {
        a < 3
    }
}

#[cfg(test)]
mod proofs {
    #![cfg(kani)]

    fn other(a: u8) -> bool {
        a == 4
    }
}
"""

HARNESS_FILE = """// Kani's harnesses, only.
#![cfg(kani)]

fn h(a: u8) -> bool {
    a != 5
}
"""

KANI_LIB = """pub fn big(a: u8) -> bool {
    a > 10
}

pub fn small(a: u8) -> bool {
    a < 3
}

#[cfg(test)]
mod proofs {
    #![cfg(kani)]

    #[kani::proof]
    fn big_is_total() {
        let a: u8 = kani::any();
        let _ = super::big(a);
    }
}
"""

STAND_IN = """#!/usr/bin/env python3
import json, os, sys, time
from pathlib import Path
diff = Path("mutants.diff")
with open(os.environ["STAND_IN_LOG"], "a") as f:
    f.write(json.dumps({"argv": sys.argv[1:], "cwd": os.getcwd(), "target": os.environ.get("CARGO_TARGET_DIR"),
                        "pid": os.getpid(), "diff": diff.read_text() if diff.exists() else None}) + "\\n")
out = os.environ.get("STAND_IN_OUTPUT")
if out:
    sys.stdout.write(Path(out).read_text())
    sys.stdout.flush()
if os.environ.get("STAND_IN") == "sleep":
    time.sleep(60)
sys.exit(int(os.environ.get("STAND_IN_CODE", "0")))
"""


def line_of(text, needle):
    return next(n for n, line in enumerate(text.splitlines(), 1) if needle in line)


def col_of(text, needle):
    line = text.splitlines()[line_of(text, needle) - 1]
    return len(line) - len(line.lstrip()) + 1


def mutant(outcome, path, text, needle, change):
    """A cargo-mutants outcome line for the line of `text` holding needle, its path relative to core/."""
    return f"{outcome:<8} {path.removeprefix('core/')}:{line_of(text, needle)}:{col_of(text, needle)}: {change}"


class Mutants(unittest.TestCase):
    def setUp(self):
        self.dir = Path(tempfile.mkdtemp()).resolve()
        self.repo = self.dir / "repo"
        self.calls = self.dir / "stand-in.log"
        self.output = self.dir / "stand-in.out"
        bin_dir = self.repo / "factory" / "bin"
        bin_dir.mkdir(parents=True)
        for name in ("mutants", "prove", "job", "child_session.py"):
            shutil.copy2(FACTORY / "bin" / name, bin_dir / name)
        (bin_dir / "test-run").write_text(STAND_IN)
        (bin_dir / "test-run").chmod(0o755)
        self.config()
        (self.dir / "claude").mkdir()
        self.env = {
            **os.environ,
            "GIT_CONFIG_GLOBAL": "/dev/null", "GIT_CONFIG_NOSYSTEM": "1",
            "GIT_AUTHOR_NAME": "test", "GIT_AUTHOR_EMAIL": "test@example.com",
            "GIT_COMMITTER_NAME": "test", "GIT_COMMITTER_EMAIL": "test@example.com",
            "CLAUDE_CONFIG_DIR": str(self.dir / "claude"),
            "STAND_IN_LOG": str(self.calls),
            "STAND_IN_OUTPUT": str(self.output),
        }
        for name in ("STAND_IN", "STAND_IN_CODE"):
            self.env.pop(name, None)
        self.stand_in("1 mutant tested: 1 caught\n")
        self.git("init", "-q", "-b", "dev")
        (self.repo / ".gitignore").write_text("/factory/jobs/\n/.worktrees/\ntarget/\n")
        base = {
            "core/crates/demo/Cargo.toml": '[package]\nname = "demo"\nversion = "0.1.0"\n',
            LIB: BASE_LIB,
            EXTRA: "pub fn extra() {}\n",
            "core/crates/second/Cargo.toml": '[package]\nname = "alpha"\nversion = "0.1.0"\n',
            SECOND: "pub fn one() -> u8 {\n    1\n}\n",
        }
        for path, text in base.items():
            (self.repo / path).parent.mkdir(parents=True, exist_ok=True)
            (self.repo / path).write_text(text)
        self.git("add", ".gitignore", *base)
        self.git("commit", "-q", "-m", "base")
        self.git("update-ref", "refs/remotes/origin/dev", "HEAD")

    def tearDown(self):
        shutil.rmtree(self.dir)

    def git(self, *args, cwd=None):
        return subprocess.run(["git", *args], cwd=cwd or self.repo, env=getattr(self, "env", None),
                              capture_output=True, text=True, check=True).stdout

    def config(self, mutants=None):
        config = {"concurrency": 4, "max_rounds": 3, "base_branch": "dev", "sprint": 1, "prices_usd_per_mtok": {}}
        if mutants is not None:
            config["mutants"] = mutants
        (self.repo / "factory" / "config.json").write_text(json.dumps(config))

    def stand_in(self, output):
        self.output.write_text(output)

    def make_job(self, job_id, files=None, deleted=()):
        """A job whose branch writes `files` (by default a change to demo's lib.rs), deletes `deleted`, and whose
        gate passed its head."""
        worktree = self.repo / ".worktrees" / job_id
        self.git("worktree", "add", "-q", "-b", f"factory/{job_id}", str(worktree), "origin/dev")
        files = files if files is not None else {LIB: SHAPE_LIB}
        for path, text in files.items():
            (worktree / path).parent.mkdir(parents=True, exist_ok=True)
            (worktree / path).write_text(text)
        if files:
            self.git("add", *files, cwd=worktree)
        if deleted:
            self.git("rm", "-q", *deleted, cwd=worktree)
        self.git("commit", "-q", "-m", "change", cwd=worktree)
        self.head = self.git("rev-parse", "HEAD", cwd=worktree).strip()
        d = self.repo / "factory" / "jobs" / job_id
        (d / "round-1").mkdir(parents=True)
        (d / "round-1" / "checks.md").write_text(f"GATE: PASS\nCOMMIT: {self.head}\nCHECKS: x\n\n## Summary\n")
        (d / "job.json").write_text(json.dumps({
            "id": job_id, "issue": "A00-99", "title": "feat: demo", "milestone": "A00", "sprint": 0, "fixes": [],
            "pr": None, "pr_url": None, "branch": f"factory/{job_id}", "worktree": str(worktree),
            "stage": "review", "round": 1, "round_limit": 3,
            "intake": {"decision": "ACCEPT", "rescoped": False, "risk": "low", "surfaces": []},
            "agents": {"reviewers": {"code": "sonnet"}}, "gate": "PASS", "reviews": {}, "cost_usd": 0,
            "started": "2026-01-01T00:00:00Z", "ended": None, "proofs_hash": "none",
        }, indent=2) + "\n")
        return d

    def mutants(self, job_id, code=None, **env):
        res = subprocess.run([str(self.repo / "factory" / "bin" / "mutants"), job_id], cwd=self.repo,
                             env={**self.env, **env}, capture_output=True, text=True, timeout=120)
        if code is not None:
            self.assertEqual(res.returncode, code, res.stdout + res.stderr)
        return res

    def mutants_md(self, d):
        return (d / "round-1" / "mutants.md").read_text()

    def stand_in_calls(self):
        return [json.loads(line) for line in self.calls.read_text().splitlines()] if self.calls.exists() else []

    def worktrees(self):
        return [line for line in self.git("worktree", "list", "--porcelain").splitlines()
                if line.startswith("worktree ")]

    def section(self, text, heading):
        return text.split(f"\n## {heading}\n", 1)[1].split("\n## ", 1)[0].strip().splitlines()

    # ---- MUTANTS-1 ------------------------------------------------------------------------------

    def test_mutants_1_a_branch_with_no_rust_change_under_core_crates_is_skipped(self):
        cases = {
            "md": ({"docs/notes.md": "# Notes\n"}, ()),
            "tools": ({"tools/thing/src/main.rs": "fn main() {}\n"}, ()),
            "deleted": ({}, (EXTRA,)),
        }
        for n, (name, (files, deleted)) in enumerate(cases.items(), 1):
            with self.subTest(name=name):
                d = self.make_job(f"00{n}-a00-99-{name}", files=files, deleted=deleted)
                before = self.worktrees()
                res = self.mutants(d.name, 0)
                lines = self.mutants_md(d).splitlines()
                self.assertEqual(lines[:2], ["MUTANTS: SKIPPED", f"COMMIT: {self.head}"])
                self.assertIn("WHY: no .rs file under core/crates/ changed", lines)
                self.assertIn("VM SECONDS: -", lines)
                self.assertIn("COMMAND: -", lines)
                self.assertNotIn("## Missed", self.mutants_md(d))
                self.assertIn(f"MUTANTS: SKIPPED ({d.name} round 1)", res.stdout)
                self.assertEqual(self.worktrees(), before)
        self.assertEqual(self.stand_in_calls(), [])

    # ---- MUTANTS-2 ------------------------------------------------------------------------------

    def test_mutants_2_one_call_on_the_touched_crates_with_a_diff_relative_to_core(self):
        d = self.make_job("001-a00-99-command", files={LIB: SHAPE_LIB, SECOND: "pub fn one() -> u8 {\n    2\n}\n",
                                                       "README.md": "# Read me\n", "core/Cargo.lock": "# lock\n"})
        worktree = Path(json.loads((d / "job.json").read_text())["worktree"])
        status = self.git("status", "--porcelain", cwd=worktree)
        before = self.worktrees()
        self.mutants(d.name, 0)
        [call] = self.stand_in_calls()
        self.assertEqual(call["argv"], ["--", "cargo", "mutants", "--in-place", "--in-diff", "mutants.diff", "-p",
                                        "alpha", "-p", "demo", "-j", "1", "--no-times", "--caught", "--unviable",
                                        "--colors", "never"])
        self.assertTrue(call["cwd"].endswith("/scratch/core"), call)
        self.assertFalse(call["cwd"].startswith(str(worktree)), call)
        self.assertEqual(call["target"], call["cwd"] + "/target")
        plus = [line for line in call["diff"].splitlines() if line.startswith("+++ ")]
        self.assertEqual(plus, ["+++ b/crates/demo/src/lib.rs", "+++ b/crates/second/src/lib.rs"])
        self.assertNotIn("README", call["diff"])
        self.assertNotIn("Cargo.lock", call["diff"])
        self.assertIn(f"COMMAND: `{self.repo / 'factory' / 'bin' / 'test-run'} -- cargo mutants --in-place "
                      "--in-diff mutants.diff -p alpha -p demo -j 1 --no-times --caught --unviable --colors never`",
                      self.mutants_md(d).splitlines())
        self.assertFalse(Path(call["cwd"]).exists())
        self.assertEqual(self.worktrees(), before)
        self.assertEqual(self.git("status", "--porcelain", cwd=worktree), status)

    # ---- MUTANTS-3 ------------------------------------------------------------------------------

    def test_mutants_3_mutants_md_has_its_shape(self):
        self.stand_in("\n".join([
            "Found 6 mutants to test",
            "ok       Unmutated baseline",
            mutant("caught", LIB, SHAPE_LIB, "a.saturating_add(b)", "replace add -> u8 with 0"),
            mutant("MISSED", LIB, SHAPE_LIB, "a > 10", "replace > with >= in big"),
            mutant("caught", LIB, SHAPE_LIB, "a < 3", "replace < with > in small"),
            mutant("MISSED", LIB, SHAPE_LIB, "a < 3", "replace < with <= in small"),
            mutant("TIMEOUT", LIB, SHAPE_LIB, "i += 1", "replace += with -= in spin"),
            mutant("unviable", LIB, SHAPE_LIB, "a.saturating_add(b)", "replace add -> u8 with Default::default()"),
            "6 mutants tested: 2 missed, 2 caught, 1 unviable, 1 timeouts",
        ]) + "\n")
        d = self.make_job("001-a00-99-shape")
        res = self.mutants(d.name, 0, STAND_IN_CODE="2")
        text = self.mutants_md(d)
        lines = text.splitlines()
        self.assertEqual(lines[0], "MUTANTS: 2 caught, 2 missed, 1 timeout, 1 unviable")
        self.assertEqual(lines[1:3], [f"COMMIT: {self.head}", "BUDGET: 1200s"])
        self.assertRegex(lines[3], r"^VM SECONDS: [0-9]+\.[0-9]$")
        self.assertTrue(lines[4].startswith("COMMAND: `"), lines[4])
        self.assertEqual(lines[5], "EXCLUDED: 0 under #[cfg(kani)] or #[cfg(test)]")
        self.assertNotIn("WHY:", text)
        self.assertEqual(self.section(text, "Missed"), [
            f"- {LIB}:{line_of(SHAPE_LIB, 'a > 10')}: replace > with >= in big (no test notices)",
            f"- {LIB}:{line_of(SHAPE_LIB, 'a < 3')}: replace < with <= in small (no test notices)",
        ])
        self.assertEqual(self.section(text, "Missed in functions a Kani proof calls"), ["None."])
        log = (d / "round-1" / "mutants.log").read_text()
        self.assertIn("==== cargo mutants --in-place --in-diff mutants.diff -p demo -j 1 --no-times --caught "
                      "--unviable --colors never (2 caught, 2 missed, 1 timeout, 1 unviable, ", log)
        self.assertIn(self.output.read_text(), log)
        self.assertEqual(res.stdout.strip(), f"MUTANTS: 2 caught, 2 missed, 1 timeout, 1 unviable ({d.name} round 1)")

    # ---- MUTANTS-4 ------------------------------------------------------------------------------

    def test_mutants_4_mutants_under_cfg_kani_or_cfg_test_are_excluded(self):
        harness = "core/crates/demo/src/harness.rs"
        self.stand_in("\n".join([
            mutant("MISSED", LIB, EXCLUDED_LIB, "a < 3", "replace < with <= in helper"),
            mutant("caught", LIB, EXCLUDED_LIB, "a == 4", "replace == with != in other"),
            mutant("MISSED", harness, HARNESS_FILE, "a != 5", "replace != with == in h"),
            mutant("MISSED", LIB, EXCLUDED_LIB, "a > 10", "replace > with >= in big"),
            "4 mutants tested: 3 missed, 1 caught",
        ]) + "\n")
        d = self.make_job("001-a00-99-excluded", files={LIB: EXCLUDED_LIB, harness: HARNESS_FILE})
        self.mutants(d.name, 0, STAND_IN_CODE="2")
        text = self.mutants_md(d)
        self.assertEqual(text.splitlines()[0], "MUTANTS: 0 caught, 1 missed, 0 timeout, 0 unviable")
        self.assertIn("\nEXCLUDED: 3 under #[cfg(kani)] or #[cfg(test)]\n", text)
        self.assertEqual(self.section(text, "Missed"),
                         [f"- {LIB}:{line_of(EXCLUDED_LIB, 'a > 10')}: replace > with >= in big (no test notices)"])
        self.assertEqual(self.section(text, "Missed in functions a Kani proof calls"), ["None."])

    # ---- MUTANTS-5 ------------------------------------------------------------------------------

    def test_mutants_5_a_missed_mutant_a_kani_proof_calls_is_listed_apart(self):
        self.stand_in("\n".join([
            mutant("MISSED", LIB, KANI_LIB, "a > 10", "replace > with >= in big"),
            mutant("MISSED", LIB, KANI_LIB, "a < 3", "replace small -> bool with false"),
            "2 mutants tested: 2 missed",
        ]) + "\n")
        d = self.make_job("001-a00-99-kani", files={LIB: KANI_LIB})
        self.mutants(d.name, 0, STAND_IN_CODE="2")
        text = self.mutants_md(d)
        self.assertEqual(text.splitlines()[0], "MUTANTS: 0 caught, 2 missed, 0 timeout, 0 unviable")
        self.assertEqual(self.section(text, "Missed"),
                         [f"- {LIB}:{line_of(KANI_LIB, 'a < 3')}: replace small -> bool with false (no test notices)"])
        self.assertEqual(self.section(text, "Missed in functions a Kani proof calls"), [
            f"- {LIB}:{line_of(KANI_LIB, 'a > 10')}: replace > with >= in big (no test notices; big is called by a "
            "#[kani::proof] harness)"])

    # ---- MUTANTS-6 ------------------------------------------------------------------------------

    def test_mutants_6_a_run_past_its_budget_is_timeout_with_what_it_tested(self):
        self.config({"budget_s": 1})
        self.stand_in("\n".join([
            mutant("caught", LIB, SHAPE_LIB, "a < 3", "replace < with > in small"),
            mutant("MISSED", LIB, SHAPE_LIB, "a > 10", "replace > with >= in big"),
        ]) + "\n")
        d = self.make_job("001-a00-99-timeout")
        worktree = Path(json.loads((d / "job.json").read_text())["worktree"])
        status = self.git("status", "--porcelain", cwd=worktree)
        before = self.worktrees()
        self.mutants(d.name, 0, STAND_IN="sleep")
        text = self.mutants_md(d)
        self.assertEqual(text.splitlines()[0], "MUTANTS: TIMEOUT after 1s: 1 caught, 1 missed, 0 timeout, 0 unviable")
        self.assertIn("\nBUDGET: 1s\n", text)
        self.assertEqual(self.section(text, "Missed"),
                         [f"- {LIB}:{line_of(SHAPE_LIB, 'a > 10')}: replace > with >= in big (no test notices)"])
        [call] = self.stand_in_calls()
        with self.assertRaises(ProcessLookupError):
            os.kill(call["pid"], 0)
        self.assertFalse(Path(call["cwd"]).exists())
        self.assertEqual(self.worktrees(), before)
        self.assertEqual(self.git("status", "--porcelain", cwd=worktree), status)

    def test_mutants_6_a_stop_ends_the_run_and_removes_the_scratch_copy(self):
        d = self.make_job("001-a00-99-stopped")
        worktree = Path(json.loads((d / "job.json").read_text())["worktree"])
        status = self.git("status", "--porcelain", cwd=worktree)
        before = self.worktrees()
        proc = subprocess.Popen([str(self.repo / "factory" / "bin" / "mutants"), d.name], cwd=self.repo,
                                env={**self.env, "STAND_IN": "sleep"}, stdout=subprocess.PIPE,
                                stderr=subprocess.PIPE, text=True)
        try:
            deadline = time.monotonic() + 30
            while not self.stand_in_calls() and time.monotonic() < deadline:
                time.sleep(0.05)
            calls = self.stand_in_calls()
            self.assertEqual(len(calls), 1)
            proc.send_signal(signal.SIGTERM)
            self.assertEqual(proc.wait(timeout=60), 128 + signal.SIGTERM)
        finally:
            proc.kill()
            proc.communicate()
        with self.assertRaises(ProcessLookupError):
            os.kill(calls[0]["pid"], 0)
        self.assertFalse((d / "round-1" / "mutants.md").exists())
        self.assertFalse(Path(calls[0]["cwd"]).exists())
        self.assertEqual(self.worktrees(), before)
        self.assertEqual(self.git("status", "--porcelain", cwd=worktree), status)

    # ---- MUTANTS-7 ------------------------------------------------------------------------------

    def test_mutants_7_a_run_with_no_result_is_never_a_count(self):
        missed = mutant("MISSED", LIB, SHAPE_LIB, "a > 10", "replace > with >= in big")
        cases = {
            "no-mutants": ("factory/bin/test-run: fell back to this Mac: ssh ended 255 before the run started\n"
                           "error: no such command: `mutants`\n", "101", "no such command"),
            "no-result-line": (missed + "\n", "2", "no result line"),
            "baseline": ("FAILED   Unmutated baseline\n", "4", "baseline"),
        }
        for n, (name, (output, code, why)) in enumerate(cases.items(), 1):
            with self.subTest(name=name):
                self.stand_in(output)
                d = self.make_job(f"00{n}-a00-99-{name}")
                res = self.mutants(d.name, 0, STAND_IN_CODE=code)
                text = self.mutants_md(d)
                lines = text.splitlines()
                self.assertEqual(lines[0], f"MUTANTS: NO RESULT (exit {code})")
                [line] = [line for line in lines if line.startswith("WHY: ")]
                self.assertIn(why, line)
                self.assertNotIn("missed", lines[0])
                self.assertNotIn("0 missed", text)
                self.assertNotIn("## Missed", text)
                self.assertIn(f"MUTANTS: NO RESULT (exit {code}) ({d.name} round 1)", res.stdout)

    # ---- MUTANTS-8 ------------------------------------------------------------------------------

    def test_mutants_8_invalid_settings_stop_before_anything(self):
        for n, (mutants, field) in enumerate((({"budget_s": 0}, "mutants.budget_s"),
                                              ({"budget_s": "20"}, "mutants.budget_s"),
                                              (5, "config.json's mutants must be an object")), 1):
            with self.subTest(mutants=mutants):
                self.config(mutants)
                d = self.make_job(f"00{n}-a00-99-settings")
                res = self.mutants(d.name, 2)
                self.assertIn(field, res.stderr)
                self.assertFalse((d / "round-1" / "mutants.md").exists())
        self.assertEqual(self.stand_in_calls(), [])

    def test_mutants_8_no_passing_gate_or_round_stops_before_anything(self):
        d = self.make_job("001-a00-99-nogate")
        checks = d / "round-1" / "checks.md"
        checks.write_text(f"GATE: FAIL\nCOMMIT: {self.head}\n\n")
        self.assertIn("GATE: PASS", self.mutants(d.name, 2).stderr)
        checks.unlink()
        self.assertIn("GATE: PASS", self.mutants(d.name, 2).stderr)
        job = json.loads((d / "job.json").read_text())
        job["round"] = 0
        (d / "job.json").write_text(json.dumps(job))
        self.assertIn("hasn't started a build round", self.mutants(d.name, 2).stderr)
        self.assertIn("no job 002-a00-99-none", self.mutants("002-a00-99-none", 2).stderr)
        usage = subprocess.run([str(self.repo / "factory" / "bin" / "mutants")], cwd=self.repo, env=self.env,
                               capture_output=True, text=True, timeout=60)
        self.assertEqual(usage.returncode, 2, usage.stdout)
        self.assertIn("Usage:", usage.stderr)
        self.assertFalse((d / "round-1" / "mutants.md").exists())
        self.assertEqual(self.stand_in_calls(), [])

    def test_mutants_8_the_budget_is_1200s_by_default(self):
        d = self.make_job("001-a00-99-default")
        self.mutants(d.name, 0)
        self.assertIn("\nBUDGET: 1200s\n", self.mutants_md(d))

    # ---- MUTANTS-9 ------------------------------------------------------------------------------

    def test_mutants_9_it_never_writes_job_json_or_takes_its_lock(self):
        missed = mutant("MISSED", LIB, SHAPE_LIB, "a > 10", "replace > with >= in big")
        cases = {
            "missed": ({}, missed + "\n1 mutant tested: 1 missed\n", {"STAND_IN_CODE": "2"}, None),
            "timeout": ({"budget_s": 1}, missed + "\n", {"STAND_IN": "sleep"}, None),
            "no-result": ({}, "error: no such command: `mutants`\n", {"STAND_IN_CODE": "101"}, None),
            "skipped": ({}, "", {}, {"docs/notes.md": "# Notes\n"}),
        }
        for n, (name, (config, output, env, files)) in enumerate(cases.items(), 1):
            with self.subTest(name=name):
                self.config(config)
                self.stand_in(output)
                d = self.make_job(f"00{n}-a00-99-{name}", files=files)
                before = (d / "job.json").read_bytes()
                self.mutants(d.name, 0, **env)
                self.assertTrue(self.mutants_md(d).startswith("MUTANTS: "))
                self.assertEqual((d / "job.json").read_bytes(), before)
                self.assertFalse((d / ".lock").exists())


if __name__ == "__main__":
    unittest.main(verbosity=2)
