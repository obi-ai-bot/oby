# Oby (Electron)

Oby's new desktop app: Electron, with a React renderer built by Vite. It starts
as a copy of Sangam's design (`design/app`, 2026-10-26) and replaces the Swift
Mac app (`apps/mac`). In the app it shows what obid, Oby's engine, has (its API,
`docs/runner-api.md`): the obid already answering on `127.0.0.1:7777`, with the
token main reads from `~/Library/Application Support/Oby/Runner/api-token`
(the page never sees it). When obid doesn't answer, the window says why: obid's
own reason (`last-start-error`), else the connection's. The app doesn't start
obid yet: the Swift Mac app's login item does.

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
```

```
src/main/       the window (opens at 994×670, at least 420×520) and the cursor feed;
                obid.ts: obid's API through main, which holds its token;
                device.ts: this Mac's device key (safeStorage), which signs a Confirm
src/preload/    window.app: the traffic lights' position, full screen, the cursor feed,
                and obid (a call, and the event stream)
src/renderer/   the screens; tokens.css holds the shared look
native/         window-style: makes the window macOS's toolbar window type
build/          the app icon: icon.png, icon.icns, and make-icon.py that draws them
                (from oby-head.png: Oby's head, rising from the icon's bottom edge)
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
