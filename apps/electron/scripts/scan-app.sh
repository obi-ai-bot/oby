#!/bin/zsh
# The app's files scanned for secrets before they leave this machine, for Apple's notary service
# too (docs/contracts/release.md, RELEASE-3): package.sh runs it on each architecture's app before
# notarizing it and stops on anything but a clean scan, so nothing is notarized or uploaded.
# gitleaks reads text: a secret inside a Mach-O binary goes unseen.
# Usage: scripts/scan-app.sh <gitleaks> <app> <config> <report>: gitleaks' redacted findings go to
# <report>.json, its own words to <report>.log.
set -euo pipefail

gitleaks=$1 app=$2 config=$3 report=$4
scan=0
"$gitleaks" dir "$app" --config "$config" --no-banner --redact --exit-code=99 \
  --report-format=json --report-path="$report.json" > "$report.log" 2>&1 || scan=$?
case $scan in
  0) ;;
  99) echo "error: possible secret in ${app:t}: $report.json says where (redacted)" >&2; exit 1 ;;
  *) tail -n 20 "$report.log" >&2; echo "error: gitleaks couldn't scan ${app:t}: nothing is notarized unscanned" >&2; exit 1 ;;
esac
