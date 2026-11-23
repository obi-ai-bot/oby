# obid's proofs

obid's Kani harnesses prove that a decider keeps its rule for every input in a bounded space, not
for the handful a test picks. They live in two places:

- **Beside the code they prove**, in a `#[cfg(test)] mod proofs { #![cfg(kani)] … }` at the end of
  its file (`merge.rs`, `push.rs`, `job/chain.rs` and the others): the factory's harnesses, named
  after their contract scenarios. `egress.rs`'s two prove the browser door's address rule (BROWSER-18):
  `browser_18_the_browser_door_refuses_the_hosts_own_network` for every IPv4 and IPv6 address, and
  `browser_18_the_browser_door_refuses_the_hosts_listeners_on_any_address` for any port and three
  listener ports too. `gateway.rs`'s two prove the one segment matcher both gateway lists are read
  with, `segments_fit` (COMPUTERS-29): `computers_29_a_star_is_one_path_segment` and
  `computers_29_a_literal_segment_matches_only_itself`, for every pattern of up to three segments
  and path of up to four.
- **In `core/crates/obid/src/proofs/`**, one file per security decider. Each names the files it
  proves on its `//! Proves:` lines, so a change to any of them requires it.

## What `src/proofs/` proves

Every input is any bytes, any case, any combination of a job's marks, up to the bound named; nothing
narrows it further with `kani::assume` but what a type requires (a `&str` is UTF-8, an index is in
range). Words longer than those bounds (`Accept, and remember for this repo`, `operator_reset`) are
compared by a decider over bytes that the `&str` one hands its own (`inbox::accepts_bytes`,
`link::owner_event_code`, `link::rebound_refusal`), proved with `any_bytes_in` for every byte string up to one past the
longest word, so every longer one differs from each word in length, which every comparison reads
first. A symbolic `&str` that long costs CBMC its UTF-8 check: 16 bytes of it took four minutes on
the Mac, where the same comparisons over bytes take a fraction of a second.
The other bounds are small (strings of 4 to 6 bytes) because CBMC's cost grows fast with symbolic
string length: past them a single proof ran for tens of minutes on the Mac. Each harness was shown to catch a planted bug
(the rule broken in the code, the proof run and seen to fail, the code restored), but the
per-domain `tools_own` proofs: their one-proof-for-every-domain predecessor ran out of memory before
it could, and the split ones haven't been run against the bug yet. The pull request that added them
lists each run.

| File | Decider | What holds |
| --- | --- | --- |
| `token.rs` | `token::same` | The runner's token is accepted exactly when it is the token (any two strings up to 6 bytes). |
| `within.rs` | `egress::within`, `egress::tools_own` | A host is a domain's only when it is the domain or a true subdomain, never a look-alike such as `xopenai.com`; each tool's own domain (one proof each) is its service on 443 only. |
| `lanes.rs` | `Jobs::own_lane` (LANES-1, 18, 19) | Another Oby's job, an untrusted one, a review and every Cursor run are boxed; an own Codex run is Bypass on macOS and never a build; every Claude Code job of the owner's own, and only those, runs in the own lane. |
| `publish.rs` | `publish::may_force` (PUBLISHING-13) | Without the owner's signed force, obid force-pushes only an `oby/` branch or a branch it made, never a `main` it didn't make (branch names up to 4 bytes). |
| `accept.rs` | `sphere::routes::unsigned_accept`, `sphere::inbox::accepts_bytes` (SPHERE-17) | An Accept, or Accept and remember, of an open review request or handoff is refused unless an app of the owner's signed the request or a paired phone sent it; Decline and Later never are; what the inbox carries out as an Accept is exactly those two answers. |
| `sealed.rs` | `sphere::link::delivery`, `sphere::routes::unsigned_unsealing` (SPHERE-18, 19) | A request goes sealed whenever the card has a seal key; with none it goes in the clear only when the owner allowed it for that member, and never to an Obi known to seal; allowing it takes the owner's signature, checked only then, and taking it back none. |
| `owner_key.rs` | `sphere::link::owner_event_code`, `sphere::pins::sight` (SPHERE-8, 23) | An owner event's reason is kept only when it is `signed_in_again`, `admin_reset` or `operator_reset`, as itself (`rotated` and anything else: none; each one's fixed sentence is `inbox::tests`); a card signed with any key but the pinned one, or a first one, is never trusted, whatever the event said. |
| `rebound.rs` | `sphere::link::rebound_refusal` (SPHERE-24) | A rebound join joins only when the directory lists this Oby active, under the body's owner and owner key, with obid's own Obi key on its card; every other combination joins nothing (each field any bytes up to 4, the status up to 7, or missing). |

What the Sphere's proofs leave to tests, each file says on its `What isn't proved here` lines: that a
signature is valid over the exact request (`Approvals::check_words`, an ECDSA check CBMC can't run),
that only a signed `PUT …/unsealed` fills `Pins::allows_clear` and a new key empties it, and that the
directory entry read is this Oby's. Replies (`link/sender.rs`) aren't under SPHERE-18: they go in the
clear to a member whose card has no seal key, as before.

Not proved yet, highest risk first: the security paths' match (`merge::caught_by`, MERGING-15)
and the secret scan's verdict (`scan::verdict`), the permission rules' shell reading
(`permissions.rs`), the gateway's allowlist (`gateway::allows`: its segment matcher is proved, not its lists or its splitting of a path), the signature checks themselves (`approval.rs`),
the vault, job and conversation state, and the limits. Each is a proof of its own. The first two
were written and taken out again: their strings (each path's lowercase copy, each finding's name
compared with `memcmp`) ran CBMC out of memory on GitHub's 16 GB runner and on the Mac, before
any check failed. `caught_by` and `verdict` stay split out for them, with the same results, and
their unit tests (`merge::tests`, `scan::tests`) still cover them.

## Required by path

`scripts/kani-required.sh` reads the changed files and prints the harness filters they require:

- a file a `src/proofs/<name>.rs` names on a `//! Proves:` line, or that file itself, requires
  `proofs::<name>::`;
- a changed `src/<a>/<b>.rs` that holds a `#[kani::proof]` requires its own, `<a>::<b>::proofs::`;
- what every harness is built from (obid's `Cargo.toml` and `build.rs`, the workspace's `Cargo.toml`
  and `Cargo.lock`, `rust-toolchain`, `src/lib.rs`, `src/proofs.rs`), the script,
  `scripts/kani-prove.sh` or `core/.config/kani-slow.txt` requires `all`.

GitHub runs no harness (docs/ci.md, "What GitHub runs"). `scripts/kani-prove.sh` proves the
ones a change requires, two at a time, each within 20 minutes and 6 GB (a harness past either fails
and is named), when it is run by hand, as below. A harness too slow for that is on
`core/.config/kani-slow.txt`, with why: the script leaves it out and says so. One there now:
`job::lifecycle::proofs::jobstate_7_holds_in_every_status`, past 100 minutes on GitHub's runner.
The factory's own `prove` stage (docs/contracts/factory.md) is unchanged: it still runs the
harnesses a task names on its gate VM.

## Run them

```bash
cd core
cargo kani -p obid --tests --harness proofs::     # this folder's
cargo kani -p obid --tests                        # every harness
git diff --name-only origin/dev | scripts/kani-required.sh   # what a change requires (from the root)
scripts/kani-prove.sh "proofs::token:: merge::proofs::"     # as the script proves them, after --only-codegen
```

Kani is 0.68.0 (`cargo install --locked kani-verifier --version 0.68.0 && cargo kani setup`). A first
build of obid under Kani takes about 7 minutes on the Mac and a few GB in `core/target/kani`.

## Models: `docs/models/`

The Quint models check a design, not its code: each one's header names the functions it follows and its
`// verify:` lines what to run, `main` the module, `steps` the depth, each invariant expected to hold and
each witness expected to be violated (so the invariant isn't vacuous). The factory's `prove` stage runs
the ones a task names (docs/contracts/factory.md); no CI job runs them yet.

```bash
quint typecheck docs/models/<model>.qnt
quint run --main=<main> --max-steps=<steps> docs/models/<model>.qnt
quint verify --main=<main> --max-steps=<steps> --invariant=<name> docs/models/<model>.qnt
```

`quint verify` runs Apalache, which needs a Java runtime (17 or later on `PATH`, or `JAVA_HOME`).

`sphere_owners.qnt` is the Obysphere server's owner lifecycle (obi-ai-bot/oby-sphere at 1607ad6: join,
link an account, sign in again, reset, the admin flag), with three people and five keys:

- **linkFirst**: every change of an owner's key by a sign-in came from the account linked to that owner,
  and whoever linked it held the owner's key bound then.
- **noTakeover**: that account is the first one ever linked to the owner; **accountNeverMoves**: once
  linked, an owner's account never changes.
- **atLeastOneAdmin**: the Sphere always has an admin. No act removes the last one, the owner's own
  included; only deleting the Sphere ends it.

Each holds to 10 steps, and each of four broken servers (rebind from any account, link without the
owner's key, a link that replaces an account, no admin count) is caught. Its witness
`linkedAccountIsTheCreators` is violated, and that is a finding: an admin, or the operator, can reset an
owner who never linked an account to a key the admin holds, then link the admin's own account to that
owner. From then on the person who joined as that owner gets back in only through another reset, never
by signing in. Linking first closes it, since a linked owner can't be linked to anyone else.
