#!/usr/bin/env bash
# The repo's checks. Exit 0 only when the branch is fit to review.
#
# The gate runs this script from the main checkout with the job's worktree as
# the working directory. It is non-interactive and rewrites no file.
#
# The check is Oby's own, scripts/oby-check.sh (AGENTS.md, "Build and run"): only
# the suites the branch's change reaches (obid's touched modules and test
# binaries with fmt and clippy, the touched iOSTests suites, swift test for mcp/,
# Electron's typecheck, tests and build), then gitleaks. It ends 0 (passed),
# 1 (failed) or 2 (couldn't judge); only 0 passes the gate.
set -euo pipefail

base="origin/dev"

# The check hands its run to the installed oby-check-blok and fails without it.
if ! command -v oby-check-blok >/dev/null; then
    echo "factory/checks.sh: oby-check-blok isn't installed. From the main checkout run:" >&2
    echo "  cargo install --locked --path tools/oby-check-blok" >&2
    exit 1
fi
if ! command -v gitleaks >/dev/null; then
    echo "factory/checks.sh: gitleaks isn't installed (brew install gitleaks): nothing passes without the secret scan." >&2
    exit 1
fi

# One Cargo build folder for every job's worktree: a folder each is about 15 GB,
# and Cargo's own lock on a shared one runs the jobs' builds one after another.
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$HOME/Library/Caches/oby-factory/core-target}"
mkdir -p "$CARGO_TARGET_DIR"

# A change to the check itself is judged by the branch's own copy here, so say so:
# the approver sends every such change to the owner (.claude/agents/approver.md).
if ! git diff --quiet "$base"...HEAD -- scripts/oby-check.sh scripts/oby-check-test.sh \
        .gitleaks.toml .gitleaksignore tools/oby-check-blok 2>/dev/null; then
    echo "factory/checks.sh: this branch changes the check's own files; its copy of them runs below." >&2
fi

exec scripts/oby-check.sh "$base"
