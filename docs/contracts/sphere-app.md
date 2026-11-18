# The app's Obysphere, for launch

The business behavior contract for what the Electron app does in an Obysphere at launch, from [the template](../business-contract-template.md): joining from a link pasted in the app, the members rule and a membership that grow to take handoffs, a revoked membership that says so, handing a member's Oby a task, and what is hidden for launch. obid's side is [obysphere.md](obysphere.md), [handoff.md](handoff.md) and [peer-thread.md](peer-thread.md); the rules' engine is [sphere-policy.md](sphere-policy.md). Scenarios are numbered `SPHEREAPP-n`, each tested as `sphereapp_n_…` in `apps/electron/src/renderer/src/test/` (`sphere.test.ts`, `sphereApp.test.tsx`, `ipc.test.ts`).

## Feature and status

- **Outcome:** A stranger joins their team's Obysphere from its invite link inside Oby, on first run or in Settings, seeing who they will join as before anything is made. Every member may hand every other a task, including members who joined before handoffs. An owner hands a member's Oby a task from that member's thread and reads what comes back there. An Oby whose admin took it out says so instead of vanishing.
- **Status:** **Implemented** in the Electron app (`apps/electron`). Changing the owner id waits on obid's add-only `PUT /v1/obid/sphere/owner` (SPHEREAPP-9): until obid serves it, the id shows read-only. A revoke's words follow obid's add-only `revokedReason` code once obid sends it.
- **Scope:** the app only. **Out of scope:** making an Obysphere, inviting, and an admin removing a member (the Obysphere's own site does those); handing over a task by talking to Oby (`oby_handoff`, #2428's conversation tool); comparing keys.

## Behavior

### Actors and ownership

- **The owner** pastes a link and clicks Join, answers main's Join dialog, fills and sends a task, and leaves.
- **Main** holds the owner key, joins through its one join path (the `oby://` link's), signs the card and delegation, renews them, and sends a handoff through obid. It acts on the page's request only right after a click or key it saw itself (`takeInput`, as POLICY-4).
- **obid** keeps the membership, sends the handoff (`POST /v1/obid/sphere/requests`), and keeps every exchange.
- **The page** holds the link and the form's fields, and shows what main and obid answer. It decides nothing and signs nothing.

### Joining from a link

- First run's Obysphere step, where Oby can't make an Obysphere, has a field for the team's invite link under its words, with Join; Let's go goes on whether or not it joined. Settings has an Obysphere page with the same field.
- A text that isn't an invite link says so under the field, and Join stays off. A link shows its Obysphere's name, its host and the owner id obid would join as (`GET /v1/obid/sphere`'s `owner`) before any key is made.
- Join hands main the link. Main reads it as `oby://` links are read, opens the same native Join dialog (its detail now also says `You’ll join as <owner>.`), and only the owner's Join there joins, signing with the app's owner key. The page shows `You’re in <name>.`, or why it failed in main's words; the dialog's Cancel shows nothing.
- In an Obysphere, the Settings page shows its name and the owner id instead of the field.
- **Changing the owner id.** Two strangers whose Macs share a login name, with no `gh` signed in, would join as the same owner, so the id is the owner's to change before their first join. While obid's membership is `none`, the page asks obid once, with `PUT /v1/obid/sphere/owner` and an empty body (which changes nothing: `422`); where obid answers `404` the id shows read-only. Where it's served, the owner line has Change: a field and Save. An id that isn't lowercase letters, digits and single hyphens (1 to 63, the spec's slug) says so and sends nothing; one obid keeps shows on the line, and main's Join dialog says it. obid's `409` (a membership exists) says `It can’t change once you’ve joined an Obysphere.`

### The members rule and a membership grow

- The members rule (`members-ask`) lets every member ask a question or a review and hand over a task: `question`, `review_request` and `handoff`, in every repository.
- An admin's "Let members ask" adds the rule when no rule has its id; when one has, it adds only the actions that rule lacks, at its end, keeping everything else in it and every other rule as it was. Nothing is written when nothing is missing.
- At every keep (launch, after a join, every 5 minutes), a membership whose card accepts less than the app's (`review_request`, `question`, `handoff`) gets a new card accepting both what it had and what it lacks; one whose delegation's scope lacks one of the app's scopes gets a new 30-day delegation with both. Signed once: the next keep finds nothing missing and signs nothing. Owner clicks aren't asked for: each handoff still waits for its owner's Accept on Home (HANDOFF-2).

### A revoked membership

- When obid's membership is `revoked`, the Obysphere stays in the side panel with no members, its settings open with a line from obid's `revokedReason` code (an open set): `removed`, or no code, says `<name>’s admin took your Oby out.`; a code this app doesn't know says `Your Oby isn’t in <name> any more.`, and a code is never shown as it is. Then the field for a new link, and Leave. Its rules and What your Oby tells are not shown. Settings › Obysphere shows the same line over its field. Joining again leaves the revoked membership first (SPHERE-1's join).

### Handing a member's Oby a task

- A member's thread, while they're in the live Obysphere, has `Hand <name>’s Oby a task` over its box. It opens a form: what to do (one line, at most 200 characters), the repository (`owner/name`, offered from obid's repositories; one picked starts from its own default branch) and the branch to start from (`main` unless changed), with `Done means its tests pass.`
- A field that can't go says why on Hand it over, and nothing is sent. Hand it over goes to main, which, right after that click, picks the member's Oby that takes `handoff` (a sealed one first) and sends one `handoff` through obid: the goal, repository and base as typed (trimmed), the one condition `tests_pass`, open for 7 days. The form closes and the thread reads again.
- Refusals stay on the form: a member whose Oby doesn't take tasks (`<owner>’s Oby doesn’t take tasks yet.`), someone not in the Obysphere, the Obysphere's rules (`This Obysphere’s rules don’t allow handing over tasks yet. An admin can change that.`), or obid's own words.
- What comes back reads in the thread under the task: `Handed over. It’s waiting on <name>.`, `<name> said yes. Their Oby is on it.`, `<name>’s Oby sent back its result.` (the result's summary is its own post), or why it was declined: `<name> said no to this task.`, `<name>’s Oby couldn’t start it.`, `<name>’s Oby doesn’t take tasks.`, the rules, busy, expired, or `declined without saying why`.

### Hidden for launch

- The room: the app never looks for one, so its tile click, Settings' "Also post in the room" and its posts never show (`ROOM_SHOWS` in `spheres.ts`).
- Other Obys in the message box are no longer hidden: on obid, "Add other Obys" and `@` list the live members that take questions, and a mention asks them (ask-an-oby.md, "Asking by @ in the message box", ASK-37 to 41). Out of a Sphere they list no one and + has no "Add other Obys".

### Failures and recovery

- obid not answering when the page reads the owner id: no owner line shows; main's dialog still says it when it can.
- A join or a handoff that fails changes nothing the page holds but the problem line; the owner can try again.
- A page script that took the window over can't join or hand over by itself: main refuses without a click or key it saw (`Joining takes your own click.`, `Handing a task over takes your own click.`), and a join still needs the owner's Join in the native dialog.

### Invariants and boundaries

- The owner key never leaves main; the page never holds anything to sign (Approvals, AGENTS.md).
- Handoffs are typed: the page sends four strings, main builds the body from them; another Oby's words come back only as quoted posts.
- obid's API is unchanged by this app: it reads `revokedReason` add-only and sends only what obid already takes.

## Interfaces and dependencies

- IPC: `sphere-join` (a string: the link) and `sphere-handoff` (`{to, goal, repo, base}`), in [ipc.md](ipc.md).
- obid: `GET /v1/obid/sphere` (with `revokedReason`, B1's), `GET /v1/obid/sphere/members`, `POST /v1/obid/sphere/requests`, `PUT /v1/obid/sphere/membership`, `GET /v1/repos`.
- obid's `PUT /v1/obid/sphere/owner` `{owner}` (add-only, B1's): `2xx` with the owner kept, `409` once any membership exists, `422` for a body without a valid owner; `404` from an obid without it.

## Acceptance scenarios

- **SPHEREAPP-1** Given an admin and a members rule without `handoff`, when they click Let members ask, then the rule gains `handoff` alone, kept where it was with everything else in it; with nothing missing, nothing is written. `sphereapp_1_…` (`sphere.test.ts`).
- **SPHEREAPP-2** Given a membership whose card accepts `review_request`, `question` and `run_request` and whose delegation lacks `send:handoff`, when main keeps it, then one new card accepts all four and one new delegation carries every scope it had plus the app's; the next keep signs nothing. `sphereapp_2_…` (`sphere.test.ts`).
- **SPHEREAPP-3** Given first run's Obysphere step, when the owner pastes a text that isn't a link, then it says so and Join stays off; a link shows its name, host and the owner id it joins as; Join hands main the link and shows `You’re in <name>.`, a failure's words, or nothing on Cancel. `sphereapp_3_…` (`sphereApp.test.tsx`).
- **SPHEREAPP-4** Given Settings › Obysphere, when not in one, then it has the same field; in one, its name and owner id and no field. Main joins a pasted link only right after a click it saw, through its own dialog, whose Cancel joins nothing. `sphereapp_4_…` (`sphereApp.test.tsx`, `ipc.test.ts`).
- **SPHEREAPP-5** Given obid's membership `revoked`, then the Obysphere stays with no members and says its admin took the Oby out (`removed` or no code; an unknown code says only that it's out, never the code), with the link field and Leave, and without its rules or What your Oby tells. `sphereapp_5_…` (`sphere.test.ts`, `sphereApp.test.tsx`).
- **SPHEREAPP-6** Given a member's thread, when the owner fills the form and clicks Hand it over, then main, only right after that click, sends obid one `handoff` to the member's Oby that takes tasks, with the trimmed fields and `tests_pass`, open 7 days; a form that can't go, a member whose Oby doesn't take tasks and the rules' refusal each say why and send nothing more. `sphereapp_6_…` (`sphereApp.test.tsx`, `ipc.test.ts`).
- **SPHEREAPP-7** Given a handed task's exchange, then the thread's line under it says where it stands in words: waiting, said yes, the result came back, or each decline's reason. `sphereapp_7_…` (`sphereApp.test.tsx`).
- **SPHEREAPP-8** Given a joined Obysphere on obid, then the app never asks obid for its room, so the room never opens, and the message box offers no other Obys. `sphereapp_8_…` (`sphereApp.test.tsx`).

- **SPHEREAPP-9** Given a link pasted before a first join, when obid doesn't serve `PUT /v1/obid/sphere/owner`, then the owner id shows with no Change; when it does, Change and Save send `{owner}` lower-cased and the line shows the id obid kept; an id that isn't one says so and sends nothing; `409` says it can't change once joined. `sphereapp_9_…` (`sphereApp.test.tsx`).

### Test it when it's done

1. On a fresh home with no `gh` login, open first run's Obysphere step, paste a local Obysphere's invite link: the owner id shows; Change it; Join, then the dialog's Join; the side panel shows the Obysphere.
2. As its admin, Settings › the Obysphere › Let members ask: the rule lists handoffs.
3. From a member's thread, Hand them a task in a fixture repository; on their Mac, Accept; the result reads in your thread.
4. Have the admin remove one Oby: its owner sees the line in its settings, not a vanished tile.
