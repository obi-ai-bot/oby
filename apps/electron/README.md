# Oby (Electron)

Oby's new desktop app: Electron, with a React renderer built by Vite. It starts
as a copy of Sangam's design (`design/app`, 2026-10-26) and replaces the Swift
Mac app (`apps/mac`). In the app it shows what obid, Oby's engine, has (its API,
`docs/runner-api.md`): the obid already answering on `127.0.0.1:7777`, with the
token main reads from `~/Library/Application Support/Oby/Runner/api-token`
(the page never sees it). When obid doesn't answer, the window says why: obid's
own reason (`last-start-error`), else the connection's.

The packaged app carries its own obid (`Contents/Helpers/obid`, with `gh` and
`gitleaks` in `Contents/MacOS`, as the Swift app does) and starts it as a launch
agent (`com.deskree.oby.electron.obid`, registered with SMAppService through
Electron), so it keeps running after Oby quits, with its jobs and the iPhone's
relay. Until users have moved over it is `Oby Electron.app`, with its own bundle
ID, `com.deskree.oby.electron`, and its own label, so dragged into Applications it
never replaces the Swift `Oby.app` (`com.deskree.oby`) and never shares its login
item. Both obids use the same configuration, home and port. While launchd has
the Swift Oby's login item (or install.sh's agent), this app registers none of
its own and uses that obid; it registers its own only when no other is
registered, so two agents never fight for 7777 at login. Before it adopts an obid
or sends it the token, it checks that what listens on 7777 is launchd's obid of
this user's: the pid of its own agent, its session job, the Swift Oby's or
install.sh's (`lsof` lists only this user's processes). 7777 is shared by every
user on the Mac, so another user's obid, or any other program (an agent's, from
its sandbox, included), gets nothing, and the window says the port is taken. The
packaged app quits when it's opened with a switch that runs another program as one
of its processes (`--browser-subprocess-path`, `--renderer-cmd-prefix`,
`--gpu-launcher`, `--utility-cmd-prefix`), and with a DevTools port or pipe unless an
admin allowed that on the Mac with a root-owned
`/Library/Application Support/Oby/allow-devtools-port` (the VM test does). When its own can't start, the
window says why: not in Applications (it offers to move itself there), waiting
for approval in System Settings › General › Login Items (it opens that page),
obid's `last-start-error`, else launchd's last exit code and the end of
`~/Library/Logs/obid.log`. macOS 26 often doesn't start a just-registered agent
until the next login (launchd exits 78: `copy_bundle_path … not a bundle`); the app
then runs the same obid as a job of this login session only. In development
(`npm run dev`) it starts nothing; `OBID_PORT` and `OBID_HOME` point it at a
scratch obid run by hand instead of 7777 (a packaged app ignores them).

Oby alerts on the Mac as the Swift app did (`src/main/notify.ts`): main follows
obid's event stream by itself, from launch until Oby quits, window closed or not,
and alerts for what obid pushes to the phones (`core/crates/obid/src/push.rs`,
`wanted`), plus Oby's answer in a conversation the window isn't showing. A click
opens the job or the conversation; Allow, Deny and Start answer from the alert as
the same tap in the window does, never from a locked Mac; a Confirm only opens the
window. Settings › Notifications turns them off (`settings.json` in the app's own
folder). macOS delivers no notification from the unsigned development build.

It shows only the conversation: a new user starts empty, Oby sets them up in
its first conversation (the tools on this Mac, GitHub, then Tetrix and the
Obysphere, both optional), and a task shows as a card in the conversation
that started it. What isn't used yet is hidden, not deleted: open the page
with `?everything` (`npm run web`, then http://localhost:5175/?everything) for
all of Sangam's design and its demo data, and `?welcome` to be set up again.
In a browser, or with `?everything`, it runs on the mock. Oby's first
conversation (`onboarding.ts`) sets a new user up: on obid, for someone it has
no conversation or job for yet (or with `?welcome`), with the tools' own
sign-ins through obid, and the first task goes to Oby.

```bash
npm install     # on macOS this also builds the native window module
npm run dev     # the app, with hot reload
npm run web     # the renderer alone in a browser, on port 5175
npm run build   # into out/
npm start       # the built app
npm run typecheck
npm run test:account-limits  # real Electron signing + scratch obid, after builds below
scripts/package.sh [arm64] [x64]   # signed, notarized DMGs in dist/ (below)
```

The account-limit integration regression runs on macOS with an unlocked login Keychain. Run
`cargo build --manifest-path ../../core/Cargo.toml -p obid`, `npm run build`, then
`npm run test:account-limits`. It uses the bundled Electron and stand-in agents, creates isolated
app and obid data, and exercises the actual preload, IPC, device key, signed HTTP and job
continuation paths. It checks grouped stop, cross-tool move, reset opt-in, and a visible error
when secure storage is unavailable. Electron retains the Mac's real `HOME` so macOS safeStorage
can reach the login Keychain; only its `userData` and obid's home are temporary. No real agent
login or installed job is used. `OBY_TEST_OBID` can select a separately built obid binary.

`scripts/package.sh` packages the Mac app on this Mac, as `scripts/release.sh`
does the Swift app's (`docs/release.md`, no CI): `scripts/helpers.sh` builds obid
universal from `core/` and stages it with gh and gitleaks through the Swift
app's own `apps/mac/app/Tools/bundle-*.sh`, each signed with the team's
Developer ID; then, per architecture, window-style is built for it,
electron-builder signs the app (`electron-builder.yml`: hardened runtime, V8's
JIT as the only entitlement, Electron's fuses closed), and the script checks it,
scans it with gitleaks, notarizes and staples it (keychain profile
`oby-notary`), and makes `dist/Oby-Electron-<version>-<arch>.dmg`, signed, notarized and
stapled too. It needs the Developer ID Application identity of team
`3UV9W5SV7W` (another team's: set `OBY_TEAM_ID` and `OBY_SIGN_IDENTITY`, its
name as in `"Name (TEAMID)"`) and both Rust targets (`rustup target add aarch64-apple-darwin
x86_64-apple-darwin`); `OBY_CARGO_TARGET_DIR` reuses a cargo target directory.
obid, gh and gitleaks are staged again only when `core/`, `plugins/` or the bundle
scripts changed since (`build/helpers/stamp`). `scripts/vm-test.sh [dmg]` then
runs the clean-install test in a throwaway macOS 26 VM.

```
src/main/       the window (opens at 994×670, at least 420×520) and the cursor feed;
                obid.ts: obid's API through main, which holds its token, and
                starting the app's own obid;
                device.ts: this Mac's device key (safeStorage), which signs a Confirm
src/preload/    window.app: the traffic lights' position, full screen, the cursor feed,
                and obid (a call, and the event stream)
src/renderer/   the screens; tokens.css holds the shared look. The icons are apps/icon's:
                the Dock's AppIcon.icon, the favicons exports/web (vite's publicDir)
native/         window-style: makes the window macOS's toolbar window type
build/          obid's launch agent and the entitlements; helpers/, what helpers.sh stages
scripts/        helpers.sh and package.sh: the signed, notarized app; vm-test.sh (with
                cdp.mjs): the clean-install test in a macOS 26 VM (Tart)
```

The renderer, in `src/renderer/src/`:

```
Home.tsx            the window: Oby's stage, the chips, the title bar and its trail
Sidebar.tsx         the side panel: Home's tasks (its foot opens Archived),
                    Conversations (yours with Oby: Pinned, Recent, each with
                    its menu), each Obysphere you're in (Knowledge, Invite
                    member, Members, Invited; its ⋮ opens its settings; + in
                    the icon strip makes another), Settings (from your
                    account picture)
GroupView.tsx       one kind of work's cards, Running or Completed
TaskView.tsx        one task on its own page; TaskCard.tsx draws its card
ChatView.tsx        a conversation: with Oby, with other people's Obys (the
                    Obysphere), or a task's own thread (one that came in from
                    GitHub, Slack…)
ObyView.tsx         someone else's Oby: your conversations with it
SettingsView.tsx    Settings' pages (as Wellington's Mac app has them, plus
                    Connectors), opened from your account menu; its page frame
                    and rows (SettingsPage, Group, Row, Switch, Select…) are
                    shared with the Obysphere's pages
Connectors.tsx      Settings › Connectors: on obid, GitHub (its gh) and Tetrix
                    as obid has them, with their sign-ins, Disconnect and
                    Tetrix's server; otherwise Sangam's demo list
SphereSettings.tsx  an Obysphere's settings: avatar, name, URL, members,
                    deleting or leaving it
KnowledgeView.tsx   an Obysphere's knowledge sources (Tetrix, MCP servers);
                    AddSourceModal.tsx connects another
ArchiveView.tsx     the archive: done tasks that left Home, as a page
InviteModal.tsx     inviting people into an Obysphere: by email, or a link
CreateSphereModal.tsx  making an Obysphere: name, URL, avatar, who to invite
EmailField.tsx      the field where addresses become tags (both modals above)
SphereTile.tsx      an Obysphere's picture: its image, a brand's mark, or a letter
Composer.tsx        the message box (its + panel, @ list and usage gauge);
                    MessageEditor.tsx is its Lexical text, and textTokens.ts
                    what it sets apart as you type
Formatted.tsx       a sent message's text, set as it was typed
Menu.tsx            the small ⋮ menu; TaskMenu.tsx is a task's, ConvoMenu.tsx a
                    conversation's
StatusDot.tsx       a task's state as one mark
Oby.tsx             Oby's frame; ObyFace.tsx its small icon and owners' pictures;
                    ObyFaceLive.tsx Oby's face in a conversation while it works
                    (2D, melting into dots)
brands.tsx          connector logos
useScrollThumb.ts   the Safari-like scroll thumb
lean.ts             what's shown: only the conversation, unless ?everything
onboarding.ts       setting up in Oby's first conversation, step by step;
                    SetupCard.tsx is the card under Oby's words where you act
chats.ts            your conversations with Oby (renamed, pinned, deleted), and
                    how Oby answers in the mock
tasks.ts, peers.ts  the tasks and the conversations with other Obys, as small stores
spheres.ts          the Obyspheres you're in, their members and invites
knowledge.ts        each Obysphere's knowledge sources
stream.ts           Oby's words coming in a word at a time
drafts.ts           each message box's unsent words, kept on this Mac (localStorage)
                    until sent, its conversation or task deleted, or a week untouched
obid.ts             obid's API: its types and calls (copied from apps/web/src/api.ts), through main
live.ts             what obid has, kept live by its event stream, put into the stores
                    above as conversations, tasks and accounts; and what the app asks of it
mock.ts             the demo data behind these stores, in a browser or with ?everything
tokens.css, app.css the shared look, and the rest of it
```

On macOS the window is the same type as Safari's (an empty, invisible
toolbar), so macOS draws its own macOS 26 corners and places the traffic
lights. Electron has no option for this yet (electron/electron#47514), so
`native/window-style` sets it, about 100 lines of Objective-C, and reports where
the lights are so the title bar lines up with them. Around full screen it takes
the toolbar off and puts it back, or leaving full screen would show dark
corners. Rebuild it with `npm run build:native` after an Electron upgrade.

Oby itself stays in `../../design/oby`, Sangam's copy of the character (ahead
of `character/`, which the Swift apps bundle). In dev it's served at `/oby/`, and a build copies
`oby.html` and three.js into `out/renderer/oby/`. The main process sends the
cursor position 60 times a second while the window is visible, so Oby watches
it anywhere on screen. In a browser it follows the page's pointer instead.
