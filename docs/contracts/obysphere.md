# Joining an Obysphere by a private link

The business behavior contract for the Electron app joining an Obysphere someone else made (Dmytro's, for his demo of two Obys on one Obysphere), from [the template](../business-contract-template.md). Issue: [#1584](https://github.com/obi-ai-bot/oby/issues/1584). Scenarios are numbered `SPHERE-n`; the tests that prove them are named after them (`sphere_1_…` in `apps/electron/src/renderer/src/test/sphere.test.ts`; SPHERE-5, and SPHERE-6 against real obids, are `apps/electron/scripts/test-sphere.mjs`, `npm run test:sphere`).

## Feature and status

- **Outcome:** Two people's Obys are on one Obysphere: each owner opens the private link the Obysphere's admin sent, clicks Join, and their Oby is a member that sees the other and can be asked a question by it.
- **Status:** **Implemented** in the Electron app (`apps/electron`): the owner's side in `src/shared/sphere.ts`, carried out by `src/main/sphere.ts`; the joined Obysphere in Sangam's side panel through `src/renderer/src/spheres.ts`. obid's side was already built (`core/crates/obid/src/sphere/`, docs/runner-api.md, "obid only: Sphere membership") and is unchanged. A port of the frozen Swift Mac app's `Sphere.swift` (its owner key, invite link, join and leave).
- **Scope:** joining by an invite link, seeing the Obysphere and its members, and leaving. **Out of scope:** creating an Obysphere, inviting people and its settings (the Obysphere server's, Dmytro's: Sangam's Create and Invite screens stay on the mock); a link limited to a Google Workspace (no Google sign-in in this app yet); Settings › Obysphere's key comparison (`pins`); showing a question another Oby asks in Home (it waits in obid's inbox as `obiRequest`, which the Electron app doesn't show yet); Mac share and Runner share (hidden for launch).

## Behavior

### Actors and ownership

- **The Obysphere's admin** (Dmytro) creates it on the Obysphere server, writes its policy and makes the invitations: an open link for a team, or a named one for one person's Oby.
- **The owner** opens the link and clicks Join, or Cancel; later, Leave in the Obysphere's settings.
- **The Electron app's main process** holds the owner key (Ed25519, made once, kept encrypted with safeStorage in the app's folder as `owner-key`, like the device key), signs the Oby's card and its delegation, and calls obid. The page can't ask it to sign anything: it can only ask to leave.
- **obid** holds the Oby's own key and seal key, redeems the invitation, gets its tokens, follows the Obysphere, seals and checks every message (docs/runner-api.md). It never holds the owner key.

### Triggers and prerequisites

- The invite link opens Oby: the Obysphere's join page's Open in Oby, `oby://join/<host>#<fragment>`, which stands for `https://<host>/join#<fragment>` (obi-sphere's `sphere-api.md` section 2.1). A packaged app makes itself the Mac's handler for `oby://` at launch (`electron-builder.yml`'s `protocols`).
- obid answers. Its owner (`GET /v1/obid/sphere`'s `owner`: the configuration's, or the GitHub login before a first join) is who the Oby acts for.

### States and transitions

| What the owner sees | What happens |
|---|---|
| A link opens Oby: "Join *name*?", who invited, and "Joining lets *host* act for your Oby for 30 days. Join only if you trust *host*." with Join and Cancel (the default) | Nothing is sent yet. Cancel, Return or Esc ends it. |
| Join | The checks first (an Oby already in an Obysphere, a named link for another owner, a Workspace link); only then is the owner key made, the first time. obid makes its key (`POST /v1/obid/sphere/key`) and card (`POST …/card`); main signs the card and a 30-day delegation with the owner key; obid joins (`POST …/join`). An open link's Oby is `obi-<a new UUID>`; a named link's is the one it names. A revoked membership is left first. |
| Joined | Main signs, with no click, what keeps it working (as the Swift app did), right after joining, at launch and every 5 minutes: the card obid asks for (`sealing` `card_needed`: a joining Oby's first card has no seal key, and once the Obysphere and every teammate can read one, the next carries obid's), and a new delegation with the same scope once 5 days or fewer are left of its 30 (`PUT /v1/obid/sphere/membership`). |
| Joined | The Obysphere shows in the side panel's icon strip, by the name its link gave, its members the other Obys' owners (`GET /v1/obid/sphere/members`), read again every minute. A member's row opens nothing: conversations with other Obys are still the mock's, which would say a message went when nothing did. Its settings show you by obid's owner id and the others by theirs, with no made-up name or address. |
| Joined with another owner key (the Swift app's, or this app's before a Keychain reset) | Nothing is signed. Once a launch the owner is told: "Oby can't keep your Obysphere working." with "Your Oby joined *sphere* with another Oby app on this Mac, whose key this one doesn't have. Nothing is broken while that app keeps running. Only if it's gone for good: leave *sphere* in its settings, then open its link again." A build for testing (`npm run drive`, `npm run dev`) only logs it: it isn't the owner's app, and has its own key on their real obid (#1780). |
| Leave, in its settings, confirmed | The Obysphere takes this Oby out (`DELETE /v0.1/spheres/{sphere}/members/{obi}` with the owner's own 5-minute token), then obid forgets it (`DELETE /v1/obid/sphere`). It's gone from the side panel. |

### Failures and recovery

- A link that isn't one by the spec's rules: "Oby can't open this link." Nothing is sent.
- A named link for another owner, or one for a Google Workspace: a dialog says why and what to ask for; only `GET /v1/obid/sphere` was called.
- The Obysphere refuses: its reason in plain words where the Swift app had them (a link used up, expired or cancelled; the owner already bound with another Mac's key), else as obid quotes it. Nothing is joined; the owner can open a new link.
- obid not answering: its reason. Nothing is joined.
- An Oby already in an Obysphere: "This Oby is already in an Obysphere. Leave it first (in its settings), then open this link again." (also for obid's `409` if two joins race).
- Leaving when the Obysphere can't be told: obid leaves all the same, and the owner sees "You left *sphere*, but it couldn't be told." with the reason and "It still lists your Oby until its admin removes it." obid not able to leave: "Oby couldn't leave the Obysphere." with its reason.

### Invariants and boundaries

- **The owner key never leaves main**, and nothing in the page can make main sign. It's made only once a join's checks pass: never for a refused link, a Leave with nothing joined, or an owner who never joins. A link joins only on the owner's click in the app's own dialog, which names the host the app will call exactly as it will call it.
- **Frames are sealed end to end, other Obys send only typed actions, their text is data** (obid's, unchanged). Approvals come only from the owner's own apps: an Oby on the Obysphere can't approve anything.
- **An Oby is a UUID with a name**: an open link's Oby id is a new UUID's, never the person's login. The owner id the Obysphere binds is still obid's `owner` (below, follow-ups).
- **Privacy first:** nothing goes to any server but the link's, and only after Join. The invite's secret is after `#` and reaches only the redeem's body (through obid). No redirect is followed with the owner's assertion.
- `oby://join` always stands for https (the spec); `http` is taken only for a Sphere on this Mac (`127.0.0.1`, `localhost`, `[::1]`) written as an `http://…/join#…` link.
- obid's API is unchanged: nothing new for the Swift iPhone and Watch apps to decode.

## Interfaces and dependencies

- **obid:** `GET /v1/obid/sphere`, `POST /v1/obid/sphere/key`, `POST /v1/obid/sphere/card`, `POST /v1/obid/sphere/join`, `GET /v1/obid/sphere/members`, `DELETE /v1/obid/sphere` (docs/runner-api.md).
- **The Obysphere server** (obi-ai-bot/obi-sphere, Dmytro's): the invite link (`sphere-api.md` 2.1, read as its conformance vectors say, `test/fixtures/invite-link-vectors.json`, copied from obi-sphere `dev` at `bf51687c`), the owner token exchange and a member's revoke.
- **Renderer ↔ main (preload `window.app`):** `onSphere(fn)` gets `{sphere, name}` once main joined; `sphereLeave()` answers why the Obysphere wasn't told, or null.

## Acceptance scenarios

- **SPHERE-1** A private link joins this Oby: obid's key and card, the card signed by the owner key (one detached JWS over its RFC 8785 JSON), a 30-day delegation to that Oby for that Obysphere with `receive` and `reply` among its scopes; an open link's Oby is `obi-<UUID>`, a named link's its own.
- **SPHERE-2** A link this Oby can't use joins nothing, makes no owner key, and says why: one the spec's reader refuses (every known answer in obi-sphere's vectors), a named one for another owner, one for a Google Workspace, an Oby already in an Obysphere, one the Obysphere refuses, obid not answering.
- **SPHERE-3** The joined Obysphere shows in the side panel by its link's name, at its own address, with the other Obys' owners as members; none shows when obid isn't joined.
- **SPHERE-4** Leaving takes this Oby out of the Obysphere with the owner's own token, then obid forgets it; when the Obysphere can't be told, obid leaves all the same and the owner is told why; with nothing joined, no owner key is made and no Sphere is called.
- **SPHERE-5** Two Obys on one Obysphere: a local Obysphere from obi-sphere, its admin's open link and named link, two real obids joined with the app's own join; each lists the other, a question one asks crosses to the other as a typed request (sealed, SPHERE-6), and the one that leaves is revoked there.
- **SPHERE-6** Words between members are sealed and the membership doesn't lapse: when obid says `card_needed`, main signs the card it makes (same name and accepts, now with obid's seal key); with 5 days or fewer left, a new 30-day delegation with the same scope; nothing else is signed, and an owner who never joined gets no owner key. A membership whose card another owner key signed signs nothing and tells the owner which app holds it, and to leave and join again only if that app is gone. In SPHERE-5, both cards end up carrying their seal keys and each Oby seals for the other before the question goes.
- **SPHERE-7** A live Obysphere's members open no made-up conversation: their rows aren't buttons, and its settings list only owner ids obid knows. The mock's Obyspheres and the tour's sample keep Sangam's conversations.

### Test it when it's done (Dmytro's demo, two Macs)

1. Dmytro creates the Obysphere on his server, writes a policy that lets members ask questions, and makes an open link (or one named link per person).
2. On each Mac, the packaged Electron app is running and obid answers. Open the link in a browser: the join page's Open in Oby opens Oby with "Join *name*?". Click Join.
3. Both Macs show the Obysphere in the side panel's icon strip; each lists the other's owner under Members within a minute. Within 5 minutes both say `"sealing": "on"` (`GET /v1/obid/sphere`), and `GET /v1/obid/sphere/members` says `"sealed": true` for the other.
4. From one Mac, `POST /v1/obid/sphere/requests` with a question to the other's Oby (`oby-mcp`'s `sphere_ask`, or curl with obid's token): the other's `GET /v1/obid/sphere/exchanges` shows it incoming.
5. Leave from one Mac (its settings, Leave): it's gone there, and Dmytro's directory lists it revoked.

Follow-ups this change leaves (not built): a join screen and the words for it from Sangam (the dialog's words are the Swift app's); conversations with a live member's Oby through obid's requests, and Home showing a question another Oby asks (`obiRequest`); Google Workspace links; verified owner ids, or named links per person, since an open link's owner id is only what that Oby's obid says (the GitHub login); `oby://` also being the Swift app's scheme for `oby://claim` links; `test:sphere` in `scripts/oby-check.sh`.
