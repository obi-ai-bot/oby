# The factory's gate on an Azure Linux VM

Status: built, and off until the owner turns it on (the owner's "Yes, plan the VM", 2026-11-13). Task A00-03
built the VM's side: `factory/remote/setup.sh` provisions it and `factory/remote/oby-remote-gate` runs a
commit's Linux check groups in one of four slots there. Task A00-10 built the gate's: `factory/bin/gate` runs
those groups on the VM, starts it when it is stopped, keeps the macOS-only part on the Mac and falls back to
the Mac, and the Mac's `factory/bin/job queue` deallocates the VM once it is idle. Gates use the VM once the
owner adds `remote_gate` to `factory/config.json` ("The owner's steps", below). Task A00-18 added a second Mac,
the owner's MacBook Pro, for the macOS part ("The second Mac", below), off until the owner adds `mac_worker`.
Task A00-19 made the VM's SSH rule follow the Mac's IP and its idle stop count only the VM's uses. Task A00-31
sent builders' heavy Rust runs to the VM, in builder slots of their own ("Builders' runs", below), off until
the owner sets `builder_slots`. Task A00-34 sent builders' macOS-only runs to the second Mac, in builder slots
apart from its gates', off until the owner sets `builder_slots` in `mac_worker`. The scenarios are [REMOTE-1 to
31 and 35 to 42](contracts/remote-gate.md).

## The VM as it is

- `oby-gate` in resource group `oby-factory`, East US 2: Standard_D32as_v6 (32 vCPU, 128 GB), Ubuntu 24.04,
  pay-as-you-go at about $1.45 an hour while it runs.
- One 256 GB OS disk and no data disk; `/srv/oby` is on it.
- SSH host `oby-gate` in the owner's `~/.ssh/config`, user `oby`, key only.
- No managed identity, so it can't deallocate itself: the Mac's `job queue` deallocates it after 30 idle
  minutes (`gate --idle`), and the owner's 05:00 UTC auto-shutdown stays as the safety net.
- Set up by `factory/remote/setup.sh` with `.github/workflows/ci.yml`'s pins (Rust toolchain, Node major,
  gitleaks and its SHA-256, the bwrap proof), read from the checkout, never copied: `setup.sh --pins` prints
  them. Run it again whenever: what is already at its pin is left as it is. It also installs every package
  ci.yml installs with apt-get (today bubblewrap, slirp4netns and zsh, the last for the ChatGPT gate's test,
  which runs `scripts/package.sh` with `/bin/zsh`), [REMOTE-12](contracts/remote-gate.md).
- Nothing on the check's path is writable by others ([REMOTE-11](contracts/remote-gate.md)): setup runs its
  host part under umask 022, removes group and other write from `~/.cargo`, `~/.rustup`, `/srv/oby` and its
  slots (`factory/remote/tighten.sh`), checks `oby-check-blok` with origin/dev's own `scripts/oby-check.sh`,
  stops naming any path still writable by others, and prints their modes. Ubuntu's default umask (0002)
  had left `~/.cargo`, `~/.cargo/bin` and `oby-check-blok` group-writable, so the check refused its tool
  (A00-09's gate); on 2026-11-13 the live VM was fixed by hand (`chmod go-w` on those three paths).

## The problem in one picture

Every factory job's gate runs Oby's check on the owner's Mac, one at a time, under one lock
(`factory/checks.sh`). A Rust gate takes 30 to 60 minutes on a Mac that is also running agents, so on
2026-11-13 five gates queued behind each other for hours. Under that load some tests time out
(`merging_39`, Electron's 30-second hooks), and each timeout costs the job a whole extra round.

The Linux suite has been green since #2170, so most of the check no longer needs the Mac.

## The design

```
 Mac (orchestrator, agents, worktrees)                Azure VM "oby-gate" (Linux, 32 vCPU)
 ─────────────────────────────────────                ─────────────────────────────────────
 factory/bin/gate <job>
   ├─ plan = OBY_CHECK_PLAN=1 oby-check.sh
   ├─ remote? (remote_gate set, plan has rust, electron or scan, the commit on origin;
   │           a diff touching the check's own files runs the whole check on the Mac instead)
   │     ├─ az vm start (if deallocated) ───────────▶ boots in ~1 min, keeps its disk
   │     ├─ ssh oby-gate oby-remote-gate \
   │     │        <commit> <base> ──────────────────▶ the first free slot N of 4, or wait for one
   │     │                                            git fetch the commit (public repo, no token)
   │     │                                            worktree in slot N, its own CARGO_TARGET_DIR
   │     │                                            OBY_CHECK_ONLY=rust, electron, scan
   │     │  ◀─── output streamed, exit 0/1/2 ──────── 
   │     └─ then on the Mac, once the VM's part passed: OBY_CHECK_ONLY=rust when the diff
   │           touches a macOS-only path, OBY_CHECK_ONLY=swift when planned,
   │           and OBY_CHECK_ONLY=factory when planned
   └─ round-N/checks.md: verdict, commit, checks.sh hash, HOST: oby-gate | oby-gate, mac | mac
 factory/bin/job queue → gate --idle
   └─ no VM use open, the last ended 30 min ago: az vm deallocate --no-wait ──▶ deallocated
```

- **Up to 4 gates run at once on the VM**, one per slot. Each slot has its own checkout and its own
  Cargo build folder, so one branch can never test another's build (the bug #2153 fixed on the Mac).
  A slot's lock replaces the Mac's single lock. The Mac's lock stays for what still runs there.
- **The Mac keeps the macOS-only suites.** About 70 tests only build on macOS (`cfg(target_os =
  "macos")`: `sandbox.rs`, `secretspec.rs`, the own lane, Keychain, `jobs_setup.rs`), plus Swift. The
  gate runs the plan's whole rust group on the Mac too when the job's diff touches macOS-only code:
  - any touched file whose copy on the branch or the base names `target_os = "macos"`, read at gate time
    (`jobs.rs`, `brain/tools.rs`, `conversation.rs`, `meter.rs`, `tests/jobs_api.rs`,
    `tests/accounts_api.rs` and 35 others today), so the rule can't go stale;
  - and `MACOS_ONLY` in `factory/bin/gate`, the one list of files whose macOS-only tests live in other
    files: `core/Cargo.lock`, obid's `Cargo.toml`, and under `core/crates/obid/` `src/sandbox.rs`,
    `egress.rs`, `process.rs`, `secretspec.rs`, `accounts.rs`, `acp.rs`, `agent.rs`, `codex.rs`,
    `cursor_usage.rs`, `job.rs`, `lib.rs`, `merge.rs`, `usage.rs`, `vault.rs`,
    `brain/harness_codex.rs`, `brain/harness_cursor.rs`, `jobs/setup.rs`, `jobs/lanes.rs`,
    `jobs/logins.rs`, `jobs/secrets.rs`, and `tests/support/mod.rs`, `tests/sandbox.rs`,
    `tests/jobs_setup.rs`, `tests/secretspec.rs`.

  When git can't tell, every touched file counts, and when in doubt a file joins the list. swift (which
  `apps/` and `mcp/` reach) and factory (the factory's own tests, which the VM never runs) run on the Mac
  whenever they are planned. A diff touching the check's own files (`scripts/oby-check.sh`,
  `scripts/oby-check-test.sh` or anything under `tools/oby-check-blok/`, REMOTE-10) runs the whole check
  on the Mac, with no VM use: `oby-check-test.sh` expects macOS's plan, so on the VM it would fail the
  change for the machine. The nightly full Mac run (R01-14) covers everything else
  every night. This is the same trade the owner accepted for merging in #2163.
- **The VM needs no secrets.** `obi-ai-bot/oby` is public, so the VM fetches commits without a token.
  The check runs offline after `cargo fetch` and `npm ci`. No owner login, agent login or vault value
  ever reaches it.
- **It decides nothing new.** The VM runs the same `scripts/oby-check.sh` (the branch's copy, as the Mac
  does today) with the same `oby-check-blok`, built from `origin/dev`. `checks.md` names the host.
- **The SSH rule follows the Mac's IP.** When SSH hasn't answered within 3 minutes, the gate reads this
  Mac's public IPv4 from `https://checkip.amazonaws.com` (AWS's plain-text echo, no key) and, when it is one
  global address other than the source of `oby-gateNSG/ssh-from-owner`, sets that rule's source to `<ip>/32`
  with `az network nsg rule update` and gives SSH 60 seconds more. `checks.md` says it moved the rule. The
  rule only ever holds one `/32`; no other rule or resource is touched (REMOTE-19, 20).
- **It falls back to the Mac.** If the VM can't be reached or started within 3 minutes, the remote gate
  couldn't judge or was cut off, or the commit isn't on origin, the gate runs the whole check on the Mac
  as without a VM, and `checks.md` says why. A real failure on the VM fails the gate.
- **The Mac stops it.** The VM has no managed identity, so it can't deallocate itself. Every gate that uses
  it records its use in `factory/jobs/remote-gate/state.json`, open while it runs and then ended;
  `factory/bin/job queue` runs `factory/bin/gate --idle`, which deallocates it (`az vm deallocate --no-wait`)
  once no use is open and the last one ended 30 minutes ago, whatever stage any job is in, so a gate running
  on the Mac doesn't keep it up. A use whose gate died counts as ended when it started (REMOTE-5, 21). Only
  deallocation stops compute billing; a stopped-but-allocated VM is billed as
  running. A VM the owner started by hand, with no gate since, is left to the 05:00 UTC shutdown.

## Measured (A00-03, 2026-11-13)

One real Rust commit, run on the VM by `oby-remote-gate` and on the Mac as a gate runs it today:

- **The commit:** `f608d154` (#2158, `core/crates/obid/src/gateway.rs`) against its parent `f3488512`. Its
  plan is rust (fmt, clippy on obid's targets, the lib's `gateway::` tests, `tests/gateway_api.rs` and
  `tests/routes_guard.rs`) and scan. #2147 (`44412fcb`) was the first choice, but its own line guard fails
  on both hosts (99,052 production lines against its limit of 99,032, which #2182 raised on `dev` later).

| Where | Time | Verdict |
|---|---|---|
| VM, cold (a new slot: empty build folder, crates fetched first) | 4 min 0 s | passed |
| VM, warm (the same slot again) | 1 min 18 s | passed |
| Mac, the factory's shared build folder, under its check lock | 4 min 35 s, after 38 min 20 s waiting for the lock behind other gates (42 min 55 s in all) | passed |

- **Setup:** `setup.sh`'s first run on the fresh VM took 1 min 35 s (packages, rustup, Node, gitleaks, the
  clone and `oby-check-blok`); its second, which installed nothing, 5 s.
- **Free disk after both runs:** `/srv/oby` (the OS disk) 234 GB free of 247 GB, 14 GB used, with one
  slot's build folder.
- **The VM's time in this job:** about 10 minutes running (two starts), about $0.25 at $1.45 an hour.

## Size and cost

The VM bought is the safe fallback below, pay-as-you-go, in East US 2. Prices are list prices for East US
in USD, as third-party trackers showed them in November 2026
([Azurespeed](https://www.azurespeed.com/AzureVmPricing/Standard_D32as_v6),
[Holori](https://calculator.holori.com/azure/vm/standard-d32ads-v6/eastus)). Check the Azure pricing
calculator before buying; regions and prices change.

| Option | Size | Per hour | 8 gate-hours a day | Notes |
|---|---|---|---|---|
| **Recommended** | Standard_D32as_v6 (32 vCPU, 128 GB), Spot, eviction = Deallocate | ≈$0.30–0.40 | ≈$3/day | An eviction mid-gate only reruns that gate on the Mac |
| Safe fallback | Standard_D32as_v6, pay-as-you-go | ≈$1.45 | ≈$12/day | No evictions |
| Smaller | Standard_D16as_v6, pay-as-you-go | ≈$0.73 | ≈$6/day | 2 slots, slower gates |

On top of that, the disk keeps paying while the VM is deallocated. The VM has one 256 GB OS disk, which
holds the clone and four slots' build folders (see the free disk measured above). A public IP is billed too unless SSH goes through Tailscale or Azure Bastion.

Expected effect, an estimate until the gate uses the VM (A00-03 measured one commit, above):
- **Gate time:** a Rust gate drops from 30–60 minutes to about 10–15 on 32 cores.
- **Throughput:** four gates run at once instead of one.
- **Flakes:** the Mac's load falls, so the timeout flakes mostly go.

## The owner's steps

1. **Install and sign in to the Azure CLI on this Mac:** `brew install azure-cli`, then `az login`.
   Or create the VM in the portal and skip to step 4.
2. **Create the VM.** Claude can do this once you say so: a resource group `oby-factory`, the size above,
   Ubuntu 24.04, SSH key only. The owner chose pay-as-you-go in East US 2 with one 256 GB OS disk.
   The network rule (`oby-gateNSG/ssh-from-owner`) allows SSH from your IP only; when your IP changes,
   the gate moves that rule's source to your new `/32` by itself (REMOTE-19).
3. **Set a budget alert** in Cost Management on the resource group (for example $150 a month), so a
   VM that fails to stop can't surprise you.
4. **Add an SSH host `oby-gate`** to `~/.ssh/config` and accept its host key once:
   `ssh oby-gate true`.

Steps 1 to 4 were done before A00-03 was built. After A00-10 merges:

5. **Install the merged `oby-remote-gate`** on the VM, so its groups run offline (REMOTE-9): start it if it
   is deallocated, then run `factory/remote/setup.sh oby-gate` from the main checkout.
6. **Turn the remote path on:** add this key to `factory/config.json` (protected, so the owner's edit). It
   sends every gate's Linux groups to the VM:
   `"remote_gate": { "ssh_host": "oby-gate", "resource_group": "oby-factory", "vm": "oby-gate" }`.
   Removing it, or setting it to `null`, turns the path off again.

## Risks

- **A dropped connection:** a gate in progress is lost (a Spot eviction would do the same, if the owner
  moves to Spot). `oby-remote-gate` stops the check and frees its slot when its SSH connection drops
  (REMOTE-7), and the gate runs that check on the Mac (REMOTE-3).
- **macOS-only coverage:** a job that breaks a macOS-only test without touching a file with a macOS
  cfg or one on the list is caught by the nightly Mac run, not its gate. Grow `MACOS_ONLY` when that
  happens.
- **Drift between the VM's toolchain and CI's:** the VM's setup script pins the same Rust toolchain
  (`RUSTUP_TOOLCHAIN` in `ci.yml`), Node 22, gitleaks and bubblewrap that `ci.yml` uses, reading them from
  `ci.yml` itself; its test fails when they can't be read or `ci.yml`'s copies disagree.
- **Security:** the VM runs code the factory's agents wrote, the same as the Mac's gate. It holds no
  secret and is reachable only by the owner's SSH key. The check runs as `oby`, which can use `sudo`, as
  the Mac's gate runs as the owner: a commit's check could change the VM's tools for later gates. A user
  without `sudo` for the check is the owner's call.

## The second Mac

Built by task A00-18, and off until the owner turns it on (below). The scenarios are
[REMOTE-13 to 18](contracts/remote-gate.md).

Once the VM's part has passed, a gate whose diff touches macOS-only code still runs its rust group on this
Mac, one at a time under the check lock (15 to 27 minutes each, 2 h 22 min of them on the night of
2026-11-13), and swift and factory when planned. The owner's MacBook Pro takes that part when it is free:

```
 This Mac (orchestrator, job files, the lock)          oby-mac-pro (Apple M1 Pro, 10 cores, 16 GB)
 ────────────────────────────────────────────          ───────────────────────────────────────────
 factory/bin/gate <job>
   ├─ the VM's part, as above; it passed
   ├─ mac_worker set, the macOS list not empty:
   │     ├─ ssh oby-mac-pro true, within 60 s ─────────▶ answers (asleep or off the network: no)
   │     └─ ssh oby-mac-pro oby-worker/bin/oby-remote-gate \
   │              --mac <commit> origin/dev rust swift factory
   │                                         ──────────▶ the first free slot N of 2, else
   │                                                     "both slots are busy" and exit 2 at once
   │                                                     fetch the commit (public repo, no token)
   │                                                     OBY_CHECK_ONLY=rust, swift, factory, offline
   │        ◀──── output streamed, exit 0/1/2 ──────────
   ├─ 0 or 1: the verdict; anything else: the same groups here, under the lock, as before
   └─ checks.md: HOST: oby-gate, oby-mac-pro | oby-gate, mac
```

- **One queue, one set of job files.** The Pro holds no factory state: only `~/oby-worker` (a full bare
  clone, two slots each with its own worktree, `target` and `cache`, and the runner in `bin/`) and
  `oby-check-blok` in `~/.cargo/bin`, built from `origin/dev`.
- **The same runner.** `oby-remote-gate --mac` is the VM's script with a flag: the same slot locks,
  checkout, steps, stop on a dropped connection and verdict. It reads no plan and runs exactly the groups the
  gate gives it, at half the Pro's cores each.
- **It waits for nothing.** With both slots held, the runner says so and ends at once, and the gate runs that
  part here as before: two gates' macOS parts run at once, one on each Mac. The factory never starts, wakes or
  stops the Pro, and its part never takes this Mac's lock.
- **The whole check stays here.** A change to the check's own files, a gate with no Linux group planned, and
  any fallback from the VM run the whole check on this Mac, as before.

### The owner's steps

1. **Check the Pro answers** from this Mac: `ssh oby-mac-pro true`.
2. **Set it up:** `factory/remote/setup-mac.sh oby-mac-pro` from the main checkout. It checks the Pro's tools
   (git, python3, Xcode, rustup, cargo, rustc, node, gitleaks, an `iPhone 17` simulator) and stops, writing
   nothing, when one is missing or the Pro's Rust or Xcode differs from this Mac's; then makes
   `~/oby-worker`, installs the runner, builds `oby-check-blok` from `origin/dev`, tightens the modes on the
   check's path and prints them, and its versions. Run it again whenever: what is there is left as it is.
3. **Turn it on:** add this key to `factory/config.json` (protected, so the owner's edit), beside
   `remote_gate`:
   `"mac_worker": { "ssh_host": "oby-mac-pro" }`
   Removing it, or setting it to `null`, turns it off. `FACTORY_MAC_WORKER` with the same JSON tries it for
   one run.

### Measured (A00-18, 2026-11-14)

One real obid commit, its rust group run on the Pro by the gate's own command and on this Mac (the Air) as a
gate runs it, through `factory/checks.sh` under the check lock:

- **The commit:** `68ac2282` (#2170, `core/crates/obid/src/sandbox.rs`, a `MACOS_ONLY` file, and six test
  files) against its parent `f2873943`. Its rust group: fmt, clippy on obid's targets, the lib's `jobs::`
  and `sandbox::` tests, and eight test binaries (`jobs_api`, `jobs_setup`, `merge_api`, `review_job`,
  `rooms`, `runner_api`, `sandbox`, `tetrix_api`).
- **On the Pro:** `ssh oby-mac-pro oby-worker/bin/oby-remote-gate --mac 68ac2282… f2873943… rust`, with the
  gate's ssh options and standard input an open pipe.

| Where | Time | Verdict |
|---|---|---|
| Pro, cold (a new slot: empty build folder, crates fetched first) | 11 min 7 s | failed: `merge_api`'s `merging_40_rounds_stay_within_reviews`, the race #2206 fixed on `dev` after this commit; every other test passed |
| Pro, warm (the same slot again) | 8 min 46 s | passed |
| Air, the factory's shared build folder, under its check lock | 12 min 44 s, after 12 min 6 s waiting for the lock behind another gate (24 min 50 s in all) | passed |

- **Setup:** `setup-mac.sh`'s first run took 33 s (the clone, the runner and `oby-check-blok`); its second,
  which installed nothing, 3 s. It reported Node v26.12.0 on the Pro against v24.22.2 here, and stopped on
  nothing.
- **Disk on the Pro:** the bare clone 30 MB, slot 1 after both runs 7.9 GB; 333 GB free.

### Risks

- **swift over SSH is unproven.** The real run above is rust only. The iPhone simulator that swift's
  `xcodebuild test` boots may need a logged-in session on the Pro. If it can't, swift there couldn't judge
  (2) or fails: watch the first gates with swift, and leave the key off until it is seen to work.
- **Homebrew's folders aren't tightened.** On the Pro the runner puts `/opt/homebrew/bin` and
  `/usr/local/bin` on PATH (gitleaks, node and python3 are Homebrew's), and `tighten.sh` covers only
  `~/.cargo`, `~/.rustup` and `~/oby-worker`. Anything that could write Homebrew's folders could change the
  tools a check runs there.
- **macOS's bash 3.2.** A command over SSH on the Pro gets `/bin/bash` 3.2, not Homebrew's bash, as
  `scripts/oby-check.sh` gets on this Mac: `tighten.sh` is tested under it, and the warm run above passed on it,
  but a later change to the check that needs bash 4 would fail there first.
- **The Pro is someone's laptop.** It sleeps, roams and runs the owner's own work. A gate that finds it
  asleep waits up to 60 seconds, then runs here; a gate on it while the owner works there is slower.
- **The same trust as the VM.** The Pro runs code the factory's agents wrote, as this Mac's gate does, in
  the owner's own account there. It holds no factory secret, and the check runs offline after `cargo fetch`.

## Builders' runs

The Mac's load sat at 31 to 37 on 10 cores with builders queued one at a time behind `test-run`'s lock while
the VM idled between gates. Now `factory/bin/test-run` sends a heavy run that doesn't need macOS to the VM,
with the builder's command line unchanged (REMOTE-22 to 31):

- **Where it goes.** The gate's own rule, imported from `factory/bin/gate`, never copied: a Swift, Xcode or
  whole-check run, a worktree whose changes against its base touch a file in `MACOS_ONLY` or naming
  `target_os = "macos"`, or a `--test` that is such a file, stays here under the lock, with one `on this Mac:`
  line.
- **What goes.** The merge-base with `origin/dev` (on origin, so the VM fetches it with no credential), the
  SHA of the worktree's tree written from an index of its own (nothing gitignored), and on ssh's standard input
  `git diff --binary --full-index` between them, after gitleaks with the main checkout's `.gitleaks.toml`
  passed it. No environment, `CARGO_TARGET_DIR`, token or login.
- **On the VM.** As `oby-builder`, a user with no sudo that `setup.sh` makes, with its own toolchain and
  `/srv/oby-builder`: `oby-remote-gate --run` takes one of `builder_slots` slots, never a gate's, so a gate
  never waits on a builder; applies the diff to the base, checks the tree's SHA, touches every file under
  `core/` and `tools/` (A00-23's rule, so another worktree's build in the slot's `target` never stands in for
  this one's, while dependencies stay built), and runs the command at nice 15, offline, with the slot's own
  Cargo folder and `cores / (slots + builder_slots)` jobs.
- **Fallback.** Anything before `running in builder slot <N>` (the VM unreachable after 3 minutes, no builder
  user, every builder slot busy for 10 minutes) runs it here exactly as before, with one `fell back to this
  Mac:` line. After it, the VM's exit code is the verdict. A stop or a dropped connection frees the slot.
- **Turning it on.** Run `factory/remote/setup.sh oby-gate` again, then add `"slots": 4, "builder_slots": 8`
  (on the 64-core VM) to `remote_gate` in `factory/config.json`. Until then nothing changes.
- **On the Mac worker (REMOTE-35 to 42).** A run that stays here only because it needs macOS (macOS-only code,
  or a macOS-only `--test`) goes to the Pro instead when `mac_worker` has `builder_slots`: test-run says `to the
  Mac worker oby-mac-pro: <why>` and sends the same tree and scanned diff with `ssh oby-mac-pro
  oby-worker/bin/oby-remote-gate --mac --run --slots 2 --builder-slots <m> …`, with no use, `az` or probe
  first. There it runs as the gates' own user, in `~/oby-worker/builders/builder-N` with a clone of its own,
  never a gate's `slot-1` or `slot-2`, each run in a slot emptied of all but its lock and `target`, at nice 15
  with `cores / (2 + builder_slots)` jobs. The same fallbacks apply (ssh's 255, no `builders/repo.git`, every
  builder slot busy for 10 minutes), and a fallback from either host runs here, never on the other. Swift,
  Xcode and whole-check runs stay here, as does a run started with `test-run --here -- …`, for one that needs
  this Mac itself (its Keychain, the owner's real home). Builders there can do what that user can (the
  contract's "Known limits on the Mac worker"): accepted with the count. To turn it on, run
  `factory/remote/setup-mac.sh oby-mac-pro` again, then set `"mac_worker": {"ssh_host": "oby-mac-pro",
  "builder_slots": 2}`.

## Still to build

- **The nightly check on the Pro:** `factory/bin/nightly` running there when it's free, so the nightly full
  check doesn't hold this Mac's lock for an hour.
- **Whole checks on the Pro:** a change to the check's own files, a gate with no Linux group planned, and a
  fallback from the VM, which all run the whole check on this Mac today.

- **Re-run `factory/remote/setup.sh oby-gate` on the real VM**, starting it only for this and deallocating
  it after, and record the modes it prints before and after (task A00-12's last step, not done by its pull
  request).
- **A check user without `sudo` for the gates** on the VM, as builders' runs already have (`oby-builder`): a
  change to how the gate reaches the VM.
- **Records in `~/.oby-flow/pipelines/runs.jsonl`**, the file the pipelines viewer reads: each gate's host,
  slot, commit, verdict and times.
- **The dashboard header:** the VM's state and its four slots in `factory/dashboard.html`.
