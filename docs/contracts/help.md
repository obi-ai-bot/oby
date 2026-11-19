# Help: obid's log and the diagnostics a bug needs

The business behavior contract for the Electron app's Help menu, from [the template](../business-contract-template.md). Scenarios are numbered `HELP-n`; the tests that prove them are named after them (`help_1_…` to `help_5_…` in `apps/electron/src/renderer/src/test/help.test.ts`).

## Feature and status

- **Outcome:** When something breaks, the owner can file a bug the same day: Help shows obid's log in Finder, and copies, ready to paste, the facts a bug needs (which Oby and which obid, which Mac, and what obid logged last). What's copied is scanned for secrets first, and copied only when the scan is clean.
- **Status:** **Implemented** in the Electron app (`src/main/help.ts`, the menu in `src/shared/menu.ts`). Sangam's screens have no Help menu: the items' names are proposed words, his to change. T3 Code keeps the same facts on a Settings page (`apps/web/src/components/settings/DiagnosticsSettings.tsx`); Oby has the two menu items only.
- **Scope:** the Mac menu bar's Help menu, and ⌘K's Reveal obid’s Log action ([shortcuts.md](shortcuts.md), SHORTCUTS-11). **Out of scope:** a diagnostics or process panel, sending a report anywhere (the owner pastes it where they choose), and what obid logs (obid's log holds no secret: [logs.md](logs.md), LOGS-1).

## Behavior

- **Actors and ownership:** the app's main process alone. obid is asked only `GET /v1/status`, for its version and commit; its log is read where obid writes it (`~/Library/Logs/obid.log`, `core/crates/obid/src/app.rs`, `log_path`), never written.
- **Reveal obid’s Log** (Help menu, and ⌘K's action): Finder opens with the log selected. With no log there yet, nothing opens and the owner is told `obid hasn’t written a log yet (~/Library/Logs/obid.log).` (a dialog from the menu, the window's passing note from ⌘K).
- **Copy Diagnostics** (Help menu) puts this text on the clipboard:

  ```
  Oby: <the app's version> (commit <the app's commit>)
  obid: <obid's version> (commit <obid's commit>)      or   obid: <why it didn't say>
  macOS: <its version>
  Architecture: <arm64 | x64>

  obid's log, its last 200 lines (~/Library/Logs/obid.log):   or   obid's log: none at ~/Library/Logs/obid.log
  <up to 200 lines>
  ```

  obid not answering within 3 seconds, not running, or answering without its version is said in its line; the rest is still copied. The log's last lines are read from at most its last 256 KiB, never a line cut in half, never through a link.
- **The secret scan:** before anything reaches the clipboard, the text goes through gitleaks, as obid's own scan runs it (`core/crates/obid/src/scan.rs`, `run_gitleaks`): the one beside the app (`Contents/MacOS/gitleaks`, as its packaging puts it), else one on the `PATH`, with `--redact`, `--ignore-gitleaks-allow`, in a private folder of its own that's removed afterwards. A made-up GitHub token goes on the text's last line, after a neutral one: a scan whose report doesn't have it didn't read the text. Clean (the made-up token its only finding): copied, and nothing is said. A finding: nothing copied, and `The secret scan found what looks like a secret (<rules>), so nothing was copied.` No gitleaks, one that stops with another code, can't start, writes no report or misses the made-up token: nothing copied, and `The secret scan couldn’t run (<why>), so nothing was copied.` From the menu, either says so in a dialog.
- **From the page:** ⌘K's action reaches main on the `help` channel ([ipc.md](ipc.md)), which takes only `reveal-log` or `copy-diagnostics` from the window's own page and answers `null` when done, else what went wrong; anything else gets `Not this app’s page.` and nothing runs.
- **Invariants:** nothing leaves the Mac; nothing reaches the clipboard unscanned, or when the scan found something or couldn't run (AGENTS.md, "Non-negotiable"). The log and the scan's files are only read; the scan's own folder is removed.

## Interfaces and dependencies

- **Renderer ↔ main:** the preload's `window.app.help(what)` for the `help` channel ([ipc.md](ipc.md)).
- **obid:** `GET /v1/status`'s `runner.version` and `runner.commit`, as About reads them. No runner API change.

## Acceptance scenarios

- **HELP-1:** With a log there, Reveal obid’s Log shows that file in Finder; with none, nothing is shown and the owner is told so. The Help menu has Reveal obid’s Log and Copy Diagnostics, each running main's action. (`help_1_reveal_obids_log_shows_the_file_in_finder`)
- **HELP-2:** With obid answering 0.2.24 and its commit, and a 250-line log, Copy Diagnostics copies the app's version and commit, obid's, the macOS version, the architecture, and lines 51 to 250 of the log, 206 lines in all; with obid not running and no log, those lines say so; from a log longer than 256 KiB, its last 200 whole lines. (`help_2_copy_diagnostics_copies_the_versions_the_mac_and_the_logs_last_200_lines`)
- **HELP-3:** With a GitHub token in the log's last line, nothing is copied and the owner is told the scan found a `github-pat`; the same text without it is copied. With gitleaks on the Mac, the real one finds that token and passes the text without it. (`help_3_a_secret_in_the_diagnostics_copies_nothing`, `help_3_with_the_real_gitleaks`, skipped where gitleaks isn't installed)
- **HELP-4:** With no gitleaks, one that stops with code 1, one that can't start, one that says clean without having read the text, or one that writes no report, nothing is copied and the owner is told the scan couldn't run, and why. (`help_4_a_scan_that_cant_run_copies_nothing`)
- **HELP-5:** The `help` channel runs Reveal obid’s Log for the window's own page, and refuses anything else than its two asks, or any ask from another page, with nothing run. (`help_5_the_page_asks_main_for_them_through_its_own_channel`; and IPC-1)

**Test it when it's done (real Mac):** in the installed Oby, Help › Reveal obid’s Log: Finder shows `obid.log`. Help › Copy Diagnostics, then paste into a note: the two versions and commits, macOS, the architecture and the log's end. ⌘K, type `log`, click Reveal obid’s Log: Finder again.
