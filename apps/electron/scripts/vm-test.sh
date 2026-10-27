#!/bin/zsh
# The clean-install test of the packaged app, in a throwaway macOS VM, so every release repeats it
# after scripts/package.sh. Tart (github.com/cirruslabs/tart, 2.32 or later) runs it:
#   1. `oby-clean`: Cirrus Labs' vanilla macOS 26 image (SIP on, as on a Mac; their other images
#      turn it off, and there macOS never starts an app's login item), cloned once. Every test
#      starts from it, and nothing is ever saved back into it;
#   2. `oby-test`: a fresh clone of it, 4 GB of memory, no window, the DMG's folder shared read-only,
#      reached over SSH as its logged-in user (admin; a throwaway key added with its password);
#   3. in the VM: the DMG mounted, the app copied into Applications as a drag does, and opened;
#   4. the checks: obid starts by itself and /v1/status answers with obid's token; the window shows
#      Oby's conversation and no engine error (its text, read over Chromium's DevTools port through
#      the SSH connection, and a picture of it in dist/vm-test.png); obid still answers after Oby
#      quits; after a restart of the VM, launchd starts the app's registered obid at login, with
#      Oby closed; and with the window closed, the app's CPU over 60 s;
#   5. `oby-test` deleted, whatever happened (obid.log kept in dist/vm-obid.log).
# Usage: scripts/vm-test.sh [the DMG] (dist/'s newest arm64 DMG by default). Signing an agent in
# inside the VM is the owner's step, by hand, and never in a VM that's kept.
set -euo pipefail

app_dir=${0:A:h:h}
cd "$app_dir"
tart=${TART:-$(command -v tart || echo ~/Applications/Tart/tart.app/Contents/MacOS/tart)}
image=${OBY_VM_IMAGE:-ghcr.io/cirruslabs/macos-tahoe-vanilla:latest}
name=$(sed -n 's/^productName: //p' electron-builder.yml)
dmg=${1:-$(ls -t dist/*-arm64.dmg | head -n 1)}
[[ -f "$dmg" ]] || { echo "error: no DMG: run scripts/package.sh arm64 first" >&2; exit 1; }
dmg=${dmg:A}
work=$(mktemp -d)
ip=""
die() { echo "error: $*" >&2; exit 1; }
say() { echo "==> $*"; }
ssh_to=(-q -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o ConnectTimeout=5 -i "$work/key")
in_vm() { ssh "${ssh_to[@]}" "admin@$ip" "$1"; }
page() { node scripts/cdp.mjs 9339 "$@"; }

cleanup() {
  [[ -n "$ip" ]] && in_vm 'cat ~/Library/Logs/obid.log' > dist/vm-obid.log 2> /dev/null || true
  [[ -n "${tunnel:-}" ]] && kill $tunnel 2> /dev/null
  "$tart" stop oby-test > /dev/null 2>&1 || true
  "$tart" delete oby-test > /dev/null 2>&1 || true
  rm -rf "$work"
}
trap cleanup EXIT

# Boots oby-test and waits for its SSH, adding the key the first time
boot() {
  "$tart" run oby-test --no-graphics --dir="dist:${dmg:h}:ro" >> "$work/run.log" 2>&1 &
  ip=$("$tart" ip oby-test --wait 180) || { cat "$work/run.log"; die "the VM didn't start"; }
  for _ in {1..90}; do nc -z "$ip" 22 2> /dev/null && break; sleep 2; done
  [[ -f "$work/key" ]] && { in_vm true || die "the VM's SSH doesn't answer"; return; }
  ssh-keygen -q -t ed25519 -N '' -f "$work/key"
  expect > /dev/null <<EOF || die "couldn't log in to the VM over SSH"
set timeout 60
spawn ssh -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o PubkeyAuthentication=no admin@$ip "mkdir -p ~/.ssh && echo '$(cat "$work/key.pub")' >> ~/.ssh/authorized_keys"
expect "assword:" { send "admin\r" }
expect eof
EOF
  in_vm true || die "the VM's SSH doesn't take the key"
}

running() { in_vm "launchctl print gui/\$(id -u)/$1 2>/dev/null | grep -q 'state = running'"; }
token_status() { in_vm "curl -sf -H \"Authorization: Bearer \$(cat ~/Library/Application\\ Support/Oby/Runner/api-token)\" http://127.0.0.1:7777/v1/status"; }

if ! "$tart" list --quiet | grep -qx oby-clean; then
  say "Making oby-clean from $image (once)"
  "$tart" clone "$image" oby-clean
fi
"$tart" delete oby-test > /dev/null 2>&1 || true
"$tart" clone oby-clean oby-test
"$tart" set oby-test --memory 4096
say "Starting the VM"
boot

say "Installing ${dmg:t}"
in_vm "mnt=\$(hdiutil attach -nobrowse -noautoopen -readonly '/Volumes/My Shared Files/dist/${dmg:t}' | awk -F'\t' '/\/Volumes\//{print \$NF}')
  ditto \"\$mnt/$name.app\" '/Applications/$name.app' && hdiutil detach \"\$mnt\" -quiet"
in_vm "lsof -nP -iTCP:7777 -sTCP:LISTEN" > /dev/null 2>&1 && die "something already listens on 7777 in the clean VM"

say "Opening $name"
in_vm "open -a '/Applications/$name.app' --args --remote-debugging-port=9333"
answered=false
for _ in {1..120}; do token_status > "$work/status.json" 2> /dev/null && { answered=true; break; }; sleep 1; done
$answered || die "obid doesn't answer /v1/status"
echo "    /v1/status: $(head -c 100 "$work/status.json")…"
for label in com.deskree.oby.electron.obid com.deskree.oby.electron.obid.session; do
  running $label && echo "    launchd runs $label"
done

ssh "${ssh_to[@]}" -N -L 9339:127.0.0.1:9333 "admin@$ip" &
tunnel=$!
text=""
for _ in {1..30}; do
  text=$(page 'document.body.innerText' 2> /dev/null) && [[ -n "$text" && "$text" != *starting* ]] && break
  sleep 2
done
page --screenshot dist/vm-test.png 2> /dev/null || true
echo "    the window says: $(print -r -- "$text" | tr '\n' ' ' | head -c 240)"
[[ -n "$text" ]] || die "the window shows nothing"
for bad in 'engine' 'Nothing answers' 'No token' 'isn’t running'; do
  [[ "$text" != *$bad* ]] || die "the window shows an engine problem: $text"
done

say "Quitting $name: obid keeps running"
in_vm "osascript -e 'tell application id \"com.deskree.oby.electron\" to quit'
  for _ in {1..30}; do pgrep -x '$name' > /dev/null || break; sleep 1; done"
token_status > /dev/null || die "obid stopped with the app"
echo "    /v1/status still answers"

say "Restarting the VM: launchd starts obid at login, Oby closed"
kill $tunnel 2> /dev/null; tunnel=""
# as the Apple menu does, through loginwindow (shutdown -h leaves it reopening apps that quit)
in_vm "osascript -e 'tell application \"loginwindow\" to «event aevtrsdn»'" || true
for _ in {1..60}; do "$tart" list | grep -q 'oby-test.*running' || break; sleep 2; done
boot
answered=false
for _ in {1..120}; do token_status > /dev/null 2>&1 && { answered=true; break; }; sleep 2; done
$answered || die "obid doesn't start at login"
running com.deskree.oby.electron.obid || die "obid answers, but not as the app's registered agent"
# pgrep -x: by name, since this SSH command's own command line holds the app's path
in_vm "pgrep -x '$name'" > /dev/null && die "Oby itself opened at login"
echo "    launchd runs com.deskree.oby.electron.obid, /v1/status answers"

say "Idle with the window closed, 60 s"
ssh "${ssh_to[@]}" -N -L 9339:127.0.0.1:9333 "admin@$ip" &
tunnel=$!
in_vm "open -a '/Applications/$name.app' --args --remote-debugging-port=9333"
for _ in {1..30}; do page 'window.close(), 1' > /dev/null 2>&1 && break; sleep 1; done
sleep 3
cpu="ps -axo time=,command= | grep -E '/Applications/$name.app/Contents/(MacOS/$name|Frameworks/)' | grep -v grep | awk '{split(\$1,t,\":\"); s+=t[1]*60+t[2]} END {printf \"%.2f\", s}'"
a=$(in_vm "$cpu"); sleep 60; b=$(in_vm "$cpu")
pct=$(( (b - a) * 100.0 / 60 ))
printf '    %.2f CPU-seconds in 60 s: %.2f%% of one core\n' $(( b - a )) $pct
(( pct < 1 )) || die "the app isn't idle with its window closed"

say "Passed"
