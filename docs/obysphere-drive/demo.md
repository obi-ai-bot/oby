# Showing the Obysphere with one Mac

Two Obys on one Obysphere, one of them yours, the other a demo Oby beside it (`scripts/sphere-demo.sh`), so one person can show a question asked and answered and a task handed over, run and its pull request opened. What the demo shows is the product as it is: [obysphere.md](../contracts/obysphere.md), [peer-thread.md](../contracts/peer-thread.md), [handoff.md](../contracts/handoff.md).

## What it needs

- **An invite link** from the Obysphere's admin, with **at least 2 uses** (yours and the demo Oby's), on an Obysphere whose **policy lets members send `question` and `handoff`**: one rule such as `{"id": "members-ask", "from_owner": "*", "to_owner": "*", "actions": ["question", "review_request", "handoff"], "repos": ["*"]}`. The app's own members rule (Settings, an admin's) has `question` and `review_request` only, so a task is refused `policy_denied` until the admin adds `handoff`.
- **Your Oby** in no Obysphere yet (Settings says so; leave the old one first), and up to date (`scripts/run-dev.sh`). An Oby that joined before #2431 rejoins: its old card doesn't take `handoff`.
- **A repository** the demo Oby takes the task in, with a check (`--check`), so its result can say the tests passed.

## Running it

1. Before the call: `scripts/sphere-demo.sh --repo OWNER/NAME --check "python3 -m pytest -q"`. It builds the app once, starts the demo Oby's own obid (port 7871, its folder `~/Library/Application Support/oby-sphere-demo`) on your installed Oby's engine and your own Claude Code and Codex logins, and opens its window, "Oby · Ben (demo)", with an orange badge. With no link anywhere it waits, unjoined.
2. The link arrives: copy it, `scripts/sphere-demo.sh --join`, and click Join in the demo window. The script prints what you click to join your own Oby (the link's page in a browser, "Open in Oby", then Join; or `open "$(pbpaste)"` with the `oby://` link it left on the clipboard). That step is yours, in your own Oby.
3. Ask: in your Oby, the Obysphere in the side panel, ben-demo's Oby, a question, then the ↑ Send arrow. The demo window's Home shows "Question from you": Answer, words, Send. Your thread shows the answer, "ben-demo answered this one, not its Oby."
4. Hand over a task: `scripts/sphere-demo.sh --send-task "Add a power(a, b) function to calc.py, with pytest tests"`. No screen sends one yet ([#2428](https://github.com/obi-ai-bot/oby/issues/2428)): the script asks your obid, on 7777, as its API says (`POST /v1/obid/sphere/requests`), only when you run it. The demo window's Home shows "Task from you": Accept. Its list shows the task running (the demo Oby opens on Sangam's Home, whose list shows a task with no conversation: chat first shows none until it ends, [chat-first.md](../contracts/chat-first.md)'s known ceiling); Open task shows its stages, Prepare to Review, and the pull request.
5. The result: your thread with ben-demo's Oby shows the task and its result, its summary through the publish gate. The demo window asks "Merge … into main?": Leave it open.
6. After: `scripts/sphere-demo.sh --stop` leaves the Obysphere (the Sphere told, with the demo's own owner key), quits the demo app and stops its obid. The folder stays: it holds the owner key the Obysphere bound to `ben-demo` on its first join, and joining as `ben-demo` again takes that key.

`--status` says what runs and lists the members. The link is never an argument the script logs: it reads it from the clipboard (or an argument, which stays in your shell's history) and hands it to the demo app through a 0600 file the app removes at once.

## Rehearsed on 2026-11-17

The whole flow ran on dev `db67273` with two scratch Obys made by this script (owners `ana` and `ben-demo`, their own ports and folders) on a local copy of obi-sphere's server (`feat/room-on-live`), its admin's policy as above and one open link in the live link's form: both joined, a question crossed and was answered, a task was handed over, accepted, run in the demo Oby's foreign lane (Codex, its check passing), pushed and its pull request opened, and the result came back to the asker's thread, in about two minutes. `--stop` left and the other's directory dropped it; a refused link printed the app's reason. What it found:

- **The result named no pull request.** obid filled a handoff result's `pr` from the job's `pr`, the pull request a review job reads, never set for a build job. It now comes from the pull request the job opened (HANDOFF-3). The asker's thread still shows only the summary.
- **A task another Oby hands over shows nowhere in chat first while it runs**, its known ceiling. The demo Oby opens on Sangam's Home.
- **First run, skipped, held the window narrow.** The launcher gives it its size back.
- The demo Oby's Home also shows what your own GitHub login brings for that repository (a failed check, a merge card): Later or Dismiss them.
