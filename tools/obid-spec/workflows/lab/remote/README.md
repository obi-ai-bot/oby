# Lab: other computers (`lab-remote`)

The owner's decisions of 2026-11-19, built and proven here first (src/lab/remote.rs, src/lab/machine.rs):
any network the person already has, routes ranked LAN, then VPN or tailnet, then public, then SSH;
SSH as a tunnel only (`ssh -N -L` to the other obid's 127.0.0.1), keys and agent only, host keys the
person's call, nothing installed over it; Macs and Linux; the whole window switches to the other
computer; when no route works, Oby says so. The Obysphere is never the transport to one's own
computers.

```bash
cargo build --manifest-path tools/obid-spec/Cargo.toml --bins
tools/obid-spec/target/debug/obid-spec run --only lab-remote
```

Each Linux workflow makes its own isolated Ubuntu machines (user `oby`) and deletes them at its
end, whatever it ended in; the window's SSH tunnels stop then too. Without OrbStack they're
`skipped`. Only the Mac workflow needs the Mac's obid that `run` builds; with `--obid /usr/bin/true`
the Linux ones run alone.

**The other computer is a Mac** (`mac-ssh-pair.json`, the owner's addition of 2026-11-19): a second
scratch obid on this Mac, with its own home, port and user folder, reached over `ssh` to
localhost. It needs macOS Remote Login on, and is `skipped` while it's off: the lab never turns it
on or changes any setting. The workflow makes its own key, and adds one line for it to the
person's `~/.ssh/authorized_keys`: `restrict,port-forwarding,permitopen="127.0.0.1:<that obid's
gateway>",expiry-time="<an hour from now>Z",command="<that obid's pairing program>"`. The key can
run nothing but that obid's pairing and open no port but its gateway, and stops working within the
hour even if nothing removes it. The line goes at the workflow's end, whatever it ended in, and
every other line is left as it was (its SHA-256 before and after each run here was the same).
ssh runs with the workflow's own config, known_hosts (this Mac's host key, from
`/etc/ssh/ssh_host_ed25519_key.pub`) and `HOME`, never the person's keys or agent.

**The other computer is a real second Mac** (`mac2-ssh-pair.json`, the owner's addition): the same
flow against another Mac on the network, named only by `OBID_SPEC_LAB_REMOTE_MAC=user@host` and
skipped while it's unset; the address and user never go in a file. The person's own ssh (their
config, keys and agent, strict about the host key) sets it up: one scratch folder there
(`$TMPDIR/obid-spec-lab-XXXXXX`) holds this Mac's arm64 obid, the stand-in agents and Node 22, a
fixture repository and the scratch obid's home, and that obid starts on free ports of its own, in a
session of its own, with a stand-in Keychain. Whatever else runs there (an Oby of the person's) is
never touched. The window's key gets the same restricted, expiring, forced-command line as above,
its host key taken over the person's own authenticated ssh. At the end, whatever the workflow ended
in, the scratch obid's process group is stopped, exactly the bytes added to `authorized_keys` are
taken out, and the scratch folder goes; the last step asserts the file's SHA-256 is what it was and
that no folder or process of it is left. Everything sent there is a bash script on stdin
(`/bin/bash -s`): the person's login shell there (fish, on the one proven) never parses it.

The Linux obid is built once per `core/` tree and arch, in a machine of its own deleted right
after, and cached in `$TMPDIR/obid-spec-lab/` (with nodejs.org's Node 22, SHA-256 checked): workflows
running at once share one build. A cold build can take longer than a step's usual 300 s, so each
workflow's first step builds it with a `limit` of its own (`"limit": 1200`: a lab step may ask for
up to 1800 s, src/main.rs `step_limit`, OBIDSPEC-16). A cold run of all four took 414 s, most of it
that one build; with the build cached the Linux three take about 90 s and the Mac one about 10 s.

## Scenarios

| Scenario | What it proves | Workflow |
|---|---|---|
| LAB-REMOTE-1 | A Linux obid on another machine listens on its own 127.0.0.1 only; the window reaches the machine over SSH with a key of its own, and a host key the person never accepted is refused. | `ssh-pair.json` |
| LAB-REMOTE-2 | Adding a computer over SSH: the Mac's device key is named in that obid's `home/pairing` and registered there, a one-time invitation comes back over SSH and is redeemed through the tunnel for a device token; a wider setting goes through only with the Mac's key's signature (no token: 401; unsigned or signed by a key never registered: 403). | `ssh-pair.json` |
| LAB-REMOTE-3 | Adding a computer with a code: the other computer's owner makes a six-digit code and a link carrying its direct address with its certificate pinned; a wrong code and a used one pair nothing; the right one gives a token, and the Mac's key added through it signs the owner's words; another machine answering at the same address gets no code and no call. | `code-pair.json` |
| LAB-REMOTE-4 | Two routes to one computer (LAN direct, SSH): the best is used, the next takes over when it's cut, the window moves up again once the better one answers twice, and with both cut the answer names each route tried and why. | `routes.json` |
| LAB-REMOTE-5 | After "Remove this computer" (`DELETE /v1/pairing` through the route, with the Mac's own token), the Mac's token is refused, its signed words too. | `ssh-pair.json` |
| LAB-REMOTE-6 | A job the window starts runs on the Linux obid with obid's stand-in agents, in that machine's folder, and its result reaches the Mac through the tunnel. | `ssh-pair.json` |
| LAB-REMOTE-7 | A direct address is HTTPS with the certificate pinned: the pinned one connects; another machine's certificate at the same address, or a new certificate on the same machine, is refused. | `code-pair.json` |
| LAB-REMOTE-8 | A computer's token reaches the gateway's list and nothing kept on loopback: not the vault, not pairing. | `ssh-pair.json` |
| LAB-REMOTE-MAC-1 | The other computer is a Mac: the workflow's key in `~/.ssh/authorized_keys` is restricted, runs nothing but that obid's pairing program (`id -un` gets "That isn't a device key."), tunnels to that obid's gateway and to no other port; a host key never accepted is refused. | `mac-ssh-pair.json` |
| LAB-REMOTE-MAC-2 | Pairing that Mac over SSH: its pairing file used up, a device token through the tunnel, and a wider setting only with the window's key's signature. | `mac-ssh-pair.json` |
| LAB-REMOTE-MAC-3 | A job the window starts runs in that obid's own worktree (`worktrees/<job>/`), and its result reaches the window. | `mac-ssh-pair.json` |
| LAB-REMOTE-MAC-4 | After a revoke there, the window's token is refused, its signed words too. | `mac-ssh-pair.json` |
| LAB-REMOTE-MAC2-1 | A real second Mac: pairing over SSH with the window's restricted, expiring, forced-command key (`id -un` gets only "That isn't a device key."), a host key never accepted refused, its pairing file used up. | `mac2-ssh-pair.json` |
| LAB-REMOTE-MAC2-2 | The tunnel reaches that obid's gateway and no other port; the window's calls go through it. | `mac2-ssh-pair.json` |
| LAB-REMOTE-MAC2-3 | The window's signed words are accepted there; unsigned, or signed by a key never registered, refused. | `mac2-ssh-pair.json` |
| LAB-REMOTE-MAC2-4 | A stand-in task runs on the second Mac, in that obid's own worktree, and its result reaches this Mac. | `mac2-ssh-pair.json` |
| LAB-REMOTE-MAC2-5 | After a revoke there, the window's token and signed words are refused. | `mac2-ssh-pair.json` |
| LAB-REMOTE-MAC2-6 | Nothing is left on the second Mac: no scratch folder, no process of it, its `authorized_keys` hash as before, and the window's key opens nothing. | `mac2-ssh-pair.json` |
| LAB-REMOTE-MAC-5 | The workflow's line leaves `~/.ssh/authorized_keys`, and its key then opens nothing ("accepts none of your SSH keys"). | `mac-ssh-pair.json` |

## What's portable

`remote::computers` is what obid takes as it is: `RouteKind`, `classify` and `rank` (the owner's
order), `Route` (a direct address is HTTPS and pinned, or an SSH alias), `Supervisor` (the best route
that answers; down on the first failure, up only after two answers in a row; `Lost` with every route
tried, once), `Ssh` (every argument ssh gets, `trouble` and `say` for what its errors mean),
`Tunnel`, `Pin` and `PinnedVerifier` (a rustls verifier that trusts one certificate, and still checks
the handshake is signed by its key), `Link` and `pair_computer_script` (the stand-in for `obid pair-computer`). The nodes only hold the
window's state and drive the machines.

T3 Code's SSH fixes were read first (59% of its SSH commits); the classes this avoids by design:

- No `~/.ssh/config` parsing at all (a third of T3's fixes are Include/Host-pattern parsing): ssh reads
  its own configuration, and `ssh -G` says what it made of an alias.
- An alias's `RemoteCommand` or `RequestTTY` can't hijack the session (`RemoteCommand=none`,
  `RequestTTY=no`); its configured forwards can't ride along (`ClearAllForwardings=yes` for commands).
- No password or code prompt (`BatchMode=yes`): keys and the agent only, and "accepts none of your SSH
  keys" when that fails.
- A destination is never an option (`-oProxyCommand=…`): it's checked and always after `--`.
- Readiness counts the forward's port taking connections after ssh signed in, not ssh's start; the
  port a tunnel takes is verified by an HTTP answer, so another server on that port isn't mistaken
  for obid; a port taken in between (`ExitOnForwardFailure=yes`) is retried on another.
- ssh's stderr is read for as long as it runs (a full pipe stalls the tunnel), the last 16 KiB kept
  for its error.
- Every ssh is in its own process group, stopped by PID at the end, and `kill_on_drop`.
- No shared ControlMaster (`ControlMaster=no`, `ControlPath=none`): a slow reconnect never replaces a
  healthy forward, and nothing outlives the window.
- A lost link is noticed in 45 s (`ServerAliveInterval=15`, `ServerAliveCountMax=3`); a cut route
  isn't redialed more than once a second.
- Host key trouble says what to do and leaves it to the person: unknown (accept it once in a
  terminal) or changed (someone else may answer: Oby won't connect).

## What obid needs when ported

Each item is something the lab does in obid's place today.

1. **An HTTPS listener for direct addresses.** Ported: obid serves its gateway over HTTPS on the
   addresses `direct` names, with a self-signed P-256 certificate kept in `home/keys`, its SHA-256 the
   pin in `GET /v1/status` (docs/contracts/remote.md, COMPUTERS-10). `code-pair.json` reaches obid's
   own (`lab.remote.direct`); `lab.remote.front` (python3) is left only for the impostor machine.
2. **`obid pair-computer`.** Ported (COMPUTERS-7): `pair_computer_script` is now only obid's own
   command's path, so it can be a key's forced command.
3. **A computer's token reaches the window's routes.** Partly ported (COMPUTERS-14): a computer's
   pairing is its own kind, and reaches the gateway's list, as a phone's. The routes the window uses
   past it (connections, the vault, adding folders, a member's unsealed setting…) are each kept on
   loopback by obid's docs today: adding them is the owner's call, route by route.
4. **Signed words bound to the token's own key.** Ported (COMPUTERS-15): through a computer's token
   only the key bound to its pairing signs the owner's words; `obid pair-computer` names the key it
   registered in its invitation, and redeeming binds it.
5. **Revoking from the Mac, and forgetting its key.** Ported (COMPUTERS-16): `DELETE /v1/pairing` on
   the gateway, with the computer's own token, ends its pairing and forgets its key there.
6. **The link in the invitation.** Ported (COMPUTERS-13): a computer's invitation answers
   `computerLink`, its direct routes each with its certificate pin; the code travels with it.
7. **One invitation at a time.** Ported (COMPUTERS-13): a computer's invitation has a slot of its
   own beside the phone's, and lasts 10 minutes.
8. **Where the window's computers live.** The supervisor, tunnels and tokens belong to the Mac's side
   (the dispatcher's: which computer, which route). Proposed: the Mac's obid keeps them
   (`home/computers.json`, tokens in the vault, never in an answer) and the window switches by
   talking to its own obid, which forwards to the computer in use. An owner's decision before the
   port.
9. **Linux obid's runtime needs.** A job's stand-ins need Node 22 (Ubuntu 24.04 has 18); the real CLIs
   bring their own. bubblewrap for boxed jobs; git.
10. **A Mac as the other computer.** macOS Remote Login is the owner's setting: with it off, Oby says
    so and how to turn it on, and never changes it. To add a key for a Mac over SSH, obid's
    `pair-computer` needs a stable path, so `authorized_keys` can make it the key's forced command
    (`restrict`, `permitopen` to the gateway only, an `expiry-time`), as the lab does. A key with
    a shell is more than pairing needs.
11. **A Mac reached over SSH has its own login shell.** The second Mac's is fish: a command line
    sent over SSH is parsed by it, so `pair-computer` must be one plain path, and anything more is a
    script on stdin to `/bin/bash -s`. obid on a Mac reached this way also needs what a job's agent
    needs there (the stand-ins need Node 22; the real CLIs bring their own).
