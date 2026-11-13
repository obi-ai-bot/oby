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

# One Cargo build folder for every job's worktree: a folder each is about 15 GB.
# Cargo locks it only while it builds, so two checks at once would still clash:
# one job's build replaces the obid binary another job's tests are running
# (R02-01's gate saw it). So the whole check holds a lock on the folder, below.
#
# Cargo also gives a workspace crate the same build record in every worktree and
# rebuilds it only when a source file is newer than the last build. So a worktree
# whose files are older than another branch's last build would be tested against
# that branch's obid (R01-06's gate ran R02-06's). Inside the lock, before the
# check, every tracked file under core/ and tools/ gets the current time, so this
# worktree's own code is what gets built. Only the times change; touch -c creates
# no file a branch deleted. Third-party crates stay built.
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$HOME/Library/Caches/oby-factory/core-target}"
mkdir -p "$CARGO_TARGET_DIR"

# A change to the check itself is judged by the branch's own copy here, so say so:
# the approver sends every such change to the owner (.claude/agents/approver.md).
if ! git diff --quiet "$base"...HEAD -- scripts/oby-check.sh scripts/oby-check-test.sh \
        .gitleaks.toml .gitleaksignore tools/oby-check-blok 2>/dev/null; then
    echo "factory/checks.sh: this branch changes the check's own files; its copy of them runs below." >&2
fi

# A change whose plan builds no Rust (docs, factory, cloud, an Electron-only change) needs neither
# the shared Cargo folder nor its lock: it runs at once instead of queueing behind Rust checks.
# A plan that can't be read takes the lock, as before.
if plan=$(OBY_CHECK_PLAN=1 scripts/oby-check.sh "$base" 2>/dev/null) \
        && ! grep -q '^oby-check: group rust$' <<<"$plan"; then
    exec scripts/oby-check.sh "$base"
fi

# One check at a time on this Mac: factory/bin/check-lock holds the lock and runs oby-check.sh in a
# session of its own, stops what it leaves running and passes its exit code through.
exec python3 "$(dirname "${BASH_SOURCE[0]}")/bin/check-lock" "$CARGO_TARGET_DIR/.factory-check.lock" scripts/oby-check.sh "$base"
