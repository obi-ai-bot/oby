#!/bin/bash
# The nightly full Mac check of dev, with or without the factory (docs/contracts/ci.md, CI-53 to 55):
# every suite of scripts/oby-check.sh (OBY_CHECK_ALL=1) on origin/dev as merged, on this Mac.
#
#   scripts/oby-nightly.sh            run the check if it's due: "oby-nightly: PASS|FAIL …" or "oby-nightly: not due (…)"
#   scripts/oby-nightly.sh --now      run it whether or not it's due (never beside another run)
#   scripts/oby-nightly.sh --install  have launchd run it every day at 03:30, this Mac's time
#   scripts/oby-nightly.sh --uninstall
#
# A run is due when no other run holds the results folder's lock and the newest result started more
# than 20 hours ago, or there is none. It fetches origin/dev, checks it out detached in the results
# folder's own worktree, installs the Electron app's modules there (npm ci), runs the check with its
# own Cargo target folder (kept, so the next night builds only what changed), writes <stamp>.md and
# <stamp>.log, and removes the worktree, after a pass, a failure or a stop alike. A stopped run
# writes no .md, so it is due again. It never creates, deletes or checks out a branch, never pushes,
# and writes nothing in the checkout it was run from, factory/ included.
#
# The results folder is OBY_NIGHTLY_DIR, by default ~/Library/Logs/Oby/nightly: <stamp>.md (the UTC
# start, YYYY-MM-DDTHHMMZ) says RESULT, COMMIT, STARTED, SECONDS, the load average as it started and
# ended, and the log's last 80 lines; `latest.md` links to the newest. A failure also shows a macOS
# notification. Exit 0 on PASS or not due, 1 on FAIL, 2 when it couldn't run.
set -eo pipefail

here="$(cd "$(dirname "$0")/.." && pwd)"
repo="${OBY_NIGHTLY_REPO:-$here}"
results="${OBY_NIGHTLY_DIR:-$HOME/Library/Logs/Oby/nightly}"
base="${OBY_NIGHTLY_BASE:-dev}"
label=dev.oby.nightly
plist="$HOME/Library/LaunchAgents/$label.plist"
every=$((20 * 3600))
tail_lines=80

case "${1:-}" in
--install)
    mkdir -p "$(dirname "$plist")" "$results"
    # launchd's PATH is bare: the run gets the one it was installed from (cargo, npm, gitleaks,
    # oby-check-blok).
    cat >"$plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>$label</string>
  <key>ProgramArguments</key><array><string>$here/scripts/oby-nightly.sh</string></array>
  <key>EnvironmentVariables</key><dict><key>PATH</key><string>$PATH</string></dict>
  <key>StartCalendarInterval</key><dict><key>Hour</key><integer>3</integer><key>Minute</key><integer>30</integer></dict>
  <key>LowPriorityIO</key><true/>
  <key>StandardOutPath</key><string>$results/launchd.log</string>
  <key>StandardErrorPath</key><string>$results/launchd.log</string>
</dict>
</plist>
PLIST
    launchctl bootout "gui/$UID/$label" 2>/dev/null || true
    launchctl bootstrap "gui/$UID" "$plist"
    echo "oby-nightly: installed: every day at 03:30, results in $results"
    exit 0
    ;;
--uninstall)
    launchctl bootout "gui/$UID/$label" 2>/dev/null || true
    rm -f "$plist"
    echo "oby-nightly: uninstalled"
    exit 0
    ;;
--now | '') ;;
*)
    echo "oby-nightly: unknown argument: $1" >&2
    exit 2
    ;;
esac

mkdir -p "$results"
lock="$results/.lock"
if ! mkdir "$lock" 2>/dev/null; then
    # A lock whose run is gone (a crash, a reboot) is taken over.
    held=$(cat "$lock/pid" 2>/dev/null || true)
    if [[ -n "$held" ]] && kill -0 "$held" 2>/dev/null; then
        echo "oby-nightly: not due (another run holds $lock)"
        exit 0
    fi
    rm -rf "$lock"
    mkdir "$lock"
fi
echo $$ >"$lock/pid"
worktree="$results/worktree"
clear_worktree() {
    git -C "$repo" worktree remove --force "$worktree" >/dev/null 2>&1 || true
    rm -rf "$worktree"
    git -C "$repo" worktree prune >/dev/null 2>&1 || true
}
trap 'clear_worktree; rm -rf "$lock"' EXIT
trap 'exit 143' TERM
trap 'exit 130' INT

newest=$(find "$results" -maxdepth 1 -name '????-??-??T????Z.md' | sort | tail -1)
if [[ "${1:-}" != --now && -n "$newest" ]]; then
    stamp=${newest##*/}
    stamp=${stamp%.md}
    started=$(date -j -u -f '%Y-%m-%dT%H%MZ' "$stamp" +%s 2>/dev/null || date -u -d "${stamp:0:10} ${stamp:11:2}:${stamp:13:2}" +%s)
    if (($(date +%s) - started < every)); then
        echo "oby-nightly: not due (the newest result, $stamp, is under 20 hours old)"
        exit 0
    fi
fi

load() { { sysctl -n vm.loadavg 2>/dev/null || cat /proc/loadavg; } | tr -d '{}' | awk '{ print $1 }'; }
stamp=$(date -u +%Y-%m-%dT%H%MZ)
log="$results/$stamp.log"
began=$(date +%s)
load_began=$(load)
clear_worktree
if ! git -C "$repo" fetch --quiet origin "$base" 2>"$log" ||
    ! commit=$(git -C "$repo" rev-parse "origin/$base^{commit}" 2>>"$log") ||
    ! git -C "$repo" worktree add --quiet --detach "$worktree" "$commit" 2>>"$log"; then
    echo "oby-nightly: couldn't check out origin/$base: $(tail -1 "$log")" >&2
    rm -f "$log"
    exit 2
fi

code=0
(
    cd "$worktree"
    echo "oby-nightly: origin/$base at $commit, started $stamp, a load average of $load_began"
    npm --prefix apps/electron ci --no-audit --no-fund --prefer-offline
    CARGO_TARGET_DIR="$results/target" OBY_CHECK_ALL=1 scripts/oby-check.sh "$commit"
) >>"$log" 2>&1 || code=$?
# Stopped (TERM, INT): no result, so the next run is due.
((code != 143 && code != 130)) || exit "$code"
seconds=$(($(date +%s) - began))
result=PASS
((code == 0)) || result=FAIL
{
    echo "RESULT: $result"
    echo "COMMIT: $commit"
    echo "STARTED: $stamp"
    echo "SECONDS: $seconds"
    echo "LOAD: $load_began as it started, $(load) as it ended"
    echo
    echo "## scripts/oby-check.sh"
    echo "OBY_CHECK_ALL=1 on origin/$base, exit $code in ${seconds}s. The last $tail_lines lines of $stamp.log:"
    echo
    echo '```'
    tail -n "$tail_lines" "$log"
    echo '```'
} >"$results/$stamp.md"
ln -sf "$stamp.md" "$results/latest.md"
echo "oby-nightly: $result (${commit:0:7}, exit $code in ${seconds}s): $results/$stamp.md"
if [[ $result == FAIL && -z "${OBY_NIGHTLY_QUIET:-}" ]] && command -v osascript >/dev/null; then
    osascript -e "display notification \"The full check failed on $base at ${commit:0:7}: $results/$stamp.md\" with title \"Oby's nightly check\"" >/dev/null 2>&1 || true
fi
((code == 0)) || exit 1
