"""A change as git stores it: the copy the factory's secret scans read before it leaves this Mac, and the payload
factory/bin/test-run sends.

Shared by factory/bin/test-run, which imports it from beside itself, and factory/bin/job, which loads it from
beside itself only when it scans. Both are `git --no-replace-objects diff --no-ext-diff --no-textconv --no-color
--full-index`, so no external diff, textconv, colour setting or replace ref changes them, and the payload holds
nothing the scan didn't read. as_text() adds `--text`: every changed file as text, whatever .gitattributes (`-diff`,
`binary`, `diff=<driver>`) say. payload() adds `--binary`, which applies byte for byte. It runs git and nothing else.
"""

import subprocess


class Failed(Exception):
    """git couldn't write the change: its standard error."""


def as_text(cwd, old, new):
    """The change from old to new in cwd, as text, in bytes; Failed when git fails."""
    return diff(cwd, old, new, "--text")


def payload(cwd, old, new):
    """The change from old to new in cwd, as a binary patch, in bytes; Failed when git fails."""
    return diff(cwd, old, new, "--binary")


def diff(cwd, old, new, form):
    res = subprocess.run(["git", "--no-replace-objects", "diff", form, "--no-ext-diff", "--no-textconv", "--no-color",
                          "--full-index", old, new], cwd=cwd, capture_output=True)
    if res.returncode != 0:
        raise Failed(res.stderr.decode(errors="replace").strip())
    return res.stdout
