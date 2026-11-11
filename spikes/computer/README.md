# Spike: a computer for an own job, through the owner's own MCP config

Run on 2026-11-11 on the owner's Mac, for epic #1176. The question: with "Dispatcher, not harness", can an own Claude Code job see and drive an app through `agent-desktop mcp` configured in the owner's own Claude Code, with no new obid code?

**Answer: the tools arrive, and obid refuses every call.** PR #2088 loads the owner's MCP servers into an own job, but `policy::fixed_denial_as` still declines any MCP tool that isn't Tetrix's or Oby's own, in Bypass too. Run by hand without that refusal, the same command works: the driver reads the app and the model sees the screenshot.

## What ran

- obid built from PR #2088's head `9cf439d8` (on `dev` at `af281d82`), started on a scratch home with `run.mjs` in this folder.
- Claude Code 2.1.302, the default account, its login in the Keychain.
- `agent-desktop` 1.2.0, installed with `npm install --prefix <scratch>` (not global), added with `claude mcp add --scope user agent-desktop -- <path> mcp` for the run and removed after it.
- One `research` job on the e2e fixture repository. Its task: call `desktop_status`, `desktop_snapshot` and `desktop_screenshot` on Calculator, and nothing else.
- Then the same Claude Code command obid had started (`--setting-sources user`, obid's `--settings` with the sandbox pinned on), run by hand with those three tools and Bash in `--allowedTools`.

Nothing was clicked or typed, and no macOS permission was granted or changed.

## Answers

| | Question | Answer |
|---|---|---|
| Q1 | Does an own job get the owner's `agent-desktop` server? | **Yes.** `init` listed it as connected with 67 tools, beside every other server of the owner's (13 in all: GitHub, Slack, Google Drive, Vercel and more). |
| Q2 | Which app does macOS ask about? | **Not answered.** See "The permission" below. |
| Q3 | Does Claude Code's sandbox hold the MCP server? | **No.** Through MCP, `desktop_status` said Accessibility and Screen Recording were granted and kept its state in `~/.agent-desktop`. The same binary through Bash, in the same run, said both were denied and `snapshot` failed with `PERM_DENIED`. So the MCP server runs outside the sandbox, and the CLI inside it. |
| Q4 | Does a call reach obid's permission prompt in Bypass? | **Yes, and obid refuses it.** All three calls ended `declined` in 0.1 s with: "obid's agents use only the MCP tools Oby connected for them (Tetrix's allowed tools, Oby's own)." |
| Q5 | Does a screenshot reach the model as an image? | **Yes**, in the hand run: the tool result held an `image` block, and the model described Calculator's keys. Claude Code saved the PNG under `~/.claude/projects/<project>/<session>/tool-results/`. Whether obid's transcript keeps an image is not known: under obid no call ran. |
| Q6 | Is the job marked `reached` or untrusted afterwards? | The job's JSON had neither key after the three declined calls. Not tested for a call that runs. |
| Q7 | A browser MCP server on a loopback page? | **Not run.** |
| Q8 | Does a foreign job see the server? | Not rerun. The PR's own test covers it (`lanes_5_an_own_job_loads_the_owners_settings_never_the_repositorys`). |

## What it means

1. **LANES-2 says one thing and the code does another.** The contract: an own job is "offered its tools, as configured, and obid leaves its calls to the job's mode" (the owner's "Allow them", #2044). The code: `core/crates/obid/src/policy.rs`, `fixed_denial_as`, refuses them before the mode is asked. Until that changes, the owner's MCP servers show up in an own job and none of them can be used.
2. **Once calls go through, a computer needs no obid code.** No helper, no broker, no new endpoint: #1183 to #1186 would not be needed for Claude Code.
3. **The MCP server is outside every sandbox.** It runs as the owner, with whatever macOS has granted the app that launched it. In Bypass nothing would ask before a click. agent-desktop's tools carry no read-only annotations, so obid can't tell a snapshot from a click by what the server says.
4. **Use the MCP route, not the CLI in Bash.** Bash is sandboxed and gets no Accessibility.
5. **A driver quirk:** `snapshot` with `interactive_only` and `compact` together gave 0 refs on Calculator (the CLI returned an error for the same flags). With `-i` alone it gave 26 refs, complete.
6. **A run needs `USER` in obid's environment.** Without it Claude Code doesn't find its Keychain login and the job ends "Not logged in · Please run /login". launchd sets it for the installed obid; a scratch obid started by a script must pass it (`run.mjs` does).

## The permission (Q2, the owner's step)

Not tested, because it means changing a macOS privacy setting, which is the owner's.

- In this run the driver had Accessibility and Screen Recording without anyone granting them to it: it inherited them from the app that started the run. So the grant follows the launching app, not `agent-desktop`.
- The installed obid is started by launchd (`com.deskree.oby.electron.obid`, parent pid 1), not by the Oby window. It is signed `Identifier=obid`, team `3UV9W5SV7W`, and its launchd entry names `com.deskree.oby.electron` in `AssociatedBundleIdentifiers`.
- So I expect macOS to ask for **obid**, not Oby, and to list it apart from the app. That is an expectation, not a result.

To find out, once an own job can call the tool: start a task in the installed Oby that calls `desktop_status`, and read what it reports and what System Settings › Privacy & Security › Accessibility lists.

## Left on this Mac

- `~/.agent-desktop/` (32 KB), the driver's state folder. Safe to delete.
- One session of the hand run under `~/.claude/projects/`, with the Calculator screenshot.
- The `agent-desktop` entry is out of `~/.claude.json` again, and Calculator is closed.

## Run it again

```bash
node spikes/computer/run.mjs <checkout of PR 2088> <its core/target/debug/obid> "$(command -v claude)" <an empty scratch folder>
```

It needs `agent-desktop` in the user-scope MCP config first, and it spends real usage on the default Claude Code account (the hand run cost $0.39).
