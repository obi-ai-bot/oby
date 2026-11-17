#!/bin/zsh
# A second, demo Oby beside yours, joined to an Obysphere with an invite link, to show two Obys
# asking each other questions and handing each other tasks (docs/obysphere-drive/demo.md).
#
#   scripts/sphere-demo.sh --repo OWNER/NAME [LINK]
#       starts the demo Oby (its own obid, port and home; its own app window, "Oby · Ben (demo)"),
#       on your own Claude Code and Codex logins, and joins it with LINK, or with the invite link on
#       your clipboard when LINK is left out (better: an argument stays in your shell's history).
#       Then it says what you click to join your own Oby with the same link: that's yours to do.
#       With no link anywhere it starts the demo Oby unjoined; run --join once you have one.
#   scripts/sphere-demo.sh --join [LINK]   joins the running demo Oby (LINK, or the clipboard's)
#   scripts/sphere-demo.sh --status        what runs, and the demo Oby's Obysphere and its members
#   scripts/sphere-demo.sh --stop          leaves the Obysphere (the Sphere told, with the demo's
#                                          own owner key), quits the demo app, stops its obid
#   scripts/sphere-demo.sh --send-task "GOAL"
#       hands the demo Oby a task from YOUR Oby (its obid on 7777, through POST
#       /v1/obid/sphere/requests: no screen sends one yet, #2428): GOAL in the demo's repository, on
#       its default branch, done when its tests pass. Only when you run it, and both Obys have to be
#       in the same Obysphere. --from-port and --from-home name another sender (a rehearsal's).
#
# Options: --name Ben (the window's "Oby · Ben (demo)"), --owner ben-demo (the owner id it joins
# as), --port 7871 (its obid's), --dir DIR (its folder, ~/Library/Application Support/oby-sphere-demo),
# --yes (answers the demo app's own "Join …?" for you), --build (builds the app again), --check CMD
# (the repository's check, e.g. "python3 -m pytest -q": a task's result says its tests pass only
# when the job's check passed).
#
# What it never does: start, stop or change your own Oby or its obid on 7777, join your Oby to
# anything, sign in or out of anything, or write the link anywhere but a 0600 file in the demo's
# folder that the demo app reads and removes at once. The demo Oby's folder stays after --stop: it
# holds the owner key the Obysphere bound to --owner on its first join, and joining again as that
# owner takes the same key.
#
# The engine is your installed Oby's obid (/Applications/Oby.app/Contents/Helpers/obid), so both
# Obys run the same one; OBY_DEMO_OBID names another (core/target/debug/obid of a checkout). The
# app is this checkout's apps/electron, built once (npm ci, npm run build).
set -euo pipefail

say() { print -r -- "==> $*"; }
die() { print -r -- "sphere-demo: $*" >&2; exit 1; }
[[ $(uname -s) == Darwin ]] || die "this is for a Mac"

repo_root=$(git -C "${0:A:h}" rev-parse --show-toplevel) || die "run it from a checkout of Oby's repository"
app_dir=$repo_root/apps/electron
launcher=$repo_root/scripts/sphere-demo/launcher.cjs

from_port=7777 from_home=$HOME/Library/Application\ Support/Oby/Runner goal=""
owner_given=0
action=start link="" repo=${OBY_DEMO_REPO:-} check=${OBY_DEMO_CHECK:-} name=Ben owner=ben-demo port=7871 yes=0 build=0
dir=${OBY_DEMO_DIR:-$HOME/Library/Application Support/oby-sphere-demo}
while (( $# )); do
  case $1 in
  --stop) action=stop; shift ;;
  --status) action=status; shift ;;
  --join) action=join; shift ;;
  --send-task) action=send_task; goal=${2:?--send-task needs the task, in one line}; shift 2 ;;
  --from-port) from_port=${2:?--from-port needs a number}; shift 2 ;;
  --from-home) from_home=${2:?--from-home needs a folder}; shift 2 ;;
  --repo) repo=${2:?--repo needs OWNER/NAME}; shift 2 ;;
  --name) name=${2:?--name needs a name}; shift 2 ;;
  --owner) owner=${2:?--owner needs an owner id}; owner_given=1; shift 2 ;;
  --port) port=${2:?--port needs a number}; shift 2 ;;
  --dir) dir=${2:?--dir needs a folder}; shift 2 ;;
  --yes) yes=1; shift ;;
  --build) build=1; shift ;;
  --check) check=${2:?--check needs a command}; shift 2 ;;
  -h | --help) awk 'NR > 1 && !/^#/ { exit } NR > 1' "${0:A}"; exit 0 ;;
  -*) die "unknown option $1 (scripts/sphere-demo.sh --help)" ;;
  *) link=$1; shift ;;
  esac
done
[[ "$dir" == /* ]] || die "--dir has to be an absolute folder"
[[ "$port" == <1024-65535> && "$port" != 7777 ]] || die "--port has to be a free port other than 7777 (your Oby's), not $port"
[[ "$owner" =~ '^[a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?$' ]] || die "--owner is an owner id: lowercase letters, digits and dashes, like ben-demo"
title="Oby · $name (demo)"
home=$dir/home
# a demo that ran before keeps its owner id (its config), unless --owner says another
if (( ! owner_given )) && [[ -r "$dir/obid/config.json" ]]; then
  owner=$(node -e 'try { process.stdout.write(JSON.parse(require("fs").readFileSync(process.argv[1], "utf8")).owner || "") } catch {}' "$dir/obid/config.json")
  [[ -n "$owner" ]] || owner=ben-demo
fi
token_file=$home/api-token

# --- small helpers ----------------------------------------------------------------------------

# A pid in $1 still running $2 (a command line's start), else nothing
alive() { local pid=$(cat "$1" 2> /dev/null || true); [[ -n "$pid" ]] && [[ "$(ps -o command= -p "$pid" 2> /dev/null)" == "$2"* ]] && print -r -- "$pid"; }
gone() { local i; for i in {1..$(( $2 * 2 ))}; do kill -0 "$1" 2> /dev/null || return 0; sleep 0.5; done; return 1; }
listening() { /usr/sbin/lsof -nP -a "-iTCP@127.0.0.1:$port" -sTCP:LISTEN -t 2> /dev/null || true; }
# An obid's API ($1 its port, $2 its token file), the token never an argument: curl reads it from stdin
call_at() {
  local at=$1 token=$2 method=$3 route=$4 body=${5:-}
  [[ -r "$token" ]] || return 1
  local args=(-s -m 20 -X "$method" "http://127.0.0.1:$at$route")
  [[ -n "$body" ]] && args+=(-H 'content-type: application/json' --data-binary "$body")
  print -r -- "header = \"authorization: Bearer $(< "$token")\"" | curl -K - "${args[@]}"
}
# the demo obid's
obid() { call_at "$port" "$token_file" "$@"; }
# A field of obid's JSON answer on stdin, by a JavaScript path (node, which the app needs anyway)
field() { node -e 'let s="";process.stdin.on("data",d=>s+=d).on("end",()=>{try{const v=eval("("+s+")"+process.argv[1]);console.log(v==null?"":typeof v==="object"?JSON.stringify(v):v)}catch{console.log("")}})' "$1"; }

# The invite link: https://<host>/join#sphere=…&id=…&secret=… (or oby://join/<host>#…; http only on
# this Mac, a rehearsal's local Sphere), from the argument or the clipboard. Never printed.
read_link() {
  local text=${link:-$(pbpaste 2> /dev/null || true)}
  text=${text//[[:space:]]/}
  if [[ "$text" =~ '^(https://[A-Za-z0-9.-]+(:[0-9]+)?|http://(127\.0\.0\.1|localhost)(:[0-9]+)?)/join#.*secret=' ]]; then
    print -r -- "$text"
  elif [[ "$text" =~ '^oby://join/[A-Za-z0-9.-]+(:[0-9]+)?#.*secret=' ]]; then
    print -r -- "https://${${text#oby://join/}%%\#*}/join#${text#*\#}"
  elif [[ -n "$link" ]]; then
    die "that isn't an Obysphere invite link (https://<host>/join#sphere=…&id=…&secret=…)"
  fi
}
# what an https link's join page opens: oby://join/<host>#<the same fragment>
oby_link() { local rest=${1#https://}; print -r -- "oby://join/${rest%%/join\#*}#${1#*\#}"; }
# the link's host and its Sphere's name, to say which (never its secret)
link_host() { local rest=${1#*://}; print -r -- "${rest%%/*}"; }

app_running() { alive "$dir/app.pid" "$app_dir/node_modules/electron/dist/Electron.app/Contents/MacOS/Electron"; }
obid_running() { alive "$dir/obid.pid" "$obid_bin serve --config $dir/obid/config.json"; }
obid_bin=${OBY_DEMO_OBID:-/Applications/Oby.app/Contents/Helpers/obid}

membership() { obid GET /v1/obid/sphere 2> /dev/null || true; }

# Asks the running demo app to do $1 (join, leave) and waits up to $2 seconds for its answer
ask_app() {
  rm -f "$dir/control.out"
  print -r -- "$1" > "$dir/control"
  local i; for i in {1..$(( $2 * 2 ))}; do [[ -s "$dir/control.out" ]] && { cat "$dir/control.out"; return 0; }; sleep 0.5; done
  return 1
}

# --- join ---------------------------------------------------------------------------------------

join_demo() {
  local url=$1 host=$(link_host "$1")
  [[ -n "$(app_running)" ]] || die "the demo Oby isn't running: start it first (scripts/sphere-demo.sh --repo OWNER/NAME)"
  local state=$(membership | field .state)
  if [[ "$state" == joined ]]; then
    say "The demo Oby is in $(membership | field .sphere) already; --stop leaves it"
    return 0
  fi
  local mark=0
  [[ -f "$dir/dialogs.log" ]] && mark=$(wc -l < "$dir/dialogs.log")
  (umask 077; print -r -- "$url" > "$dir/join-link")
  ask_app join 15 > /dev/null || { rm -f "$dir/join-link"; die "the demo app didn't take the link (is its window open?)"; }
  if (( yes )); then say "Joining the demo Oby through $host"
  else say "Joining the demo Oby through $host: click Join in the demo window (\"$title\")"; fi
  local i; for i in {1..360}; do
    [[ "$(membership | field .state)" == joined ]] && break
    local said=$(tail -n +$(( mark + 1 )) "$dir/dialogs.log" 2> /dev/null | grep -E 'couldn’t join|can’t open this link' | tail -n 1 || true)
    [[ -n "$said" ]] && die "the demo Oby couldn't join: $(print -r -- "$said" | field .detail)"
    tail -n +$(( mark + 1 )) "$dir/dialogs.log" 2> /dev/null | grep -q '"answered":"Cancel"' && die "Join was cancelled in the demo window"
    sleep 0.5
  done
  [[ "$(membership | field .state)" == joined ]] || die "the demo Oby didn't join within 3 minutes: was Join clicked in its window?"
  local m=$(membership)
  say "The demo Oby joined $(print -r -- "$m" | field .sphere) as $(print -r -- "$m" | field .owner) ($(print -r -- "$m" | field .obiId))"
}

owner_steps() {
  local url=$1
  print
  print -r -- "Now join your own Oby with the same link (only you do this, in your own Oby):"
  print -r -- "  1. Open the invite link in Safari and click \"Open in Oby\" on its page,"
  if [[ "$url" == https://* ]]; then
    print -rn -- "$(oby_link "$url")" | pbcopy
    print -r -- "     or paste in Terminal:  open \"\$(pbpaste)\"   (the oby:// link is on your clipboard now)"
  fi
  print -r -- "  2. In Oby, click Join in \"Join <the Obysphere>?\"."
  print -r -- "  3. In Oby's side panel the Obysphere shows with $owner's Oby among its members."
  print -r -- "Your Oby must not be in another Obysphere yet (it says so if it is)."
}

# --- the actions --------------------------------------------------------------------------------

case $action in
status)
  print -r -- "demo folder: $dir"
  print -r -- "obid:  $( [[ -n "$(obid_running)" ]] && print "running, pid $(obid_running), 127.0.0.1:$port" || print stopped)"
  print -r -- "app:   $( [[ -n "$(app_running)" ]] && print "running, pid $(app_running)" || print stopped)"
  if [[ -n "$(obid_running)" ]]; then
    m=$(membership)
    print -r -- "Obysphere: $(print -r -- "$m" | field .state) $(print -r -- "$m" | field .sphere) $(print -r -- "$m" | field .url)"
    if [[ "$(print -r -- "$m" | field .state)" == joined ]]; then
      obid GET /v1/obid/sphere/members | field '.map(m=>`  member: ${m.owner}  ${m.obi}  takes ${m.accepts.join(", ")}`).join("\n")'
    fi
  fi
  exit 0 ;;

stop)
  if [[ -n "$(obid_running)" && "$(membership | field .state)" == joined ]]; then
    sphere=$(membership | field .sphere)
    if [[ -n "$(app_running)" ]]; then
      say "Leaving $sphere"
      said=$(ask_app leave 60 | field .leave || true)
      case $said in
      left) say "Left $sphere: the Obysphere took the demo Oby out" ;;
      "") print -r -- "sphere-demo: the demo app didn't answer the leave; its window may be closed" >&2 ;;
      *) print -r -- "sphere-demo: left with a problem: $said" >&2 ;;
      esac
    else
      print -r -- "sphere-demo: the demo app isn't running, so $sphere can't be told: start it (scripts/sphere-demo.sh --repo …) and run --stop again, or ask its admin to remove $owner's Oby" >&2
      exit 1
    fi
  fi
  if pid=$(app_running); [[ -n "$pid" ]]; then
    say "Quitting the demo app (pid $pid)"
    print -r -- quit > "$dir/control"
    gone "$pid" 15 || kill "$pid" 2> /dev/null || true
    gone "$pid" 10 || die "the demo app (pid $pid) didn't quit"
  fi
  rm -f "$dir/app.pid" "$dir/control" "$dir/join-link"
  if pid=$(obid_running); [[ -n "$pid" ]]; then
    say "Stopping the demo obid (pid $pid)"
    kill "$pid"; gone "$pid" 20 || die "the demo obid (pid $pid) didn't stop"
  fi
  rm -f "$dir/obid.pid"
  say "Stopped. The demo's folder stays in $dir (its owner key for $owner): remove it when you won't join as $owner again."
  exit 0 ;;

send_task)
  [[ "$(membership | field .state)" == joined ]] || die "the demo Oby isn't in an Obysphere: start and join it first"
  to=$(membership | field .obiId) sphere=$(membership | field .sphere)
  mine=$(call_at "$from_port" "$from_home/api-token" GET /v1/obid/sphere 2> /dev/null || true)
  [[ -n "$mine" ]] || die "your Oby's obid doesn't answer on $from_port"
  [[ "$(print -r -- "$mine" | field .state)" == joined && "$(print -r -- "$mine" | field .sphere)" == "$sphere" ]] \
    || die "your Oby isn't in $sphere: join it with the same link first"
  body=$(TO=$to GOAL=$goal CONFIG=$dir/obid/config.json node -e '
    const e = process.env, r = JSON.parse(require("fs").readFileSync(e.CONFIG, "utf8")).repos[0];
    process.stdout.write(JSON.stringify({ to: e.TO, type: "handoff", expiresIn: 86400,
      body: { goal: e.GOAL, repo: r.repo, base_branch: r.defaultBranch || "main", acceptance: [{ kind: "tests_pass" }] } }));')
  said=$(call_at "$from_port" "$from_home/api-token" POST /v1/obid/sphere/requests "$body")
  id=$(print -r -- "$said" | field .correlationId)
  [[ -n "$id" ]] || die "your Oby didn't send it: $(print -r -- "$said" | field .error)"
  say "Sent: your Oby handed $owner's Oby the task ($id)"
  print -r -- "On the demo Oby's Home: \"Task from <you>\", then Accept. Its result comes back to your thread with $owner's Oby."
  exit 0 ;;

join)
  url=$(read_link)
  [[ -n "$url" ]] || die "no invite link: give it, or copy it first"
  join_demo "$url"
  owner_steps "$url"
  exit 0 ;;
esac

# start
[[ "$repo" =~ '^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$' ]] || die "--repo OWNER/NAME names the repository the demo Oby takes tasks in (OBY_DEMO_REPO)"
[[ -x "$obid_bin" ]] || die "no obid at $obid_bin: install Oby, or name one with OBY_DEMO_OBID"
url=$(read_link)
mkdir -p "$dir/obid" "$home" "$dir/app"
chmod 700 "$dir" "$dir/obid" "$home" "$dir/app"

# 1. obid: its own port and home, your own agent logins (the CLIs' defaults, as your Oby's), the
#    one repository; no gateway (that's your Oby's, for your phone)
if [[ -z "$(obid_running)" ]]; then
  [[ -z "$(listening)" ]] || die "something already listens on 127.0.0.1:$port: pick another --port"
  branch=$(gh repo view "$repo" --json defaultBranchRef -q .defaultBranchRef.name 2> /dev/null || print main)
  codex=0; command -v codex > /dev/null && codex=1
  OWNER=$owner PORT=$port HOME_DIR=$home REPO=$repo BRANCH=${branch:-main} CHECK=$check CODEX=$codex node -e '
    const e = process.env;
    const agents = { claude: { bin: "claude", accounts: [{ id: "default", name: "Default", configDir: null }] } };
    if (e.CODEX === "1") agents.codex = { bin: "codex", accounts: [{ id: "codex", name: "Codex", configDir: null }] };
    const repo = { repo: e.REPO, remote: `https://github.com/${e.REPO}.git`, defaultBranch: e.BRANCH };
    if (e.CHECK) repo.check = e.CHECK;
    process.stdout.write(JSON.stringify({ listen: `127.0.0.1:${e.PORT}`, home: e.HOME_DIR, owner: e.OWNER, agents, repos: [repo], telemetry: false }, null, 2) + "\n");
  ' > "$dir/obid/config.json"
  chmod 600 "$dir/obid/config.json"
  say "Starting the demo obid on 127.0.0.1:$port ($obid_bin)"
  nohup "$obid_bin" serve --config "$dir/obid/config.json" >> "$dir/obid.log" 2>&1 &
  print -r -- $! > "$dir/obid.pid"
  for i in {1..60}; do [[ "$(obid GET /v1/status 2> /dev/null | field .runner.engine)" == obid ]] && break; sleep 0.5; done
  [[ -n "$(obid_running)" ]] || { tail -n 20 "$dir/obid.log" >&2; die "the demo obid stopped: $dir/obid.log"; }
  [[ "$(obid GET /v1/status | field .runner.engine)" == obid ]] || die "the demo obid doesn't answer on $port: $dir/obid.log"
  say "The demo obid answers ($(obid GET /v1/status | field '.runner.commit.slice(0,9)'))"
else
  say "The demo obid runs already (pid $(obid_running))"
fi

# 2. the app: this checkout's, built once
if [[ -z "$(app_running)" ]]; then
  if [[ ! -x "$app_dir/node_modules/electron/dist/Electron.app/Contents/MacOS/Electron" ]]; then
    say "Installing the app's packages (npm ci)"
    (cd "$app_dir" && npm ci --no-audit --no-fund --loglevel=error)
  fi
  if (( build )) || [[ ! -f "$app_dir/out/main/index.js" ]]; then
    say "Building the app"
    (cd "$app_dir" && npm run build --silent)
  fi
  say "Opening the demo app, \"$title\""
  rm -f "$dir/ready" "$dir/control" "$dir/control.out" "$dir/join-link"
  extra=()
  [[ -n "${OBY_DEMO_CDP:-}" ]] && extra=(--remote-debugging-port="$OBY_DEMO_CDP")
  OBID_PORT=$port OBID_HOME=$home OBY_DEMO_DIR=$dir OBY_DEMO_TITLE=$title OBY_DEMO_MAIN=$app_dir/out/main/index.js OBY_DEMO_YES=$yes \
    nohup "$app_dir/node_modules/electron/dist/Electron.app/Contents/MacOS/Electron" "$launcher" "${extra[@]}" >> "$dir/app.log" 2>&1 &
  print -r -- $! > "$dir/app.pid"
  # the launcher writes `ready` once the page loaded (main has run sphering(), which takes a link)
  for i in {1..120}; do [[ -f "$dir/ready" || -z "$(app_running)" ]] && break; sleep 0.5; done
  [[ -n "$(app_running)" ]] || { tail -n 20 "$dir/app.log" >&2; die "the demo app stopped: $dir/app.log"; }
  [[ -f "$dir/ready" ]] || die "the demo app's window didn't load within a minute: $dir/app.log"
else
  say "The demo app runs already (pid $(app_running))"
fi

# 3. the Obysphere
if [[ -n "$url" ]]; then
  join_demo "$url"
  owner_steps "$url"
elif [[ "$(membership | field .state)" == joined ]]; then
  say "The demo Oby is in $(membership | field .sphere) ($(membership | field .url)) as $(membership | field .owner)"
else
  print
  print -r -- "No invite link given or on the clipboard: the demo Oby runs, unjoined."
  print -r -- "Once you have the link, copy it and run:  scripts/sphere-demo.sh --join"
fi
print
print -r -- "The demo Oby takes tasks in $repo. --status shows the Obysphere's members; --stop leaves and stops it."
