# Journeys: testing Oby the way a person uses it

The owner's ask of 2026-10-25: Claude should be able to run Oby itself, from a clean install through every permission and every daily workflow, and check it's flawless, so Oby becomes the best dispatcher and agent orchestrator for software engineers while staying as simple as it is. This is the plan; nothing here is built yet.

The unit and API suites (`cargo test`, MacTests, `swift test`) say the parts work. A journey says the product works: one person's path through the real app, start to finish, with what they see, what they wait for and every time Oby makes them click.

## What a journey run gives

Each journey is a script with numbered steps. A run produces one report (an Artifact page, kept per release):

- **Pass or fail per step**, with a screenshot of the window at that step and obid's own record (the job, its `outgoing`, its inbox items).
- **Time**: from the step's start to what it waits for.
- **Friction**: every click, prompt, sheet and wait the person meets, counted. A step that asks for something it didn't need fails, even if everything else works.
- **What a person would read**: every line Oby showed, so wording that's wrong or confusing is caught as a failure, not a nit.

## The measures

These are what "best dispatcher" means here, measured on every run and compared release to release, and against the flows in the REA teardowns (Grok Bot, Cursor) and the others the owner named:

| Measure | Target |
|---|---|
| Install to first pull request | under 10 minutes, under 15 clicks |
| Times Oby asks per task | 0 for routine work; at most 1 where a decision is truly the owner's |
| Tasks finished with no help | every journey task |
| An account running out | the task carries on, on another account, with no ask |
| A teammate's review, asked to approved | no click on the asking side; one on the reviewing side, only where GitHub needs a person |
| Wrong or unclear words on screen | none |

## The journeys

| # | Journey | Covers | Where |
|---|---|---|---|
| J1 | **Clean install to first PR.** Open the `.dmg`, drag Oby to Applications, open it: the onboarding, what's signed in found (one tool signed in, one installed and signed out, one missing), Connect opening the tool's own login, GitHub, then a first task from Home to a merged pull request in a fixture repository. | Cores 1–3 | a fresh macOS VM |
| J2 | **Permissions, each at its moment.** Notifications on the first task, Accessibility on the first fn press, Screen Recording on the first screenshot, the microphone on the first voice click: each with Oby's one line before macOS's prompt, never in a batch; then denied (Oby says what's missing and still works), then granted (Oby notices without a restart). | First launch | the VM |
| J3 | **Dispatch across tools and accounts.** Three tasks at once going to Claude Code, Codex and Cursor; one account filling up mid-task (the fake agents report a full window), the task carrying on on another account from the same branch, with no ask. | Cores 1–2 | the VM, fake agents |
| J4 | **Asked only when it matters.** A plan to approve, a permission, a question and a merge in an unlearned repository: all four arrive in Home's approval stack ("1/4"), are answered there without opening a task, and everything else finishes alone. "Can I remember that?" is asked once, and the second time Oby just does it and says so. | Core 3, #1442 | the VM, fake agents |
| J5 | **What was done.** Each finished task's page: its result in one line, its pull request, what went out, how long it took; the conversation's report matching it. | Core 3 | the VM |
| J6 | **Two Obys review each other.** Owner A's pull request needs a review; A's Oby asks B's over a test Obysphere; B's Oby reviews and its owner taps once; A's Oby merges as A's workflow says. Then the other way. | Core 4 | two VMs, a test Sphere |
| J7 | **Tetrix, flawless.** Connect Tetrix once in Settings; a job on each tool uses Tetrix's tools; Tetrix answering 503 or losing the session mid-job, and the agent still having its tools on the next call (the audit's finding 2); signing Tetrix out and Oby saying so. | Tetrix | the VM, a test Tetrix |
| J8 | **Bad days.** obid killed mid-job, the Mac asleep and woken, the network off, a CLI signed out mid-run (Codex's 401): each said in one line, and the work picks up where it was. | All | the VM |
| J9 | **Updating.** Install the last release, use it, update to the new one: tasks, accounts and the Obysphere kept, and only first launch's new steps shown. | First launch | the VM |

J4 and J5 come first: they need no VM. They run against obid with the fake agents (`core/crates/obid/tests/agents/`) behind the real Mac app, built from the branch, in a throwaway `OBY_HOME`, as MacTests' host does.

## How Claude runs them

- **A clean Mac per run: macOS VMs.** [Tart](https://github.com/cirruslabs/tart) runs macOS guests on Apple's Virtualization framework. A base image (macOS 26, tens of GB) is made once; each run clones it (an APFS copy, seconds) and throws the clone away. Two images:
  - `oby-clean`: macOS as it ships, with nothing else installed. J1 and J2 start from it.
  - `oby-ready`: the same, after the owner's one-time setup below. The other journeys start from it.
- **The screen: computer use.** Claude clicks and reads Oby's window inside the VM's window, as a person would, and screenshots each step. Nothing is driven through private APIs.
- **The truth: obid's API.** After each step Claude reads obid inside the VM (`tart exec`, or SSH with a key made for the image) to check what really happened behind the window: the job, its account, its pull request, its inbox items.
- **The agents.** Real Claude Code, Codex and Cursor for J1, J6 and J7, signed in once in `oby-ready` by the owner. The fake agents for J3, J4 and J8, so failover and asks happen on cue, every run.
- **The repositories: a GitHub organization for tests**, with fixture repositories that journeys may open pull requests in and merge into, never a real one.
- **The report:** one Artifact per run, and the measures above in a table across runs.

## What Claude never does in a journey

- Type a password, a token or a login: macOS's admin prompts, the CLIs' sign-ins, GitHub's. Those are the owner's, once, when `oby-ready` is made.
- Quit or touch the owner's own Oby on their Mac: journeys run in VMs, or against a throwaway `OBY_HOME`.
- Open a pull request or merge anywhere but the fixture repositories.
- Change macOS's security settings by script: a permission is granted as a person grants it, in System Settings, by the owner when a journey reaches it, or kept in `oby-ready`'s snapshot.

## Decisions for the owner

1. **The VM tool and image.** Install Tart (Homebrew), and download a macOS 26 image (tens of GB; the Mac has 178 GB free)?
2. **A one-time setup of `oby-ready`** (about 15 minutes): in the VM, sign in Claude Code, Codex, Cursor and GitHub, and grant the permissions Oby asks for. Snapshotted, so it isn't done again.
3. **A test GitHub organization** for the fixture repositories (`obi-e2e` is free), owned by the owner.
4. **A test Obysphere** for J6: a second Sphere on Deskree's Cloudflare, or a test Sphere inside Deskree's.
5. **A test Tetrix** for J7: the dogfood server, or another.

## Order

1. J4 and J5 against the real Mac app and obid with fake agents, no VM: the approval stack and the task page, as they're built (#1442's Mac half).
2. The `oby-clean` image, then J1 and J2.
3. `oby-ready` (the owner's setup), then J3, J8 and J9.
4. The test Sphere, then J6. The test Tetrix, then J7.
5. Every merge into `main`, and every release, runs all of them; a failing journey holds the release.
