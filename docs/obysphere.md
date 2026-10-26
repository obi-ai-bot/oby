# The Obysphere in the Mac app, as Sangam designed it: the plan

The owner, 2026-10-25: clicking Deskree's Obysphere in Sangam's design isn't what the Mac app does today. Sangam's design leads (AGENTS.md), so the Mac app changes to match it, with the rules kept underneath. This is the plan for all of it: the side panel, a page for each member's Oby, inviting, the Obysphere's settings, and Knowledge, with what obid and the Obysphere server need for each. The owner decided its open questions the same day ("Decided", at the end). Step 1 is #1448; nothing else is built yet.

It builds on [onboarding.md](onboarding.md) section 4 (roles, Knowledge per Obysphere, another Obysphere is another world) and [trim.md](trim.md) core 4 (Obys talk: reviews first, working together later).

## What each side has today

| Sangam's design (`design/app/src/renderer/src/`) | The Mac app | obid | The Obysphere server (obi-sphere) |
|---|---|---|---|
| **The tile opens a side panel** (`Sidebar.tsx`): "Deskree", "9 members", ⋮ (Settings; Leave for a member), Knowledge, Invite member, Members, Invited | The tile opens Settings › Obysphere, one long form (`SphereSettings.swift`) | One membership (`/v1/obid/sphere`) | Members list, one admin per Obysphere |
| **A member's Oby** (`ObyView.tsx`, `ChatView.tsx` as `peer`): "Ernesto's Oby · Assisted with 3 tasks", your conversations with it, "Message Ernesto's Oby" | Nothing: no page, no conversations with another Oby | Exchanges kept one file each (`home/exchanges/`), listed only all together; `POST /v1/obid/sphere/requests` sends any request type, but the Mac never calls it | No conversation or thread: a `question` is one exchange that ends at its answer; one `to` per message, so no groups |
| **Invite member** (`InviteModal.tsx`): emails as tags, Send invitation, the invite link, "Only allow @deskree.com emails to join" | Only when creating: one open invitation for that many people, and a Mail draft for the owner to send | Nothing (the Mac calls the server) | Named or open invitations, `domain`, withdraw. No list of invitations, no resend, no email: the Obysphere never sends mail or keeps an address |
| **⋮ › Settings** (`SphereSettings.tsx`): avatar, name, URL, members with Remove, invited with Cancel invite, Delete or Leave | Members with Remove and key checks, the team link, Leave, "Asked of your Oby", the server, Host your own | — | Remove and leave exist. No avatar, rename, URL change, delete, or second admin |
| **Knowledge** (`KnowledgeView.tsx`, `AddSourceModal.tsx`): Tetrix connected once for everyone, who can use it, when Oby uses it, tools on or off, admin tools never shared | Tetrix per Mac, in Connectors | Tetrix per Mac: OAuth, the vault, the broker with its allowlist for this Mac's agents | Nothing: no request type for a lookup |
| **Asks on Home** (`AskStack.tsx`): what other Obys ask waits in the card stack | Only a run request (`start`) and a share running out reach Home; a review request or a question waits in Settings only, answered with buttons, never in words | The inbox's `obiRequest` items, with any words as an answer to a question | — |

## The plan, part by part

### 1. The side panel

What the person sees, from `Sidebar.tsx`: the tile switches the side panel to the Obysphere and leaves the middle as it was. The header reads "Deskree" and "9 members" (you included). Its ⋮ has the Obysphere's tile, name and address, then **Settings**, and **Leave Deskree** for a member. Under it:
- **Knowledge** and **Invite member**.
- **Members**: each with their picture and the Oby badge, a blue dot when a conversation with their Oby has something new, and how many conversations you have with it. With nobody else: "Just you so far".
- **Invited**, once someone is: each email, and for an admin a ⋮ with Resend invite and Cancel invite.

- **Mac:** a `sphere` section in the side panel (`App/Sidebar.swift`), as Home and Conversations are. The tile selects it instead of opening Settings.
- **obid:** nothing new for the header and members. The dots and counts come with part 2, and Invited with part 3.
- **Underneath:** "Leave" ends this Mac's membership as Settings does today.

### 2. A member's Oby, and conversations with it

What the person sees, from `ObyView.tsx` and `ChatView.tsx`:
- **The page:** "Ernesto's Oby" and "Assisted with 3 tasks", then every conversation with it, the latest first. Each shows who's in it, its title, the last line ("You: …", "Ernesto's Oby: …"), when, and how its task stands.
- **Starting one:** "Message Ernesto's Oby" at the foot starts a new conversation.
- **Inside a conversation:** your messages, your Oby's lines and theirs, and a task's card under a line that's about a task. A group (other Obys joined in) shows them all.
- **Review requests count too:** what Oby sent or received for a task, like "Review of #412 retry jitter", is a conversation as well.

**The protocol has no chat.** obid builds conversations out of exchanges, and the protocol stays as it is but for one optional link:
- **A conversation is obid's own record:** its id, the Obys in it, its title, its lines, the tasks they're about, and whether it has something new. Every exchange with another Oby belongs to one, the review requests Oby sends by itself included.
- **Each message you send is a `question`** (its `text`, at most 2,000 characters, secret-scanned before it goes). Their Oby's lines are its `status` notes and its `answer`. As the protocol says, it answers by itself only from job status and shared results; anything else goes to Ernesto, and his words come back as the answer.
- **The spec gains one optional field:** a `question` may name the exchange it follows (obi-sphere, protocol.md section 16 lists "no field links a question to an earlier exchange" as open). Then Ernesto's Mac shows the same conversation, not a pile of separate questions.
- **A group is one exchange per Oby:** obid sends them and merges the answers into one conversation. The protocol keeps each Oby unaware of who else was asked, and that doesn't change.
- **"Assisted with N tasks"** counts the distinct tasks in its conversations (an exchange already keeps its `job`).

- **obid:**
  - The conversation store, filled from exchanges both ways, kept across restarts.
  - `GET /v1/obid/sphere/conversations?peer=`, `GET …/conversations/{id}`, `POST …/conversations` (to whom, the words), `POST …/conversations/{id}/messages` and `POST …/conversations/{id}/read`.
  - A `peerConversation` event on `/v1/events`.
  - All documented in runner-api.md.
- **Mac:** the member's page and the conversation, reusing the conversation page's lines and task cards; the dots and counts in the panel.
- **obi-sphere:** the optional link on `question` in the spec, its schema and the server's checks, with conformance cases.
- **Underneath:**
  - Another Oby's words are data: shown quoted, never handed to an agent as instructions.
  - Every line you send is secret-scanned, and appears in the job's or the conversation's outgoing list.
  - Until obid seals message bodies (#1233), the Obysphere's server, and every member who reads the audit chain, can read these words. Decided: sealing comes first, before conversations and Knowledge ship.

### 3. Invite member

What the person sees, from `InviteModal.tsx`:
- **The dialog:** "Invite to Deskree's Obysphere", emails typed or pasted as tags (a wrong one in red), then **Send invitation**, which says "Sent" and closes.
- **The other way in:** the invite link with a copy button, and "Only allow @deskree.com emails to join" when the Obysphere has a domain.
- **Afterwards:** the people invited appear under Invited, in the panel and in Settings.

- **obi-sphere:** `GET /v0.1/spheres/{id}/invitations` for the admin (pending invitations: id, created, expires, uses, domain, and a label the admin gave it), and a `label` on an invitation (the email, for the admin's own list; the server never mails it).
- **obid or the Mac:** one named invitation per email (one use), labelled with the address. Cancel withdraws it. Resend withdraws it and makes a new one (the server keeps only the link's hash, so the old link can't be sent again).
- **Sending the email (decided):** the admin sends it from their own Mail. Send invitation opens a draft with the link for each address, as Create does today. The server never sends mail and never keeps the address.
- **Underneath:** the link keeps its secret after `#`, as today, so a shorter `deskree.obysphere.com/join/abc123` (the design's mock) isn't possible as it is. The dialog shows the real link.

### 4. The Obysphere's settings

What the person sees, from `SphereSettings.tsx`: "Deskree settings", with:
- **Avatar, Name and Obysphere URL:** saved as they change; only an admin edits them.
- **"Members · 9":** you first, then each member with their role and an admin's Remove, then the invited with Cancel invite.
- **Delete Obysphere (admin)** with its name typed to confirm, or **Leave (member)**.

- **obi-sphere:**
  - `PATCH /v0.1/spheres/{id}` (name, image; admin only), with the image kept small in the database (a square PNG or JPEG of at most 256 KB). Today the logo lives on the creator's Mac only.
  - `DELETE /v0.1/spheres/{id}`: every token revoked, every delivery dropped, every member revoked, then the Obysphere gone. Its audit chain goes too (decided: each member's Oby keeps its own record of its own exchanges).
  - A second admin: onboarding.md already decided admins make other admins ("Make admin"), and the server has one `admin_owner` today.
- **Mac:** the page, opened from ⋮ › Settings. App-wide things move out of it:
  - **The server address and Host your own:** to Settings › General, under Obysphere, since the design has no screen for them.
  - **"Asked of your Oby":** to Home's card stack (part 6).
- **The URL:** the id is signed into every message and token (`aud: obi-sphere:<id>`), so changing it isn't a rename. Decided: it stays as it was made, shown read-only, for now.
- **Underneath, the key checks:** comparing a member's key, and accepting a new one, aren't in the design. Pins stay automatic (the first key is trusted, a later change is caught). A changed key becomes an ask on Home ("Ernesto's Oby has a new key: compare it with him"), and comparing one stays on the member's page ⋮. Decided as written.

### 5. Knowledge

What the person sees, from `KnowledgeView.tsx` and `AddSourceModal.tsx`:
- **The page:** "Knowledge systems every Oby in Deskree's Obysphere has access to."
- **Tetrix's group:** "Tetrix · deskree-inc", connected by an admin, then:
  - **Who can use it:** Everyone or Admins only.
  - **When Oby uses it:** Automatically or Ask first.
  - **Its tools:** Search, Look up, Query the graph, Impact and Service map on; Cost off; Diagnostics and Replay and purge locked off ("everyone would act as the admin who connected it").
- **Admin actions:** add a source (Tetrix, or a custom MCP server: Neo4j, Qdrant, pgvector) and disconnect one.
- **What a member sees:** every value, and no buttons.

AGENTS.md already decides how it works: "the admin's Oby, or Oby Cloud, holds the connection, runs the members' requests against it (as an Obi-to-Obi request, typed and quoted like any other) and returns only the answer". So:

- **obi-sphere, the spec:**
  - A new request type, `lookup`. It names the source, a tool and its arguments, and is answered by a `result` with the tool's content.
  - It comes with a `send:lookup` scope, the policy action, a card's `accepts`, the exchange rules and the policy's `actions` limit (5 today, exactly the request types).
  - **A record of the Obysphere's sources:** `GET/PUT /v0.1/spheres/{id}/knowledge`, written by the admin, read by members. It names each source, who holds it, who may use it, whether to ask first, and which tools are on. It never holds a secret.
  - A `question` can't do this: the protocol keeps a question to words.
- **obid on the admin's Mac:**
  - Connecting Tetrix for the Obysphere uses the login obid already holds (`oby.tetrix.*`) and writes the record.
  - It answers each `lookup` itself, without the owner, when the record allows it: the asker may use the source, the tool is on, and it isn't an admin tool. The call goes through the same allowlist, secret scan and scrubbing as this Mac's own agents' calls (`tetrix.rs`). Only the content goes back.
  - Each lookup is listed in `connectionUses`.
- **obid on a member's Mac:** its Tetrix broker offers the Obysphere's tools to its agents next to its own. Each call becomes a `lookup` to the admin's Oby: its arguments are secret-scanned first, it's listed in the job's `outgoing`, and it waits for the answer. With "Ask first", the member is asked on Home before each lookup.
- **When the admin's Mac is away:** the lookup fails at once with a plain reason ("Deskree's Tetrix is on Doti's Mac, which isn't reachable"). It's never silent, and never an empty answer. This is the Tetrix audit's "losing Tetrix is silent", fixed for shared sources from the start. Decided: the admin's Mac holds it for now, and the Knowledge page says whose Mac; Oby Cloud may hold it later.
- **Mac:** the Knowledge page and the Add source dialog (Tetrix first; a custom MCP server after, through the generic broker in `connections.rs`).
- **Underneath:**
  - The admin's Tetrix login never leaves the admin's Mac: no member, agent or device gets it.
  - Tetrix's answers are data.
  - Admin tools are never offered.
  - **The audit chain is plaintext, readable by every member** (spec section 15). A lookup's question and its answer would be readable by every member, Admins only or not. Decided: message bodies are sealed end to end (#1233) before Knowledge ships.
- **First:** the Tetrix audit's fixes. One Tetrix error left an agent with no tools for the whole job, and losing Tetrix was silent. Shared Knowledge rests on the same broker. Done before launch: a failed listing is asked again at the next call, an agent always lists its tools, and a Tetrix that can't be reached is said on the job (`tetrix.unreachable`, its summary) and in Settings (docs/runner-api.md, "obid only: Tetrix").

### 6. Asks on Home

The design puts every ask in Home's stack, those from other Obys too. Today a review request or a question from another Oby waits only in Settings, because Engine's inbox kinds don't include them (`Engine/Inbox.swift` maps only `run_request` and `share_used`).

- **Engine and Mac:** map `obiRequest` items (review request, question, approval on the code host) into Home's cards with their options. A question also takes your own words, through the composer, as obid already accepts.
- **The key change from part 4** is one more kind of ask.

## Order

Each step is one pull request, tested as the merge ritual says, and checked against the design in the browser and on the Mac.

1. **Asks on Home** (part 6). Small, Mac and Engine only, and it fixes a real gap today.
2. **The side panel and the settings page** (parts 1 and 4), over what the server has today. Rename, avatar, delete and the invited list show once the server has them, and say so until then. The server address and Host your own move to Settings › General.
3. **obi-sphere: the Obysphere's edits** (rename, image, delete, a second admin) and **invitations** (list, label). Then Invite member and Invited in the Mac (part 3).
4. **Sealed message bodies** (#1233): only the two Obys in an exchange read it; the server and the other members see only that it happened.
5. **Conversations with other Obys** (part 2): the spec's optional link first, then obid's store and API, then the member's page and the conversation in the Mac.
6. **Knowledge** (part 5): the Tetrix audit's fixes, then the spec's `lookup` and the record of sources, the admin's and the member's obid, then the page.
7. **Several Obyspheres**, after conversations (decision 1).

The tests the plan in #1444 calls journeys get one each: invite someone and see them join (J6), a conversation with a teammate's Oby, and a member's agent looking something up in the admin's Tetrix (J7).

## Decided (the owner, 2026-10-25)

1. **One Obysphere now, several later.** obid keeps one membership until conversations work; then a membership per Obysphere, and a tile for each in the strip.
2. **The admin sends invitations from their own Mail.** Send invitation opens a draft with the link for each address. The Obysphere server sends no mail and keeps no address.
3. **The Obysphere URL can't change, for now.** It stays as it was made and shows read-only.
4. **Key checks stay underneath.** Pins are automatic: the first key is trusted, and a later change is caught. A changed key is an ask on Home ("compare it with them"), and comparing a key on purpose is in the member's ⋮ on their Oby's page. Nothing else in Sangam's screens changes.
5. **Sealed first.** Message bodies are sealed end to end (#1233) before conversations and Knowledge ship: only the two Obys in an exchange read it, and the server and the other members see only that it happened.
6. **The admin's Mac holds a shared Tetrix, for now.** When it's away, a member's lookup fails at once with a plain reason, never silently, and the Knowledge page says whose Mac holds it. Oby Cloud may hold it later.
7. **Delete erases.** Deleting an Obysphere removes it and its audit chain from the server, as Sangam's words say. Each member's Oby keeps its own record of its own exchanges.

## Not in this plan

- Several Obyspheres at once (decided: after conversations).
- Custom MCP servers in Knowledge after Tetrix.
- The iPhone (it waits for the Mac, trim.md).
- Who may see an Oby's presence. Neither the design nor the protocol has any.
