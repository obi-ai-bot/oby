# Other computers, in the app

The business behavior contract for the computer picker in the Electron app, from [the template](../business-contract-template.md). Scenarios are numbered `COMPUTERSAPP-n`; the app's unit tests that prove them are named after them (`computersapp_<n>_…`, in `apps/electron/src/renderer/src/test/computers.test.tsx`). obid's side is [remote.md](remote.md) (`COMPUTERS-n`).

## Feature and status

- **Outcome:** The owner sees which computer the window works on, adds another Mac or Linux box over SSH or with a code, switches the whole window to it and back, renames or removes one, and is told plainly, and only then, when a computer can't be reached.
- **Status:** Every scenario that doesn't say it's proposed is **implemented on the demo data** (`?everything`, and `?everything&computers=<screen>` for one of the design's screens, 01 to 10): the button, its menu, the sheets and the window's states, driven by one store (`computers/store.ts`) over one interface to obid (`computers/obid.ts`, `ComputersApi`). **On obid nothing is live yet:** obid serves none of the routes the picker needs (the design's "What obid provides", all proposed), so on obid the picker isn't offered (COMPUTERSAPP-12). Each lands live in `computers/obid.ts` once obid serves its route, with main signing the owner's acts. **Proposed:** the message box's `Message Oby on {name}…`, its draft kept and Send waiting while no route answers (COMPUTERSAPP-13); the window's lists re-read from the computer in use after a switch (COMPUTERSAPP-14).
- **Design:** the owner's approved design of 2026-11-20, [docs/design/computer-picker/](../design/computer-picker/README.md) (#2635): every word on its screens and in its "Every word" tables is the owner's approved words, and the app uses them as they are.
- **Scope:** the Electron app: `apps/electron/src/renderer/src/computers/`, the button's place in `Sidebar.tsx`, the stage and the connection line in `Home.tsx`. **Out of scope:** obid's side ([remote.md](remote.md)); choosing a computer per task (later); the iPhone; the light theme.

## Behavior

### Actors and ownership

- **obid** keeps the window's computers (which exist, which is in use, each one's state and route, every route tried), adds, switches, retries, renames and removes them, and says what went wrong in its own words (`say`). The window never parses a link or SSH's output itself.
- **Main** holds obid's token; it will sign the owner's acts on computers (adding, switching, renaming, removing, the reach switches) as it signs their words ([signed-words.md](signed-words.md)), once obid takes them.
- **The window** shows what obid says, in the design's words, and sends the owner's clicks.

### Where it sits

- **One new control: the computer button**, at the mini sidebar's foot, right above Settings (or the account's picture with `?everything`). Its icon is the computer in use: a laptop for This Mac, a desktop for another Mac, a server for Linux; its tooltip that computer's name; for screen readers `Computer: {name}. Change`.
- **One menu does everything**, beside the rail, its foot level with the button's, shaped like the profile menu: This Mac, then each computer by name with a second line, then `Add a computer…`. With only This Mac, the line `Work on another Mac or Linux box from this window.` sits above `Add a computer…`. A computer's ⋯, on hover, has `Rename…` and `Remove…`. There is no Settings page for computers.
- **A row's second line:** the computer in use, its kind and route (`Mac · Local network`, `· Tailscale`, `· VPN`, `· Internet`, `· SSH`; the same with `Linux`) with a green dot while connected; another computer, its kind (`Mac`, `Linux`); one no route answers, `Can’t reach it right now`.

### States

- **Switching:** a row picked (or `Back to This Mac`) closes the menu; Home's line under Oby says `Switching to {name}…` / `{other} keeps its tasks running.` and the side panel and message box dim until obid says the new computer is in use.
- **Connected:** quiet. The menu marks the computer and says how it's reached.
- **Reconnecting on a fallback route:** quiet for 2 s; past that, the window's connection line says `{name}’s {route} doesn’t answer. Reconnecting over {route}…` (`local network`, `Tailscale`, `VPN`, `internet address`, `SSH`). Moving back up shows nothing.
- **Unreachable:** the button carries the orange needs-you dot; on Home, `Oby can’t reach {name} right now.` / `Its tasks keep running there, and Oby keeps trying.`, each route tried (`Local network · {address} · no answer`, `SSH · {obid's words}`) and the chips `Try again` and `Back to This Mac`; the side panel and message box stay, dimmed; outside Home, the connection line says `Oby can’t reach {name} right now.` with its red edge.
- **Adding over SSH:** the sheet's three steps (`Reached {host} over SSH`, `Pairing this Mac with Oby on {host}…`, `Finding the fastest way to reach it`) tick off as obid reports them; added, the window switches to it. When SSH says no, obid's words show as it says them, what to type in a terminal as code, with `Cancel` and `Try again`.
- **Adding with a code:** the person pastes the link; obid reads it and the sheet shows the computer's name, its fingerprint (16 bytes, groups of four hex digits) and `Check that {name} shows the same fingerprint.`; then the code. A link or code obid refuses shows its words, and the sheet stays.
- **This computer's code:** `This Mac’s code`, the code, its fingerprint, the addresses others reach it at (each with a switch), `Good until {time}, for one computer.`, `Copy link` and `Done`; with none to offer, `This Mac has no address another computer can reach. Add it over SSH instead.`
- **Rename:** the row's name becomes a field in place; Enter or a click elsewhere keeps it, Esc leaves it as it was.
- **Remove:** `Remove {name}?`, `This Mac stops reaching {name}, and {name} forgets this Mac’s key. Its tasks and folders stay on {name}.`, `Cancel` and `Remove`. Removing the computer in use switches to This Mac first.

### Invariants

- Every word the picker shows is the approved design's, or obid's own (`say`), shown as obid says it.
- The window never parses a link or what SSH printed: obid does (COMPUTERS-5, 6).
- Colour appears only when no route answers: the needs-you dot and the connection line's red edge.
- Nothing of the demo data shows on obid; on obid the picker shows only once obid serves its list.

## Interfaces and dependencies

`ComputersApi` (`computers/obid.ts`), one method per route the design proposes: `list` and `follow` (`GET /v1/computers`, the `computers` and `computer.route` events), `addOverSsh` (`POST /v1/computers {ssh}`, `computer.adding` events), `readLink` (`POST /v1/computers/link`), `addWithCode` (`POST /v1/computers {link, code}`), `invite` and `setReach` (`POST /v1/pairing/invite {computer: true}`, `PUT /v1/computers/reach`), `use` (`PUT /v1/computers/current`), `retry` (`POST /v1/computers/{id}/retry`), `rename` (`PATCH /v1/computers/{id}`), `remove` (`DELETE /v1/computers/{id}`). The demo's stand-in (`computers/demo.ts`) answers as the design says obid will. The client names no route obid doesn't serve ([ci.md](ci.md), CI-10).

## Acceptance scenarios

- **COMPUTERSAPP-1** (screen 01) With only This Mac, the button at the rail's foot, right above Settings, is This Mac's laptop, `Computer: This Mac. Change`, its tooltip `This Mac`; its menu has `This Mac` checked, `Work on another Mac or Linux box from this window.` and `Add a computer…`. (`computersapp_1_…`, three tests)
- **COMPUTERSAPP-2** (screen 06) With Studio in use and devbox, the button says `Computer: Studio. Change`; the menu lists This Mac, Studio (checked, `Mac · Local network`, the green dot) and devbox (`Linux`), with no note; each route's name as the design says it; a computer's ⋯ has `Rename…` and `Remove…`. (`computersapp_2_…`, three tests)
- **COMPUTERSAPP-3** (screen 09) A row picked closes the menu, Home says `Switching to Studio…` / `This Mac keeps its tasks running.` and the window dims until obid says Studio is in use; switching back says `Switching to This Mac…` / `Studio keeps its tasks running.`. (`computersapp_3_…`, two tests)
- **COMPUTERSAPP-4** (screens 02, 03) Add a computer has its words, the switch `Over SSH` | `With a code`, `SSH host` with `devbox, or you@192.168.1.20`, the hint, and `Add computer` waits for a host; adding, `Adding {host}` ticks its three steps off as obid reports them, and once added the window switches to it. (`computersapp_4_…`, two tests)
- **COMPUTERSAPP-5** (screen 03b) When SSH says no, obid's words show as it says them, `ssh studio` as code, with `Cancel` and `Try again`, which adds again. (`computersapp_5_…`)
- **COMPUTERSAPP-6** (screen 04) With a code, obid reads the pasted link: `devbox`, `7F3A 91C2 5D0E 44B8 · 0C6E A1F7 93D2 6B10`, `Check that devbox shows the same fingerprint.`, then `Code`; a link obid refuses says `That isn’t a link from Oby.`; a wrong code shows obid's refusal and the sheet stays; the right one adds it and the window switches to it. (`computersapp_6_…`, three tests)
- **COMPUTERSAPP-7** (screen 05) `Show this computer’s code` opens `This Mac’s code`: the code (`482 913`, read out digit by digit), the fingerprint, `Local network` and `Tailscale` with their addresses and switches (a switch turned goes to obid), `Good until {time}, for one computer.`, `Copy link` and `Done`; with no address, the words to add it over SSH instead. (`computersapp_7_…`, three tests)
- **COMPUTERSAPP-8** (screen 07) On a fallback route the connection line says `Studio’s local network doesn’t answer. Reconnecting over SSH…` only once it has lasted 2 s, with no dot and nothing dimmed; a fallback that ends sooner shows nothing. (`computersapp_8_…`, two tests)
- **COMPUTERSAPP-9** (screen 08) With no route answering, the button has the needs-you dot, the row says `Can’t reach it right now`, Home says `Oby can’t reach Studio right now.` / `Its tasks keep running there, and Oby keeps trying.` with each route tried and `Try again` (obid probes every route) and `Back to This Mac` (switches back); the window dims; outside Home the connection line says `Oby can’t reach Studio right now.` with its red edge. (`computersapp_9_…`, four tests)
- **COMPUTERSAPP-10** `Rename…` makes the row's name a field in place; Enter keeps the new name (sent to obid); Esc leaves it as it was and the menu open, even if the field then blurs. (`computersapp_10_…`, two tests)
- **COMPUTERSAPP-11** (screen 10) `Remove…` asks `Remove Studio?` with the design's words, `Cancel` and `Remove`; removing the computer in use switches to This Mac first; removing another leaves the window where it is. (`computersapp_11_…`, two tests)
- **COMPUTERSAPP-12** On obid the picker is offered only once obid serves its list: today no button and no call; outside `?everything` nothing of the demo data shows. (`computersapp_12_…`, two tests)
- **COMPUTERSAPP-13** (proposed, screens 06 to 08) On another computer the message box says `Message Oby on {name}…` (This Mac: `Message Oby…`); while no route answers, the draft stays and Send waits until the computer answers.
- **COMPUTERSAPP-14** (proposed) After a switch, the side panel, conversations, tasks, folders and Settings are the computer in use's, re-read as after obid restarts, once obid forwards the window's routes to it (COMPUTERS-14).
