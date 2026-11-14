#!/usr/bin/env bash
# Sets up the factory's second Mac, the Mac worker (docs/factory-remote-gate.md, "The second Mac"), over
# SSH, from this Mac.
#
#   factory/remote/setup-mac.sh [ssh-host]        set up the host (default oby-mac-pro)
#
# Everything on the host runs through `bash -s`, never its login shell (fish on the Pro), and under
# macOS's bash 3.2. In order:
#   a. it checks the host has git, python3, xcodebuild, rustup, cargo, rustc, node and gitleaks, and an
#      `iPhone 17` simulator (the swift group's destination in scripts/oby-check.sh), and that its
#      `rustc --version` and `xcodebuild -version` match this Mac's: a missing tool or a Rust or Xcode
#      mismatch stops it, naming each, before anything is written. A Node or gitleaks difference is only
#      said: the worker runs no electron or scan group;
#   b. ~/oby-worker, with a full bare clone (repo.git, gc.auto 0) and slot-1, slot-2;
#   c. this checkout's oby-remote-gate as ~/oby-worker/bin/oby-remote-gate, 0755, compared with the
#      installed copy first;
#   d. oby-check-blok, built from origin/dev's commit (git archive in the clone), never from one being
#      checked, and again only when origin/dev's tools/oby-check-blok changed;
#   e. tighten (tighten.sh) with origin/dev's own scripts/oby-check.sh, so nothing on the check's path is
#      writable by group or others;
#   f. the versions it found.
# Run it again whenever: what is already there is left as it is. It installs nothing else, runs nothing as
# root, writes no secret, token, key or login to the host, and changes no system setting. The host part
# runs under umask 022.
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
host=${1:-oby-mac-pro}

# This Mac's versions, read from its home so no folder's toolchain file picks another.
rust=$(cd && rustc --version)
xcode=$(xcodebuild -version | sed -n 1p)
node=$(node --version 2>/dev/null || echo none)
gitleaks=$(gitleaks version 2>/dev/null || echo none)
echo "setup-mac.sh: setting up $host to match this Mac: $rust, $xcode"

# One function, read whole before it starts, with no standard input, so nothing it runs reads the script.
{
    printf '%s=%q\n' RUST "$rust" XCODE "$xcode" NODE "$node" GITLEAKS "$gitleaks" \
        GATE_B64 "$(base64 <"$here/oby-remote-gate" | tr -d '\n')"
    cat "$here/tighten.sh"
    cat <<'REMOTE'
setup() {
    set -euo pipefail
    umask 022
    say() { printf 'setup-mac.sh: %s\n' "$*"; }
    local url=https://github.com/obi-ai-bot/oby.git root="$HOME/oby-worker" tool problems=() found
    # A command over ssh reads no profile: rustup's and Homebrew's folders, as oby-remote-gate --mac has them.
    export PATH="$HOME/.cargo/bin:/opt/homebrew/bin:/usr/local/bin:$PATH"
    tmp=$(mktemp -d)
    trap 'rm -rf "$tmp"' EXIT

    for tool in git python3 xcodebuild xcrun rustup cargo rustc node gitleaks; do
        command -v "$tool" >/dev/null || problems+=("missing: $tool")
    done
    if command -v xcrun >/dev/null; then
        # Read whole first: grep -q ending early would break the pipe, and pipefail would call that a miss.
        found=$(xcrun simctl list devices available 2>&1 || true)
        grep -qE '^ +iPhone 17 \(' <<<"$found" ||
            problems+=("no iPhone 17 simulator (xcrun simctl list devices available)")
    fi
    if command -v rustc >/dev/null; then
        found=$(cd && rustc --version)
        [[ "$found" == "$RUST" ]] || problems+=("rust: this host has $found, the Mac $RUST")
    fi
    if command -v xcodebuild >/dev/null; then
        found=$(xcodebuild -version | sed -n 1p)
        [[ "$found" == "$XCODE" ]] || problems+=("xcode: this host has $found, the Mac $XCODE")
    fi
    if ((${#problems[@]})); then
        for found in "${problems[@]}"; do say "$found"; done
        say "nothing was written"
        return 1
    fi
    found=$(node --version)
    [[ "$found" == "$NODE" ]] ||
        say "node differs: this host has $found, the Mac $NODE (the worker runs no electron group)"
    found=$(gitleaks version)
    [[ "$found" == "$GITLEAKS" ]] ||
        say "gitleaks differs: this host has $found, the Mac $GITLEAKS (the worker runs no scan)"

    mkdir -p "$root/bin"
    if [[ ! -d "$root/repo.git" ]]; then
        say "cloning $url into $root/repo.git"
        git clone --bare --quiet "$url" "$root/repo.git"
    fi
    git -C "$root/repo.git" config gc.auto 0
    mkdir -p "$root/slot-1" "$root/slot-2"

    printf '%s' "$GATE_B64" | base64 --decode >"$tmp/oby-remote-gate"
    if cmp -s "$tmp/oby-remote-gate" "$root/bin/oby-remote-gate" && [[ -x "$root/bin/oby-remote-gate" ]]; then
        say "oby-remote-gate already installed"
    else
        install -m 0755 "$tmp/oby-remote-gate" "$root/bin/oby-remote-gate"
        say "installed $root/bin/oby-remote-gate"
    fi

    # Built again only when origin/dev's copy of the tool changed since the last build.
    git -C "$root/repo.git" -c credential.helper= fetch --quiet --no-tags "$url" +refs/heads/dev:refs/remotes/origin/dev
    local dev tree stamp="$root/oby-check-blok.built"
    dev=$(git -C "$root/repo.git" rev-parse refs/remotes/origin/dev)
    tree=$(git -C "$root/repo.git" rev-parse "$dev:tools/oby-check-blok")
    if [[ "$(cut -d' ' -f2 "$stamp" 2>/dev/null)" == "$tree" ]] && [[ -x "$HOME/.cargo/bin/oby-check-blok" ]]; then
        say "oby-check-blok already built from origin/dev $(cut -d' ' -f1 "$stamp")"
    else
        git -C "$root/repo.git" archive "$dev" tools/oby-check-blok | tar -x -C "$tmp"
        cargo install --locked --quiet --path "$tmp/tools/oby-check-blok"
        echo "$dev $tree" >"$stamp"
        say "oby-check-blok built from origin/dev $dev"
    fi

    # origin/dev's own check script, in a folder that isn't a checkout, judges the tool (tighten.sh).
    mkdir "$tmp/vet"
    git -C "$root/repo.git" archive "$dev" scripts/oby-check.sh | tar -x -C "$tmp/vet"
    tighten "$HOME" "$root" "$root/bin/oby-remote-gate" "$tmp/vet/scripts/oby-check.sh" "$tmp"

    say "versions:"
    git --version
    python3 --version
    rustc --version
    cargo --version
    cargo fmt --version
    cargo clippy --version
    xcodebuild -version | sed -n 1p
    echo "node $(node --version)"
    echo "gitleaks $(gitleaks version)"
    echo "oby-check-blok from origin/dev $(cut -d' ' -f1 "$stamp")"
    echo "$root/repo.git shallow: $(git -C "$root/repo.git" rev-parse --is-shallow-repository), gc.auto $(git -C "$root/repo.git" config gc.auto)"
}
setup </dev/null
REMOTE
} | ssh "$host" bash -s
echo "setup-mac.sh: $host is set up"
