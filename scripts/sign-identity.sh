#!/bin/sh
# The identity scripts/run-dev.sh signs Oby with, picked from the lines of
# `security find-identity -v -p codesigning` on stdin. Prints its SHA-1, then its name, on two lines;
# `-` twice for ad hoc.
#
#   sign-identity.sh TEAM        the Developer ID of TEAM first, else the first Apple Development one
#   OBY_SIGN_IDENTITY=NAME       the identity whose name starts with NAME instead; `-` is ad hoc
#
# Ad hoc only when OBY_SIGN_IDENTITY=- asks for it (docs/contracts/keychain.md, KEYCHAIN-12). The
# Keychain knows a signed app by its designated requirement, which stays the same from one build to
# the next, so one Always Allow lasts. An ad hoc app it knows only by the hash of its code, which every
# build changes: each one would be a new app, asked for the login password for "oby-app Safe Storage".
set -eu
team=${1:?sign-identity.sh takes the team whose Developer ID comes first}
lines=$(cat)
want=${OBY_SIGN_IDENTITY:-}
if [ "$want" = - ]; then
  printf '%s\n%s\n' - -
  exit 0
fi
if [ -n "$want" ]; then
  line=$(printf '%s\n' "$lines" | grep -F "\"$want" | head -n 1 || true)
  [ -n "$line" ] || { echo "no signing identity named $want in the keychain" >&2; exit 1; }
else
  line=$(printf '%s\n' "$lines" | grep -F 'Developer ID Application' | grep -F "($team)" | head -n 1 || true)
  [ -n "$line" ] || line=$(printf '%s\n' "$lines" | grep -F '"Apple Development' | head -n 1 || true)
  if [ -z "$line" ]; then
    echo "this Mac has no Developer ID ($team) or Apple Development identity to sign Oby with. Signed ad hoc, every build would be a new app to the Keychain, and macOS would ask for your login password for \"oby-app Safe Storage\" after each one. Add an Apple Development certificate (Xcode › Settings › Accounts › Manage Certificates), or run again with OBY_SIGN_IDENTITY=- to build it ad hoc anyway." >&2
    exit 1
  fi
fi
printf '%s\n' "$line" | awk '{print $2}'
printf '%s\n' "$line" | sed 's/^[^"]*"\(.*\)"$/\1/'
