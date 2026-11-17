#!/bin/zsh
# Puts today's dev on this Mac: Oby and its engine, obid, built from one commit of origin/dev (or
# the ref you give), packaged as "Oby", installed where the installed app is, and restarted, as
# docs/build-and-run.md, "Today's dev on this Mac", says. Then it checks obid answers and prints
# the commit both were built from, which Settings › About shows too.
#
#   scripts/run-dev.sh [REF]                  today's origin/dev, or REF, over the installed Oby
#   scripts/run-dev.sh [REF] --prefix DIR --home DIR --port N
#                                             a scratch install beside it: DIR/Oby.app and an obid
#                                             of its own on port N with that home, the real one untouched
#   scripts/run-dev.sh --stop --prefix DIR --home DIR
#                                             stops that scratch install's app and obid
#
# Its steps, in order:
#   1. a clean checkout of the commit: a detached worktree of this repository in ~/.oby-dev/src
#      (OBY_DEV_SRC), forced to the commit, with nothing untracked left in it; cargo builds into
#      ~/.oby-dev/target (OBY_CARGO_TARGET_DIR) so the next run builds only what changed;
#   2. obid (release, for this Mac's architecture), gh and gitleaks, staged and signed by the app's
#      own apps/electron/scripts/helpers.sh, obid told the commit;
#   3. the app (electron-vite), told the commit too, packaged by electron-builder from
#      apps/electron/electron-builder.yml: Oby.app, bundle ID com.deskree.oby.electron, obid inside
#      it at Contents/Helpers/obid with its launch agent's plist, as every install has it. Signed
#      with the Developer ID of OBY_TEAM_ID when this Mac has it, else this Mac's Apple Development
#      identity, else ad hoc (OBY_SIGN_IDENTITY names another). Not notarized and not scanned for
#      secrets: it never leaves this Mac (apps/electron/scripts/package.sh makes releases);
#   4. installed as /Applications/Oby.app, Oby quit first. An "Oby Electron.app" from the first
#      releases goes: its app quit, its launch agent booted out, its folder removed, so this Mac
#      has one Oby and it's named Oby (the owner's word, 2026-11-17). UPDATES-10 keeps a release's
#      updates in that old folder; a build from here replaces it;
#   5. obid restarted: the app's launch agent (com.deskree.oby.electron.obid) kicked onto the new
#      Contents/Helpers/obid when launchd has it; an obid you started by hand on 7777 stopped, so
#      the app registers and starts its own; then Oby opened;
#   6. obid's GET /v1/status read with its token (from its home's api-token, never an argument):
#      it must answer, built from the same commit.
# Run it from any checkout of this repository; it needs Xcode's command line tools, Rust, Node.
set -euo pipefail

say() { print -r -- "==> $*"; }
die() { print -r -- "run-dev: $*" >&2; exit 1; }

# zsh reads a script as it runs it, and step 1 rewrites ~/.oby-dev/src: it runs from a copy, with
# the repository it was started from
if [[ -z "${OBY_RUN_DEV_REPO:-}" ]]; then
  repo=$(git -C "${0:A:h}" rev-parse --show-toplevel) || die "run it from a checkout of Oby's repository"
  copy=$(mktemp -t run-dev)
  cp "${0:A}" "$copy"
  OBY_RUN_DEV_REPO=$repo exec /bin/zsh "$copy" "$@"
fi
repo=$OBY_RUN_DEV_REPO
rm -f "${0:A}"

ref=origin/dev prefix="" home="" port="" stop=0
while (( $# )); do
  case $1 in
  --prefix) prefix=${2:?--prefix needs a folder}; shift 2 ;;
  --home) home=${2:?--home needs a folder}; shift 2 ;;
  --port) port=${2:?--port needs a number}; shift 2 ;;
  --stop) stop=1; shift ;;
  -h | --help) sed -n '2,32p' "$repo/scripts/run-dev.sh"; exit 0 ;;
  -*) die "unknown option $1 (scripts/run-dev.sh --help)" ;;
  *) ref=$1; shift ;;
  esac
done
[[ $(uname -s) == Darwin ]] || die "this is for a Mac"

# A scratch install: all three, its own port, never /Applications
scratch=0
if [[ -n "$prefix$home$port" ]]; then
  [[ -n "$prefix" && -n "$home" && ( -n "$port" || $stop == 1 ) ]] || die "a scratch install takes --prefix, --home and --port together"
  [[ "$prefix" == /* && "$home" == /* ]] || die "--prefix and --home have to be absolute folders"
  [[ "${prefix:A}" != /Applications ]] || die "a scratch install never goes in /Applications: that's the installed Oby's"
  scratch=1
fi
(( stop == 0 || scratch == 1 )) || die "--stop is for a scratch install: give its --prefix and --home"
if (( scratch )) && (( ! stop )); then
  [[ "$port" == <1024-65535> && "$port" != 7777 ]] || die "--port has to be a free port other than 7777, not $port"
fi
port=${port:-7777}
uid=$(id -u)
agent=com.deskree.oby.electron.obid

# The pids of this user's listening on 127.0.0.1:$port
listening() { /usr/sbin/lsof -nP -a "-iTCP@127.0.0.1:$port" -sTCP:LISTEN -t 2> /dev/null || true; }
# Waits for a pid to go, up to $2 seconds
gone() { local i; for i in {1..$(( $2 * 2 ))}; do kill -0 "$1" 2> /dev/null || return 0; sleep 0.5; done; return 1; }
# Stops the app at $1 (only processes running its own executable), up to 20 seconds
quit_app() {
  local pids=(${(f)"$(pgrep -f "^$1/Contents/MacOS/" || true)"})
  (( ${#pids} )) || return 0
  say "Quitting ${1:t} (pid ${pids[1]})"
  if (( ! scratch )); then osascript -e 'tell application id "com.deskree.oby.electron" to quit' > /dev/null 2>&1 || true; fi
  gone "${pids[1]}" 10 || kill "${pids[@]}" 2> /dev/null || true
  gone "${pids[1]}" 10 || die "${1:t} didn't quit: quit it and run this again"
}

scratch_pid=${home:h}/obid.pid
stop_scratch() {
  quit_app "$prefix/Oby.app"
  local pid=$(cat "$scratch_pid" 2> /dev/null || true)
  # only the obid this script started, from this scratch install
  if [[ -n "$pid" ]] && [[ "$(ps -o command= -p "$pid" 2> /dev/null)" == "$prefix/Oby.app/Contents/Helpers/obid serve"* ]]; then
    say "Stopping the scratch obid (pid $pid)"
    kill "$pid"; gone "$pid" 15 || die "the scratch obid (pid $pid) didn't stop"
  fi
  rm -f "$scratch_pid"
}
if (( stop )); then
  stop_scratch
  # LaunchServices forgets the scratch copy, so nothing opens it for the installed Oby's bundle ID
  [[ -d "$prefix/Oby.app" ]] && /System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -u "$prefix/Oby.app" 2> /dev/null || true
  say "Stopped. Its files stay in $prefix and ${home:h}: remove them when you're done."
  exit 0
fi

# 1. The commit, in a clean checkout
say "Fetching origin"
git -C "$repo" fetch --quiet origin
sha=$(git -C "$repo" rev-parse --verify "$ref^{commit}") || die "$ref isn't a commit here"
src=${OBY_DEV_SRC:-$HOME/.oby-dev/src}
if [[ ! -e "$src/.git" ]]; then
  say "A checkout of ${sha:0:9} in $src"
  mkdir -p "${src:h}"
  git -C "$repo" worktree add --quiet --detach "$src" "$sha"
else
  say "Checking out ${sha:0:9} in $src"
  [[ "$(git -C "$src" rev-parse --show-toplevel)" == "${src:A}" ]] || die "$src isn't a checkout of its own: move it, or set OBY_DEV_SRC"
  git -C "$src" checkout --quiet --detach --force "$sha"
  # nothing untracked; ignored build outputs that step 3 makes again go below
  git -C "$src" clean -ffdq
fi
[[ "$(git -C "$src" rev-parse HEAD)" == "$sha" && -z "$(git -C "$src" status --porcelain)" ]] || die "$src isn't a clean checkout of $sha"
app_dir=$src/apps/electron
rm -rf "$app_dir/out" "$app_dir/dist"

export OBY_BUILD_COMMIT=$sha
export OBY_CARGO_TARGET_DIR=${OBY_CARGO_TARGET_DIR:-$HOME/.oby-dev/target}
case $(uname -m) in
arm64) arch=arm64 target=aarch64-apple-darwin ;;
x86_64) arch=x64 target=x86_64-apple-darwin ;;
*) die "$(uname -m) isn't a Mac architecture Oby builds for" ;;
esac

# The identity it's signed with: its SHA-1 for helpers.sh, its name for electron-builder
team=${OBY_TEAM_ID:-3UV9W5SV7W}
identities=$(security find-identity -v -p codesigning 2> /dev/null || true)
# (each lookup may find nothing: || true, or pipefail would end the script here)
if [[ -n "${OBY_SIGN_IDENTITY:-}" ]]; then
  line=$(print -r -- "$identities" | grep -F "\"$OBY_SIGN_IDENTITY" | head -n 1 || true)
  [[ -n "$line" ]] || die "no signing identity named $OBY_SIGN_IDENTITY in the keychain"
else
  line=$(print -r -- "$identities" | grep "Developer ID Application" | grep "($team)" | head -n 1 || true)
  [[ -n "$line" ]] || line=$(print -r -- "$identities" | grep '"Apple Development' | head -n 1 || true)
fi
if [[ -n "$line" ]]; then
  sign_hash=$(print -r -- "$line" | awk '{print $2}')
  sign_name=$(print -r -- "$line" | sed 's/^[^"]*"\(.*\)"$/\1/')
else
  sign_hash=- sign_name=-
fi
say "Signing with ${sign_name/#-/an ad hoc signature}"

# 2. obid, gh and gitleaks
cd "$app_dir"
if [[ "$(cat node_modules/.oby-lock 2> /dev/null)" != "$(shasum -a 256 package-lock.json)" ]]; then
  say "Installing the app's packages (npm ci)"
  npm ci --no-audit --no-fund --loglevel=error
  shasum -a 256 package-lock.json > node_modules/.oby-lock
fi
# every time, not --if-stale: obid is told this commit even when core/ didn't change (cargo then
# builds only obid's own crate again)
say "Building obid for $target, with gh and gitleaks"
OBY_HELPERS_IDENTITY=$sign_hash OBY_OBID_TARGETS=$target zsh scripts/helpers.sh

# 3. The app, packaged as Oby
if (( scratch )); then
  export OBY_SCRATCH_PORT=$port OBY_SCRATCH_HOME=$home
fi
say "Building the app"
npm run build --silent
[[ -f native/window-style/build/Release/window_style.node ]] || npm run build:native --silent
say "Packaging Oby.app"
mkdir -p dist
signing=(-c.mac.identity="$sign_name")
[[ "$sign_name" == - ]] || signing=(-c.mac.identity="${sign_name#*: }")
CSC_IDENTITY_AUTO_DISCOVERY=false npx electron-builder --mac dir "--$arch" --publish never "${signing[@]}" > dist/electron-builder.log 2>&1 \
  || { tail -n 30 dist/electron-builder.log; die "electron-builder failed: $app_dir/dist/electron-builder.log"; }
built=(dist/mac*/Oby.app(N))
(( ${#built} == 1 )) || die "electron-builder made no Oby.app: $app_dir/dist/electron-builder.log"
built=${built[1]:A}
codesign --verify --deep --strict "$built" || die "Oby.app's signature doesn't verify"
[[ "$(defaults read "$built/Contents/Info.plist" CFBundleIdentifier)" == com.deskree.oby.electron ]] || die "Oby.app isn't com.deskree.oby.electron"

# 4. Installed
if (( scratch )); then
  dest=$prefix/Oby.app
  mkdir -p "$prefix" "$home"
  chmod 700 "${home:h}" "$home"
  stop_scratch
else
  dest=/Applications/Oby.app
  old="/Applications/Oby Electron.app"
  pgrep -f "electron-vite dev" > /dev/null && say "Oby also runs from npm run dev: quit it (Ctrl-C in its terminal), it shares this Oby's folder"
  quit_app "$dest"
  if [[ -d "$old" ]]; then
    # the first releases' folder: one Oby on this Mac, named Oby. Its engine is stopped with it;
    # the new Oby registers its own agent from its own folder when it opens (step 5)
    quit_app "$old"
    if launchctl print "gui/$uid/$agent" > /dev/null 2>&1; then
      say "Stopping ${old:t}'s obid ($agent)"
      launchctl bootout "gui/$uid/$agent" 2> /dev/null || true
    fi
    say "Removing $old: Oby is Oby.app from now on"
    rm -rf "$old"
  fi
fi
say "Installing $dest"
rm -rf "$dest.new"
ditto "$built" "$dest.new"
rm -rf "$dest"
mv "$dest.new" "$dest"

# 5. obid restarted, then Oby opened
if (( scratch )); then
  config=${home:h}/config.json
  if [[ ! -f "$config" ]]; then
    # its own port and home, Claude Code's default login, no gateway (the real one has 8810)
    print -r -- "{\"listen\": \"127.0.0.1:$port\", \"home\": \"$home\", \"owner\": \"$(id -un)\",
 \"agents\": {\"claude\": {\"bin\": \"claude\", \"accounts\": [{\"id\": \"default\", \"name\": \"Default\", \"configDir\": null}]}}}" > "$config"
    chmod 600 "$config"
  fi
  [[ -z "$(listening)" ]] || die "something else listens on 127.0.0.1:$port: pick another --port"
  say "Starting the scratch obid on 127.0.0.1:$port"
  nohup "$dest/Contents/Helpers/obid" serve --config "$config" >> "${home:h}/obid.log" 2>&1 < /dev/null &!
  print $! > "$scratch_pid"
else
  if launchctl print "gui/$uid/$agent" > /dev/null 2>&1; then
    say "Restarting obid ($agent) on the new build"
    launchctl kickstart -k "gui/$uid/$agent"
  elif launchctl print "gui/$uid/com.deskree.obid" > /dev/null 2>&1; then
    say "install.sh's obid (com.deskree.obid) runs here: Oby uses it, and this script doesn't update it (sh install/install.sh uninstall removes it)"
  else
    # an obid started by hand: stopped, so Oby starts its own
    for pid in $(listening); do
      [[ "$(ps -o comm= -p "$pid")" == */obid ]] || die "something other than obid listens on 127.0.0.1:7777 (pid $pid): quit it and run this again"
      say "Stopping the obid you started by hand (pid $pid, $(ps -o comm= -p "$pid"))"
      kill "$pid"; gone "$pid" 15 || die "that obid (pid $pid) didn't stop"
    done
  fi
fi
say "Opening ${dest:t}"
if (( scratch )); then open -n "$dest"; else open "$dest"; fi

# 6. obid answers, built from the same commit
token=${home:-$HOME/Library/Application Support/Oby/Runner}/api-token
say "Waiting for obid on 127.0.0.1:$port"
answer=""
for i in {1..240}; do
  if [[ -r "$token" ]]; then
    # the token goes in a header read from a pipe, never an argument
    answer=$(curl -q -fsS --max-time 2 -H @<(printf 'Authorization: Bearer %s\n' "$(<"$token")") "http://127.0.0.1:$port/v1/status" 2> /dev/null) && break
  fi
  answer=""
  sleep 0.5
done
if [[ -z "$answer" ]]; then
  (( scratch )) && die "obid doesn't answer on 127.0.0.1:$port: ${home:h}/obid.log says why"
  die "obid doesn't answer on 127.0.0.1:$port. If Oby asks, turn it on in System Settings › General › Login Items; ~/Library/Logs/obid.log says why"
fi
engine=$(print -r -- "$answer" | node -e 'let s="";process.stdin.on("data",d=>s+=d).on("end",()=>console.log(JSON.parse(s).runner?.commit??""))')
say "obid answers on 127.0.0.1:$port"
print -r -- "Oby.app built from: $sha"
print -r -- "obid built from:    ${engine:-(it doesn't say: an obid older than this script)}"
[[ "$engine" == "$sha" ]] || die "obid on 127.0.0.1:$port isn't the one just built"
print -r -- "Settings › About shows: $(node -p "require('./package.json').version") (${sha:0:7})"
