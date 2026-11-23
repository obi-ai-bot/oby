#!/usr/bin/env python3
"""`job advance <job> gate` closes again a task that merging dev brought back (A00-99).

Run from the repository root: python3 factory/tests/test_task_close.py
Each test builds a throwaway bare `origin` and a job worktree cloned from it, beside copies of factory/bin/job and
its modules and the real factory/config.json and .gitleaks.toml. The job is in `build` with a PR already, so the
advance calls no `gh`; a push runs the real gitleaks. dev adds factory/queue/tasks/A00-16.md after the branch began,
and the branch merges dev, as a builder does.
"""

import hashlib
import json
import os
import shlex
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

FACTORY = Path(__file__).resolve().parents[1]
JOB_ID = "001-a00-16-something"
KEY = "A00-16"
TODO = f"factory/queue/tasks/{KEY}.md"
DONE = f"factory/queue/done/{KEY}.md"
TASK = f"KEY: {KEY}\nTITLE: fix: something\nDEPENDS: none\n\nThe task as the orchestrator queued it.\n"
EARLIER = f"KEY: {KEY}\nTITLE: fix: something\nDEPENDS: none\n\nThe task as the builder copied it.\n"
# A git that fails any call holding --text, as only the secret scan's copy of the change has it, and runs the real one
# otherwise.
STAND_IN_GIT = """#!/bin/sh
for arg in "$@"; do
  if [ "$arg" = --text ]; then echo "the stand-in git refuses --text" >&2; exit 1; fi
done
exec {git} "$@"
"""


class TaskClose(unittest.TestCase):
    def setUp(self):
        self.dir = Path(tempfile.mkdtemp()).resolve()
        self.repo = self.dir / "repo"
        (self.repo / "factory" / "bin").mkdir(parents=True)
        self.job = self.repo / "factory" / "bin" / "job"
        shutil.copy2(FACTORY / "bin" / "job", self.job)
        for module in (FACTORY / "bin").glob("*.py"):
            shutil.copy2(module, self.job.with_name(module.name))
        shutil.copy2(FACTORY / "config.json", self.repo / "factory" / "config.json")
        shutil.copy2(FACTORY.parent / ".gitleaks.toml", self.repo / ".gitleaks.toml")
        self.wt = self.dir / "worktree"
        self.branch = f"factory/{JOB_ID}"
        self.job_dir = self.repo / "factory" / "jobs" / JOB_ID
        (self.job_dir / "round-1").mkdir(parents=True)
        (self.job_dir / "job.json").write_text(json.dumps({
            "id": JOB_ID, "issue": KEY, "title": "fix: something", "milestone": "A00", "sprint": 0,
            "fixes": [], "pr": 7, "pr_url": "https://github.com/o/r/pull/7", "branch": self.branch,
            "worktree": str(self.wt), "stage": "build", "round": 1, "round_limit": 3,
            "intake": None, "agents": None, "gate": None, "reviews": {},
            "cost_usd": 0, "started": "2026-01-01T00:00:00Z", "ended": None,
        }, indent=2) + "\n")
        (self.job_dir / "build.md").write_text("STATUS: READY\n")
        (self.dir / "claude").mkdir()
        self.env = {
            **{k: v for k, v in os.environ.items() if not k.startswith("GIT_")},
            "GIT_CONFIG_GLOBAL": "/dev/null", "GIT_CONFIG_NOSYSTEM": "1",
            "GIT_AUTHOR_NAME": "test", "GIT_AUTHOR_EMAIL": "test@example.com",
            "GIT_COMMITTER_NAME": "test", "GIT_COMMITTER_EMAIL": "test@example.com",
            "CLAUDE_CONFIG_DIR": str(self.dir / "claude"),
        }
        origin = self.dir / "origin.git"
        self.git(self.dir, "init", "-q", "--bare", "-b", "dev", str(origin))
        self.git(self.dir, "clone", "-q", str(origin), str(self.wt))
        self.git(self.wt, "checkout", "-q", "-b", "dev")
        self.write("README.md", "dev\n")
        self.commit("chore: start")
        self.git(self.wt, "push", "-q", "origin", "dev")
        self.git(self.wt, "checkout", "-q", "-b", self.branch)

    def tearDown(self):
        shutil.rmtree(self.dir)

    def git(self, cwd, *args):
        return subprocess.run(["git", *args], cwd=cwd, env=self.env, capture_output=True, text=True,
                              check=True).stdout

    def write(self, path, text):
        (self.wt / path).parent.mkdir(parents=True, exist_ok=True)
        (self.wt / path).write_text(text)

    def commit(self, subject):
        self.git(self.wt, "add", "-A")
        self.git(self.wt, "commit", "-q", "-m", subject)

    def dev_queues_the_task_and_the_branch_merges_it(self, task=TASK):
        """dev adds tasks/<KEY>.md after the branch began; the branch merges dev and pushes."""
        self.git(self.wt, "push", "-q", "origin", self.branch)
        self.git(self.wt, "checkout", "-q", "dev")
        self.write(TODO, task)
        self.commit(f"chore(factory): queue {KEY}")
        self.git(self.wt, "push", "-q", "origin", "dev")
        self.git(self.wt, "checkout", "-q", self.branch)
        self.git(self.wt, "fetch", "-q", "origin")
        self.git(self.wt, "merge", "-q", "--no-edit", "origin/dev")

    def push(self):
        self.git(self.wt, "push", "-q", "origin", self.branch)

    def advance(self, env=None):
        return subprocess.run([str(self.job), "advance", JOB_ID, "gate"], cwd=self.repo, env=env or self.env,
                              capture_output=True, text=True, timeout=60)

    def in_head(self, path):
        return subprocess.run(["git", "cat-file", "-e", f"HEAD:{path}"], cwd=self.wt, env=self.env).returncode == 0

    def stage(self):
        return json.loads((self.job_dir / "job.json").read_text())["stage"]

    def test_taskclose_1_a_task_merging_dev_brought_back_is_closed_again(self):
        self.write("code.txt", "the change\n")
        self.write(DONE, EARLIER)
        self.commit("fix: something")
        self.dev_queues_the_task_and_the_branch_merges_it()
        self.push()
        self.assertTrue(self.in_head(TODO) and self.in_head(DONE))
        before = self.git(self.wt, "rev-parse", "HEAD").strip()

        res = self.advance()

        self.assertEqual(res.returncode, 0, res.stdout + res.stderr)
        self.assertEqual(self.stage(), "gate")
        self.assertEqual(self.git(self.wt, "rev-list", "--count", f"{before}..HEAD").strip(), "1")
        self.assertEqual(self.git(self.wt, "log", "-1", "--format=%s").strip(),
                         f"chore(factory): close {KEY} again after merging dev")
        self.assertFalse(self.in_head(TODO))
        self.assertEqual(self.git(self.wt, "show", f"HEAD:{DONE}"), TASK)
        self.assertEqual(self.git(self.wt, "ls-remote", "origin", self.branch).split()[0],
                         self.git(self.wt, "rev-parse", "HEAD").strip())
        self.assertEqual(self.git(self.wt, "status", "--porcelain"), "")

    def test_taskclose_1_a_done_copy_the_branch_made_a_symlink_is_replaced_and_its_target_left_alone(self):
        target = self.dir / "outside.txt"
        target.write_text("not the factory's\n")
        self.write("code.txt", "the change\n")
        (self.wt / DONE).parent.mkdir(parents=True)
        (self.wt / DONE).symlink_to(target)
        self.commit("fix: something")
        self.dev_queues_the_task_and_the_branch_merges_it()
        self.push()

        res = self.advance()

        self.assertEqual(res.returncode, 0, res.stdout + res.stderr)
        self.assertEqual(target.read_text(), "not the factory's\n")
        self.assertFalse((self.wt / DONE).is_symlink())
        self.assertEqual(self.git(self.wt, "ls-tree", "HEAD", DONE).split()[0], "100644")
        self.assertEqual(self.git(self.wt, "show", f"HEAD:{DONE}"), TASK)
        self.assertEqual((self.wt / DONE).read_text(), TASK)
        self.assertFalse(self.in_head(TODO))
        self.assertEqual(self.git(self.wt, "status", "--porcelain"), "")

    def test_taskclose_1_an_unpushed_branch_is_closed_again_here_and_nothing_is_pushed(self):
        self.write("code.txt", "the change\n")
        self.write(DONE, EARLIER)
        self.commit("fix: something")
        self.dev_queues_the_task_and_the_branch_merges_it()  # the merge itself isn't pushed
        on_origin = self.git(self.wt, "ls-remote", "origin", self.branch).split()[0]

        res = self.advance()

        self.assertEqual(res.returncode, 1, res.stdout + res.stderr)
        self.assertIn("isn't pushed", res.stderr)
        self.assertEqual(self.stage(), "build")
        self.assertEqual(self.git(self.wt, "ls-remote", "origin", self.branch).split()[0], on_origin)
        self.assertEqual(self.git(self.wt, "log", "-1", "--format=%s").strip(),
                         f"chore(factory): close {KEY} again after merging dev")
        self.assertFalse(self.in_head(TODO))

    def assert_scan_refused_the_push(self, env, why):
        on_origin = self.git(self.wt, "ls-remote", "origin", self.branch).split()[0]

        res = self.advance(env)

        self.assertEqual(res.returncode, 1, res.stdout + res.stderr)
        self.assertIn(why, res.stderr)
        self.assertIn("nothing was pushed", res.stderr)
        self.assertEqual(self.stage(), "build")
        self.assertEqual(self.git(self.wt, "ls-remote", "origin", self.branch).split()[0], on_origin)
        self.assertEqual(self.git(self.wt, "log", "-1", "--format=%s").strip(),
                         f"chore(factory): close {KEY} again after merging dev")

    def test_taskclose_1_a_secret_in_what_the_re_close_pushes_refuses_the_push(self):
        # A GitHub token made here, so this file holds none: the scan must find it in the re-closed task's text.
        token = "ghp_" + hashlib.sha256(b"taskclose").hexdigest()[:36]
        self.write("code.txt", "the change\n")
        self.write(DONE, EARLIER)
        self.commit("fix: something")
        self.dev_queues_the_task_and_the_branch_merges_it(TASK + f"token = {token}\n")
        self.push()
        self.assert_scan_refused_the_push(self.env, "the secret scan found a secret")

    def test_taskclose_1_a_secret_scan_that_cant_run_refuses_the_push(self):
        self.write("code.txt", "the change\n")
        self.write(DONE, EARLIER)
        self.commit("fix: something")
        self.dev_queues_the_task_and_the_branch_merges_it()
        self.push()
        tools = self.dir / "no-gitleaks"  # git and python3 only
        tools.mkdir()
        (tools / "git").symlink_to(shutil.which("git"))
        (tools / "python3").symlink_to(sys.executable)
        self.assert_scan_refused_the_push({**self.env, "PATH": str(tools)}, "the secret scan couldn't run")

    def assert_attribute_hides_no_secret(self, line, config=()):
        """The secret's set-up, the branch adding this .gitattributes line over the queue's task files, with these
        settings in the repository's git config: the scan still reads the re-closed task and refuses the push."""
        token = "ghp_" + hashlib.sha256(b"taskclose").hexdigest()[:36]
        for key, value in config:
            self.git(self.wt, "config", key, value)
        self.write(".gitattributes", f"factory/queue/*/*.md {line}\n")
        self.write("code.txt", "the change\n")
        self.write(DONE, EARLIER)
        self.commit("fix: something")
        self.dev_queues_the_task_and_the_branch_merges_it(TASK + f"token = {token}\n")
        self.push()
        self.assert_scan_refused_the_push(self.env, "the secret scan found a secret")

    def test_taskclose_1_a_minus_diff_line_hides_no_secret_from_the_re_close_scan(self):
        self.assert_attribute_hides_no_secret("-diff")

    def test_taskclose_1_a_binary_line_hides_no_secret_from_the_re_close_scan(self):
        self.assert_attribute_hides_no_secret("binary")

    def test_taskclose_1_a_textconv_diff_driver_hides_no_secret_from_the_re_close_scan(self):
        self.assert_attribute_hides_no_secret("diff=hide", [("diff.hide.textconv", "true")])

    def test_taskclose_1_an_external_diff_driver_hides_no_secret_from_the_re_close_scan(self):
        self.assert_attribute_hides_no_secret("diff=ext", [("diff.ext.command", "true")])

    def test_taskclose_1_a_clean_filter_hides_no_secret_from_the_re_close_scan(self):
        self.assert_attribute_hides_no_secret("filter=keep", [("filter.keep.clean", "cat")])

    def test_taskclose_1_a_colour_setting_hides_no_secret_from_the_re_close_scan(self):
        # color.ui=always writes a colour code right after an unquoted value, where gitleaks' generic rule wants the
        # line to end. Made here, so this file holds no key.
        digest = hashlib.sha256(b"taskclose-colour").hexdigest()
        key = "".join(chr(ord("a") + int(c, 16)) for c in digest[:8]) + digest[8:32]
        self.git(self.wt, "config", "color.ui", "always")
        self.write("code.txt", "the change\n")
        self.write(DONE, EARLIER)
        self.commit("fix: something")
        self.dev_queues_the_task_and_the_branch_merges_it(TASK + f"api_token = {key}\n")
        self.push()
        self.assert_scan_refused_the_push(self.env, "the secret scan found a secret")

    def test_taskclose_1_a_replacement_object_hides_no_secret_from_the_re_close_scan(self):
        # A replace ref shows a harmless blob in place of the task's, while the push sends the real one.
        token = "ghp_" + hashlib.sha256(b"taskclose").hexdigest()[:36]
        self.write("code.txt", "the change\n")
        self.write(DONE, EARLIER)
        self.commit("fix: something")
        self.dev_queues_the_task_and_the_branch_merges_it(TASK + f"token = {token}\n")
        self.push()
        real = self.git(self.wt, "rev-parse", f"origin/dev:{TODO}").strip()
        harmless = subprocess.run(["git", "hash-object", "-w", "--stdin"], cwd=self.wt, env=self.env, input=TASK,
                                  capture_output=True, text=True, check=True).stdout.strip()
        self.git(self.wt, "replace", real, harmless)
        self.assert_scan_refused_the_push(self.env, "the secret scan found a secret")

    def test_taskclose_1_a_git_that_cant_write_the_scans_copy_refuses_the_push(self):
        self.write("code.txt", "the change\n")
        self.write(DONE, EARLIER)
        self.commit("fix: something")
        self.dev_queues_the_task_and_the_branch_merges_it()
        self.push()
        tools = self.dir / "git-bin"
        tools.mkdir()
        (tools / "git").write_text(STAND_IN_GIT.format(git=shlex.quote(shutil.which("git"))))
        (tools / "git").chmod(0o755)
        self.assert_scan_refused_the_push({**self.env, "PATH": f"{tools}{os.pathsep}{self.env['PATH']}"},
                                          "`git diff` failed")

    def assert_refused_as_today(self):
        self.push()
        before = self.git(self.wt, "rev-parse", "HEAD").strip()

        res = self.advance()

        self.assertEqual(res.returncode, 1, res.stdout + res.stderr)
        self.assertIn("the branch doesn't close its task", res.stderr)
        self.assertEqual(self.stage(), "build")
        self.assertEqual(self.git(self.wt, "rev-parse", "HEAD").strip(), before)
        self.assertTrue(self.in_head(TODO))

    def test_taskclose_2_a_branch_without_a_done_file_is_refused(self):
        self.write("code.txt", "the change\n")
        self.commit("fix: something")
        self.dev_queues_the_task_and_the_branch_merges_it()
        self.assert_refused_as_today()

    def test_taskclose_2_a_branch_whose_task_file_differs_from_the_base_is_refused(self):
        self.write("code.txt", "the change\n")
        self.write(DONE, EARLIER)
        self.commit("fix: something")
        self.dev_queues_the_task_and_the_branch_merges_it()
        self.write(TODO, TASK + "The builder's own line.\n")
        self.commit("docs: change the task")
        self.assert_refused_as_today()


if __name__ == "__main__":
    unittest.main(verbosity=2)
