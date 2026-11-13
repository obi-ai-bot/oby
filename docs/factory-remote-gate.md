# The factory's gate on an Azure Linux VM

Status: planned (the owner's "Yes, plan the VM", 2026-11-13). Nothing exists in Azure yet. Task A00-03
(`factory/queue/tasks/A00-03.md`) builds the factory's side once the owner's steps below are done.

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
   ├─ remote? (VM configured, plan has rust/electron)
   │     ├─ az vm start (if deallocated) ───────────▶ boots in ~1 min, keeps its disk
   │     ├─ ssh oby-gate oby-remote-gate \
   │     │        <repo> <commit> <base> <slot> ────▶ git fetch the commit (public repo, no token)
   │     │                                            worktree in slot N, its own CARGO_TARGET_DIR
   │     │                                            OBY_CHECK_ONLY=rust, electron, scan
   │     │  ◀─── output streamed, exit 0/1/2 ──────── 
   │     └─ on the Mac too, only when the job touches macOS-only code:
   │           OBY_CHECK_ONLY=rust (macOS-only test files) and swift
   └─ round-N/checks.md: verdict, commit, checks.sh hash, HOST: oby-gate | mac
 idle 30 min ─────────────────────────────────────▶ the VM deallocates itself
```

- **Up to 4 gates run at once on the VM**, one per slot. Each slot has its own checkout and its own
  Cargo build folder, so one branch can never test another's build (the bug #2153 fixed on the Mac).
  A slot's lock replaces the Mac's single lock. The Mac's lock stays for what still runs there.
- **The Mac keeps the macOS-only suites.** About 70 tests only build on macOS (`cfg(target_os =
  "macos")`: `sandbox.rs`, `secretspec.rs`, the own lane, Keychain, `jobs_setup.rs`), plus Swift. The
  gate runs those on the Mac only when the job's diff touches the paths they cover (`sandbox.rs`,
  `egress.rs`, `process.rs`, `jobs/setup.rs`, `secretspec.rs`, lanes, keychain, `apps/`, `mcp/`). The
  nightly full Mac run (R01-14) covers everything else every night. This is the same trade the owner
  accepted for merging in #2163.
- **The VM needs no secrets.** `obi-ai-bot/oby` is public, so the VM fetches commits without a token.
  The check runs offline after `cargo fetch` and `npm ci`. No owner login, agent login or vault value
  ever reaches it.
- **It decides nothing new.** The VM runs the same `scripts/oby-check.sh` (the branch's copy, as the Mac
  does today) with the same `oby-check-blok`, built from `origin/dev`. `checks.md` names the host.
- **It falls back to the Mac.** If the VM can't be reached or started within 3 minutes, the gate runs
  on the Mac as it does today, and `checks.md` says why.
- **It stops itself.** A timer on the VM deallocates it after 30 minutes with no gate. Only deallocation
  stops compute billing; a stopped-but-allocated VM is billed as running.

## Size and cost

Prices are list prices for East US in USD, as third-party trackers showed them in November 2026
([Azurespeed](https://www.azurespeed.com/AzureVmPricing/Standard_D32as_v6),
[Holori](https://calculator.holori.com/azure/vm/standard-d32ads-v6/eastus)). Check the Azure pricing
calculator before buying; regions and prices change.

| Option | Size | Per hour | 8 gate-hours a day | Notes |
|---|---|---|---|---|
| **Recommended** | Standard_D32as_v6 (32 vCPU, 128 GB), Spot, eviction = Deallocate | ≈$0.30–0.40 | ≈$3/day | An eviction mid-gate only reruns that gate on the Mac |
| Safe fallback | Standard_D32as_v6, pay-as-you-go | ≈$1.45 | ≈$12/day | No evictions |
| Smaller | Standard_D16as_v6, pay-as-you-go | ≈$0.73 | ≈$6/day | 2 slots, slower gates |

On top of that, the disk keeps paying while the VM is deallocated. A 512 GB Premium SSD holds four
slots' build folders. A public IP is billed too unless SSH goes through Tailscale or Azure Bastion.

Expected effect, an estimate until A00-03 measures it:
- **Gate time:** a Rust gate drops from 30–60 minutes to about 10–15 on 32 cores.
- **Throughput:** four gates run at once instead of one.
- **Flakes:** the Mac's load falls, so the timeout flakes mostly go.

## The owner's steps (before A00-03 can be built)

1. **Install and sign in to the Azure CLI on this Mac:** `brew install azure-cli`, then `az login`.
   Or create the VM in the portal and skip to step 4.
2. **Create the VM.** Claude can do this once you say so: a resource group `oby-factory`, the size above,
   Ubuntu 24.04, SSH key only, a 512 GB Premium SSD data disk, Spot with eviction policy Deallocate.
   The network rule allows SSH from your IP only, or use Tailscale.
3. **Set a budget alert** in Cost Management on the resource group (for example $150 a month), so a
   VM that fails to stop can't surprise you.
4. **Add an SSH host `oby-gate`** to `~/.ssh/config` and accept its host key once:
   `ssh oby-gate true`.

## Risks

- **Spot eviction:** a gate in progress is lost. The gate notices the dropped connection and reruns that
  gate on the Mac.
- **macOS-only coverage:** a job that breaks a macOS-only test without touching the listed paths is
  caught by the nightly Mac run, not its gate. Grow the path list when that happens.
- **Drift between the VM's toolchain and CI's:** the VM's setup script pins the same Rust toolchain
  (`RUSTUP_TOOLCHAIN` in `linux.yml`), Node 22, gitleaks and bubblewrap that the Linux workflow uses.
  A00-03 checks they match.
- **Security:** the VM runs code the factory's agents wrote, the same as the Mac's gate. It holds no
  secret and is reachable only by the owner's SSH key.
