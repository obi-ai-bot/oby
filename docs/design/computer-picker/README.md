# The computer picker: other computers in Oby's window

A design for the owner to approve, not built. It shows how a person adds another Mac or Linux box to Oby and switches the whole window to it, as T3 Code's remote environments do. Sangam owns the UI: where his screens or words differ, he leads, and this changes.

It follows the owner's decisions of 2026-11-19 ([the decision](https://github.com/obi-ai-bot/oby/pull/2633), `docs/decisions/2026-11-19-other-computers.md` once #2633 merges):

- any network the person has, Tailscale optional;
- SSH as a tunnel only, nothing installed over it;
- Macs and Linux;
- never the Obysphere;
- the whole window switches first; a computer per task comes later.

**See it:** open [`index.html`](index.html) in a browser for every screen at once, or `index.html?screen=06` for one at the window's size. The PNGs below are the same screens, 1280 by 800, on mock data.

Each screen is drawn with the Electron app's own CSS, from its production build at `522f6b6`, and Sangam's markup from its rendered Home. What the picker adds uses only his tokens and existing patterns:

- the profile menu's rows;
- the New project sheet;
- the Projects | Agents switch;
- the window's connection line ("Connecting to Oby's engine…");
- Home's grey line with chips under Oby.

## What holds on every screen

- **One new control: the computer button.** It sits at the mini sidebar's foot, above Settings. The sidebar is otherwise unchanged. The icon is the computer in use: a laptop for This Mac, a desktop for another Mac, a server for Linux. Its tooltip is that computer's name.
- **One menu does everything.** The button opens a menu shaped like the profile menu: the computers, then "Add a computer…". A computer's ⋯ (on hover) has Rename… and Remove…. There is no Settings page for computers.
- **The whole window is that computer's.** The side panel, conversations, tasks, folders and Settings all belong to the computer in use, and its own Oby runs the work there. The message box says where a message goes: "Message Oby on Studio…". On This Mac it stays "Message Oby…".
- **Quiet while it works.** Moving between routes shows nothing, except a fallback that takes more than 2 s. Colour appears only when no route answers: the orange "needs you" dot on the button, and the connection line's red edge.
- **Names.** "This Mac" is always this Mac. Another computer goes by the name its Oby gives (its host name), and Rename… changes it on this Mac only. The words are "computer", "Mac" and "Linux": never environment, backend, server, host or runner.

## Screens

| Screen | What it shows |
|---|---|
| [`01-no-computer-yet.png`](01-no-computer-yet.png) | **No other computer yet.** The button's menu: This Mac, checked; one line on what this is for; Add a computer…. |
| [`02-add-over-ssh.png`](02-add-over-ssh.png) | **Adding one over SSH.** One small sheet with the switch Over SSH \| With a code. The person types the SSH host as they would in a terminal. |
| [`03-adding-over-ssh.png`](03-adding-over-ssh.png) | **Adding over SSH: working.** The same sheet, its three steps ticking off. |
| [`03b-ssh-says-no.png`](03b-ssh-says-no.png) | **SSH says no.** obid's own words for what ssh said, and what to do. Here it's a host key never accepted, the most common case. |
| [`04-add-with-a-code.png`](04-add-with-a-code.png) | **Adding one with a code.** The person pastes the other computer's link. Its name and fingerprint show, to check against that computer's screen. Then they type the code. |
| [`05-the-other-computers-code.png`](05-the-other-computers-code.png) | **The other computer's code.** On the other Mac: its code, its fingerprint, the addresses it can be reached at (each can be turned off), and how long the code lasts. |
| [`06-connected.png`](06-connected.png) | **Connected.** The window works on Studio. The menu marks it and says how it's reached; a row's ⋯ shows on hover. |
| [`07-reconnecting.png`](07-reconnecting.png) | **Reconnecting on a fallback route.** The local network stopped answering, so Oby moves to SSH by itself. The window's connection line says so. |
| [`08-unreachable.png`](08-unreachable.png) | **Unreachable.** No route answers. Home says so with every route tried, plus Try again and Back to This Mac. Studio's last list stays, dimmed. |
| [`09-switching-back.png`](09-switching-back.png) | **Switching back to this Mac.** For the moment it takes: where it's going, and that Studio's work keeps running. |
| [`10-remove.png`](10-remove.png) | **Removing a computer.** From a row's ⋯. Studio forgets this Mac, and its work stays there. |

That's the whole flow: one button, one menu and one sheet, plus the window's existing connection line and Home's existing line under Oby.

## Every word, for approval

`{name}` is a computer's name, `{host}` what the person typed, `{time}` a local clock time. Words from obid are marked **obid**: the window shows them as obid says them, and they are listed here so the owner approves them too.

### The button and its menu (01, 06, 10)

| Where | Words |
|---|---|
| Button tooltip | `This Mac` · `{name}` |
| Button, for screen readers | `Computer: {name}. Change` |
| A row | `This Mac` · `{name}` |
| A row's second line, the computer in use | `Mac · Local network`, `Mac · Tailscale`, `Mac · VPN`, `Mac · Internet`, `Mac · SSH`, and the same with `Linux` |
| A row's second line, another computer | `Mac` · `Linux` |
| A row's second line, no route answers | `Can't reach it right now` |
| Only This Mac | `Work on another Mac or Linux box from this window.` |
| Last row | `Add a computer…` |
| A row's ⋯ | `Rename…` · `Remove…` |

### Add a computer (02, 03, 03b, 04)

| Where | Words |
|---|---|
| Title | `Add a computer` |
| Under it | `Its conversations, tasks and folders open in this window. Oby has to be running there.` |
| Switch | `Over SSH` · `With a code` |
| Over SSH: label, placeholder | `SSH host` · `devbox, or you@192.168.1.20` |
| Over SSH: hint | `Oby uses your own SSH: your keys, agent and known hosts. Nothing is installed there.` |
| Buttons | `Cancel` · `Add computer` |
| Working: title | `Adding {host}` |
| Working: steps | `Reached {host} over SSH` · `Pairing this Mac with Oby on {host}…` · `Finding the fastest way to reach it` (each step reads as it happens, and ✓ once done) |
| Trouble: buttons | `Cancel` · `Try again` |
| Trouble, **obid** (COMPUTERS-5) | `{host}'s host key isn't one you've accepted. Check it, then run ssh {host} once in a terminal to accept it.` |
| | `{host}'s host key changed: another machine may be answering. Oby won't connect until you check it and fix your known_hosts.` |
| | `{host} accepts none of your SSH keys (Oby uses your keys and agent, never a password).` |
| | `{host} didn't answer over SSH.` |
| | `ssh couldn't hold the tunnel's port on this Mac.` |
| | `ssh to {host}: {what ssh said}` |
| Trouble from `obid pair-computer` there, **obid** (COMPUTERS-7) | `This obid has no gateway, so no other computer can reach it: …` · `obid doesn't answer on 127.0.0.1:{port}: is it running?` · `obid refused /v1/devices (…): …` |
| Trouble, a Mac with Remote Login off (COMPUTERS-17, proposed) | `{host} doesn't take SSH. On it, turn on Remote Login in System Settings › General › Sharing.` |
| With a code: label | `Link` |
| With a code: hint | `On the other computer, choose Show this computer's code below. On Linux, run obid invite-computer.` |
| Link read | `{name}` · the fingerprint · `Check that {name} shows the same fingerprint.` |
| Link refused, **obid** (COMPUTERS-6) | `That isn't a link from Oby.` (proposed) |
| Code: label | `Code` |
| Code wrong, used or expired (COMPUTERS-13, proposed) | `That code doesn't match. Check it on {name}.` · `That code was used or has ended. Show a new one on {name}.` · `Too many wrong codes. Show a new one on {name}.` |
| Certificate isn't the pinned one (COMPUTERS-11, proposed) | `Another machine answered at {name}'s address, so Oby didn't connect.` |
| Link to the other side | `Show this computer's code` |

### The other computer's code (05)

| Where | Words |
|---|---|
| Title | `This Mac's code` (on Linux, `obid invite-computer` prints the same) |
| Under it | `On the Mac you're adding {name} to, paste its link and type this code.` |
| Labels | `Fingerprint` · `Other computers reach this Mac at` |
| Each address | `Local network` · `Tailscale` · `VPN` · `Internet`, with the address beside it, and a switch |
| None to offer | `This Mac has no address another computer can reach. Add it over SSH instead.` (proposed) |
| Footer | `Good until {time}, for one computer.` |
| Buttons | `Copy link` · `Done` |

The fingerprint shows the first 16 bytes of the certificate's SHA-256, in groups of four hex digits. That's enough to compare by eye. The link carries the whole pin.

### Connected, reconnecting, unreachable, switching (06 to 09)

| Where | Words |
|---|---|
| Message box | `Message Oby on {name}…` (This Mac: `Message Oby…`, as now) |
| Connection line, a fallback after 2 s | `{name}'s local network doesn't answer. Reconnecting over SSH…` (the route kinds: `local network`, `Tailscale`, `VPN`, `internet address`) |
| Connection line, no route (red edge), outside Home | `Oby can't reach {name} right now.` **obid** (COMPUTERS-3) |
| Home, no route | `Oby can't reach {name} right now.` / `Its tasks keep running there, and Oby keeps trying.` |
| Home, each route tried | `Local network · {address} · no answer`, `Tailscale · {address} · no answer`, `SSH · {obid's SSH words}` |
| Home, chips | `Try again` · `Back to This Mac` |
| Switching | `Switching to {name}…` / `{other} keeps its tasks running.` (to This Mac: `Switching to This Mac…`) |
| Message box, no route | the draft stays; Send waits until the computer answers |

### Remove (10) and Rename

| Where | Words |
|---|---|
| Title | `Remove {name}?` |
| Under it | `This Mac stops reaching {name}, and {name} forgets this Mac's key. Its tasks and folders stay on {name}.` |
| Buttons | `Cancel` · `Remove computer` (changed by the owner on 2026-11-21 from `Remove`, their choice "Remove computer (Recommended)": the button signs the removal, and a bare Remove would let any other Remove button's click sign it, as Settings › Vault's `Remove Secret` avoids; Sangam's to see) |
| Rename | the row's name becomes a field in place, as a conversation's name does in the side panel |

## What obid provides, state by state

This design assumes the proposal in the lab's README ([item 8](../../../tools/obid-spec/workflows/lab/remote/README.md#what-obid-needs-when-ported)): the Mac's own obid keeps the window's computers (`home/computers.json`, tokens in the vault, never in an answer), runs the supervisor and the tunnels, and forwards the window's calls to the computer in use. Where computers live is still the owner's decision. If they live elsewhere, the endpoints below move with them; the screens stay the same.

Every key below is new and optional, so the API only grows. **Existing** marks what obid has on `dev` or in #2633. **Proposed** marks what the picker needs, to add to `docs/contracts/remote.md` (in #2633) as COMPUTERS-18 onward once the owner approves.

| State (screens) | What obid provides | Scenarios |
|---|---|---|
| The list and the button (01, 06) | **Proposed:** `GET /v1/computers` → `{"current": "this" \| <id>, "computers": [{"id", "name", "os": "mac" \| "linux", "state": "connected" \| "reconnecting" \| "lost" \| "idle", "route": {"kind": "lan" \| "tailnet" \| "vpn" \| "public" \| "ssh", "address"}?, "tried": [{"route", "what", "why"}]?}]}`. Plus the event `computers` on the window's stream whenever it changes. `os` comes from the other obid's status (proposed key). | COMPUTERS-1, 3; LAB-REMOTE-4 |
| Adding over SSH (02, 03) | **Proposed:** `POST /v1/computers` `{"ssh": "{host}"}`, an owner's action, signed like any wider setting. obid runs the person's own `ssh` with COMPUTERS-4's options. It sends `obid pair-computer` this Mac's name and device key on stdin (**existing**, COMPUTERS-7), opens the tunnel (COMPUTERS-12) and redeems the invitation through it (**existing** `POST /v1/pairing`). Then it asks the other obid for its direct routes and pin (proposed: `pair-computer`'s `invite` gains `routes`, so a computer added over SSH also gets its faster routes). Progress comes as events: `computer.adding {"id", "step": "ssh" \| "pair" \| "routes", "state": "doing" \| "done" \| "failed", "trouble"?}`. | COMPUTERS-4, 7, 8, 9, 12, 14, 15; LAB-REMOTE-1, 2, MAC-1, MAC-2, MAC2-1 to 3 |
| SSH says no (03b) | **Existing:** `SshTrouble` and its `say` (COMPUTERS-5); `pair-computer`'s exit-1 lines (COMPUTERS-7). **Proposed:** `trouble` on `computer.adding` carries `{"trouble": <kind>, "say": <words>}` as obid already serialises it. Remote Login off: COMPUTERS-17. | COMPUTERS-5, 7, 17; LAB-REMOTE-1, MAC-1, MAC2-1 |
| Reading a link (04) | **Proposed:** `POST /v1/computers/link` `{"link"}` → `{"name", "fingerprint", "routes"}`, or `400` with COMPUTERS-6's refusal in words. The window never parses the link itself. | COMPUTERS-2, 6 |
| Adding with a code (04) | **Proposed:** `POST /v1/computers` `{"link", "code"}`. obid reaches a route with its pin before it sends the code (COMPUTERS-11, 13). A wrong, used or ended code, or the fifth wrong try, comes back as its own refusal for the words above. | COMPUTERS-10, 11, 13; LAB-REMOTE-3, 7 |
| The other computer's code (05) | **Existing:** `POST /v1/pairing/invite` `{"computer": true, …}` (COMPUTERS-8), its `code`, `link` and `expiresAt`. **Proposed:** the link holds the direct routes and the pin (lab item 6). `GET /v1/computers/reach` and `PUT /v1/computers/reach` `{"lan": true, "tailnet": true, …}` let the owner pick the interfaces the HTTPS listener serves (COMPUTERS-10). `obid invite-computer` prints the same link, code, fingerprint and end time on a Linux box with no window. One open invitation at a time (lab item 7). | COMPUTERS-8, 10, 13; LAB-REMOTE-3 |
| Switching (06, 09) | **Proposed:** `PUT /v1/computers/current` `{"id": <id> \| "this"}`, then the event `computers` with the new `current`. Every route the window already uses then answers for that computer (forwarded with its token, COMPUTERS-14), so the window re-reads its lists the way it does after obid restarts. Nothing on the other computer stops. | COMPUTERS-14, 15; LAB-REMOTE-6, MAC-3, MAC2-4 |
| Connected (06) | The route in use, from `GET /v1/computers` (`route.kind`). The window's own connection (`useLive`) stays live while the computer in use answers. | COMPUTERS-3 |
| Reconnecting on a fallback route (07) | **Existing:** the supervisor's `Change::Moved {from, to}` (COMPUTERS-3). **Proposed:** obid sends it as `computer.route {"id", "change": "moved", "from", "to"}` and sets `state: "reconnecting"` from the cut until a route answers. The window shows its connection line only when that lasts over 2 s. Moving back up sends the same event, and nothing shows. | COMPUTERS-1, 3, 12; LAB-REMOTE-4 |
| Unreachable (08) | **Existing:** `Change::Lost {tried}` and `unreachable()`'s words (COMPUTERS-3). **Proposed:** `computer.route {"id", "change": "lost", "tried"}` and `state: "lost"`. The supervisor keeps probing, and `POST /v1/computers/{id}/retry` probes every route now. "Back to This Mac" is `PUT /v1/computers/current {"id": "this"}`. The window keeps the last lists it read, dimmed, until a route answers. | COMPUTERS-3; LAB-REMOTE-4 |
| Rename | **Proposed:** `PATCH /v1/computers/{id}` `{"name"}`, kept on this Mac only. | — |
| Remove (10) | **Proposed in the contract:** COMPUTERS-16, a revoke over the computer's own token that also forgets its key there. Exposed as `DELETE /v1/computers/{id}`, an owner's action, signed. Removing the computer in use switches to This Mac first. | COMPUTERS-16; LAB-REMOTE-5, MAC-4, MAC2-5 |

## Decisions for the owner

1. **The button's place:** the mini sidebar's foot, above Settings, for the whole window. T3 Code puts it by the message box, but there it's per conversation, which comes later here.
2. **No Settings page for computers:** the menu lists them, and a row's ⋯ renames or removes one.
3. **"Show this computer's code" lives inside Add a computer › With a code**, so there's one way in on both sides.
4. **The code's life:** `Good until {time}`. Phone invitations last two minutes; this design suggests 10 minutes for a computer, since the person walks between machines.
5. **While on another computer, Settings is that computer's** (its accounts, connectors, routines). Only the computer menu is always this Mac's.
6. **Where the window's computers live** (lab item 8): this design assumes the Mac's own obid.

## Not designed yet

- Choosing a computer per task (later, per the owner's decision).
- The iPhone.
- Light theme.
- A computer's own routes list (add or reorder a route by hand); the routes come from pairing.
