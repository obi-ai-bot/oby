# How a change is proven

Every change to Oby is proven by driving the real thing from outside, the way a person or another program would, before it merges. Unit tests named after a contract's scenarios stay required ("Done" in [AGENTS.md](../AGENTS.md)); this page adds what proves the change *works*, by what it changes ([why](decisions/2026-11-19-proven-by-blok.md)).

Think of a car inspection: checking each part on the bench (unit tests) isn't enough; someone drives the car round the block. Here, a BLOK workflow is that drive, written down so it runs again on every change.

## What proves what

| What changed | What proves it | Where it lives | Status |
|---|---|---|---|
| **obid's behavior** (a route, a trigger, a rule, a connector: anything not the window) | A BLOK workflow that drives the real obid binary through its API, each asserting step naming the scenario it proves | `tools/obid-spec/workflows/`, run with `tools/obid-spec/target/debug/obid-spec run` ([obid-spec.md](contracts/obid-spec.md)) | **In force** |
| **The window** (anything a person sees or clicks in the Electron app) | A BLOK workflow that drives the real Electron app on a scratch obid with **Playwright** (`app.*` nodes: launch, click, type, read, screenshot), through the driver the human drive already uses (`apps/electron/scripts/drive-lib.mjs`, `_electron.launch`) | `tools/obid-spec/workflows/app/` | **In force** |
| **What's outside the page** (the macOS menu bar, keyboard shortcuts the menu takes, system dialogs, notifications, the Dock, a Keychain prompt) | A BLOK workflow that drives the Mac with **agent-desktop** (`desktop.*` nodes: snapshot, click, press keys, read), which reads the screen the way a person sees it ([spikes/computer/](../spikes/computer/README.md)) | `tools/obid-spec/workflows/desktop/` | **In force** for the menu bar and the keys it takes; system dialogs, notifications, the Dock and a Keychain prompt still **Proposed** |
| **The Obysphere** (anything obid sends, receives or decides between Obys, including what reaches it indirectly, such as code-host approvals) | The launch gate, L1 to L9, on the test rig | `e2e/launch.mjs --gate` ([test-rig.md](contracts/test-rig.md)) | **In force** |
| **A release or a phase of work** | The real-Mac steps: the issue's "Test it when it's done" list, and the human drive on a scratch repository | `.claude/skills/human-drive/` | **In force** |

A change can need more than one row: a connector that shows a new button needs an obid workflow and an `app.*` one.

## The rules every workflow follows

1. **Red first.** A workflow, or a step, is shown failing against the code before the change (or with the fix taken out) before it's trusted. A check that has never failed proves nothing. The pull request says how it was made red.
2. **One scenario per assertion.** Each asserting step names the contract scenario it proves (`"proves": ["CHATFIRST-37"]`), and every scenario a pull request adds is listed in `tools/obid-spec/scope.json` in the same pull request, so `obid-spec coverage --gate` passes on its own checks, not first in the merge queue.
3. **Scratch only.** A workflow runs its own obid on its own port and home, its own Sphere stand-in and its own app instance with its own user-data folder. It never reaches the owner's obid on 127.0.0.1:7777, their Oby.app, their Keychain or their accounts.
4. **Stub agents.** Agents are the suite's stub CLIs. A live model run is used only where a scenario can't be proven without one, and the pull request says why.
5. **No flakes merged.** A workflow that fails without a code change is fixed or marked `flaky` with its fixing issue the same day; the merge queue runs it, so a flake costs everyone.
6. **Look, then measure.** A UI workflow checks what a person would see, not only what the page holds: an `app.*` step can assert an element's place and size against its neighbours (a button must not sit on a card), and a screenshot of each new state goes in the pull request.
7. **Pixel-identical where nothing should change.** A change meant to look the same (a refactor, colours moved onto tokens) carries a before-and-after screenshot comparison of the screens it touches.

## Until the launch

The suite is frozen until the launch, by the owner's decision of 2026-11-18 ([tools/obid-spec/README.md](../tools/obid-spec/README.md), "Frozen until launch"). Until then, this page applies to what the freeze allows: a bug fix's proof and a launch flow's. From the launch on, it applies to every change. The `app.*` and `desktop.*` nodes themselves may be built before the launch, as tooling that proves the launch flows.

## agent-desktop on a Mac

agent-desktop reads the screen through macOS's Accessibility and Screen Recording permissions. They're granted once, by the owner, to the terminal or runner that runs the suite on that Mac; no agent or workflow grants or changes them. `desktop.*` workflows run on the Mac's check and nightly; GitHub's Linux runners skip them and say so in their output.
