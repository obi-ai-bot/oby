# The factory's gate on an Azure Linux VM

Status: built, and off until the owner turns it on (the owner's "Yes, plan the VM", 2026-11-13). Task A00-03
built the VM's side: `factory/remote/setup.sh` provisions it and `factory/remote/oby-remote-gate` runs a
commit's Linux check groups in one of four slots there. Task A00-10 built the gate's: `factory/bin/gate` runs
those groups on the VM, starts it when it is stopped, keeps the macOS-only part on the Mac and falls back to
the Mac, and the Mac's `factory/bin/job queue` deallocates the VM once it is idle. Gates use the VM once the
owner adds `remote_gate` to `factory/config.json` ("The owner's steps", below). The scenarios are
[REMOTE-1 to 9, REMOTE-11 and REMOTE-12](contracts/remote-gate.md).

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
   ├─ remote? (remote_gate set, plan has rust, electron or scan, the commit on origin)
   │     ├─ az vm start (if deallocated) ───────────▶ boots in ~1 min, keeps its disk
   │     ├─ ssh oby-gate oby-remote-gate \
   │     │        <commit> <base> ──────────────────▶ the first free slot N of 4, or wait for one
   │     │                                            git fetch the commit (public repo, no token)
   │     │                                            worktree in slot N, its own CARGO_TARGET_DIR
   │     │                                            OBY_CHECK_ONLY=rust, electron, scan
   │     │  ◀─── output streamed, exit 0/1/2 ──────── 
   │     └─ then on the Mac, once the VM's part passed: OBY_CHECK_ONLY=swift when planned,
   │           and OBY_CHECK_ONLY=rust when the diff touches a macOS-only path
   └─ round-N/checks.md: verdict, commit, checks.sh hash, HOST: oby-gate | oby-gate, mac | mac
 factory/bin/job queue → gate --idle
   └─ idle 30 min, no job in gate or merging: az vm deallocate --no-wait ──▶ deallocated
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
  `apps/` and `mcp/` reach) runs on the Mac whenever it is planned. The nightly full Mac run (R01-14) covers everything else
  every night. This is the same trade the owner accepted for merging in #2163.
- **The VM needs no secrets.** `obi-ai-bot/oby` is public, so the VM fetches commits without a token.
  The check runs offline after `cargo fetch` and `npm ci`. No owner login, agent login or vault value
  ever reaches it.
- **It decides nothing new.** The VM runs the same `scripts/oby-check.sh` (the branch's copy, as the Mac
  does today) with the same `oby-check-blok`, built from `origin/dev`. `checks.md` names the host.
- **It falls back to the Mac.** If the VM can't be reached or started within 3 minutes, the remote gate
  couldn't judge or was cut off, or the commit isn't on origin, the gate runs the whole check on the Mac
  as without a VM, and `checks.md` says why. A real failure on the VM fails the gate.
- **The Mac stops it.** The VM has no managed identity, so it can't deallocate itself. Every gate that uses
  it stamps `factory/jobs/remote-gate/state.json`; `factory/bin/job queue` runs `factory/bin/gate --idle`,
  which deallocates it (`az vm deallocate --no-wait`) once that stamp is 30 minutes old and no job is in
  `gate` or `merging`. Only deallocation stops compute billing; a stopped-but-allocated VM is billed as
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
   The network rule allows SSH from your IP only, or use Tailscale.
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

## Still to build

- **Re-run `factory/remote/setup.sh oby-gate` on the real VM**, starting it only for this and deallocating
  it after, and record the modes it prints before and after (task A00-12's last step, not done by its pull
  request).
- **A check user without `sudo`** on the VM, separate from the admin user `oby` (a change to `setup.sh` and
  to how the gate reaches the VM).
- **Records in `~/.oby-flow/pipelines/runs.jsonl`**, the file the pipelines viewer reads: each gate's host,
  slot, commit, verdict and times.
- **The dashboard header:** the VM's state and its four slots in `factory/dashboard.html`.
