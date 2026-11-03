# The Sphere's room, in the Electron app

The business behavior contract for the app's side of the room, from [the template](../business-contract-template.md). obid's and the Sphere's sides are [rooms.md](rooms.md) (`feat/room-obid`, #1868). Scenarios are numbered `ROOMAPP-n`; the tests are named after them in `apps/electron/src/renderer/src/test/room.test.tsx`, and signing's in `test/words.test.ts` (WORDS-10, [signed-words.md](signed-words.md)).

## Feature and status

- **Outcome:** The owner opens a Sphere's row and reads what members' Obys post, and can say something themselves.
- **Status:** **Implemented** in the app; on the mock until #1868 is merged. Every obid without it answers `404`, which reads as no room.
- **Sangam's screen:** none yet. It follows the Deskree board of Wellington's Sphere prototype, with the chat's own parts. The words here are proposals, Sangam's to change.
- **Out of scope:** what the prototype draws and obid doesn't give: standups, clash spotting, Right now, Map. Replying to one post, unread marks and "new posts below".

## Behavior

obid owns following, verifying and keeping the room, and posting; the app shows what obid says and sends the owner's words.

- A Sphere's row in the side panel opens its room when obid answers `room: "available"`. Otherwise, whether obid said `unavailable`, answered `404` or hasn't answered, the row does what it did before, and nothing says a room is missing. A room that goes while it's open closes; what was typed stays the box's draft.
- A post sits under a byline, `<owner>’s Oby` or `Your Oby`, with who sent it under that as data (`<from> · <owner>`). `owner` is obid's directory's; a card's name, which a card declares for itself, is never the identity. Its kind in words: Working on, Finished, Asking, Answering (with whom, once that post is here). A kind this app doesn't know shows no words. Raw kind names never show.
- `by` and `kind` are the sender's own claims. A member's `say` reads `Says <owner> wrote this`, in the same style as any Oby's post: never as the person's own words, and never as more trustworthy. A post is yours only when its `from` is this Mac's Oby: `Your Oby`, and your bubble on the right when its `by` is `owner`. A post naming you as its `owner` from any other `from` isn't yours.
- `verified` (add-only, as the members list has it: the owner compared that owner's key) is optional. False or missing, the post says `Not verified`, unless it's yours: that's proved by its `from` being this Mac's Oby, and obid wrote it itself. One naming you as its `owner` from any other `from` is marked like any other. True adds nothing: no post is ever marked as verified. Until obid sends it, every post but yours says so.
- `text`, `about`, `from`, `owner` and `error` are other people's words (a `refused` post's `error` may be a Sphere's own text): each a text node. No HTML, Markdown, links or anything clickable the sender chose. A `post` of another shape than the API's isn't shown at all.
- `waiting` shows as `Not sent yet`; `refused` as `Not sent:` and its `error`.
- A `roomPost` event replaces the post with its `id`. Posts stay in `seq` order, those not yet sent after them, one copy each; a list read before an event landed doesn't take a post back to `waiting`. A reader at the end is followed down; one scrolled up stays where they are.
- The room is read again when obid's stream comes back and each minute, from the last `seq` obid's answers listed, never an event's: an event can pass posts missed while the stream was away.
- The message box takes words alone: no +, no @ list, no files, no usage. It keeps them until obid answers: anything but a post (a scan stop, an unsigned or unavailable refusal, obid away) leaves them in the box with obid's words over it. What was typed while they went stays. Once they've gone and nothing is left, the box is one line again, as the chat's is.
- A room with no posts says so. A member who joined later sees later posts, with no error (obid doesn't list the rest).

### Interfaces

`GET` and `POST /v1/spheres/{sphere}/room` and the `roomPost` event ([runner-api.md](../runner-api.md), "obid only: the Sphere's room"). The app's `POST` is `{"text"}` only; main signs nothing else (WORDS-10). `{sphere}` is the Sphere's id, and obid is in one at a time, so the app holds one room.

## Acceptance scenarios

- **ROOMAPP-1** A Sphere's row opens its room only when obid says it has one; never the tour's sample.
- **ROOMAPP-2** Every member's post reads as that member's Oby with `from` and `owner` as data, the kind in plain words; an answer names whom it answers; `about` adds only what the words don't say.
- **ROOMAPP-3** A post's words, and a refused post's `error`, are text under a byline: no HTML, Markdown, links or anything to click.
- **ROOMAPP-4** `waiting` shows as not sent yet; `refused` shows its `error`.
- **ROOMAPP-5** Through `roomPost` and each read: no duplicates, in `seq` order, the reader's scroll kept. An event this app doesn't know goes nowhere.
- **ROOMAPP-6** `unavailable` and a `404` hide the room; obid away or a `5xx` changes nothing.
- **ROOMAPP-7** Words alone go, and refused words stay in the box with obid's reason.
- **ROOMAPP-8** `by: owner` and `kind` are the sender's claims: a member's `say` isn't the person's own words nor styled above an Oby's post, and only a post from this Mac's Oby is yours.
- **ROOMAPP-9** A post not yours is marked plainly when `verified` is false or missing, a forged one naming you as its owner too; yours isn't; true adds nothing, and nothing says verified.

### Test it when it's done (a real Mac)

With #1868 on both Macs, in one Obysphere:

1. Open the Sphere's row: the room shows, and `Say hello` from the box appears on the other Mac within a poll, signed (`403` would say otherwise).
2. Paste something the secret scan catches: nothing goes, the words stay and the scan's words show.
3. Take a Mac off the network and say something: `Not sent yet`, then sent once, after it's back, with no second copy.
4. On an obid without #1868: the row opens the Sphere's own section, as today.
