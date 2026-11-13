#!/usr/bin/env bash
# Sets up the factory's gate VM (docs/factory-remote-gate.md) over SSH, from the Mac.
#
#   factory/remote/setup.sh [ssh-host]        set up the host (default oby-gate)
#   factory/remote/setup.sh --pins [ci.yml]   print the pins it would install, one a line, and touch no host
#
# The pins are .github/workflows/ci.yml's, read from this checkout and never copied here:
# RUSTUP_TOOLCHAIN, GITLEAKS_VERSION with GITLEAKS_SHA256, node-version, and its bwrap line. A pin
# that's missing, or whose copies in ci.yml disagree, stops it before it touches the host.
#
# On the host, with sudo where it needs it: git, curl and the build tools; rustup with the pinned
# toolchain as the default, rustfmt and clippy; the newest Node of the pinned major; gitleaks at its
# pin, checked against its SHA-256; bubblewrap and slirp4netns, with unprivileged user namespaces
# allowed now and after a reboot (/etc/sysctl.d), proven with ci.yml's bwrap line; oby-check-blok,
# built from origin/dev's commit, never from one being checked; this checkout's oby-remote-gate as
# /usr/local/bin/oby-remote-gate; /srv/oby, the ssh user's, with a full bare clone (repo.git, gc.auto
# 0) and slot-1 to slot-4. Run it again whenever: what is already at its pin is left as it is. It
# writes no secret, token, key or login to the host, and changes no sshd setting, firewall or network
# rule.
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
ci="$here/../../.github/workflows/ci.yml"

# The one value every `<name>: <value>` line of ci.yml gives, quotes and comments dropped.
pin() {
    local values
    values=$(sed -nE "s/^[[:space:]]*$1:[[:space:]]*\"?([^\"[:space:]#]+)\"?[[:space:]]*(#.*)?$/\1/p" "$ci" | sort -u)
    if [[ -z "$values" ]]; then
        echo "setup.sh: $ci has no $1" >&2
        return 1
    fi
    if [[ "$values" == *$'\n'* ]]; then
        echo "setup.sh: $ci's copies of $1 disagree: $(tr '\n' ' ' <<<"$values")" >&2
        return 1
    fi
    printf '%s\n' "$values"
}

if [[ "${1:-}" == --pins ]]; then
    ci=${2:-$ci}
    pins=1
else
    host=${1:-oby-gate}
    pins=''
fi
rust=$(pin RUSTUP_TOOLCHAIN)
node=$(pin node-version)
gitleaks=$(pin GITLEAKS_VERSION)
gitleaks_sha=$(pin GITLEAKS_SHA256)
bwrap=$(sed -nE 's/^[[:space:]]*(bwrap .*[^[:space:]])[[:space:]]*$/\1/p' "$ci" | sort -u)
if [[ -z "$bwrap" || "$bwrap" == *$'\n'* ]]; then
    echo "setup.sh: $ci needs exactly one bwrap line, the proof bubblewrap works" >&2
    exit 1
fi
if [[ -n "$pins" ]]; then
    printf 'rust %s\nnode %s\ngitleaks %s %s\n' "$rust" "$node" "$gitleaks" "$gitleaks_sha"
    exit 0
fi

echo "setup.sh: setting up $host with rust $rust, node $node, gitleaks $gitleaks"

# This checkout's remote gate, compared with the installed one before it's replaced.
ssh "$host" 'set -e
tmp=$(mktemp)
trap "rm -f \"\$tmp\"" EXIT
cat >"$tmp"
if cmp -s "$tmp" /usr/local/bin/oby-remote-gate && [[ "$(stat -c %U:%a /usr/local/bin/oby-remote-gate)" == root:755 ]]; then
    echo "setup.sh: oby-remote-gate already installed"
else
    sudo -n install -o root -g root -m 0755 "$tmp" /usr/local/bin/oby-remote-gate
    echo "setup.sh: installed /usr/local/bin/oby-remote-gate"
fi' <"$here/oby-remote-gate"

# The rest runs there as one function, read whole before it starts, with no standard input, so
# nothing it runs reads the script.
{
    printf '%s=%q\n' RUST "$rust" NODE "$node" GITLEAKS "$gitleaks" GITLEAKS_SHA "$gitleaks_sha" BWRAP "$bwrap"
    cat <<'REMOTE'
setup() {
    set -euo pipefail
    say() { printf 'setup.sh: %s\n' "$*"; }
    local url=https://github.com/obi-ai-bot/oby.git root=/srv/oby user
    user=$(id -un)
    [[ "$(uname -m)" == x86_64 ]] || { say "this installs x86_64 builds, and this host is $(uname -m)"; return 1; }
    tmp=$(mktemp -d)
    trap 'rm -rf "$tmp"' EXIT

    local packages=(git curl ca-certificates build-essential pkg-config libssl-dev cmake xz-utils bubblewrap slirp4netns)
    local missing=() package
    for package in "${packages[@]}"; do
        dpkg-query -W -f='${Status}' "$package" 2>/dev/null | grep -q 'install ok installed' || missing+=("$package")
    done
    if ((${#missing[@]})); then
        say "installing ${missing[*]}"
        sudo -n apt-get update -qq
        sudo -n DEBIAN_FRONTEND=noninteractive apt-get install -y -qq "${missing[@]}"
    else
        say "packages already installed: ${packages[*]}"
    fi

    # Ubuntu 24.04's AppArmor refuses the unprivileged user namespaces bubblewrap needs, as on ci.yml's runner.
    local conf=/etc/sysctl.d/60-oby-gate-userns.conf line='kernel.apparmor_restrict_unprivileged_userns = 0'
    if [[ "$(cat "$conf" 2>/dev/null)" != "$line" ]]; then
        printf '%s\n' "$line" | sudo -n tee "$conf" >/dev/null
        say "wrote $conf"
    fi
    sudo -n sysctl -q -w kernel.apparmor_restrict_unprivileged_userns=0
    sysctl kernel.apparmor_restrict_unprivileged_userns
    bash -c "$BWRAP"
    say "bubblewrap works: $BWRAP"

    export PATH="$HOME/.cargo/bin:$PATH"
    if ! command -v rustup >/dev/null; then
        say "installing rustup"
        curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain none --profile minimal
    fi
    local components
    components=$(rustup component list --installed --toolchain "$RUST" 2>/dev/null || true)
    if grep -q '^rustfmt' <<<"$components" && grep -q '^clippy' <<<"$components"; then
        say "rust $RUST already installed"
    else
        rustup toolchain install "$RUST" --profile minimal --component rustfmt,clippy
    fi
    [[ "$(rustup default 2>/dev/null)" == "$RUST"-* ]] || rustup default "$RUST"

    if [[ "$(node --version 2>/dev/null)" == "v$NODE".* ]]; then
        say "node $(node --version) already installed"
    else
        local dist="https://nodejs.org/dist/latest-v$NODE.x" archive
        curl -fsSLo "$tmp/SHASUMS256.txt" "$dist/SHASUMS256.txt"
        archive=$(awk '$2 ~ /^node-v[0-9.]+-linux-x64\.tar\.xz$/ { print $2 }' "$tmp/SHASUMS256.txt")
        curl -fsSLo "$tmp/$archive" "$dist/$archive"
        (cd "$tmp" && grep "  $archive\$" SHASUMS256.txt | sha256sum -c -)
        sudo -n mkdir -p /usr/local/lib/nodejs
        sudo -n tar -xJf "$tmp/$archive" -C /usr/local/lib/nodejs
        local tool
        for tool in node npm npx; do
            sudo -n ln -sfn "/usr/local/lib/nodejs/${archive%.tar.xz}/bin/$tool" "/usr/local/bin/$tool"
        done
    fi

    local found
    found=$(gitleaks version 2>/dev/null || true)
    if [[ "${found#v}" == "$GITLEAKS" ]]; then
        say "gitleaks $GITLEAKS already installed"
    else
        local archive="gitleaks_${GITLEAKS}_linux_x64.tar.gz"
        curl -fsSLo "$tmp/$archive" "https://github.com/gitleaks/gitleaks/releases/download/v${GITLEAKS}/$archive"
        echo "$GITLEAKS_SHA  $tmp/$archive" | sha256sum -c -
        tar -xzf "$tmp/$archive" -C "$tmp" gitleaks
        sudo -n install -o root -g root -m 0755 "$tmp/gitleaks" /usr/local/bin/gitleaks
    fi

    sudo -n install -d -o "$user" -g "$user" "$root"
    if [[ ! -d "$root/repo.git" ]]; then
        say "cloning $url into $root/repo.git"
        git clone --bare --quiet "$url" "$root/repo.git"
    fi
    git -C "$root/repo.git" config gc.auto 0
    local n
    for n in 1 2 3 4; do mkdir -p "$root/slot-$n"; done

    # Built again only when origin/dev's copy of the tool changed since the last build.
    git -C "$root/repo.git" -c credential.helper= fetch --quiet --no-tags "$url" +refs/heads/dev:refs/remotes/origin/dev
    local dev tree stamp="$root/oby-check-blok.built"
    dev=$(git -C "$root/repo.git" rev-parse refs/remotes/origin/dev)
    tree=$(git -C "$root/repo.git" rev-parse "$dev:tools/oby-check-blok")
    if [[ "$(cut -d' ' -f2 "$stamp" 2>/dev/null)" == "$tree" ]] && command -v oby-check-blok >/dev/null; then
        say "oby-check-blok already built from origin/dev $(cut -d' ' -f1 "$stamp")"
    else
        git -C "$root/repo.git" archive "$dev" tools/oby-check-blok | tar -x -C "$tmp"
        cargo install --locked --quiet --path "$tmp/tools/oby-check-blok"
        echo "$dev $tree" >"$stamp"
        say "oby-check-blok built from origin/dev $dev"
    fi

    say "versions:"
    git --version
    rustc --version
    cargo --version
    cargo fmt --version
    cargo clippy --version
    echo "node $(node --version), npm $(npm --version)"
    echo "gitleaks $(gitleaks version)"
    bwrap --version
    slirp4netns --version | sed -n 1p
    echo "oby-check-blok from origin/dev $(cut -d' ' -f1 "$stamp")"
    echo "$root/repo.git shallow: $(git -C "$root/repo.git" rev-parse --is-shallow-repository), gc.auto $(git -C "$root/repo.git" config gc.auto)"
    ls -ld /usr/local/bin/oby-remote-gate "$root" "$root"/slot-*
}
setup </dev/null
REMOTE
} | ssh "$host" bash -s
echo "setup.sh: $host is set up"
