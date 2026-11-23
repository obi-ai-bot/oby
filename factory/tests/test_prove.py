#!/usr/bin/env python3
"""factory/bin/prove and the `prove` stage in factory/bin/job (docs/contracts/factory.md, PROVE-1 to 20), with a
stand-in test-run, never cargo, Kani, Quint, Java or a real host.

Run from the repository root: python3 -m unittest discover -s factory/tests -p 'test_prove.py' -v
Each test builds a throwaway repository holding copies of factory/bin/prove, factory/bin/job and
factory/bin/child_session.py, a config.json the test writes, a crate with one Kani harness on the branch, and a
job whose gate passed that branch's head. The stand-in test-run beside them records each call and prints
`VERIFICATION:- FAILED` when a line of the tree it runs in holds BROKEN, `VERIFICATION:- SUCCESSFUL` otherwise;
STAND_IN makes it always succeed, sleep, print no verdict, miss Kani or exit 1 after a success. For a `quint`
command it prints the lines Quint 0.33.1 printed on the gate VM: a typecheck prints nothing, a run `[ok] No violation
found`, and a verify holds unless its invariant's name starts with `violated` (a counterexample), `missing` (a QNT404
name error), `silent` (exit 0 with no verdict), `badexit` (`[ok]` but exit 1) or `fallback` (test-run fell back to this
Mac without quint, exit 127); STAND_IN=typecheck-fails fails the typecheck.
"""

import hashlib
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
HARNESS = "add_never_panics"
MODEL = "docs/models/demo.qnt"
MODEL_TEXT = "module demo_dev {\n  var x: int\n}\n"

BASE_LIB = """pub fn add(a: u8, b: u8) -> u8 {
    a.saturating_add(b)
}
"""

PROOF_MODULE = """
#[cfg(test)]
mod proofs {
    #![cfg(kani)]

    #[kani::proof]
    fn add_never_panics() {
        let a: u8 = kani::any();
        let b: u8 = kani::any();
        kani::assume(a < 100);
        let _ = super::add(a, b);
    }
}
"""

ENTRY = """### PROOF-1
- Property: add never panics for any two bytes.
- Scenario: DEMO-1
- Crate: `demo`
- Harness: `add_never_panics`
- Assumes: `a < 100`
- Unwind: {unwind}
- Broken file: `{file}`
- Broken old: `{old}`
- Broken new: `{new}`"""


def entry(unwind="none", file=LIB, old="a.saturating_add(b)", new="a.saturating_add(b) // BROKEN"):
    return ENTRY.format(unwind=unwind, file=file, old=old, new=new)


def model(file=MODEL, main="demo_dev", depth=None, invariants=(("holdsInv", "DEMO-1", "violatedWit"),)):
    """A `## Models` entry: one `Invariant:` line per (invariant, scenario, witness)."""
    return (f"### MODEL-1\n- File: `{file}`\n- Main: `{main}`\n" + (f"- Depth: {depth}\n" if depth else "")
            + "".join(f"- Invariant: `{i}` for {s}, witness `{w}`\n" for i, s, w in invariants)).strip()


STAND_IN = """#!/usr/bin/env python3
import json, os, sys, time
from pathlib import Path
with open(os.environ["STAND_IN_LOG"], "a") as f:
    f.write(json.dumps({"argv": sys.argv[1:], "cwd": os.getcwd(), "target": os.environ.get("CARGO_TARGET_DIR"),
                        "pid": os.getpid()}) + "\\n")
mode = os.environ.get("STAND_IN", "kani")
words = sys.argv[2:]
if words[:1] == ["quint"]:
    if mode == "sleep":
        time.sleep(60)
    if words[1] == "typecheck":
        if mode == "typecheck-fails":
            print("error: [QNT404] Name 'nope' not found\\nerror: name resolution failed")
            sys.exit(1)
        sys.exit(0)
    if words[1] == "run":
        print("[ok] No violation found (51ms at 196078 traces/second).")
        sys.exit(0)
    name = next(w for w in words if w.startswith("--invariant=")).split("=", 1)[1]
    if name.startswith("violated"):
        print("The outcome is: Error\\n[State 0]\\n{ x: 0 }\\n\\n[State 1]\\n{ x: 1 }\\n\\n"
              "[violation] Found an issue (3727ms).\\nerror: found a counterexample")
        sys.exit(1)
    if name.startswith("missing"):
        print(f"error: [QNT404] Name '{name}' not found\\nerror: name resolution failed")
        sys.exit(1)
    if name.startswith("silent"):
        sys.exit(0)
    if name.startswith("badexit"):
        print("The outcome is: NoError\\n[ok] No violation found (4931ms).")
        sys.exit(1)
    if name.startswith("fallback"):
        print("factory/bin/test-run: fell back to this Mac: ssh ended 2 before the run started: oby-remote-gate: "
              "quint isn't on the builder's PATH", file=sys.stderr)
        print("env: quint: No such file or directory", file=sys.stderr)
        sys.exit(127)
    print("The outcome is: NoError\\n[ok] No violation found (4931ms).")
    sys.exit(0)
if mode in {"sleep", "kani-sleep"}:
    time.sleep(60)
if mode == "silent":
    print("Checking harness proofs::add_never_panics...")
    sys.exit(0)
if mode == "no-kani":
    print("factory/bin/test-run: fell back to this Mac: ssh ended 255 before the run started", file=sys.stderr)
    print("error: no such command: `kani`", file=sys.stderr)
    sys.exit(101)
if mode == "bad-exit":
    print("VERIFICATION:- SUCCESSFUL")
    sys.exit(1)
broken = any("BROKEN" in p.read_text() for p in Path.cwd().rglob("*.rs"))
if broken and mode != "vacuous":
    print("Failed Checks: broken on purpose\\nVERIFICATION:- FAILED\\nVerification Time: 0.5s")
    sys.exit(1)
print("VERIFICATION:- SUCCESSFUL\\nVerification Time: 1.25s")
"""


def line_of(text, needle):
    return next(n for n, line in enumerate(text.splitlines(), 1) if needle in line)


class Prove(unittest.TestCase):
    def setUp(self):
        self.dir = Path(tempfile.mkdtemp()).resolve()
        self.repo = self.dir / "repo"
        self.calls = self.dir / "stand-in.log"
        bin_dir = self.repo / "factory" / "bin"
        bin_dir.mkdir(parents=True)
        for name in ("prove", "job", "child_session.py"):
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
        }
        self.env.pop("STAND_IN", None)
        self.git("init", "-q", "-b", "dev")
        (self.repo / ".gitignore").write_text("/factory/jobs/\n/.worktrees/\ntarget/\n")
        (self.repo / LIB).parent.mkdir(parents=True)
        (self.repo / LIB).write_text(BASE_LIB)
        self.git("add", ".gitignore", LIB)
        self.git("commit", "-q", "-m", "base")
        self.git("update-ref", "refs/remotes/origin/dev", "HEAD")

    def tearDown(self):
        shutil.rmtree(self.dir)

    def git(self, *args, cwd=None):
        return subprocess.run(["git", *args], cwd=cwd or self.repo, env=getattr(self, "env", None),
                              capture_output=True, text=True, check=True).stdout

    def config(self, proofs=None):
        config = {"concurrency": 4, "max_rounds": 3, "base_branch": "dev", "sprint": 1, "prices_usd_per_mtok": {}}
        if proofs is not None:
            config["proofs"] = proofs
        (self.repo / "factory" / "config.json").write_text(json.dumps(config))

    def make_job(self, job_id, surfaces=("proofs",), files=None, proofs=None, stage="gate", models=None):
        """A job in `stage` whose branch adds `files` (by default the harness and a model) and whose gate passed its
        head; `models` is its spec's `## Models` section, with its hash recorded."""
        worktree = self.repo / ".worktrees" / job_id
        self.git("worktree", "add", "-q", "-b", f"factory/{job_id}", str(worktree), "origin/dev")
        files = files if files is not None else {LIB: BASE_LIB + PROOF_MODULE, MODEL: MODEL_TEXT}
        for path, text in files.items():
            (worktree / path).parent.mkdir(parents=True, exist_ok=True)
            (worktree / path).write_text(text)
        self.git("add", *files, cwd=worktree)
        self.git("commit", "-q", "-m", "proofs", cwd=worktree)
        head = self.git("rev-parse", "HEAD", cwd=worktree).strip()
        proofs = entry() if proofs is None else proofs
        d = self.repo / "factory" / "jobs" / job_id
        (d / "round-1").mkdir(parents=True)
        (d / "spec.md").write_text(f"# A spec\n\n## Proofs\n{proofs}\n\n"
                                   + (f"## Models\n{models}\n\n" if models is not None else "")
                                   + "## Out of scope\n- nothing\n")
        (d / "round-1" / "checks.md").write_text(f"GATE: PASS\nCOMMIT: {head}\nCHECKS: x\n\n## Summary\n")
        (d / "job.json").write_text(json.dumps({
            "id": job_id, "issue": "A00-99", "title": "feat: proofs", "milestone": "A00", "sprint": 0, "fixes": [],
            "pr": None, "pr_url": None, "branch": f"factory/{job_id}", "worktree": str(worktree),
            "stage": stage, "round": 1, "round_limit": 3,
            "intake": {"decision": "ACCEPT", "rescoped": False, "risk": "high", "surfaces": list(surfaces)},
            "agents": {"reviewers": {"code": "sonnet"}}, "gate": "PASS", "reviews": {}, "cost_usd": 0,
            "started": "2026-01-01T00:00:00Z", "ended": None,
            "proofs_hash": hashlib.sha256(proofs.strip().encode()).hexdigest(),
            "models_hash": hashlib.sha256(models.strip().encode()).hexdigest() if models is not None else "none",
        }, indent=2) + "\n")
        self.head = head
        return d

    def prove(self, job_id, code=None, **env):
        res = subprocess.run([str(self.repo / "factory" / "bin" / "prove"), job_id], cwd=self.repo,
                             env={**self.env, **env}, capture_output=True, text=True, timeout=120)
        if code is not None:
            self.assertEqual(res.returncode, code, res.stdout + res.stderr)
        return res

    def job(self, *args):
        return subprocess.run([str(self.repo / "factory" / "bin" / "job"), *args], cwd=self.repo, env=self.env,
                              capture_output=True, text=True, timeout=60)

    def proofs_md(self, d, n=1):
        return (d / f"round-{n}" / "proofs.md").read_text()

    def state(self, d):
        return json.loads((d / "job.json").read_text())

    def stand_in_calls(self):
        return [json.loads(line) for line in self.calls.read_text().splitlines()] if self.calls.exists() else []

    def worktrees(self):
        return [line for line in self.git("worktree", "list", "--porcelain").splitlines()
                if line.startswith("worktree ")]

    # ---- PROVE-1 --------------------------------------------------------------------------------

    def test_prove_1_a_job_without_the_surface_is_skipped(self):
        d = self.make_job("001-a00-99-skip", surfaces=("security",))
        res = self.prove(d.name, 0)
        lines = self.proofs_md(d).splitlines()
        self.assertEqual(lines[:2], ["PROOFS: SKIPPED", f"COMMIT: {self.head}"])
        self.assertNotIn("## Why", self.proofs_md(d))
        self.assertIn("PROOFS: SKIPPED (001-a00-99-skip round 1)", res.stdout)
        self.assertEqual(self.stand_in_calls(), [])
        self.assertEqual(len(self.worktrees()), 2)

    # ---- PROVE-2 --------------------------------------------------------------------------------

    def change_properties(self, d):
        spec = d / "spec.md"
        spec.write_text(spec.read_text().replace("for any two bytes", "for some bytes"))

    def test_prove_2_changed_properties_fail_in_blocking_mode(self):
        self.config({"mode": "blocking"})
        d = self.make_job("001-a00-99-changed")
        self.change_properties(d)
        self.prove(d.name, 1)
        text = self.proofs_md(d)
        self.assertTrue(text.startswith(f"PROOFS: FAIL\nCOMMIT: {self.head}\nMODE: blocking\n\n## Why\n"), text)
        self.assertIn("properties changed after spec", text)
        self.assertEqual(self.stand_in_calls(), [])

    def test_prove_2_changed_properties_are_advisory_by_default(self):
        for n, proofs in enumerate((None, {"mode": "advisory"}), 1):
            with self.subTest(proofs=proofs):
                self.config(proofs)
                d = self.make_job(f"00{n}-a00-99-changed")
                self.change_properties(d)
                self.prove(d.name, 0)
                text = self.proofs_md(d)
                self.assertTrue(text.startswith(f"PROOFS: ADVISORY-FAIL\nCOMMIT: {self.head}\nMODE: advisory\n"
                                                "RESULT: FAIL\n\n## Why\n"), text)
                self.assertIn("properties changed after spec", text)

    def test_prove_2_a_job_with_no_recorded_hash_counts_as_none(self):
        self.config({"mode": "blocking"})
        d = self.make_job("001-a00-99-nohash")
        job = self.state(d)
        del job["proofs_hash"]
        (d / "job.json").write_text(json.dumps(job))
        self.prove(d.name, 1)
        self.assertIn("properties changed after spec", self.proofs_md(d))

    # ---- PROVE-3 --------------------------------------------------------------------------------

    def test_prove_3_each_forbidden_construct_is_caught(self):
        self.config({"mode": "blocking"})
        module = BASE_LIB + PROOF_MODULE
        cases = {
            "stub": (module.replace("    #[kani::proof]\n", "    #[kani::proof]\n    #[kani::stub(super::add, fake)]\n"),
                     "kani::stub"),
            "should_panic": (module.replace("    #[kani::proof]\n", "    #[kani::proof]\n    #[should_panic]\n"),
                             "#[should_panic]"),
            "cfg_kani_top_level": ("#[cfg(kani)]\npub fn only_for_proofs() {}\n" + module, "#[cfg(kani)]"),
            "unlisted_assume": (module.replace("kani::assume(a < 100);", "kani::assume(a < 100);\n"
                                               "        kani::assume(b < 7);"), "kani::assume(b < 7)"),
            "unlisted_unwind": (module.replace("    #[kani::proof]\n", "    #[kani::proof]\n    #[kani::unwind(9)]\n"),
                                "kani::unwind(9)"),
        }
        for n, (name, (text, needle)) in enumerate(cases.items(), 1):
            with self.subTest(name=name):
                d = self.make_job(f"00{n}-a00-99-{name.replace('_', '-')}", files={LIB: text})
                self.prove(d.name, 1)
                proofs = self.proofs_md(d)
                self.assertTrue(proofs.startswith("PROOFS: FAIL\n"), proofs)
                self.assertIn(f"forbidden construct: {LIB}:{line_of(text, needle)}:", proofs)
        self.assertEqual(self.stand_in_calls(), [])

    def test_prove_3_listed_and_allowed_constructs_pass_the_scan(self):
        self.config({"mode": "blocking"})
        text = BASE_LIB + PROOF_MODULE.replace("    #[kani::proof]\n", "    #[kani::proof]\n    #[kani::unwind(4)]\n")
        d = self.make_job("001-a00-99-allowed", proofs=entry(unwind="4"), files={
            LIB: text,
            "core/crates/demo/Cargo.toml": "[lints.rust]\nunexpected_cfgs = { level = \"warn\", "
                                           "check-cfg = ['cfg(kani)'] }\n",
            "core/crates/demo/tests/more.rs": "#![cfg(kani)]\n#[kani::proof]\nfn more() {}\n",
        })
        self.prove(d.name, 0)
        self.assertTrue(self.proofs_md(d).startswith("PROOFS: PASS\n"), self.proofs_md(d))
        self.assertEqual(len(self.stand_in_calls()), 2)

    # ---- PROVE-4 --------------------------------------------------------------------------------

    def test_prove_4_a_green_harness_with_a_red_broken_rule_passes(self):
        d = self.make_job("001-a00-99-pass")
        worktree = Path(self.state(d)["worktree"])
        before = (worktree / LIB).read_text()
        res = self.prove(d.name, 0)
        text = self.proofs_md(d)
        self.assertTrue(text.startswith(f"PROOFS: PASS\nCOMMIT: {self.head}\nMODE: advisory\n\n## PROOF-1 "
                                        f"{HARNESS}\n"), text)
        self.assertNotIn("## Why", text)
        self.assertIn(f"cargo kani -p demo --tests --harness {HARNESS}`", text)
        self.assertIn("- Result: SUCCESSFUL\n", text)
        self.assertIn("- Verification seconds: 1.25\n", text)
        self.assertRegex(text, r"- VM seconds: [0-9]+\.[0-9]\n")
        self.assertIn("- Broken rule: RED", text)
        self.assertIn("PROOFS: PASS (001-a00-99-pass round 1)", res.stdout)
        calls = self.stand_in_calls()
        self.assertEqual(len(calls), 2)
        for call in calls:
            self.assertEqual(call["argv"], ["--", "cargo", "kani", "-p", "demo", "--tests", "--harness", HARNESS])
            self.assertTrue(call["cwd"].endswith("/core"), call)
            self.assertFalse(call["cwd"].startswith(str(worktree)), call)
            self.assertEqual(call["target"], call["cwd"] + "/target")
        self.assertIn("VERIFICATION:- FAILED", (d / "round-1" / "proofs.log").read_text())
        self.assertEqual((worktree / LIB).read_text(), before)
        self.assertEqual(self.git("status", "--porcelain", cwd=worktree), "")
        self.assertEqual(len(self.worktrees()), 2)
        self.assertFalse(Path(calls[0]["cwd"]).exists())

    # ---- PROVE-5 --------------------------------------------------------------------------------

    def test_prove_5_a_harness_green_on_its_broken_rule_is_vacuous(self):
        for n, (proofs, code, first) in enumerate(((None, 0, "PROOFS: ADVISORY-FAIL"),
                                                   ({"mode": "blocking"}, 1, "PROOFS: FAIL")), 1):
            with self.subTest(proofs=proofs):
                self.config(proofs)
                d = self.make_job(f"00{n}-a00-99-vacuous")
                self.prove(d.name, code, STAND_IN="vacuous")
                text = self.proofs_md(d)
                self.assertTrue(text.startswith(first + "\n"), text)
                why = text.split("## Why\n", 1)[1].split("\n## ", 1)[0]
                self.assertIn("vacuous", why)
                self.assertIn("- Broken rule: GREEN", text)

    # ---- PROVE-6 --------------------------------------------------------------------------------

    def test_prove_6_a_run_past_its_timeout_is_timeout(self):
        for n, (mode, code, head) in enumerate((("blocking", 1, "PROOFS: TIMEOUT\n"),
                                                ("advisory", 0, "PROOFS: ADVISORY-FAIL\n")), 1):
            with self.subTest(mode=mode):
                self.config({"mode": mode, "timeout_s": 1})
                self.calls.unlink(missing_ok=True)
                d = self.make_job(f"00{n}-a00-99-timeout")
                self.prove(d.name, code, STAND_IN="sleep")
                text = self.proofs_md(d)
                self.assertTrue(text.startswith(head), text)
                if mode == "advisory":
                    self.assertIn("RESULT: TIMEOUT\n", text)
                self.assertIn("- Result: TIMEOUT\n", text)
                calls = self.stand_in_calls()
                self.assertEqual(len(calls), 1)
                with self.assertRaises(ProcessLookupError):
                    os.kill(calls[0]["pid"], 0)
                self.assertEqual(len(self.worktrees()), n + 1)
                self.assertFalse(Path(calls[0]["cwd"]).exists())

    def test_prove_6_a_stop_ends_the_run_and_removes_the_scratch_copy(self):
        d = self.make_job("001-a00-99-stopped")
        proc = subprocess.Popen([str(self.repo / "factory" / "bin" / "prove"), d.name], cwd=self.repo,
                                env={**self.env, "STAND_IN": "sleep"}, stdout=subprocess.PIPE,
                                stderr=subprocess.PIPE, text=True)
        try:
            deadline = time.monotonic() + 30
            while not self.stand_in_calls() and time.monotonic() < deadline:
                time.sleep(0.05)
            calls = self.stand_in_calls()
            self.assertEqual(len(calls), 1)
            proc.send_signal(signal.SIGTERM)
            self.assertEqual(proc.wait(timeout=30), 128 + signal.SIGTERM)
        finally:
            proc.kill()
            proc.communicate()
        with self.assertRaises(ProcessLookupError):
            os.kill(calls[0]["pid"], 0)
        self.assertFalse((d / "round-1" / "proofs.md").exists())
        self.assertEqual(len(self.worktrees()), 2)
        self.assertFalse(Path(calls[0]["cwd"]).exists())

    # ---- PROVE-7 --------------------------------------------------------------------------------

    def test_prove_7_a_run_with_no_verdict_is_not_a_pass(self):
        self.config({"mode": "blocking"})
        for n, (mode, result) in enumerate((("silent", "NO VERDICT (exit 0)"),
                                            ("no-kani", "NO VERDICT (exit 101)"),
                                            ("bad-exit", "NO VERDICT (exit 1)")), 1):
            with self.subTest(mode=mode):
                d = self.make_job(f"00{n}-a00-99-{mode}")
                self.prove(d.name, 1, STAND_IN=mode)
                text = self.proofs_md(d)
                self.assertTrue(text.startswith("PROOFS: FAIL\n"), text)
                self.assertIn(f"- Result: {result}\n", text)
                self.assertIn("- Broken rule: not run", text)

    # ---- PROVE-8 --------------------------------------------------------------------------------

    def test_prove_8_a_broken_rule_that_cant_apply_fails(self):
        self.config({"mode": "blocking"})
        outside = self.dir / "outside.rs"
        outside.write_text(BASE_LIB)
        repo_status = self.git("status", "--porcelain")
        cases = {
            "no-line": (entry(old="a.wrapping_add(b)"), "DID NOT APPLY"),
            "two-lines": (entry(old="}"), "DID NOT APPLY"),
            "parent": (entry(file="../x.rs"), "OUTSIDE THE REPOSITORY"),
            "absolute": (entry(file=str(outside)), "OUTSIDE THE REPOSITORY"),
        }
        for n, (name, (proofs, word)) in enumerate(cases.items(), 1):
            with self.subTest(name=name):
                d = self.make_job(f"00{n}-a00-99-{name}", proofs=proofs)
                worktree = Path(self.state(d)["worktree"])
                self.prove(d.name, 1)
                text = self.proofs_md(d)
                self.assertTrue(text.startswith("PROOFS: FAIL\n"), text)
                self.assertIn(f"- Broken rule: {word}", text)
                self.assertEqual(self.git("status", "--porcelain", cwd=worktree), "")
                self.assertEqual(self.git("status", "--porcelain"), repo_status)
                self.assertEqual(outside.read_text(), BASE_LIB)
                self.assertFalse((self.dir / "x.rs").exists())
                self.assertEqual(len(self.worktrees()), n + 1)

    # ---- PROVE-9 --------------------------------------------------------------------------------

    def test_prove_9_a_proofs_job_goes_through_prove(self):
        d = self.make_job("001-a00-99-stage")
        res = self.job("advance", d.name, "review")
        self.assertEqual(res.returncode, 1, res.stdout)
        self.assertIn("factory/bin/prove", res.stderr)
        self.assertEqual(self.state(d)["stage"], "gate")
        res = self.job("advance", d.name, "prove")
        self.assertEqual(res.returncode, 0, res.stderr)
        self.assertEqual(self.state(d)["stage"], "prove")
        refused = self.job("advance", d.name, "review")
        self.assertEqual(refused.returncode, 1, refused.stdout)
        self.assertIn("factory/bin/prove", refused.stderr)
        self.prove(d.name, 0)
        res = self.job("advance", d.name, "review")
        self.assertEqual(res.returncode, 0, res.stderr)
        self.assertEqual(self.state(d)["stage"], "review")
        # RISK high: Codex's review joins the routed one (factory/bin/job's two_reviews()).
        self.assertEqual(self.state(d)["reviews"], {"code": "pending", "codex": "pending"})

    def test_prove_9_an_advisory_fail_goes_on_to_review(self):
        d = self.make_job("001-a00-99-advisory", stage="prove")
        self.prove(d.name, 0, STAND_IN="vacuous")
        self.assertTrue(self.proofs_md(d).startswith("PROOFS: ADVISORY-FAIL\n"))
        self.assertEqual(self.job("advance", d.name, "build").returncode, 1)
        res = self.job("advance", d.name, "review")
        self.assertEqual(res.returncode, 0, res.stderr)
        self.assertEqual(self.state(d)["stage"], "review")

    def test_prove_9_a_fail_goes_back_to_build_or_to_a_human_on_the_last_round(self):
        self.config({"mode": "blocking"})
        d = self.make_job("001-a00-99-fail", stage="prove")
        self.prove(d.name, 1, STAND_IN="vacuous")
        self.assertEqual(self.job("advance", d.name, "review").returncode, 1)
        self.assertEqual(self.job("advance", d.name, "needs-human").returncode, 1)
        res = self.job("advance", d.name, "build")
        self.assertEqual(res.returncode, 0, res.stderr)
        job = self.state(d)
        self.assertEqual((job["stage"], job["round"], job["proofs"]), ("build", 2, None))

        last = self.make_job("002-a00-99-last", stage="prove")
        job = self.state(last)
        job["round_limit"] = 1
        (last / "job.json").write_text(json.dumps(job))
        self.prove(last.name, 1, STAND_IN="vacuous")
        res = self.job("advance", last.name, "build")
        self.assertEqual(res.returncode, 1, res.stdout)
        res = self.job("advance", last.name, "needs-human")
        self.assertEqual(res.returncode, 0, res.stderr)
        self.assertEqual(self.state(last)["stage"], "needs-human")
        self.assertIn("Proofs failed in round 1, the last allowed round. See round-1/proofs.md.",
                      (last / "decision.md").read_text())
        # Feedback starts round 2 and saves without syncing again: the new round's proofs start empty.
        self.assertEqual(self.state(last)["proofs"], "FAIL")
        no_gh = self.dir / "no-gh"
        no_gh.mkdir()
        (no_gh / "gh").write_text("#!/bin/sh\nexit 1\n")
        (no_gh / "gh").chmod(0o755)
        res = subprocess.run([str(self.repo / "factory" / "bin" / "job"), "feedback", last.name], cwd=self.repo,
                             env={**self.env, "PATH": f"{no_gh}{os.pathsep}{self.env['PATH']}"},
                             input="Try again.", capture_output=True, text=True, timeout=60)
        self.assertEqual(res.returncode, 0, res.stderr)
        job = self.state(last)
        self.assertEqual((job["stage"], job["round"], job["proofs"]), ("build", 2, None))

    def test_prove_9_proofs_md_must_be_for_the_gated_commit(self):
        d = self.make_job("001-a00-99-commit", stage="prove")
        for stage in ("review", "build", "needs-human"):
            res = self.job("advance", d.name, stage)
            self.assertEqual(res.returncode, 1, res.stdout)
            self.assertIn(f"run factory/bin/prove {d.name} again", res.stderr)
        (d / "round-1" / "proofs.md").write_text(f"PROOFS: PASS\nCOMMIT: {'b' * 40}\nMODE: advisory\n\n")
        res = self.job("advance", d.name, "review")
        self.assertEqual(res.returncode, 1, res.stdout)
        self.assertIn(f"run factory/bin/prove {d.name} again", res.stderr)
        # proofs.md and checks.md agree, but the branch moved on since.
        self.prove(d.name, 0)
        worktree = Path(self.state(d)["worktree"])
        (worktree / "later.txt").write_text("later\n")
        self.git("add", "later.txt", cwd=worktree)
        self.git("commit", "-q", "-m", "later", cwd=worktree)
        res = self.job("advance", d.name, "review")
        self.assertEqual(res.returncode, 1, res.stdout)
        self.assertIn(f"run factory/bin/prove {d.name} again", res.stderr)
        self.assertEqual(self.state(d)["stage"], "prove")

    # ---- PROVE-10 -------------------------------------------------------------------------------

    def test_prove_10_a_job_without_the_surface_goes_gate_to_review_as_before(self):
        d = self.make_job("001-a00-99-plain", surfaces=("security",))
        res = self.job("advance", d.name, "review")
        self.assertEqual(res.returncode, 0, res.stderr)
        self.assertEqual(self.state(d)["stage"], "review")

    def test_prove_10_a_job_without_the_surface_may_go_through_prove(self):
        d = self.make_job("001-a00-99-through", surfaces=())
        self.assertEqual(self.job("advance", d.name, "prove").returncode, 0)
        self.prove(d.name, 0)
        self.assertTrue(self.proofs_md(d).startswith("PROOFS: SKIPPED\n"))
        res = self.job("advance", d.name, "review")
        self.assertEqual(res.returncode, 0, res.stderr)
        self.assertEqual(self.state(d)["stage"], "review")

    # ---- PROVE-11 -------------------------------------------------------------------------------

    def test_prove_11_leaving_spec_records_the_proofs_hash(self):
        for n, (section, want) in enumerate(((entry(), hashlib.sha256(entry().encode()).hexdigest()),
                                             (None, "none")), 1):
            with self.subTest(section=bool(section)):
                job_id = f"00{n}-a00-99-hash"
                d = self.repo / "factory" / "jobs" / job_id
                d.mkdir(parents=True)
                (d / "spec.md").write_text("# A spec\n\n" + (f"## Proofs\n{section}\n\n" if section else "")
                                           + "## Out of scope\n- nothing\n")
                (d / "job.json").write_text(json.dumps({
                    "id": job_id, "issue": "A00-99", "title": "feat: proofs", "pr": None, "branch": "b",
                    "worktree": str(self.dir / "none"), "stage": "spec", "round": 0, "round_limit": 3,
                    "intake": None, "agents": {"reviewers": {"code": "sonnet"}}, "gate": None, "reviews": {},
                    "cost_usd": 0, "started": "2026-01-01T00:00:00Z", "ended": None,
                }))
                res = self.job("advance", job_id, "build")
                self.assertEqual(res.returncode, 0, res.stderr)
                self.assertEqual(self.state(d)["proofs_hash"], want)

    def test_prove_11_sync_records_the_rounds_proofs(self):
        d = self.make_job("001-a00-99-sync", stage="prove")
        res = self.job("sync", d.name)
        self.assertEqual(res.stdout.strip(), f"{d.name}: prove round 1 gate PASS proofs -")
        self.assertIsNone(self.state(d)["proofs"])
        self.prove(d.name, 0)
        res = self.job("sync", d.name)
        self.assertEqual(res.returncode, 0, res.stderr)
        self.assertEqual(res.stdout.strip(), f"{d.name}: prove round 1 gate PASS proofs PASS")
        self.assertEqual(self.state(d)["proofs"], "PASS")

    # ---- PROVE-12 -------------------------------------------------------------------------------

    def test_prove_12_invalid_settings_stop_before_anything(self):
        for n, (proofs, field) in enumerate((({"mode": "strict"}, "proofs.mode"),
                                             ({"timeout_s": 0}, "proofs.timeout_s"),
                                             ({"timeout_s": "600"}, "proofs.timeout_s")), 1):
            with self.subTest(proofs=proofs):
                self.config(proofs)
                d = self.make_job(f"00{n}-a00-99-settings")
                res = self.prove(d.name, 2)
                self.assertIn(field, res.stderr)
                self.assertFalse((d / "round-1" / "proofs.md").exists())
        self.assertEqual(self.stand_in_calls(), [])

    def test_prove_12_blocking_mode_is_accepted(self):
        self.config({"mode": "blocking", "timeout_s": 30})
        d = self.make_job("001-a00-99-blocking")
        self.prove(d.name, 0)
        self.assertTrue(self.proofs_md(d).startswith(f"PROOFS: PASS\nCOMMIT: {self.head}\nMODE: blocking\n"))

    def test_prove_12_no_passing_gate_or_round_stops_before_anything(self):
        d = self.make_job("001-a00-99-nogate")
        (d / "round-1" / "checks.md").write_text(f"GATE: FAIL\nCOMMIT: {self.head}\n\n")
        self.assertIn("GATE: PASS", self.prove(d.name, 2).stderr)
        job = self.state(d)
        job["round"] = 0
        (d / "job.json").write_text(json.dumps(job))
        self.prove(d.name, 2)
        self.assertFalse((d / "round-1" / "proofs.md").exists())


    # ---- PROVE-13 -------------------------------------------------------------------------------

    def model_job(self, job_id, models=None, surfaces=("model",), **kw):
        return self.make_job(job_id, surfaces=surfaces, models=model() if models is None else models, **kw)

    def set_models(self, d, models):
        """spec.md's `## Models` replaced by `models` (none when None), and its hash recorded as leaving spec would."""
        spec = d / "spec.md"
        text = spec.read_text()
        start = text.find("## Models\n")
        if start >= 0:
            text = text[:start] + text[text.index("## Out of scope", start):]
        if models is not None:
            text = text.replace("## Out of scope", f"## Models\n{models}\n\n## Out of scope")
        spec.write_text(text)
        job = self.state(d)
        job["models_hash"] = hashlib.sha256(models.strip().encode()).hexdigest() if models is not None else "none"
        (d / "job.json").write_text(json.dumps(job))

    def test_prove_13_a_model_job_goes_through_prove(self):
        d = self.model_job("001-a00-99-model-stage")
        res = self.job("advance", d.name, "review")
        self.assertEqual(res.returncode, 1, res.stdout)
        self.assertIn(f"factory/bin/prove {d.name}", res.stderr)
        self.assertEqual(self.state(d)["stage"], "gate")
        res = self.job("advance", d.name, "prove")
        self.assertEqual(res.returncode, 0, res.stderr)
        self.assertEqual(self.state(d)["stage"], "prove")
        (d / "round-1" / "proofs.md").write_text(f"PROOFS: SKIPPED\nCOMMIT: {self.head}\nMODE: advisory\n\n")
        refused = self.job("advance", d.name, "review")
        self.assertEqual(refused.returncode, 1, refused.stdout)
        self.assertIn("PROOFS: SKIPPED", refused.stderr)
        self.prove(d.name, 0)
        self.assertTrue(self.proofs_md(d).startswith("PROOFS: PASS\n"), self.proofs_md(d))
        res = self.job("advance", d.name, "review")
        self.assertEqual(res.returncode, 0, res.stderr)
        self.assertEqual(self.state(d)["stage"], "review")

        vacuous = model(invariants=(("holdsInv", "DEMO-1", "holdsWit"),))
        advisory = self.model_job("002-a00-99-model-advisory", models=vacuous, stage="prove")
        self.prove(advisory.name, 0)
        self.assertTrue(self.proofs_md(advisory).startswith("PROOFS: ADVISORY-FAIL\n"))
        res = self.job("advance", advisory.name, "review")
        self.assertEqual(res.returncode, 0, res.stderr)

        self.config({"mode": "blocking"})
        failed = self.model_job("003-a00-99-model-fail", models=vacuous, stage="prove")
        self.prove(failed.name, 1)
        self.assertEqual(self.job("advance", failed.name, "review").returncode, 1)
        res = self.job("advance", failed.name, "build")
        self.assertEqual(res.returncode, 0, res.stderr)
        self.assertEqual((self.state(failed)["stage"], self.state(failed)["round"]), ("build", 2))

        last = self.model_job("004-a00-99-model-last", models=vacuous, stage="prove")
        job = self.state(last)
        job["round_limit"] = 1
        (last / "job.json").write_text(json.dumps(job))
        self.prove(last.name, 1)
        self.assertEqual(self.job("advance", last.name, "build").returncode, 1)
        res = self.job("advance", last.name, "needs-human")
        self.assertEqual(res.returncode, 0, res.stderr)
        self.assertEqual(self.state(last)["stage"], "needs-human")

    # ---- PROVE-14 -------------------------------------------------------------------------------

    def test_prove_14_leaving_spec_records_the_models_hash(self):
        for n, (section, want) in enumerate(((model(), hashlib.sha256(model().encode()).hexdigest()),
                                             (None, "none")), 1):
            with self.subTest(section=bool(section)):
                job_id = f"00{n}-a00-99-models-hash"
                d = self.repo / "factory" / "jobs" / job_id
                d.mkdir(parents=True)
                (d / "spec.md").write_text("# A spec\n\n" + (f"## Models\n{section}\n\n" if section else "")
                                           + "## Out of scope\n- nothing\n")
                (d / "job.json").write_text(json.dumps({
                    "id": job_id, "issue": "A00-99", "title": "feat: models", "pr": None, "branch": "b",
                    "worktree": str(self.dir / "none"), "stage": "spec", "round": 0, "round_limit": 3,
                    "intake": None, "agents": {"reviewers": {"code": "sonnet"}}, "gate": None, "reviews": {},
                    "cost_usd": 0, "started": "2026-01-01T00:00:00Z", "ended": None,
                }))
                res = self.job("advance", job_id, "build")
                self.assertEqual(res.returncode, 0, res.stderr)
                self.assertEqual(self.state(d)["models_hash"], want)
                self.assertEqual(self.state(d)["proofs_hash"], "none")

    def test_prove_14_changed_models_fail_with_nothing_run(self):
        cases = (("blocking", "changed", 1, "PROOFS: FAIL\n"),
                 ("advisory", "changed", 0, "PROOFS: ADVISORY-FAIL\n"),
                 ("blocking", "no-hash", 1, "PROOFS: FAIL\n"))
        for n, (mode, how, code, first) in enumerate(cases, 1):
            with self.subTest(mode=mode, how=how):
                self.config({"mode": mode})
                d = self.model_job(f"00{n}-a00-99-models-{how}")
                if how == "changed":
                    spec = d / "spec.md"
                    spec.write_text(spec.read_text().replace("DEMO-1", "DEMO-2"))
                else:
                    job = self.state(d)
                    del job["models_hash"]
                    (d / "job.json").write_text(json.dumps(job))
                self.prove(d.name, code)
                text = self.proofs_md(d)
                self.assertTrue(text.startswith(first), text)
                if mode == "advisory":
                    self.assertIn("RESULT: FAIL\n", text)
                self.assertIn("models changed after spec", text)
                self.assertNotIn("## MODEL-1", text)
                self.assertEqual(self.stand_in_calls(), [])
                self.assertEqual(len(self.worktrees()), n + 1)

    # ---- PROVE-15 -------------------------------------------------------------------------------

    def test_prove_15_an_invalid_models_section_fails_with_nothing_run(self):
        self.config({"mode": "blocking"})
        cases = {
            "no-section": (None, "spec.md has no `## Models` section"),
            "no-entry": ("Just words.", "has no `### MODEL-<n>` entry"),
            "outside": (model(file="core/models/demo.qnt"), "MODEL-1: File has an invalid value"),
            "parent": (model(file="docs/models/../demo.qnt"), "MODEL-1: File has an invalid value"),
            "absolute": (model(file=f"/{MODEL}"), "MODEL-1: File has an invalid value"),
            "not-qnt": (model(file="docs/models/demo.tla"), "MODEL-1: File has an invalid value"),
            "not-in-commit": (model(file="docs/models/other.qnt"),
                              "MODEL-1: File `docs/models/other.qnt` isn't in the commit"),
            "main": (model(main="demo-dev"), "MODEL-1: Main has an invalid value"),
            "invariant": (model(invariants=(("holds Inv", "DEMO-1", "violatedWit"),)),
                          "MODEL-1: Invariant has an invalid value"),
            "witness": (model(invariants=(("holdsInv", "DEMO-1", "1violated"),)),
                        "MODEL-1: Invariant has an invalid value"),
            "scenario": (model(invariants=(("holdsInv", "demo-1", "violatedWit"),)),
                         "MODEL-1: Invariant has an invalid value"),
            "shape": (model() + " and more", "MODEL-1: Invariant has an invalid value"),
            "depth-0": (model(depth="0"), "MODEL-1: Depth has an invalid value"),
            "depth-x": (model(depth="x"), "MODEL-1: Depth has an invalid value"),
            "no-invariant": (model(invariants=()), "MODEL-1: Invariant is missing"),
            "no-main": (model().replace("- Main: `demo_dev`\n", ""), "MODEL-1: Main is missing"),
            "two-files": (model().replace("- Main:", f"- File: `{MODEL}`\n- Main:"), "MODEL-1: File appears 2 times"),
        }
        for n, (name, (section, why)) in enumerate(cases.items(), 1):
            with self.subTest(name=name):
                d = self.model_job(f"{n:03}-a00-99-models-{name}")
                self.set_models(d, section)
                self.prove(d.name, 1)
                text = self.proofs_md(d)
                self.assertTrue(text.startswith("PROOFS: FAIL\n"), text)
                self.assertIn(why, text.split("## Why\n", 1)[1])
                self.assertEqual(len(self.worktrees()), n + 1)
        # A File in the commit that is a link out of docs/models/ is refused in the scratch copy, before any run.
        link = "docs/models/link.qnt"
        d = self.model_job("100-a00-99-models-link", models=model(file=link))
        worktree = Path(self.state(d)["worktree"])
        (worktree / link).symlink_to("../../core/crates/demo/src/lib.rs")
        self.git("add", link, cwd=worktree)
        self.git("commit", "-q", "-m", "link", cwd=worktree)
        head = self.git("rev-parse", "HEAD", cwd=worktree).strip()
        (d / "round-1" / "checks.md").write_text(f"GATE: PASS\nCOMMIT: {head}\nCHECKS: x\n\n## Summary\n")
        self.prove(d.name, 1)
        text = self.proofs_md(d)
        self.assertIn(f"- MODEL-1: `{link}` isn't a regular file inside docs/models/ in the scratch copy\n", text)
        self.assertIn("- Typecheck: not run: ", text)
        self.assertEqual(len(self.worktrees()), len(cases) + 2)
        self.assertEqual(self.stand_in_calls(), [])

    # ---- PROVE-16 -------------------------------------------------------------------------------

    def model_calls(self):
        return [call["argv"][1:] for call in self.stand_in_calls()]

    def test_prove_16_a_model_whose_invariants_hold_and_witnesses_are_violated_passes(self):
        models = model(depth="7", invariants=(("holdsA", "DEMO-1", "violatedA"), ("holdsB", "DEMO-2", "violatedB")))
        d = self.model_job("001-a00-99-model-pass", models=models)
        worktree = Path(self.state(d)["worktree"])
        before = (worktree / MODEL).read_text()
        res = self.prove(d.name, 0)
        text = self.proofs_md(d)
        self.assertTrue(text.startswith(f"PROOFS: PASS\nCOMMIT: {self.head}\nMODE: advisory\n\n"
                                        f"## MODEL-1 {MODEL} demo_dev\n"), text)
        self.assertNotIn("## Why", text)
        self.assertIn("PROOFS: PASS (001-a00-99-model-pass round 1)", res.stdout)
        options = ["--main=demo_dev", "--max-steps=7"]
        self.assertEqual(self.model_calls(), [
            ["quint", "typecheck", MODEL],
            ["quint", "run", *options, MODEL],
            ["quint", "verify", *options, "--invariant=holdsA", MODEL],
            ["quint", "verify", *options, "--invariant=holdsB", MODEL],
            ["quint", "verify", *options, "--invariant=violatedA", MODEL],
            ["quint", "verify", *options, "--invariant=violatedB", MODEL],
        ])
        calls = self.stand_in_calls()
        for call in calls:
            self.assertEqual(call["cwd"], calls[0]["cwd"])
            self.assertFalse(call["cwd"].endswith("/core"), call)
            self.assertFalse(call["cwd"].startswith(str(worktree)), call)
            self.assertEqual(call["target"], call["cwd"] + "/core/target")
        self.assertIn("- Depth: 7\n", text)
        self.assertIn(f"`{self.repo / 'factory' / 'bin' / 'test-run'} -- quint …`, from the root of a scratch copy",
                      text)
        self.assertRegex(text, rf"- Typecheck: OK \([0-9]+\.[0-9]s\): `quint typecheck {MODEL}`\n")
        self.assertRegex(text, rf"- Run: OK \([0-9]+\.[0-9]s\): `quint run --main=demo_dev --max-steps=7 {MODEL}`\n")
        self.assertRegex(text, r"- Invariant `holdsA` \(DEMO-1\): HOLDS \([0-9]+\.[0-9]s\): `quint verify "
                               r"--main=demo_dev --max-steps=7 --invariant=holdsA ")
        self.assertRegex(text, r"- Invariant `holdsB` \(DEMO-2\): HOLDS \(")
        self.assertRegex(text, r"- Witness `violatedA` of `holdsA`: VIOLATED \([0-9]+\.[0-9]s\): `quint verify "
                               r"--main=demo_dev --max-steps=7 --invariant=violatedA ")
        self.assertRegex(text, r"- Witness `violatedB` of `holdsB`: VIOLATED \(")
        log = (d / "round-1" / "proofs.log").read_text()
        for label in ("typecheck", "run", "invariant holdsA", "invariant holdsB", "witness violatedA",
                      "witness violatedB"):
            self.assertIn(f"==== MODEL-1 {label}: quint ", log)
        self.assertIn("[violation] Found an issue", log)
        self.assertEqual((worktree / MODEL).read_text(), before)
        self.assertEqual(self.git("status", "--porcelain", cwd=worktree), "")
        self.assertEqual(len(self.worktrees()), 2)
        self.assertFalse(Path(calls[0]["cwd"]).exists())

    def test_prove_16_the_depth_is_12_when_left_out(self):
        d = self.model_job("001-a00-99-model-depth")
        self.prove(d.name, 0)
        calls = self.model_calls()
        self.assertEqual(len(calls), 4)
        for call in calls[1:]:
            self.assertIn("--max-steps=12", call)
        self.assertIn("- Depth: 12\n", self.proofs_md(d))

    # ---- PROVE-17 -------------------------------------------------------------------------------

    def test_prove_17_a_witness_that_holds_is_vacuous(self):
        vacuous = model(invariants=(("holdsInv", "DEMO-1", "holdsWit"),))
        for n, (proofs, code, first) in enumerate(((None, 0, "PROOFS: ADVISORY-FAIL"),
                                                   ({"mode": "blocking"}, 1, "PROOFS: FAIL")), 1):
            with self.subTest(proofs=proofs):
                self.config(proofs)
                d = self.model_job(f"00{n}-a00-99-model-vacuous", models=vacuous)
                self.prove(d.name, code)
                text = self.proofs_md(d)
                self.assertTrue(text.startswith(first + "\n"), text)
                why = text.split("## Why\n", 1)[1].split("\n## ", 1)[0]
                self.assertIn("MODEL-1 witness `holdsWit` of `holdsInv`: vacuous", why)
                self.assertIn("- Witness `holdsWit` of `holdsInv`: HOLDS (", text)

    def test_prove_17_a_violated_invariant_fails(self):
        self.config({"mode": "blocking"})
        d = self.model_job("001-a00-99-model-violated",
                           models=model(invariants=(("violatedInv", "DEMO-1", "violatedWit"),)))
        self.prove(d.name, 1)
        text = self.proofs_md(d)
        self.assertTrue(text.startswith("PROOFS: FAIL\n"), text)
        self.assertIn("- MODEL-1 `violatedInv` (DEMO-1): VIOLATED\n", text)
        self.assertIn("- Invariant `violatedInv` (DEMO-1): VIOLATED (", text)

    # ---- PROVE-18 -------------------------------------------------------------------------------

    def test_prove_18_a_run_with_no_verdict_is_never_a_pass_or_a_violation(self):
        self.config({"mode": "blocking"})
        cases = (("silent", "exit 0"), ("badexit", "exit 1"), ("missing", "exit 1"), ("fallback", "exit 127"))
        for n, (kind, code) in enumerate(cases, 1):
            with self.subTest(kind=kind):
                d = self.model_job(f"00{n}-a00-99-model-{kind}", models=model(
                    invariants=((f"{kind}Inv", "DEMO-1", "violatedWit"), ("holdsInv", "DEMO-2", f"{kind}Wit"))))
                self.prove(d.name, 1)
                text = self.proofs_md(d)
                self.assertTrue(text.startswith("PROOFS: FAIL\n"), text)
                why = text.split("## Why\n", 1)[1].split("\n## ", 1)[0]
                self.assertIn(f"- MODEL-1 `{kind}Inv` (DEMO-1): NO VERDICT ({code})\n", why)
                self.assertIn(f"- MODEL-1 witness `{kind}Wit` of `holdsInv`: NO VERDICT ({code})\n", why)
                self.assertIn(f"- Invariant `{kind}Inv` (DEMO-1): NO VERDICT ({code}) (", text)
                self.assertIn(f"- Witness `{kind}Wit` of `holdsInv`: NO VERDICT ({code}) (", text)

    def test_prove_18_a_failed_typecheck_runs_nothing_else(self):
        self.config({"mode": "blocking"})
        d = self.model_job("001-a00-99-model-typecheck")
        self.prove(d.name, 1, STAND_IN="typecheck-fails")
        text = self.proofs_md(d)
        self.assertTrue(text.startswith("PROOFS: FAIL\n"), text)
        self.assertIn("- MODEL-1 typecheck: FAILED (exit 1)\n", text)
        self.assertRegex(text, r"- Typecheck: FAILED \(exit 1\) \([0-9]+\.[0-9]s\)")
        self.assertIn("- Run: not run: the typecheck was FAILED (exit 1) (-)", text)
        self.assertIn("- Invariant `holdsInv` (DEMO-1): not run: the typecheck was FAILED (exit 1) (-)", text)
        self.assertIn("- Witness `violatedWit` of `holdsInv`: not run: the typecheck was FAILED (exit 1) (-)", text)
        self.assertEqual(self.model_calls(), [["quint", "typecheck", MODEL]])

    # ---- PROVE-19 -------------------------------------------------------------------------------

    def test_prove_19_a_model_run_past_its_timeout_is_timeout(self):
        for n, (mode, code, head) in enumerate((("blocking", 1, "PROOFS: TIMEOUT\n"),
                                                ("advisory", 0, "PROOFS: ADVISORY-FAIL\n")), 1):
            with self.subTest(mode=mode):
                self.config({"mode": mode, "timeout_s": 1})
                self.calls.unlink(missing_ok=True)
                d = self.model_job(f"00{n}-a00-99-model-timeout")
                self.prove(d.name, code, STAND_IN="sleep")
                text = self.proofs_md(d)
                self.assertTrue(text.startswith(head), text)
                if mode == "advisory":
                    self.assertIn("RESULT: TIMEOUT\n", text)
                self.assertIn("- MODEL-1 typecheck: TIMEOUT after 1s\n", text)
                self.assertIn("- Typecheck: TIMEOUT (", text)
                calls = self.stand_in_calls()
                self.assertEqual(len(calls), 1)
                with self.assertRaises(ProcessLookupError):
                    os.kill(calls[0]["pid"], 0)
                self.assertEqual(len(self.worktrees()), n + 1)
                self.assertFalse(Path(calls[0]["cwd"]).exists())

    def test_prove_19_a_stop_during_a_model_run_removes_the_scratch_copy(self):
        d = self.model_job("001-a00-99-model-stopped")
        proc = subprocess.Popen([str(self.repo / "factory" / "bin" / "prove"), d.name], cwd=self.repo,
                                env={**self.env, "STAND_IN": "sleep"}, stdout=subprocess.PIPE,
                                stderr=subprocess.PIPE, text=True)
        try:
            deadline = time.monotonic() + 30
            while not self.stand_in_calls() and time.monotonic() < deadline:
                time.sleep(0.05)
            calls = self.stand_in_calls()
            self.assertEqual(len(calls), 1)
            self.assertEqual(calls[0]["argv"][1:3], ["quint", "typecheck"])
            proc.send_signal(signal.SIGTERM)
            self.assertEqual(proc.wait(timeout=30), 128 + signal.SIGTERM)
        finally:
            proc.kill()
            proc.communicate()
        with self.assertRaises(ProcessLookupError):
            os.kill(calls[0]["pid"], 0)
        self.assertFalse((d / "round-1" / "proofs.md").exists())
        self.assertEqual(len(self.worktrees()), 2)
        self.assertFalse(Path(calls[0]["cwd"]).exists())

    # ---- PROVE-20 -------------------------------------------------------------------------------

    def test_prove_20_a_job_with_both_surfaces_gets_one_verdict_the_worse(self):
        both = ("proofs", "model")
        violated = model(invariants=(("violatedInv", "DEMO-1", "violatedWit"),))
        cases = (("kani-pass-model-fail", violated, {}, "FAIL"),
                 ("kani-timeout-model-pass", model(), {"STAND_IN": "kani-sleep"}, "TIMEOUT"),
                 ("both-pass", model(), {}, "PASS"))
        for n, (name, models, env, verdict) in enumerate(cases, 1):
            with self.subTest(name=name):
                self.config({"mode": "blocking", "timeout_s": 1 if env else 30})
                d = self.model_job(f"00{n}-a00-99-{name}", models=models, surfaces=both)
                self.prove(d.name, 0 if verdict == "PASS" else 1, **env)
                text = self.proofs_md(d)
                self.assertTrue(text.startswith(f"PROOFS: {verdict}\n"), text)
                self.assertEqual(text.count("\n## PROOF-1 "), 1, text)
                self.assertEqual(text.count("\n## MODEL-1 "), 1, text)
                self.assertLess(text.index("\n## PROOF-1 "), text.index("\n## MODEL-1 "))
                self.assertEqual(len(self.worktrees()), n + 1)
        # A `## Models` section in a job without the model surface is ignored.
        d = self.model_job("004-a00-99-models-ignored", models="Just words.", surfaces=("proofs",))
        self.prove(d.name, 0)
        self.assertTrue(self.proofs_md(d).startswith("PROOFS: PASS\n"), self.proofs_md(d))
        self.assertNotIn("MODEL", self.proofs_md(d))


if __name__ == "__main__":
    unittest.main(verbosity=2)
