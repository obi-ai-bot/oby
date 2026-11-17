#!/bin/bash
# Which of obid's Kani harnesses a change requires, by path, not by anyone's choice (docs/proofs.md):
# the changed files on stdin, one a line, from the repository's root; one `cargo kani --harness`
# filter a line on stdout, or `all`.
#
#   - core/crates/obid/src/proofs/<name>.rs proves the files on its `//! Proves:` lines: a change
#     to any of them, or to the proof file, requires `proofs::<name>::`.
#   - A changed core/crates/obid/src/<a>/<b>.rs holding a `#[kani::proof]` requires its own,
#     `<a>::<b>::proofs::` (`<a>::proofs::` for a mod.rs).
#   - What every harness is built from (obid's Cargo.toml or build.rs, the workspace's Cargo.toml
#     or Cargo.lock, rust-toolchain, src/lib.rs, src/proofs.rs) or this script requires `all`.
set -euo pipefail
cd "$(dirname "$0")/.."
src=core/crates/obid/src
out=()
while IFS= read -r file; do
    [[ -n "$file" ]] || continue
    case "$file" in
        core/Cargo.toml | core/Cargo.lock | core/crates/obid/Cargo.toml | core/crates/obid/build.rs | \
            rust-toolchain* | "$src/lib.rs" | "$src/proofs.rs" | scripts/kani-required.sh)
            echo all
            exit 0 ;;
    esac
    for proof in "$src"/proofs/*.rs; do
        [[ -f "$proof" ]] || continue
        if [[ "$file" == "$proof" ]] || grep -qxF "//! Proves: $file" "$proof"; then
            name=${proof##*/}
            out+=("proofs::${name%.rs}::")
        fi
    done
    case "$file" in
        "$src"/proofs/*) ;;
        "$src"/*.rs)
            if [[ -f "$file" ]] && grep -qF '#[kani::proof]' "$file"; then
                module=${file#"$src"/}
                module=${module%.rs}
                module=${module%/mod}
                out+=("${module//\//::}::proofs::")
            fi ;;
    esac
done
[[ ${#out[@]} -eq 0 ]] || printf '%s\n' "${out[@]}" | sort -u
