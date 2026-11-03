# The test rig: two Obys and a Sphere on one Mac

The business behavior contract for `e2e/rig.mjs`, from [the template](../business-contract-template.md). Scenarios are numbered `RIG-n`, and those that need no Sphere or obid are tests named after them: `node --test e2e/rig.test.mjs e2e/lib/rig-guard.test.mjs`, on scratch folders with a stand-in `HOME`. The others are run by running the rig, on a real Mac.

## Feature and status

- **Outcome:** anyone working on talking to a person's Oby ([ask-an-oby.md](ask-an-oby.md)) can have a real second Oby with real work, with one command, and throw it away with another. A milestone is tested on it before it's called done.
- **Status:** **Implemented.** Run for real on stand-ins, and with the real Claude Code up to its missing login. **Not run yet:** a real job and a real answer, which need the owner's sign-in below.
- **Scope:** one Mac. **Out of scope:** the second pass on a second Mac and on the live Sphere (PEER-18), and changing obi-sphere.

## Behavior

### Actors and ownership

- **You** (`you/`, owner id `alex`, on port 8794): the scratch obid the Electron dev app points at. Always on stand-ins: asking from a thread runs no model of yours, and `ask` drives your own chat's `oby_ask` with exact words.
- **Dana** (`dana/`, owner id `dana`, on port 8795): a test teammate's obid with its own job. On the real Claude Code once its account is signed in, else on stand-ins (`e2e/rig-agent`).
- **The Sphere** (on the first of ports 8791 to 8793 free at the first start, then always that one): `server/scripts/local.mjs` from a scratch clone of obi-sphere at the commit in `e2e/rig-sphere.ref`, persisted in the rig's folder. The rig is its admin (`rig-admin`).
- **The owner** signs Dana's account in once. Nothing else can.

### The owner's step

`start` prints the command until it's done, with the account's folder filled in:

```
CLAUDE_CONFIG_DIR="<rig>/dana/home/accounts/<id>" claude auth login
```

Then `node e2e/rig.mjs stop` and `start`. The account is one Dana's obid added, so it refreshes its own login (AGENTS.md, #877). Its job and answers cost real usage on whichever account signed in.

### Commands

| `node e2e/rig.mjs …` | What it does |
|---|---|
| `start` | Builds this checkout's obid (`OBID_BIN` names another), clones the Sphere at its commit, starts all three, joins both by one private link, writes the rule, starts Dana's job if none runs, and prints what's up and any step left. Real if Dana's account is signed in. While it's all up, it only starts a job if the last one ended. |
| `start --stand-in` | Dana on stand-ins whatever its login: exact states, no usage, and the real tool isn't run at all. |
| `start --real` | Dana on the real Claude Code with or without a login. Without one it starts no job, and every question waits on Dana. |
| `status` | What's up, the checkout, obid and Sphere commit it runs, with each obid's `OBID_HOME` and `OBID_PORT`. |
| `ask <words>` | You ask Dana's Oby; prints your conversation, the exchange's state and Dana's inbox. Comes back after a word comes back, at most 2 minutes and a half. |
| `app` | Builds and opens the Electron dev app on you, with an app folder of its own (`you/electron-data`) and Home, not the first run. |
| `job [task]` | One more job for Dana. On stand-ins the task is `tests/agents/fake-claude`'s words (`SLEEP`, `COMMIT: a.txt`). |
| `policy on\|off` | The Sphere's rule written, or none. |
| `dana stop\|start` | Dana's obid only: its Mac is off. |
| `stop` | Nothing of the rig's left running: the app, both obids with what they started, the Sphere. The folder stays. `--force`: also when the rig is up from another checkout. |
| `reset` | `stop`, then everything goes but the clone and Dana's signed-in account. `--force` as `stop`. |

The folder is `~/Library/Application Support/oby-rig`, or `OBY_RIG_DIR`: an empty folder, or one of the rig's. It stays where it is because Dana's Keychain item is named by its account folder's path.

### Failures and recovery

- A start that fails stops what it started and says why, naming the log; the next goes on from what it kept. A start cut short with something still up (a kill, a closed terminal) never says it's running: `start` and `status` say it was cut short, and to stop first. A start while everything is up says so; while only part is, it says to stop first.
- A clone or install cut short is done again: its marker, `obi-sphere.ready`, is written last.
- A `rig.json` that doesn't read: `stop` and `reset` still work and say what's unknown; the rest say to reset. It's written whole or not at all.
- One command at a time changes the rig (`rig.lock`, nobody's once its process has gone). `status` and `ask` don't wait for it.
- An obid that was killed hard can leave an agent with no parent. `stop` names one whose command line is under the rig's folder and doesn't touch it: it can't tell it from someone's own.
- A port taken by something else is named. The Sphere's port is kept in the memberships: when it's taken, `reset` picks another.
- A stop and start keeps the Sphere, the memberships, the conversations and Dana's job, which goes on. obid leaves a job `interrupted` once it went down 4 times (docs/contracts/recovery.md): then the rig starts another, and the old one stays in what Dana's Oby tells until a `reset`.
- Moving between real and stand-ins removes Dana's jobs before its obid starts, so a stand-in's words never reach the real model.

### Invariants and boundaries

- **The owner's real Oby is never read or written.** The rig refuses before anything else, with exit 2, when its folder or `OBID_HOME` is, is inside or holds `~/Library/Application Support/Oby`, by any spelling (a link, letter case), under either the password database's home or `HOME`, or when `OBID_PORT` is 7777.
- **No login of the owner's reaches a scratch obid.** Every tool names an account of the rig's, so a first start lists none it finds on the Mac (they're only offered in `availableLogins`, and the rig never asks for one). On stand-ins it has a scratch `HOME` and a stand-in Keychain; with the real Claude Code it has the real `HOME`, since macOS finds the login Keychain by it, and reads only the items of the accounts it names.
- **No GitHub and no live Sphere.** Each obid gets an empty `GH_CONFIG_DIR`; the repository is a local fixture (`pullRequests` false); the Sphere's address is always loopback, and no setting changes it.
- **Nothing in obi-sphere changes.** The clone is the rig's, at the one commit `e2e/rig-sphere.ref` names, so a milestone's result says what it ran on and doesn't move when `dev` does. A newer one: change that file, in a pull request, and the next start clones it. It's apart from `e2e/oby-sphere.ref`, the Linux scenario's pin, an older commit than the one this rig was run on.
- **A signal goes only to the rig's own.** A remembered process is signalled only while it's the same process (its start time) and names a path under the rig's folder; what it started is found by parent and by group before it stops. So a person's `tail`, another session's folder or the shell that called the rig isn't touched. One case goes by command line alone: when a remembered group's leader has gone, there's no start time to check, so the group counts as the rig's if a member names a path under the rig's folder (a group's id isn't another's while it has members, and the Sphere's launcher can go before its `workerd`).
- **The rig deletes and writes only inside its own folder.** A folder with other things in it and no `rig.json` is refused, and so is a link at any path the rig makes, reads or writes (its folders, accounts, `gh`, configuration, logs), by every command. `reset` looks again just before it deletes, since stopping takes time; a link swapped in within that last moment isn't caught.
- **A rig up from another checkout isn't stopped or used from this one** without `--force`: it runs that checkout's obid, so a test from here wouldn't test this code.
- **What a tool gets of the person's environment:** `git`, `pnpm`, `npm`, the dev app and `claude`'s own login check get `PATH`, `HOME`, `USER`, `LOGNAME`, `TMPDIR` and `LANG`, with their real `HOME`: git's sign-in, the package stores, and Electron's safeStorage, which keeps the app's keys sealed in the login Keychain. `cargo` gets everything, as a person's build does (`CARGO_TARGET_DIR`).

## Interfaces and dependencies

- **Reused:** the app's own joining (`apps/electron/src/shared/sphere.ts`), `e2e/lib/fixture.mjs`, `e2e/lib/processes.mjs`, the stand-ins in `core/crates/obid/tests/agents`. Nothing in obid's API changed.
- **The Sphere's rules** the rig found, at obi-sphere 586a694: a policy can name only owners who have joined, so the rule is written after both join; an invitation lasts at most 30 days.
- **You joined with the rig's owner key, not the dev app's.** The app logs `Your Oby joined rig with a key this app doesn't have` and shows nothing (only a packaged app tells the owner). The rig keeps the card and the delegation until a membership is 30 days old; it signs only at `start`.
- **Later tasks** drive it the way `apps/electron/scripts/drive-lib.mjs` does: `rig.json` in the folder holds the Sphere's port, the running processes and the device keys; each obid's port and home are in `status`. PEER scenarios go in `apps/electron/scripts/human-drive.scenarios.mjs` as the screens they need land (PEER-1 needs SPH-4).

## Acceptance scenarios

- **RIG-1** Pointed at the owner's real Oby, under `HOME` or the password database's home, by its folder, a folder inside it or holding it, a link (one that leads nowhere too), a firmlink, another spelling of the same name or `OBID_HOME`, every command refuses with exit 2 and changes nothing.
- **RIG-2** Pointed at port 7777, it refuses the same way.
- **RIG-3** `start` from nothing: the Sphere, both obids joined with sealing on, the rule written, Dana's starter rule, Dana's job running, and the owner's step printed.
- **RIG-4** With the rule and the job, `ask "What are you working on?"`: the job quoted, "Asked Dana's Oby. Dana wasn't bothered.", Dana's inbox empty.
- **RIG-5** On stand-ins, a question holding `need-owner`: "It's waiting on Dana", and an `obiRequest` in Dana's inbox.
- **RIG-6** `policy off` and ask: "This Obysphere's rules don't allow questions yet", no exchange kept, also with words already answered once, whose old exchange isn't this one. `policy on` restores it.
- **RIG-7** `dana stop` and ask: after 2 minutes "Dana's Oby hasn't answered yet"; `dana start`: it answers in the same conversation.
- **RIG-8** `start --real` with no login: no job, a question waits on Dana, the owner's step is printed, and no login of the owner's is used.
- **RIG-9** `stop`: no process of the rig's, no agent and no port left, an agent in a group of its own too. `start` again: still joined, the job goes on, and an ask answers.
- **RIG-10** `app`: the dev app shows you, with the Sphere's tile and Dana's row, and its device key and settings are in the rig's folder.
- **RIG-11** `reset` then `start`: a new Sphere, with Dana's account the same folder and still known to Dana's obid.
- **RIG-12** Moving between stand-ins and real: the other kind's job never runs, and a new one starts where it can.
- **RIG-13** A folder with other things in it and no `rig.json`, as a broad `OBY_RIG_DIR` would be: every command refuses with exit 2, and nothing in it changes.
- **RIG-14** `stop` leaves alone a process that only names the rig's folder, one in a sibling folder, and the shell that called it.
- **RIG-15** A remembered pid that started at another time, or names nothing under the rig's folder, isn't signalled, and is forgotten.
- **RIG-16** A link at any of the rig's paths, to the real Oby or anywhere else: every command refuses, and what it points at stays. One swapped in while `reset` stops: refused before it deletes. A link further in: `reset` removes the link, not what it points at.
- **RIG-17** A start cut short with all three up: `start` exits 1 saying so, never "running already".
- **RIG-18** Up from another checkout: every command that changes or uses it refuses, naming it; `stop --force` stops it.
- **RIG-19** A second command while one runs says so and does nothing; a lock whose process has gone is taken.
- **RIG-20** A `rig.json` that doesn't read: `status` says to reset; `stop` and `reset` work.
- **RIG-21** `stop` and `reset` on a folder that isn't there say so, exit 0, and make nothing.
- **RIG-22** While it's all up and Dana's job has ended, `start` starts another, and says it's running.
- **RIG-23** A wrong `OBID_BIN`: `start` says what's wrong, exit 1, with nothing left up.

### Test it when it's done

The owner's step, then on a real Mac:

1. `node e2e/rig.mjs stop`, `node e2e/rig.mjs start`: it says Dana is on the real Claude Code, and Dana's job makes commits in `<rig>/dana/home/worktrees`.
2. `node e2e/rig.mjs ask "What are you working on?"`: the real model's words from the job's title and state, quoted, and "Dana wasn't bothered".
3. `node e2e/rig.mjs ask "What's your favorite editor?"`: "It's waiting on Dana".
4. `node e2e/rig.mjs stop`: nothing left running.
