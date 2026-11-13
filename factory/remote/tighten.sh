# shellcheck shell=bash
# What factory/remote/setup.sh runs last on the gate VM (docs/contracts/remote-gate.md, REMOTE-11): it
# leaves nothing on the check's path writable by group or others. setup.sh sends this file to the host
# ahead of its own function; reading it only defines tighten and what tighten uses, and runs nothing.
#
#   tighten <home> <root> <gate> <oby-check.sh> <scratch>
#
# It removes group and other write from every entry but a link under <home>/.cargo and <home>/.rustup,
# and from <root> and its four slots (not what is in them); <gate> is installed root:root 0755 by setup's
# first step, so it is only checked. Then it fails, naming each path, when any of them is still writable
# by others or a binary is missing, and when <oby-check.sh>'s own rule refuses <home>/.cargo/bin's
# oby-check-blok. Last, it prints their modes.

# The entries but links under <home>/.cargo and <home>/.rustup, and <root> with its slots (not inside
# them), that group or others may write, each ended by a NUL.
tighten_writable() {
    local home=$1 root=$2 path deep=()
    for path in "$home/.cargo" "$home/.rustup"; do
        if [[ -e "$path" ]]; then deep+=("$path"); fi
    done
    if ((${#deep[@]})); then
        find "${deep[@]}" ! -type l \( -perm -020 -o -perm -002 \) -print0
    fi
    find "$root" "$root/slot-1" "$root/slot-2" "$root/slot-3" "$root/slot-4" -prune \
        ! -type l \( -perm -020 -o -perm -002 \) -print0
}

tighten() {
    local home=$1 root=$2 gate=$3 script=$4 scratch=$5
    local path found=0 changed=0 bad=0 output status modes=()

    while IFS= read -r -d '' path; do
        found=$((found + 1))
        # One that fails is left to the check below.
        if chmod go-w "$path"; then changed=$((changed + 1)); fi
    done < <(tighten_writable "$home" "$root")
    if ((changed)); then
        printf 'setup.sh: %s\n' "removed group and other write from $changed paths"
    elif ((!found)); then
        printf 'setup.sh: %s\n' "modes already right"
    fi

    # oby-check-blok-nodes is the check's own rule's to name, below.
    for path in "$home/.cargo/bin/oby-check-blok" "$gate"; do
        if [[ ! -e "$path" ]]; then
            printf 'setup.sh: %s\n' "missing: $path"
            bad=1
        fi
    done
    while IFS= read -r -d '' path; do
        printf 'setup.sh: %s\n' "still writable by others: $path"
        bad=1
    done < <(
        tighten_writable "$home" "$root"
        if [[ -e "$gate" ]]; then find "$gate" -prune ! -type l \( -perm -020 -o -perm -002 \) -print0; fi
    )
    ((!bad)) || return 1

    # scripts/oby-check.sh's own vetting of oby-check-blok, run rather than copied, so the rule is
    # written once: unset, the OBY_CHECK_* variables would skip it. setup runs origin/dev's copy from a
    # folder that isn't a git checkout, so a vetted `oby-check-blok check` stops at `checkout has no
    # HEAD` (exit 2) and runs no check. Only the refusal line or exit 1 counts as a refusal.
    if output=$(env -u OBY_CHECK_INNER -u OBY_CHECK_PLAN -u OBY_CHECK_ONLY -u OBY_CHECK_PATHS -u OBY_CHECK_ALL \
        PATH="$home/.cargo/bin:$PATH" TMPDIR="$scratch" bash "$script" 2>&1); then
        status=0
    else
        status=$?
    fi
    if ((status == 1)) || grep -q '^oby-check: failed: oby-check-blok (' <<<"$output"; then
        printf '%s\n' "$output"
        printf 'setup.sh: %s\n' "oby-check-blok fails scripts/oby-check.sh's own vetting"
        return 1
    fi
    printf 'setup.sh: %s\n' "oby-check-blok passes scripts/oby-check.sh's own vetting"

    printf 'setup.sh: %s\n' "modes:"
    modes=("$home/.cargo" "$home/.cargo/bin" "$home/.cargo/bin/oby-check-blok" "$home/.cargo/bin/oby-check-blok-nodes")
    for path in "$home/.rustup" "$home/.rustup/toolchains"; do
        if [[ -e "$path" ]]; then modes+=("$path"); fi
    done
    ls -ld "${modes[@]}" "$gate" "$root" "$root"/slot-*
}
