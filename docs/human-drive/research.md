# Research: testing Oby the way its owner uses it

The owner's ask of 2026-10-31: Claude drives the real Oby app as a person would, on the owner's own obid and accounts, and a run can be repeated, extended and judged. A short look (about 15 minutes) at how teams do this in 2026, and what this repository adopts. [journeys.md](../journeys.md) is the longer plan (VMs, every journey); this is its first built piece, the conversation loop on the owner's Mac.

## What's out there

- **Playwright drives Electron directly.** Electron's own testing guide points to Playwright's `_electron.launch`, which starts the app over the DevTools protocol, gives a `Page` for each window and runs code in main ([Electron, Automated Testing](https://electronjs.org/docs/latest/tutorial/automated-testing)). `npm run test:core` already uses it against a scratch obid with stand-in agents (#1634, #1637).
- **Claude Code's own pieces** ([Extend Claude Code](https://code.claude.com/docs/en/features-overview)): skills hold a repeatable procedure loaded on demand; subagents isolate context; hooks run deterministic commands on events; Workflow scripts orchestrate many agents. Browser and computer-use tools let Claude look and click itself.
- **Agent-driven QA in practice.** Teams keep the driving deterministic (Playwright scripts) and use Claude around it: a `/qa` skill that runs and reads a smoke suite ([Skyvern, Getting Claude to QA its own work](https://skyvern.com/blog/getting-claude-to-qa-its-own-work)); slash-command "councils" of narrow agents that write, run and triage E2E tests, reporting flaky tests going from about 30 to 5 (their own numbers; [OpenObserve](https://openobserve.ai/blog/autonomous-qa-testing-ai-agents-claude-code/)). The repeated warning: an E2E suite that grows without discipline becomes one nobody trusts, and tests written by a model can check the wrong behavior and pass ([Testsigma](https://testsigma.com/blog/claude-code-testing/)).
- **LLM-judged acceptance.** Checks that can be exact are code and fail hard; a judge only rates what code can't (does this read right, does it match the design), with a specific rubric of what is correct, what is a failure and what to ignore, and categorical labels rather than scores; a judge is calibrated against a person's verdicts before it's trusted ([promptfoo, LLM as a judge](https://www.promptfoo.dev/docs/guides/llm-as-a-judge/); [TestDino, a 4-layer evaluation](https://testdino.com/blog/llm-evaluation-framework)).

## Which input counts as the owner's (signed words, #1611)

Main signs words only right after a keystroke (Enter) or mouse-down it saw itself (`before-input-event`, `before-mouse-event`), and the preload only for `isTrusted` events (docs/contracts/signed-words.md).

| Driver | Signed? | Why |
|---|---|---|
| Playwright `keyboard.press('Enter')`, `locator.click()` (`Input.dispatchKeyEvent` / `dispatchMouseEvent` over DevTools) | **yes** | Chromium treats them as real input: main's `before-input-event` fires and the page's event is trusted. Proven by `test:core` (WORDS-6) and by this driver's runs (no "couldn't sign" line in main's log, `vetted: true` on its jobs). |
| `webContents.sendInputEvent` | yes, expected | The same input path as above; not exercised here. |
| Computer use (real macOS events) | yes | Real input to a visible, focused window. The driver's hidden window is unfocusable, so computer use needs the window shown (`OBY_DRIVE_SHOW=1`) or the installed app. |
| `element.dispatchEvent`, `element.click()` from page script, Playwright `locator.dispatchEvent` | **no** | `isTrusted` is false and main sees no input: the words go unsigned, which `test:core` proves (WORDS-7). |

So the driver uses only Playwright's real input (`press`, `click`), and nothing in production code changes: no test bypass, no switch.

## What this adopts

1. **A deterministic driver, the judge after it.** `npm run drive` (`apps/electron/scripts/human-drive.mjs`) launches this checkout's built app on the owner's obid and drives it with Playwright's real input; every hard check (a reply came, a card showed, the PR merged, the follow-up kept its agent) fails in code. Claude judges the screenshots and words against Sangam's screens and the contracts afterwards, in categories (broken, differs from the design, wrong words, fine), through the `human-drive` skill.
2. **Wait on what a person sees, never on time.** Every wait is "until the window shows X" with a generous ceiling; selectors are what a person reads (a button's name, a box's placeholder). That is most of the anti-flake work.
3. **Scenarios are data.** `human-drive.scenarios.mjs` is a list; a new scenario is one object of a few lines, `needs` skips the ones that depend on a failure instead of cascading.
4. **Every run leaves a folder, and nothing else**: `report.md`, `report.json`, numbered screenshots, main's log (any words it couldn't sign are listed), outside this repository. At the end it closes the issue and pull requests its run made, deletes their branches and revokes the device it registered for the run, and the report says what it undid.
5. **Cheap and contained.** One scratch repository the owner names each time (required; `obi-ai-bot/` is refused), cards answered only when their title is a task this run started, one-line tasks, `--only` to run a few scenarios, Oby's brain is already Haiku.
6. **A person confirms before anything is filed.** Findings are proposed; bug issues are opened only after the owner says yes, as `type:bug` in `bug.yml`'s shape.

Not adopted, for now: a multi-agent Workflow (the owner prefers fast, small runs; one Claude session reads one report); Playwright's test runner (plain Node, as `test:core`); pixel snapshots (the design is still moving); computer use as the main driver (slower, takes the screen; kept for a smoke pass of the installed app).
