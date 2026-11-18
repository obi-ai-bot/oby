# First run: never a dead end, and only what's built

The business behavior contract for Sangam's first run in the Electron app (`FirstRun.tsx`: Hi, Tools, Knowledge, Obysphere, then Home), from [the template](../business-contract-template.md). Scenarios are numbered `FIRSTRUN-n`; the tests that prove them are named after them (`firstrun_1_…` to `firstrun_4_…` in `apps/electron/src/renderer/src/test/firstRun.test.tsx`). Why a sign-in didn't take is [sign-in.md](sign-in.md); the Tetrix address on the Knowledge step is [tetrix-url.md](tetrix-url.md).

## Feature and status

- **Outcome:** A new owner gets through first run to Home whatever is on their Mac: with no coding agent signed in, with an agent's CLI not installed yet, and without being promised something this build doesn't have.
- **Status:** **Implemented.** Sangam's screens lead how each step looks; the words below that his screens don't have yet are proposals, his to rewrite.
- **Scope:** the Tools step (Skip for now, an agent's Install), the words of the Tools, Knowledge and Obysphere steps on obid. **Out of scope:** the first conversation's setting up (`onboarding.ts`'s tour), GitHub's own install (`gh`), installing a CLI for the owner (Oby shows the vendor's command; the owner runs it), the mock's words (`?everything` keeps Sangam's full design).

## Behavior

- **Actors and ownership:** obid says which tools it finds on this Mac (`GET /v1/status`'s `tools[]`: each agent's `path`, null when its CLI isn't found) and which are signed in (`accounts`); the app shows each row and the steps. obid looks for a CLI again at every status, so one installed meanwhile shows up without a restart.
- **Skip for now:** the Tools step's Continue needs a signed-in agent, as before (it shakes its head otherwise); under it, Skip for now goes on to the Knowledge step with nothing signed in, and fades once an agent is ready (keeping its room, so nothing moves), as on the Knowledge step.
- **Install:** an agent whose CLI obid doesn't find shows Install where Connect would be, before any Connect. Install shows, under the row, the vendor's own install command with Copy, and "Paste it in Terminal. Oby finds it once it's installed.":
  - Claude Code: `curl -fsSL https://claude.ai/install.sh | bash`
  - Codex: `curl -fsSL https://chatgpt.com/codex/install.sh | sh`
  - Cursor: `curl https://cursor.com/install -fsS | bash`

  While a row shows Install, the app asks obid's status again every 3 seconds; once the CLI is found, the row shows Connect. An obid that sends no `tools` (an older one) shows Connect as before. Oby runs nothing: the owner runs the command in their own Terminal.
- **Only what's built:** on obid the Tools step says nothing of GitLab or Bitbucket, the Knowledge step nothing of adding an MCP server in Settings, and the Obysphere step says the team's Obysphere is joined from its invite link, not created (making one is `?everything` only, `CAN_CREATE` in `spheres.ts`). On the mock (`?everything`) Sangam's words stay as his screen has them.
- **Failure and recovery:** Copy that the clipboard refuses leaves the command shown, to select by hand. obid not answering keeps each row's spinner, as before.
- **Invariants:** no step waits on something the owner can't do from it; the app never runs an install command itself.

## Interfaces and dependencies

`GET /v1/status`'s `tools[]` (`{agent, path, version, versionStatus, knownBadReason}`, [capabilities.md](../api/capabilities.md)) and `accounts`; `shared/obid-wire.ts`'s `Status.tools`. `FirstRun.tsx` (the steps), `SetupCard.tsx` (`AgentRows`, the Install line), `install.ts` (the commands).

## Acceptance scenarios

- **FIRSTRUN-1** (`firstrun_1_skip_goes_on_without_an_agent`): on the Tools step with no agent signed in, Skip for now opens the Knowledge step; with one signed in, Skip for now is faded out and out of the tab order, and Continue opens the Knowledge step.
- **FIRSTRUN-2** (`firstrun_2_an_agent_not_installed_shows_install_first`): with `tools[]` giving Codex `path: null`, Codex's row shows Install and no Connect; Install shows `curl -fsSL https://chatgpt.com/codex/install.sh | sh` and Copy, which puts that text on the clipboard; Claude Code's and Cursor's rows (found) show Connect, as before.
- **FIRSTRUN-3** (`firstrun_3_an_install_is_found_without_a_restart`): Cursor's row showing Install, obid's next status giving Cursor a `path` turns it into Connect within the app's next look (3 seconds); a status with no `tools` shows no Install on any row (Claude Code's and Cursor's Connect, Codex's its usual sign-in).
- **FIRSTRUN-4** (`firstrun_4_first_run_promises_only_what_is_built`): on obid, the Tools step's text holds neither GitLab nor Bitbucket, the Knowledge step's no "MCP server", and the Obysphere step's no "Create"; it says the team's Obysphere is joined from its invite link.

**Test it when it's done (real Mac):**

1. Rename `codex` out of the PATH (or on a Mac without it), reset first run from the control center, and open Oby: Codex's row shows Install; Install shows the command; Copy, paste it in Terminal and run it; within a few seconds the row shows Connect.
2. Reset first run again and, with no agent signed in, click Skip for now: the Knowledge step opens; Skip for now, Let's go: Home opens.
3. Read the three steps: nothing promises GitLab, Bitbucket, adding an MCP server in Settings, or creating an Obysphere.
