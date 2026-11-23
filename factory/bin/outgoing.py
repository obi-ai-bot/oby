"""The copy of a change the factory's secret scans read before it leaves this Mac.

Shared by factory/bin/test-run, which imports it from beside itself, and factory/bin/job, which loads it from
beside itself only when it scans. as_text() is `git diff --text --no-ext-diff --no-textconv --full-index`: every
changed file as text, whatever .gitattributes (`-diff`, `binary`, `diff=<driver>`), an external diff or a textconv
say, so none of them can hide a line from gitleaks. It runs git and nothing else.
"""

import subprocess


class Failed(Exception):
    """git couldn't write the copy: its standard error."""


def as_text(cwd, old, new):
    """The change from old to new in cwd, as bytes; Failed when git fails."""
    res = subprocess.run(["git", "diff", "--text", "--no-ext-diff", "--no-textconv", "--full-index", old, new],
                         cwd=cwd, capture_output=True)
    if res.returncode != 0:
        raise Failed(res.stderr.decode(errors="replace").strip())
    return res.stdout
