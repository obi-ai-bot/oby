#!/usr/bin/env python3
"""factory/bin/oby-feed's own tests (docs/contracts/factory.md, FEEDMODEL-1 to 4), with no gh, ssh, Quint or network.

check_models() runs with subprocess.run stubbed: `gh api` returns a model whose first line is a `// verify:` line,
the `ssh … cat >` and `ssh … rm -f` calls succeed, and each `ssh … quint verify … --invariant=<name> …` returns the
output and exit code the test gives that name, as Quint 0.33.1 printed them on the gate VM. Every output that isn't
empty starts with a stand-in for the VM's protobuf warning, a line that holds the word "error" without starting with
it; an empty one stands for a run that printed nothing at all (a timeout, a dropped connection).

Run from the repository root: python3 -m unittest discover -s factory/tests -p 'test_oby_feed.py' -v
"""

import importlib.machinery
import importlib.util
import re
import subprocess
import unittest
from pathlib import Path
from unittest import mock

FACTORY = Path(__file__).resolve().parents[1]
_loader = importlib.machinery.SourceFileLoader("oby_feed", str(FACTORY / "bin" / "oby-feed"))
feed = importlib.util.module_from_spec(importlib.util.spec_from_loader("oby_feed", _loader))
_loader.exec_module(feed)

PATH = "docs/models/oby_factory.qnt"
HEAD = "0123456789abcdef0123456789abcdef01234567"
WARNING = "[libprotobuf WARNING] a stand-in for the VM's warning, which mentions an error mid-line"
HOLDS = ("The outcome is: NoError\n[ok] No violation found (12ms).\n", 0)
VIOLATED = ("The outcome is: Error\n[violation] Found an issue (12ms).\nerror: found a counterexample\n", 1)
VERIFY = re.compile(r"\bquint verify\b")


class CheckModels(unittest.TestCase):
    def check(self, line, runs):
        """check_models() over one model whose first line is `line`; `runs` maps a check's name to its output and
        exit code. Returns check_models()'s result and the `quint verify` commands it sent."""
        verifies = []

        def run(cmd, **_):
            if cmd[0] == "gh":
                return subprocess.CompletedProcess(cmd, 0, f"{line}\nmodule oby_factory {{}}\n", "")
            remote = cmd[-1]
            if VERIFY.search(remote):
                verifies.append(remote)
                out, code = runs[re.search(r"--invariant=(\w+)", remote).group(1)]
                return subprocess.CompletedProcess(cmd, code, f"{WARNING}\n{out}" if out else "", "")
            return subprocess.CompletedProcess(cmd, 0, "", "")

        config = dict(feed.job.CONFIG, remote_gate={"ssh_host": "oby-gate"})
        with mock.patch.object(feed.subprocess, "run", side_effect=run), mock.patch.object(feed.job, "CONFIG", config):
            return feed.check_models([PATH], HEAD), verifies

    def assert_own_ports(self, verifies):
        """Every `quint verify` carries --server-endpoint right after `verify`, on a port of its own above 8886."""
        ports = []
        for remote in verifies:
            m = re.search(r"\bquint verify --server-endpoint=localhost:(\d+) ", remote)
            self.assertIsNotNone(m, remote)
            ports.append(int(m.group(1)))
        self.assertEqual(len(ports), len(set(ports)), ports)
        self.assertTrue(all(port > 8886 for port in ports), ports)

    def assert_no_verdict(self, got, check, name, main, detail):
        self.assertIsNotNone(got)
        self.assertTrue(got.startswith(f"{PATH}: the {check} {name} (main={main}) gave no verdict: "), got)
        self.assertIn(detail, got)
        self.assertNotIn("does not hold", got)
        self.assertNotIn("was not violated", got)

    def test_feedmodel_1_holding_invariant_and_violated_witness_pass_on_ports_of_their_own(self):
        got, verifies = self.check("// verify: main=oby_factory steps=4 invariants=safe witnesses=neverMerges",
                                   {"safe": HOLDS, "neverMerges": VIOLATED})
        self.assertIsNone(got)
        self.assertEqual(len(verifies), 2)
        self.assert_own_ports(verifies)

    def test_feedmodel_2_misspelt_witness_gives_no_verdict_naming_the_error_line(self):
        out = ("error: [QNT404] Name 'neverMergesByItslf' not found\nerror: name resolution failed\n", 1)
        got, verifies = self.check("// verify: main=oby_factory steps=4 witnesses=neverMergesByItslf",
                                   {"neverMergesByItslf": out})
        self.assert_no_verdict(got, "witness", "neverMergesByItslf", "oby_factory",
                               "error: [QNT404] Name 'neverMergesByItslf' not found")
        self.assertIn("docs/models/oby_factory.qnt", got)
        self.assert_own_ports(verifies)

    def test_feedmodel_2_misspelt_main_gives_no_verdict_on_an_invariant(self):
        got, verifies = self.check("// verify: main=oby_factry steps=4 invariants=safe",
                                   {"safe": ("error: module oby_factry does not exist\n", 1)})
        self.assert_no_verdict(got, "invariant", "safe", "oby_factry", "error: module oby_factry does not exist")
        self.assert_own_ports(verifies)

    def test_feedmodel_3_port_clash_timeout_and_dropped_connection_give_no_verdict(self):
        cases = (
            (("Failed to bind to address 0.0.0.0:8822: Address already in use\n", 1), "Address already in use"),
            (("", 124), "it timed out after 900s"),
            (("", 255), "it printed nothing (exit 255)"),
        )
        for run, detail in cases:
            for kind, check in (("invariants", "invariant"), ("witnesses", "witness")):
                with self.subTest(detail=detail, check=check):
                    got, verifies = self.check(f"// verify: main=oby_factory steps=4 {kind}=broken", {"broken": run})
                    self.assert_no_verdict(got, check, "broken", "oby_factory", detail)
                    self.assert_own_ports(verifies)

    def test_feedmodel_4_holding_witness_and_violated_invariant_keep_todays_words(self):
        got, verifies = self.check("// verify: main=oby_factory steps=4 witnesses=neverFails",
                                   {"neverFails": ("[ok] No violation found (12ms).\n", 0)})
        self.assertEqual(got, f"{PATH}: the witness neverFails was not violated, so the model may prove nothing")
        self.assert_own_ports(verifies)
        got, verifies = self.check("// verify: main=oby_factory steps=4 invariants=safe",
                                   {"safe": ("[violation] Found an issue (12ms).\nerror: found a counterexample\n", 1)})
        self.assertEqual(got, f"{PATH}: the invariant safe does not hold")
        self.assert_own_ports(verifies)


if __name__ == "__main__":
    unittest.main()
