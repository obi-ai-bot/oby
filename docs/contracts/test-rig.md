# The test rig: two Obys and a Sphere on one Mac

The business behavior contract for `e2e/rig.mjs`, from [the template](../business-contract-template.md). Scenarios are numbered `RIG-n`; RIG-1 and RIG-2 are tests in `e2e/lib/rig-guard.test.mjs` (`node --test e2e/lib/rig-guard.test.mjs`), and the others are run by running the rig, on a real Mac.

## Feature and status

- **Outcome:** anyone working on talking to a person's Oby ([ask-an-oby.md](ask-an-oby.md)) can have a real second Oby with real work, with one command, and throw it away with another. A milestone is tested on it before it's called done.
- **Status:** **Implemented.** Run for real on stand-ins, and with the real Claude Code up to its missing login. **Not run yet:** a real job and a real answer, which need the owner's sign-in below.
- **Scope:** one Mac. **Out of scope:** the second pass on a second Mac and on the live Sphere (PEER-18), and changing obi-sphere.

## Behavior

### Actors and ownership

- **You** (`you/`, owner id `alex`, on port 8794): the scratch obid the Electron dev app points at. Always on stand-ins: asking from a thread runs no model of yours, and `ask` drives your own chat's `oby_ask` with exact words.
- **Dana** (`dana/`, owner id `dana`, on port 8795): a test teammate's obid with its own job. On the real Claude Code once its account is signed in, else on stand-ins (`e2e/rig-agent`).
- **The Sphere** (on the first of ports 8791 to 8793 free at the first start, then always that one): `server/scripts/local.mjs` from a scratch clone of obi-sphere's `dev`, persisted in the rig's folder. The rig is its admin (`rig-admin`).
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
| `start` | Builds this checkout's obid (`OBID_BIN` names another), clones the Sphere at the first start, starts all three, joins both by one private link, writes the rule, starts Dana's job if none runs, and prints what's up and any step left. Real if Dana's account is signed in. |
| `start --stand-in` | Dana on stand-ins whatever its login: exact states, no usage. |
| `start --real` | Dana on the real Claude Code with or without a login. Without one it starts no job, and every question waits on Dana. |
| `status` | What's up, with each obid's `OBID_HOME` and `OBID_PORT`. |
| `ask <words>` | You ask Dana's Oby; prints your conversation, the exchange's state and Dana's inbox. Comes back after a word comes back, at most 2 minutes and a half. |
| `app` | Builds and opens the Electron dev app on you, with an app folder of its own (`you/electron-data`) and Home, not the first run. |
| `job [task]` | One more job for Dana. On stand-ins the task is `tests/agents/fake-claude`'s words (`SLEEP`, `COMMIT: a.txt`). |
| `policy on\|off` | The Sphere's rule written, or none. |
| `dana stop\|start` | Dana's obid only: its Mac is off. |
| `stop` | Nothing left running: the app, both obids with their agents, the Sphere. The folder stays. |
| `reset` | `stop`, then everything goes but the clone and Dana's signed-in account. |

The folder is `~/Library/Application Support/oby-rig`, or `OBY_RIG_DIR`. It stays where it is because Dana's Keychain item is named by its account folder's path.

### Failures and recovery

- A start that fails stops what it started and says why; the next goes on from what it kept. A start while everything is up says so; while only part is, it says to stop first.
- A port taken by something else is named. The Sphere's port is kept in the memberships: when it's taken, `reset` picks another.
- A stop and start keeps the Sphere, the memberships, the conversations and Dana's job, which goes on. obid leaves a job `interrupted` once it went down 4 times (docs/contracts/recovery.md): then the rig starts another, and the old one stays in what Dana's Oby tells until a `reset`.
- Moving between real and stand-ins removes Dana's jobs before its obid starts, so a stand-in's words never reach the real model.

### Invariants and boundaries

- **The owner's real Oby is never read or written.** The rig refuses before anything else, with exit 2, when its folder or `OBID_HOME` is, is inside or holds `~/Library/Application Support/Oby`, by any spelling (a link, letter case), under either the password database's home or `HOME`, or when `OBID_PORT` is 7777.
- **No login of the owner's reaches a scratch obid.** Every tool names an account of the rig's, so a first start lists none it finds on the Mac (they're only offered in `availableLogins`, and the rig never asks for one). On stand-ins it has a scratch `HOME` and a stand-in Keychain; with the real Claude Code it has the real `HOME`, since macOS finds the login Keychain by it, and reads only the items of the accounts it names.
- **No GitHub and no live Sphere.** Each obid gets an empty `GH_CONFIG_DIR`; the repository is a local fixture (`pullRequests` false); the Sphere's address is always loopback, and no setting changes it.
- **Nothing in obi-sphere changes.** The clone is the rig's. A newer `dev`: delete `obi-sphere` in the folder and start.

## Interfaces and dependencies

- **Reused:** the app's own joining (`apps/electron/src/shared/sphere.ts`), `e2e/lib/fixture.mjs`, `e2e/lib/processes.mjs`, the stand-ins in `core/crates/obid/tests/agents`. Nothing in obid's API changed.
- **The Sphere's rules** the rig found, as of obi-sphere `dev` 586a694: a policy can name only owners who have joined, so the rule is written after both join; an invitation lasts at most 30 days.
- **You joined with the rig's owner key, not the dev app's.** The app logs `Your Oby joined rig with a key this app doesn't have` and shows nothing (only a packaged app tells the owner). The rig keeps the card and the delegation until a membership is 30 days old; it signs only at `start`.
- **Later tasks** drive it the way `apps/electron/scripts/drive-lib.mjs` does: `rig.json` in the folder holds the ports and the device keys. PEER scenarios go in `apps/electron/scripts/human-drive.scenarios.mjs` as the screens they need land (PEER-1 needs SPH-4).

## Acceptance scenarios

- **RIG-1** Pointed at the owner's real Oby by its folder, a folder inside it or holding it, a link, another letter case or `OBID_HOME`, every command refuses with exit 2 and changes nothing.
- **RIG-2** Pointed at port 7777, it refuses the same way.
- **RIG-3** `start` from nothing: the Sphere, both obids joined with sealing on, the rule written, Dana's starter rule, Dana's job running, and the owner's step printed.
- **RIG-4** With the rule and the job, `ask "What are you working on?"`: the job quoted, "Asked Dana's Oby. Dana wasn't bothered.", Dana's inbox empty.
- **RIG-5** On stand-ins, a question holding `need-owner`: "It's waiting on Dana", and an `obiRequest` in Dana's inbox.
- **RIG-6** `policy off` and ask: "This Obysphere's rules don't allow questions yet", no exchange kept. `policy on` restores it.
- **RIG-7** `dana stop` and ask: after 2 minutes "Dana's Oby hasn't answered yet"; `dana start`: it answers in the same conversation.
- **RIG-8** `start --real` with no login: no job, a question waits on Dana, the owner's step is printed, and no login of the owner's is used.
- **RIG-9** `stop`: no process of the rig's, no agent and no port left. `start` again: still joined, the job goes on, and an ask answers.
- **RIG-10** `app`: the dev app shows you, with the Sphere's tile and Dana's row, and its device key and settings are in the rig's folder.
- **RIG-11** `reset` then `start`: a new Sphere, with Dana's account the same folder and still known to Dana's obid.
- **RIG-12** Moving between stand-ins and real: the other kind's job never runs, and a new one starts where it can.

### Test it when it's done

The owner's step, then on a real Mac:

1. `node e2e/rig.mjs stop`, `node e2e/rig.mjs start`: it says Dana is on the real Claude Code, and Dana's job makes commits in `<rig>/dana/home/worktrees`.
2. `node e2e/rig.mjs ask "What are you working on?"`: the real model's words from the job's title and state, quoted, and "Dana wasn't bothered".
3. `node e2e/rig.mjs ask "What's your favorite editor?"`: "It's waiting on Dana".
4. `node e2e/rig.mjs stop`: nothing left running.
