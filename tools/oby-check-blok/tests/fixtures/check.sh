#!/bin/bash
set -eu
# The tool runs the script under it, never itself again (the script execs it only without this).
[[ "${OBY_CHECK_INNER:-}" == 1 ]] || { echo 'oby-check: OBY_CHECK_INNER is unset'; exit 3; }
if [[ -n "${OBY_CHECK_PLAN:-}" ]]; then
    echo "oby-check: group $OBY_CHECK_ONLY"
    exit 0
fi
case "$OBY_CHECK_ONLY" in
    rust)
        echo "couldn't judge: tests/sandbox.rs makes a sandbox of its own and can't run in a job's"
        exit 1 ;;
    electron)
        echo ' FAIL  src/example.test.ts > fails'
        exit 1 ;;
    scan) echo 'oby-check: passed' ;;
esac
