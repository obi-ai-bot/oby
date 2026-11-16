# Oby's factory on Azure: what exists, how it works, how to run it, and how to rebuild it anywhere

This guide is for whoever runs Oby's software factory next, on this Azure account or another one. It says what the factory has on Azure today, how each piece is used, the commands to look at it, change it, pay for it and stop it, and how to build the same thing from nothing on a new account. The design history and the test scenarios live in [factory-remote-gate.md](factory-remote-gate.md) and [contracts/remote-gate.md](contracts/remote-gate.md) (REMOTE-1 to 65). This guide is the operating manual.

Everything here was read from the live account on 2026-11-15, with read-only commands, and from the repository at `origin/dev` (`2f458a8c`). Values that belong to one person's account are not written down: the subscription id, the tenant, the VM's public IP, the owner's own IP, and SSH key files. The command that reads each one is given instead, so this file is safe in a public repository.

## In plain words

Oby's factory builds roadmap tasks with AI agents. Every change must pass Oby's whole check (Rust, Electron, the secret scan, formal proofs), which takes 30 to 60 minutes on a laptop that is also running agents. So the factory rents one big Linux computer on Azure, the **gate VM**, and sends it the heavy, Linux-only part of every check, eight at a time.

Think of the Mac as the foreman and the VM as a rented workshop. The foreman keeps every plan and every decision. The workshop only does the work it is handed, holds no secrets, and is switched off when nobody has used it for 30 minutes, so it is paid for only while it works.

Three things use the VM:

- **The gate** (`factory/bin/gate`): every job's check, the Linux part, in one of 8 **gate slots**.
- **Builders' heavy runs** (`factory/bin/test-run`): an agent's own `cargo test`, `clippy`, Kani proofs, `cargo mutants` and Quint models, in one of 8 **builder slots**, as a user with no `sudo`.
- **Oby's own factory feeder** (`factory/bin/oby-feed`): it verifies the Quint models a pull request adds, on the VM, before merging it.

The macOS-only part of a check still runs on a Mac: this one, or the second Mac (`oby-mac-pro`, not on Azure). Oby itself (obid, the Electron app, conversations) uses no Azure today.

## 1. What exists on Azure today

**Subscription:** the one `az account show` prints as default. Read its name and id with:

```bash
az account show --query "{name:name, id:id, tenant:tenantId, user:user.name}" -o table
```

**Resource group `oby-factory`, region `eastus2` (East US 2).** Everything the factory owns is in it, and nothing else is:

| Resource | Type | What it is | Cost |
|---|---|---|---|
| `oby-gate` | Virtual machine | The gate VM. **Standard_D32as_v6** (32 vCPU AMD, 128 GB RAM), Ubuntu 24.04 LTS (Canonical `ubuntu-24_04-lts`, `server`), pay-as-you-go (not Spot), admin user `oby`, SSH key only, no managed identity | CAD 2.04 an hour **only while running**; nothing for compute while deallocated |
| `oby-gate_OsDisk_1_…` | Managed disk | The OS disk: 256 GB, Premium SSD (P15). Holds `/`, the toolchains, `/srv/oby` (gate slots 1 to 4) and `/srv/oby-builder` | About USD 38 a month, **always**, even deallocated |
| `oby-gate-slots` | Managed disk | A data disk added 2026-11-15: 256 GB, Premium SSD (P15), LUN 0, ext4 labelled `oby-slots`, mounted at `/srv/oby-x`. Holds gate slots 5 to 8 | About USD 38 a month, always |
| `oby-gatePublicIP` | Public IP | Standard SKU, static. How the Mac reaches the VM over SSH | A few dollars a month, always |
| `oby-gateNSG` | Network security group | The firewall. One inbound rule, `ssh-from-owner` (priority 1000, TCP 22, Allow), whose source is the owner's current IP as a `/32` | Free |
| `oby-gateVNET`, `oby-gateVMNic` | Virtual network, network card | Made by `az vm create`; nothing to change | Free |
| `shutdown-computevm-oby-gate` | Auto-shutdown schedule | Shuts the VM down every day at **05:00 UTC**, a safety net if the Mac never deallocates it | Free |
| `oby-factory-monthly` | Budget (on the group) | **CAD 150 a month**, with alerts | Free |

The names `oby-gateVNET`, `oby-gatePublicIP`, `oby-gateNSG` and `oby-gateVMNic` are the defaults `az vm create --name oby-gate` makes; the factory's code relies only on `oby-gate`, `oby-factory`, `oby-gateNSG` and `ssh-from-owner`.

**This month's spend** on 2026-11-15: CAD 78.43 of the CAD 150 budget.

### Inside the VM

| What | Where | Notes |
|---|---|---|
| Gate user | `oby` (the admin user, can `sudo`) | Runs every gate. Owns `/srv/oby` (`0700`, nobody else can enter). |
| Gate slots | `/srv/oby/slot-1` to `slot-4` on the OS disk; `slot-5` to `slot-8` are symlinks to `/srv/oby-x/slot-5..8` on the data disk | Each slot: its own git worktree, its own Cargo build folder and its own lock, so one branch never tests another's build. |
| The repository | `/srv/oby/repo.git`, a full bare clone of `obi-ai-bot/oby` (`gc.auto 0`) | Public, so it fetches with no token. |
| Builder user | `oby-builder` (uid 1001), **no `sudo`**, no admin group | Runs builders' heavy commands. Its home, `.ssh`, `authorized_keys` and toolchain belong to root, so a run can't add a key or change the next run's tools. |
| Builder slots | `/srv/oby-builder/builder-1..N` and its own bare clone `repo.git` | Made on demand by `oby-remote-gate --run`. |
| The runner | `/usr/local/bin/oby-remote-gate` | The repository's `factory/remote/oby-remote-gate`, installed by `setup.sh`. |
| The check's tool | `oby-check-blok` in `~oby/.cargo/bin` | Built from `origin/dev`, never from a commit being checked. |
| Toolchains | Rust at CI's pin, Node at CI's major, gitleaks at CI's pin and SHA-256, bubblewrap, slirp4netns, zsh, OpenJDK 21 | Read from `.github/workflows/ci.yml`, never copied: `factory/remote/setup.sh --pins` prints them. |
| Builder-only tools | Kani (setup.sh's own pin), Quint 0.33.1 with Apalache, cargo-mutants 27.1.0 | Installed root's in `oby-builder`'s home; nothing downloads during a run. |
| Kernel setting | `/etc/sysctl.d/…`: `kernel.apparmor_restrict_unprivileged_userns = 0` | Lets bubblewrap make user namespaces, as CI does. |
| Data disk mount | `/etc/fstab`: `LABEL=oby-slots /srv/oby-x ext4 defaults,nofail,noatime 0 2` | `nofail`: the VM still boots if the disk is missing. **Added by hand on 2026-11-15; `setup.sh` doesn't do it** (see "Add a data disk"). |

On 2026-11-15 the OS disk was **86% full (35 GB free of 247 GB)**, mostly gate slots 1 to 4's build folders. See "Free disk on the VM".

### How the factory is told about it: `factory/config.json`

```json
"remote_gate": { "ssh_host": "oby-gate", "resource_group": "oby-factory", "vm": "oby-gate", "slots": 8, "builder_slots": 8 },
"gate_shards": { "linux": 4, "mac": 2 },
"mac_worker": { "ssh_host": "oby-mac-pro", "builder_slots": 2 }
```

| Key | Meaning |
|---|---|
| `remote_gate.ssh_host` | The SSH host name, from `~/.ssh/config` |
| `remote_gate.resource_group`, `remote_gate.vm` | What `az vm start`, `deallocate` and `get-instance-view` act on |
| `remote_gate.nsg`, `remote_gate.nsg_rule` (optional) | The firewall rule the gate may move to the Mac's new IP; by default `<vm>NSG` and `ssh-from-owner` |
| `remote_gate.slots` | Gate slots on the VM (1 to 64). Size it to the cores: each slot gets `cores / (slots + builder_slots)` build jobs |
| `remote_gate.builder_slots` | Builder slots on the VM |
| `gate_shards` | One gate's Rust tests split into `linux` shards on the VM and `mac` shards on the Macs, all at once |
| `mac_worker` | The second Mac (not Azure) |

Removing `remote_gate`, or setting it to `null`, turns the VM off for the factory: every check runs on the Mac again. `FACTORY_REMOTE_GATE='<same JSON>'` turns it on for one run. `factory/config.json` is a protected path: changes to it are the owner's.

### How the Mac reaches it: `~/.ssh/config`

```
Host oby-gate
  HostName <the VM's public IP>
  User oby
  IdentityFile <the owner's private key>
  IdentitiesOnly yes
  ServerAliveInterval 30
```

Read the IP with `az vm show -d -g oby-factory -n oby-gate --query publicIps -o tsv`. The private key never leaves the Mac. The VM holds only the public key, in `~oby/.ssh/authorized_keys` (and, for builders, root's copy in `~oby-builder/.ssh/authorized_keys`).

## 2. How it runs, step by step

```
 This Mac (the foreman)                                       Azure VM oby-gate (the workshop)
 ───────────────────────                                      ─────────────────────────────────
 factory/bin/gate <job>
   ├─ az vm get-instance-view → deallocated? az vm start ───▶ boots in about a minute, disks kept
   ├─ ssh oby-gate true (3 min) fails?
   │     └─ reads this Mac's IP (checkip.amazonaws.com) and
   │        az network nsg rule update … ssh-from-owner <ip>/32, then 60 s more
   ├─ ssh oby-gate oby-remote-gate --slots 8 … <commit> <base> ▶ first free gate slot; fetch commit; run
   │                                                             the Linux groups offline after cargo fetch
   │     ◀── output streamed, exit 0 pass / 1 fail / 2 couldn't judge ──
   ├─ macOS-only part on this Mac or oby-mac-pro
   └─ records the VM use in factory/jobs/remote-gate/state.json

 factory/bin/test-run -- cargo test …   (a builder's run)
   └─ gitleaks the diff, then ssh -l oby-builder oby-gate oby-remote-gate --run … ▶ a builder slot, nice 15

 factory/bin/oby-feed   (Oby's own factory)
   └─ ssh oby-gate quint verify <model> --server-endpoint=localhost:8900+ ▶ the model's invariants and witnesses

 factory/bin/job queue → factory/bin/gate --idle
   └─ no VM use open and the last one ended 30 min ago → az vm deallocate --no-wait
```

- **Nothing secret reaches the VM.** The repository is public; the check runs offline after `cargo fetch` and `npm ci`; builders' diffs pass gitleaks before they leave the Mac. No agent login, owner login, token or vault value is ever sent.
- **It falls back.** VM unreachable for 3 minutes, a slot that couldn't judge, a commit not on origin: the check runs on the Mac and `round-N/checks.md` says why. A real failure on the VM fails the gate.
- **It stops itself, from the Mac.** The VM has no managed identity, so it can't deallocate itself. The Mac does it 30 minutes after the last use; the 05:00 UTC auto-shutdown catches anything missed. **Only deallocation stops billing:** a VM stopped from inside (`sudo shutdown`) is still billed as running.
- **Apalache ports.** Quint's model checker listens on a local port: builder slot N uses `8822 + N` (8823 to 8886), and `oby-feed` uses 8900 upward (`APALACHE_PORT` in `factory/bin/oby-feed`), so runs never share a server.

## 3. Everyday commands

Run them on the Mac, signed in with `az login`. Commands that only read are safe at any time.

### Look

```bash
# Is it running? (VM running / VM deallocated)
az vm get-instance-view -g oby-factory -n oby-gate --query "instanceView.statuses[1].displayStatus" -o tsv

# Everything in the group
az resource list -g oby-factory -o table

# Its size, image and disks
az vm show -g oby-factory -n oby-gate --query "{size:hardwareProfile.vmSize, image:storageProfile.imageReference.sku, disks:storageProfile.dataDisks[].name}" -o json
az disk list -g oby-factory --query "[].{name:name, gb:diskSizeGB, tier:tier, state:diskState}" -o table

# The firewall rule (its source is who may SSH in)
az network nsg rule show -g oby-factory --nsg-name oby-gateNSG -n ssh-from-owner --query "{source:sourceAddressPrefix, port:destinationPortRange}" -o table

# The auto-shutdown time
az resource show -g oby-factory -n shutdown-computevm-oby-gate --resource-type Microsoft.DevTestLab/schedules --query "{time:properties.dailyRecurrence.time, tz:properties.timeZoneId, status:properties.status}" -o table

# Who is using it right now (the factory's own record of VM uses)
cat factory/jobs/remote-gate/state.json

# Inside it: cores, memory, disks, load, what runs in each slot
ssh oby-gate 'nproc; free -g | head -2; df -h / /srv/oby-x; uptime; ls /srv/oby /srv/oby-builder'
```

### Money

```bash
# This month's spend against the budget, in CAD
az consumption budget list -g oby-factory --query "[].{budget:name, amount:amount, spent:currentSpend.amount, currency:currentSpend.unit}" -o table

# The hourly price of a size, from Azure's public price list (no sign-in), in CAD
curl -s "https://prices.azure.com/api/retail/prices?currencyCode=CAD&\$filter=serviceName%20eq%20'Virtual%20Machines'%20and%20armRegionName%20eq%20'eastus2'%20and%20armSkuName%20eq%20'Standard_D32as_v6'%20and%20priceType%20eq%20'Consumption'" \
  | python3 -c "import json,sys; [print(i['skuName'], i['retailPrice'], i['unitOfMeasure']) for i in json.load(sys.stdin)['Items'] if 'Windows' not in i['productName']]"

# The factory's model spend (Claude, Codex), from its own job files, in USD
python3 -c "import json,glob; print(round(sum(json.load(open(f)).get('cost_usd') or 0 for f in glob.glob('factory/jobs/*/job.json')), 2))"
```

Known prices in East US 2 (Linux, pay-as-you-go, CAD, November 2026): D16as_v6 about 1.02 an hour (half a D32's), **D32as_v6 2.04**, D64as_v6 4.08. A D32 running 8 hours a day is about CAD 16 a day; left running for a month, about CAD 1,490. The two disks and the IP cost about CAD 110 a month whether the VM runs or not.

### Start, stop, connect

```bash
az vm start      -g oby-factory -n oby-gate               # the gate does this by itself when it needs it
az vm deallocate -g oby-factory -n oby-gate --no-wait     # stops billing for compute; disks are kept
ssh oby-gate                                              # a shell as oby
ssh -l oby-builder oby-gate                               # a shell as the builder user (no sudo)
```

Before deallocating by hand, check that no gate or builder run is using it: `cat factory/jobs/remote-gate/state.json` (an entry with no end is a use still open) and `ssh oby-gate 'pgrep -fa oby-remote-gate'`.

### When your IP changed

The gate moves the firewall rule to the Mac's new IP by itself after 3 minutes without SSH. To do it now:

```bash
az network nsg rule update -g oby-factory --nsg-name oby-gateNSG -n ssh-from-owner \
  --source-address-prefixes "$(curl -s https://checkip.amazonaws.com)/32"
```

### After `factory/remote/` changes on dev

```bash
factory/remote/setup.sh oby-gate            # the VM: tools at their pins, the runner, oby-check-blok, the users and modes
factory/remote/setup-mac.sh oby-mac-pro     # the second Mac (not Azure)
factory/remote/setup.sh --pins              # just print the pins it would install; touches nothing
```

Run them from the main checkout, with the VM started and no gate or builder run going. They are safe to run again: what is already at its pin is left as it is (about 5 seconds when nothing changes).

### Free disk on the VM

Build folders grow with every branch. To see and free space:

```bash
ssh oby-gate 'df -h / /srv/oby-x; du -sh /srv/oby/slot-*/ /srv/oby-x/slot-*/ /srv/oby-builder/builder-*/ 2>/dev/null'
# with no gate or builder run going, empty one slot's build folder (it rebuilds on its next use):
ssh oby-gate 'rm -rf /srv/oby/slot-1/target'
```

## 4. Changing it

Every change below raises or lowers what the owner pays: decide it first, and say the new monthly figure.

### Change the VM's size

The size is the price and the speed. Steps:

```bash
az vm list-vm-resize-options -g oby-factory -n oby-gate --query "[?contains(name,'as_v6')].name" -o tsv  # what it can become
az vm deallocate -g oby-factory -n oby-gate                                                              # wait for it
az vm resize     -g oby-factory -n oby-gate --size Standard_D64as_v6
```

Then size the slots to the cores in `factory/config.json` (each slot gets `cores / (slots + builder_slots)` build jobs): 16 cores, 4 + 4; 32 cores, 8 + 8 (today); 64 cores, 8 + 8 or more. The VM keeps its disks, IP and setup through a resize. The VM was bought as a D32 and has been resized since (a D16 for part of 2026-11-15); on 2026-11-15 it is a D32 again.

### Add a data disk (what was done on 2026-11-15)

The OS disk filled with build folders, so a second disk was added for slots 5 to 8:

```bash
az disk create -g oby-factory -n oby-gate-slots --size-gb 256 --sku Premium_LRS -l eastus2
az vm disk attach -g oby-factory --vm-name oby-gate --name oby-gate-slots --lun 0
ssh oby-gate '
  DEV=$(lsblk -dnpo NAME,SIZE | awk "\$2==\"256G\"{print \$1}" | tail -1)   # check this is the new, empty disk
  sudo mkfs.ext4 -L oby-slots "$DEV"
  sudo mkdir -p /srv/oby-x
  echo "LABEL=oby-slots /srv/oby-x ext4 defaults,nofail,noatime 0 2" | sudo tee -a /etc/fstab
  sudo mount /srv/oby-x && sudo chown oby:oby /srv/oby-x && chmod 700 /srv/oby-x
  for n in 5 6 7 8; do mkdir -p /srv/oby-x/slot-$n; ln -sfn /srv/oby-x/slot-$n /srv/oby/slot-$n; done'
```

Then raise `remote_gate.slots`. Check the device name before formatting: `mkfs` erases the disk it is given. This step is not in `setup.sh` yet, so a VM rebuilt from scratch needs it run by hand.

### Spot instead of pay-as-you-go (cheaper, can be evicted)

Spot costs roughly a quarter of pay-as-you-go, but Azure may take the VM back at any time; a gate cut off that way only reruns on the Mac. The priority can't be changed on an existing VM: make a new VM from the same disks, or rebuild (section 5) with `--priority Spot --eviction-policy Deallocate --max-price -1`.

### The auto-shutdown and the budget

```bash
az vm auto-shutdown -g oby-factory -n oby-gate --time 0500           # UTC, HHMM
az vm auto-shutdown -g oby-factory -n oby-gate --off                 # removes the safety net: not advised
az consumption budget update -g oby-factory --budget-name oby-factory-monthly --amount 200
```

Budget alerts (the e-mails at 80% and 100%) are set in the portal: Cost Management, Budgets, `oby-factory-monthly`, Alert conditions.

### Turn the VM off for the factory, without deleting it

Remove `remote_gate` from `factory/config.json` (or set it to `null`) and deallocate the VM. Every check runs on the Mac again. Disks keep costing until deleted.

### Delete everything

```bash
az group delete -n oby-factory --yes --no-wait     # the VM, both disks, IP, network, schedule and budget: cannot be undone
```

## 5. Rebuild it on another Azure account (or a new subscription)

This is the whole setup, from nothing, in the order it was done. Names are the ones the factory expects; change them and change `factory/config.json` to match.

1. **The Azure CLI on the Mac, signed in to the account to use:**

   ```bash
   brew install azure-cli
   az login                                   # a browser opens; sign in to the new account
   az account list -o table                   # find the subscription
   az account set --subscription "<subscription name or id>"
   ```

2. **Check the size is allowed and has quota in the region** (a new subscription often has a low vCPU quota):

   ```bash
   az vm list-usage -l eastus2 --query "[?contains(name.value,'DASv6') || name.value=='cores'].{family:name.localValue, used:currentValue, limit:limit}" -o table
   ```

   If the limit is below 32 (or the size you want), ask for more in the portal: Quotas, Compute, the region, "Standard DASv6 Family vCPUs".

3. **The resource group and the VM:**

   ```bash
   az group create -n oby-factory -l eastus2
   az vm create -g oby-factory -n oby-gate \
     --image Canonical:ubuntu-24_04-lts:server:latest \
     --size Standard_D32as_v6 \
     --admin-username oby --ssh-key-values ~/.ssh/<your key>.pub \
     --os-disk-size-gb 256 --storage-sku Premium_LRS \
     --public-ip-sku Standard --nsg-rule NONE
   ```

   This makes `oby-gateVNET`, `oby-gatePublicIP`, `oby-gateNSG` and `oby-gateVMNic` with the names the factory expects.

4. **Allow SSH from your IP only** (the gate moves this rule when your IP changes):

   ```bash
   az network nsg rule create -g oby-factory --nsg-name oby-gateNSG -n ssh-from-owner \
     --priority 1000 --direction Inbound --access Allow --protocol Tcp --destination-port-ranges 22 \
     --source-address-prefixes "$(curl -s https://checkip.amazonaws.com)/32"
   ```

5. **The safety nets:**

   ```bash
   az vm auto-shutdown -g oby-factory -n oby-gate --time 0500
   az consumption budget create -g oby-factory --budget-name oby-factory-monthly --amount 150 \
     --category cost --time-grain monthly --start-date "$(date +%Y-%m-01)" --end-date 2030-12-31
   ```

   Then add the budget's alert e-mails in the portal. The budget's currency is the account's billing currency.

6. **The second disk for slots 5 to 8,** if you'll run 8 gate slots: section 4, "Add a data disk".

7. **The SSH host on the Mac,** then accept the host key once:

   ```bash
   cat >> ~/.ssh/config <<'EOF'
   Host oby-gate
     HostName <the IP from: az vm show -d -g oby-factory -n oby-gate --query publicIps -o tsv>
     User oby
     IdentityFile ~/.ssh/<your key>
     IdentitiesOnly yes
     ServerAliveInterval 30
   EOF
   ssh oby-gate true
   ```

8. **Set the VM up,** from the repository's main checkout: `factory/remote/setup.sh oby-gate`. It installs every tool at CI's pins, the runner, `oby-check-blok`, `/srv/oby` with its clone and slots, and the `oby-builder` user with Kani, Quint and cargo-mutants, and tightens every mode on the check's path. First run about 2 minutes.

9. **Tell the factory,** in `factory/config.json` (the owner's edit; it's a protected path):

   ```json
   "remote_gate": { "ssh_host": "oby-gate", "resource_group": "oby-factory", "vm": "oby-gate", "slots": 8, "builder_slots": 8 }
   ```

   Add `"gate_shards": {"linux": 4, "mac": 2}` for sharded gates once it works.

10. **Prove it** with one real gate: start any factory job's gate and read its `round-N/checks.md`: `HOST: oby-gate, …` and a verdict. Then let `factory/bin/job queue` deallocate it, and check `az vm get-instance-view` says deallocated 30 minutes later.

Several people or Macs can share one gate VM only by sharing the `oby-gate` SSH key and the firewall rule; the rule holds one `/32`, so each Mac would move it away from the other. One VM per factory Mac is simpler.

## 6. Using it for other work, or on another cloud

**What is Azure-specific.** Only four things in `factory/bin/gate`: reading the power state (`az vm get-instance-view`), starting (`az vm start`), deallocating when idle (`az vm deallocate`), and moving the SSH rule (`az network nsg rule show/update`). Everything else (the runner, the slots, setup, builders, Quint) is plain SSH to an Ubuntu host.

**Another cloud (AWS, Google Cloud, Hetzner) or a Linux box of your own.** `setup.sh` and `oby-remote-gate` work on any Ubuntu 24.04 host reachable over SSH. The gate's start, stop and firewall calls would need the other cloud's equivalents (`aws ec2 start-instances` and `stop-instances` plus a security group, `gcloud compute instances start` and `stop` plus a firewall rule); that is a change to `factory/bin/gate`, not done yet. An always-on machine needs no start or stop at all, but the gate today expects `az` when `remote_gate` is set.

**Oby's conversations and obid.** Nothing of Oby itself runs on Azure today. A cloud VM is a different trust level from the gate VM: the gate VM holds no secret and runs only public code, while obid runs the owner's agents with their logins and vault. Running obid on a cloud VM (task R12's headless obid) would put the owner's agent logins there, so it must follow AGENTS.md's "Logins and secrets" in full and is the owner's decision, not a copy of this setup.

**A second factory (another product, another repository).** Use a resource group of its own, its own VM and SSH host name, and its own `remote_gate` block; the runner's clone is the repository `setup.sh` is run from.

## 7. When something goes wrong

| What you see | Why | What to do |
|---|---|---|
| `checks.md` says the gate fell back to the Mac: SSH didn't answer | Your IP changed and the rule wasn't moved yet, or the VM is starting | It moves the rule by itself; or run "When your IP changed" |
| `az couldn't start oby-gate` | Not signed in, wrong subscription, or no quota | `az account show`; `az login`; check the quota (section 5, step 2) |
| The VM is running and nothing uses it | A hand start, or a gate that died | Check `state.json` and `pgrep`, then `az vm deallocate … --no-wait` |
| Gates fail with "No space left on device" | Build folders filled a disk | "Free disk on the VM" |
| `setup.sh` stops on a pin | `ci.yml`'s copies of a pin disagree, or a tool's SHA-256 didn't match | Fix `ci.yml`; never edit `setup.sh`'s check away |
| A builder run "fell back to this Mac: every builder slot busy" | Builders outnumber `builder_slots` | Raise `builder_slots` (and the size, if the cores are taken) |
| The bill is higher than expected | The VM ran idle, or a bigger size | `az consumption budget list`; Cost Management, Cost analysis, group by resource |

## 8. Still to do

- `setup.sh` doesn't make the data disk or the slots 5 to 8 symlinks: a rebuilt VM needs section 4's commands by hand.
- The OS disk was 86% full on 2026-11-15: move slots 1 to 4's build folders to the data disk too, or empty them now and then.
- Gates still run as `oby`, which can `sudo`; builders already run without it. A gate user without `sudo` is the owner's call.
- Starting, stopping and the firewall rule are Azure-only in `factory/bin/gate` (section 6).
