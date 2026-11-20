# The terminal

The business behavior contract for the owner's own shell in a task's or a conversation's working copy, inside Oby's window, from [the template](../business-contract-template.md). Scenarios are numbered `TERMINAL-n`; the tests that prove them are named after them: main's in `apps/electron/src/renderer/src/test/terminal.test.ts` (`terminal_1_…` to `terminal_8_…`), the page's in `apps/electron/src/renderer/src/test/terminalPanel.test.tsx` (`terminal_9_…` to `terminal_12_…`).

## Feature and status

- **Outcome:** The owner runs a command where a task works (its worktree) or where a conversation's work is (its first folder) without leaving Oby, as T3 Code's terminal drawer does for a thread: `npm test`, `git log`, a dev server. Before, the only way was **Open in Terminal**, another app.
- **Status:** **Implemented** in the Electron app (`apps/electron`). Sangam has no screen for it yet: the panel follows T3 Code's drawer and Oby's Changes panel, and its words (below) are new, all his and the owner's to change.
- **Scope:** main's shells (`src/main/terminal.ts`, `src/main/terminalInput.ts`), the preload's `terminal`, the panel (`src/renderer/src/terminal/`), **Terminal** in a task's and a conversation's menu. **Out of scope:** obid (it doesn't change: main finds the folder from what obid already reports), a terminal in a boxed or remote job's own sandbox, an agent using the terminal, keeping a shell's output across launches, splits, a keyboard shortcut, the iPhone.

## Behavior

### Actors and ownership

- **The owner** clicks **Terminal** in a task's menu (one with a worktree) or a conversation's (one with a folder), **+** for another, × to close one or all, and types.
- **Main** (`src/main/terminal.ts`) runs each shell with node-pty, as the owner, unboxed: the owner's own act, with no agent, no job and no box, as their Terminal would run it. It decides where the shell starts, what environment it gets, what reaches it, and when it ends. The page only asks.
- **The page** (`TerminalPanel.tsx`, `store.ts`, `xterm.ts`) draws each shell with xterm.js, sends main where (a task or a conversation, by its id), the size its box holds, and what's typed; it never names a folder, a program or a variable.
- **obid** is unchanged. Main reads a task's worktree as obid keeps it (`<home>/worktrees/<id>`, a real folder there, while `GET /v1/jobs/{id}` has the job) and a conversation's folders as obid lists them (main's snapshot, its project's while it's in one), as **Open in Terminal** (CHANGES-6) and a reply's paths (CHATFIRST-39) do.

### Opening

- **Terminal** opens the panel at the window's right, where Changes and the browser show, for that task or conversation; it starts a shell when that place has none, and shows the ones it has when it has some. **+** starts another. Each place has its own terminals.
- A shell starts only right after a click or Enter main saw reach the window itself, one shell per input (`takeInput`, as the owner's other acts, [signed-words.md](signed-words.md)): never because the page asked alone.
- The shell is the owner's login shell as the system has it for their user (`os.userInfo().shell`, never `$SHELL`), else `/bin/zsh`, started as a login shell (`-l`), as Terminal starts one, in the folder main found, at the size the panel's box holds, measured before main is asked, so the first prompt fits.
- At most 8 shells run at once.

### What the shell gets

- A clean environment: `HOME`, `USER`, `LOGNAME`, `SHELL`, the system's `PATH` (`/usr/bin:/bin:/usr/sbin:/sbin`; the login shell's profile adds the owner's), `LANG` (the app's when it's UTF-8, else `en_US.UTF-8`), `TERM=xterm-256color`, `COLORTERM=truecolor`, `TERM_PROGRAM=Oby` and its version, and the app's `TMPDIR`, `SSH_AUTH_SOCK` and `__CF_USER_TEXT_ENCODING` when it has them (the owner's own temporary folder, ssh-agent and text encoding, as Terminal has them).
- Nothing else of the app's environment: no token of obid's or anyone's, no vault secret, no `OBID_*`, `ELECTRON_*`, `NODE_*` or `DYLD_*` variable, nothing an agent or the app's launch set. Oby passes the shell nothing of its own; the shell reads what the owner's user can, as their Terminal does.

### What reaches the shell

- Main lets the page's bytes through only as it saw them typed (`terminalInput.ts`): each real key main saw reach the window in the last 2 seconds lets one write through, and only the bytes that key makes in xterm.js: its own character (⌥'s character included), its control character with ⌃ (a carriage return only from ⌃M), a named key's escape sequence (arrows, Tab, ⇧Tab, Backspace, Esc, Delete, Home, End, Page Up and Down, F-keys), Enter a carriage return, an input method's composed text (no control character). ⌘ and a key types nothing.
- A paste is the owner's ⌘V (or ⌃⇧V) followed by exactly the clipboard's text as main reads it, its line ends as carriage returns, bracketed or not, as xterm.js sends it; anything else after ⌘V goes nowhere. Writes reach the shell in the order they came, a paste waiting for the clipboard.
- The terminal's own answers to a program's questions (device attributes, cursor and status reports, a mode's state, the window's size, focus in and out, a color) go without a key: fixed shapes of digits and `;` that carry no command. Mouse reports go only within 2 seconds of a real mouse input main saw.

### Ending

- × on a tab ends that shell; × on the panel ends every shell of the place it shows; a shell that exits takes its tab with it, and the last one closes the panel. Ending is SIGHUP (which an interactive shell passes on to its jobs, and which ends what it runs in the foreground), as Terminal.app sends, then SIGKILL after 1 second if it's still there.
- The page that opened it closing, crashing or loading again ends its shells (main follows the page, never the page's word).
- Its task deleted (obid says it deleted it), its worktree gone from obid's home, or its conversation deleted, ends its shells.
- Oby quitting ends every shell (`before-quit`), and SIGKILLs any still there before the app ends (`will-quit`).
- Changes or the browser opening puts the panel away (one panel at the window's right at a time, BROWSERAPP-3): its shells keep going, their output kept on their screens, and **Terminal** brings them back. The panel opening closes Changes and puts the browser away.

### Decisions and invariants

- **The owner's own act, like a click in Terminal.** A shell runs in the own lane's sense: the owner's user, the owner's shell and profile, no box. It is not an agent's: no job, no worktree change of obid's, no check, nothing signed, nothing sent to obid.
- **The page names no folder, program or variable.** Where, by id; the folder, the program and the environment are main's.
- **Nothing reaches a shell on the page's say alone.** A script that took the page over can't open a shell, and can't type into one beyond what the owner's own keys make, key for key: one write per real key, of that key's bytes, and a paste of the clipboard's own text. What's left: within 2 seconds of each real key of the owner's, that key's own bytes, wherever the owner was typing; and the first click after the page is taken over could open a shell in a place of its choosing, which runs nothing until the owner types into it. It closes with keys read in a view main owns.
- **No shell outlives its tab, its page, its place or Oby.**
- **Scrollback** is the screen's, 5,000 lines, in the page only: nothing is written to disk.

### Failure and recovery

- No click main saw: `Opening a terminal takes your own click.`; nothing starts.
- A task's worktree that isn't on this Mac, or a job obid doesn't have: `This task’s working copy isn’t on this Mac.`; a conversation with no folder obid lists: `This conversation has no folder on this Mac.`; a folder gone since: `Its folder isn’t on this Mac any more.`
- The shell can't start (node-pty's error): `The shell didn’t start: <why>`.
- 8 running: `Oby keeps 8 terminals open at most: close one first.`
- A resize that races its shell's end is dropped, never thrown; a size of nothing (a hidden panel has none) is never sent.
- Main ends a shell the page closed while it was opening as soon as main answers.

## Interfaces and dependencies

No change to obid's API. The app's IPC (main, [ipc.md](ipc.md)): `terminal-open` (`oneOf('task', 'conversation')`, an id of letters, digits, `:`, `_` and `-`, the columns and rows; answers `{term, cwd, name}` or `{error}`), `terminal-write` (the shell's id, at most 1 MB of text), `terminal-resize` (id, columns 2 to 1,000, rows 1 to 500), `terminal-close` (id); main sends the page `terminal-data` (id, text) and `terminal-exit` (id, code, signal). The preload's `window.app.terminal`: `open`, `write`, `resize`, `close`, `onData`, `onExit`. node-pty 1.1.0 (N-API: no rebuild for Electron; its macOS builds and `spawn-helper` ship unpacked, `scripts/package.sh` makes the helper executable and checks it's in the app and signed), xterm.js 6.0.0 and its fit add-on 0.11.0 (bundled into the page).

## Words on screen

New, for the owner and Sangam to approve: **Terminal** (a task's and a conversation's menu item; the panel's heading and its label), **New terminal**, **Close Terminal**, **Terminal <n>** (a tab, with two or more), **Close Terminal <n>**, the panel's subtitle `<title> · <folder>`, and the failures above, with `No terminal here: open Oby’s app.` (a page without main's half).

## Acceptance scenarios

- **TERMINAL-1** A shell opens only right after a click or Enter main saw, one per input; a page that isn't the window's own gets `Not this app’s page.`; at most 8 at once. (`terminal_1_a_shell_opens_only_right_after_a_click_main_saw_one_per_click`, `terminal_1: another page…`, `terminal_1: at most 8 shells at once`)
- **TERMINAL-2** A task's shell starts in its worktree's real path under obid's home, only while obid has the job, as the owner's login shell with `-l`, at the size the page measured, UTF-8; a job obid doesn't have, a missing worktree, a link out of obid's home, an id with a path or an unknown place start nothing. A conversation's starts in its first folder as obid lists it; one with none, nothing. The shell is the system's for the owner, never `$SHELL`. (`terminal_2_…`)
- **TERMINAL-3** The shell's environment is exactly the variables above: a poisoned app environment (obid's token, a vault secret, API keys, `ELECTRON_RUN_AS_NODE`, `NODE_OPTIONS`, `DYLD_INSERT_LIBRARIES`, an agent's `PATH`, `$SHELL`, `LANG=C`) passes none of it. (`terminal_3_…`, `terminal_3: main's own wiring…`)
- **TERMINAL-4** × on a tab sends SIGHUP (never a SIGTERM behind it, which can end a shell before it passes the SIGHUP on) and SIGKILLs after 1 second what ignores it, with every process group the shell started (its jobs, which never got its SIGHUP); a shell that ends on SIGHUP gets nothing more, and what it leaves running is its own choice (`nohup`, `disown`). Quitting signals every shell and SIGKILLs those left before the app ends. The page closing, crashing or loading again ends its shells, not another page's (a same-page link or a frame doesn't). A task deleted, its worktree gone, or a conversation deleted ends its shells only. A shell's exit tells its page once; nothing comes after its close. (`terminal_4_…`)
- **TERMINAL-5** The page alone types nothing; a key main saw lets through its own bytes once (`l`, not `ls\r`; Enter, `\r` only); named keys their escape sequences, never text; ⌘V the clipboard's own text, bracketed or not, in order; the terminal's answers without a key, never with text added; mouse reports only after a real mouse input; a key is good for 2 seconds. (`terminal_5_…`)
- **TERMINAL-6** A resize reaches the shell once per new size, never a size of nothing or out of bounds, and one that races the shell's end is dropped without a throw; no room, no shell. (`terminal_6_…`)
- **TERMINAL-7** Output that arrives within 4 ms goes to the page in one message. (`terminal_7_…`)
- **TERMINAL-8** A real shell (bash, node-pty) runs in the folder, sees no secret of the app's environment, prints 6,000 bytes of `é` in whatever chunks the pty cuts with none garbled, and on close ends with the job it left running. (`terminal_8_…`)
- **TERMINAL-9** Terminal (beside Changes in a task's menu, with a worktree) opens the panel and a shell at the size its box holds; output shows on its screen only; typing goes to main for that shell; main's refusal shows its words. (`terminal_9_…`)
- **TERMINAL-10** A resize while the shell opens reaches it once main answers; hidden, a terminal sends no size; shown again, the size it has, once. (`terminal_10_…`)
- **TERMINAL-11** + opens another; each tab's × ends its own shell; the panel's × ends them all; a tab closed while its shell opens ends that shell as soon as main answers, and opening again keeps the new one; a shell's exit takes its tab, the last closes the panel, and Terminal opens a new one. (`terminal_11_…`)
- **TERMINAL-12** Changes opening puts the terminal away with its shell kept and its output still drawn; Terminal brings it back without a new shell. Each place's terminals are its own. (`terminal_12_…`)

### Test it when it's done (a real Mac)

1. On a task with a worktree, ⋯ › **Terminal**: the panel opens, `pwd` prints the task's worktree, `echo $PATH` shows the owner's own (their profile ran), `env | grep -i -e token -e obid` prints nothing.
2. Type `ls`, Enter, arrows, ⌃C, ⌥e then e, a CJK word with an input method, and paste a two-line command: each does what it does in Terminal.
3. `npm run dev` (or `sleep 600 &`), then × the panel: `ps` in Terminal shows neither. Again, then quit Oby: same.
4. Open Changes: the terminal steps aside; Terminal brings it back with its output.
5. Delete the task with its terminal open: the terminal closes.
6. From the DMG's app (packaged): steps 1 and 3 again (node-pty's helper runs signed from the app).
