# Asking another Oby from your conversation

The business behavior contract for your Oby asking another person's Oby something and telling you in your own conversation, from [the template](../business-contract-template.md). Scenarios are numbered `ASK-n`; the tests that prove them are named after them (`ask_n_…` in `core/crates/obid/tests/ask_an_oby.rs`, `tests/sphere_questions.rs`, and unit tests in `core/crates/obid/src/brain/ask.rs`, `brain/supervisor.rs`, `sphere/sharing.rs` and `sphere/link/asking.rs`).

## Feature and status

- **Outcome:** "What's Dima working on?" just works, in your conversation: your Oby asks Dima's Oby, Dima's Oby answers from what it knows of its own jobs, and you read the answer. Neither of you has to bother Dima. The owner's words, 2026-11-02: "My Oby should talk to other Obys, to ask for progress, to ask for other information that their Oby will be able to answer without having to bother their humans."
- **Status:** **Implemented** in obid, and run for real with two scratch obids, the real `claude` and a local obi-sphere (every state in the table but expiry and the other declines, which the automated tests cover). The words are proposed: Sangam owns them. The design is the "You talk to your Oby. Your Oby talks to theirs." canvas (boards Ask, DimaAnswered, DimaSilent, DimaRefused and DimaDeclined).
- **Scope:** a question from a conversation to one member's Oby, every state of what comes back, and answering by itself. **Out of scope:** reviews and handoffs from a conversation; a teammate's chat (the two Obys talking); Sangam's screen for a question that waits for you (the apps don't show an `obiRequest` yet, so it reaches you as a conversation, below); changing a Sphere's policy from Oby.

## Behavior

### Actors and ownership

- **The owner** asks in their own words. Nothing else makes their Oby ask: no job, no other Oby, no turn nobody watches.
- **The conversation's brain** picks whom to ask and words the question (`oby_ask`), and says it asked. It never sees or retells what comes back unless a later turn reads it (`oby_asks`), quoted.
- **obid** checks and sends the question, follows what comes back, and writes what the owner reads: its own words around a quote.
- **The other Oby's obid** decides by its owner's rules alone: a sharing rule lets it answer by itself from its material (docs/runner-api.md, "obid only: questions from other Obis"), and otherwise the question waits for its owner.
- **The Obysphere** allows or refuses the request by its policy, which only its admin changes.
- **The apps** show the turn. A quote is Markdown (`> `), which the Electron app shows as a quote already; they add nothing and decide nothing.

### Triggers and prerequisites

- The Oby is a member of an Obysphere and the other Oby's card takes questions. The conversation's orientation says it's in one and tells the brain to ask rather than say it can't, never who is there. The brain gives `oby_ask` the name the owner used, and obid works out who that is.
- The Sphere's policy allows `question` between the two owners. A new Sphere's policy allows nothing (below).

### States and transitions

The asker's side. Each line is one turn of Oby's in the conversation, after the turn that asked. Each is said at most once, also across a restart.

| What happened | What the owner reads | Marks the conversation |
|---|---|---|
| The other Oby answered by itself (`answer`, `by: obi`) | `> <its words>` and "Asked Dima's Oby. Dima wasn't bothered." (the name the owner knew when they asked, whatever the card says by then) | yes |
| The person answered (`answer`, `by: owner`) | `> <their words>` and "Dima answered this one, not its Oby." | yes |
| Only the person can answer (`status`, `waiting_on_owner`) | "Dima's Oby hasn't answered. It's waiting on Dima." | no |
| 2 minutes with no word (a Mac that's off, a slow model) | "Dima's Oby hasn't answered yet." | no |
| The person said no (`decline`, `owner_declined`) | "Dima said no to this one." | no |
| Other declines | `busy`: "Dima's Oby is busy with other questions and didn't take this one. Ask again later."; `out_of_scope`: "Dima's Oby doesn't take questions like this one."; `policy`: "This Obysphere's rules stopped Dima's Oby answering. An admin can change them."; else "Dima's Oby declined without saying why." A decline's note, if it has one, is quoted first. | only with a note |
| A day with no answer | "Dima's Oby never answered: the question expired." | no |

The question wasn't sent, which the brain says in the turn itself:

| Why | What the owner reads |
|---|---|
| The Sphere's rules (`policy_denied`) | "I can't ask Dima's Oby. This Obysphere's rules don't allow questions yet. An admin can change that." |
| The Sphere refused otherwise | "I can't ask Dima's Oby: the Obysphere refused it (`<its code>`)." |
| The Sphere can't be reached | "I can't ask Dima's Oby right now: the Obysphere can't be reached." |
| A possible secret, or a scan that can't run | What the scan said: nothing was sent. |
| Any other reason obid won't send it (a card that dropped its seal key) | "I can't ask Dima's Oby: nothing was sent." |
| No member goes by that name, or not in an Obysphere | That none does and `oby_members` lists who is there, or that joining takes a private link from its admin. |

These name what the brain gave in `to`, never who that turned out to be, since they reach the brain. A question refused only once the owner clicked its Confirm answers the click with these words, naming the owner id, and says them as Oby's turn in the conversation too, after any turn under way, so it never ends on "waits for the owner's OK" with no card (marked `relayed`, like every turn about another Oby).

The answerer's side:

- **By itself.** An obid's first join to a Sphere makes one sharing rule (below), so a member's question is answered from the status of its owner's jobs, through the model run docs/runner-api.md describes. A membership from before this has no such rule: its questions go to its owner until they add one.
- **Only the person can answer.** When the run says the material doesn't answer it, or no rule covers it, obid sends `status` `waiting_on_owner`, keeps the question in the inbox as an `obiRequest`, and says it in the one conversation for every member's questions, "Questions from other Obys": "Dima's Oby (dmytro) asks something only you can answer:" (the owner id when the card goes by another name: a name can look like someone else's), the question quoted, "Tell me what to say back, or say no." One conversation, so it marks none of their other work, and however many members ask, it takes no room of theirs and pushes nothing out of the 200 obid keeps. The app never opens on it, and words that name no conversation never go to it (`fromObys`); it stays in their list. Every turn there starts untrusted, whatever it does, so their words reach `oby_answer` on a Confirm card, labelled `by: owner`. Decline says no. A question its sender calls off or that runs out says so there, quoted.

### Failures and recovery

- obid restarted while waiting: what comes back while it's down arrives when it follows the Sphere again, and is told then. What's already told isn't told again (`told`, kept with the exchange).
- A turn under way in the conversation: what came back waits for it to end, so it reads in the order it happened.
- The conversation was deleted: nothing to tell it in; the exchange stays.
- The answerer has no Claude Code login that runs: its model fails, and the question goes to its owner, as any question the run can't answer. On a Mac whose login is only in the Keychain, the run gets a copy of what calls the API (ASK-15).

### Invariants and boundaries

- **What comes back is data.** It only ever becomes text inside a quote: every line prefixed, so it can't step out and read as obid's words. It starts, changes and approves nothing. It never reaches a prompt (below). A later turn reads it only quoted, from `oby_asks`, which makes that turn untrusted whatever came back: each entry's owner id is what a member chose. A conversation's mark says "what another Oby said", never who, since a job it starts carries the mark into a reviewer's input.
- **It always marks the conversation** as having read something untrusted, whoever sent it: another Oby's text is always data (AGENTS.md). The organization exemption covers issues, pull requests and comments, never Obys.
- **The owner's own word is kept through marks that carry no other Oby's words:** an unsigned turn, or a tool that read nothing, leaves their next signed words theirs, in a new session too. A file attached, or a session that read another Oby's words itself (`oby_asks`), taints the session, and that stays with the conversation (`read_untrusted`), also if the session's own file is lost.
- **Your Oby only sends what you asked.** On any turn but the owner's own word (signed by their app, having read nothing untrusted, and watched), the question waits on a Confirm card showing who it goes to and every word. Every question passes the secret scan first, and a finding sends nothing. Questions are sealed for cards with a seal key.
- **The answerer tells status only.** Its model gets only a question file and a material file: each of its owner's jobs' title ("An Oby task" when it has none, never its prompt), repository name, job id, stage, state and pull request. Private repositories' names are told too: that's what the rule shares. Never logs, diffs, prompts, files, secrets or logins. Its answer passes the secret scan before it goes. Nothing in a question can make it run, change or publish anything, and approvals never come from another Oby.
- **Ids and names are what a member says of itself.** Whoever redeems an open invitation picks their Obi id, owner id and card. So `to` means an exact owner id, else a name one member goes by, never an Obi id. Only an exact owner id that no other card claims goes on the owner's word. A name waits for a Confirm that shows it with the owner id ("Ask Dima's Oby (dmytro)"), and so does an owner id another card also claims, naming both ("Ask sam's Oby, not copycat, whose card goes by that name too"); that is a Confirm and not a refusal, or anyone could stop questions to a colleague by copying their name. A name comes from a card only when it reads `<name>'s Oby`, is at most 3 words of one writing system with no `.`, and shows only when it couldn't be mistaken for another member's name or owner id, or the owner's own; else the owner id shows.
- **Nothing a member chose reaches a prompt.** The orientation is fixed text, on watched turns only. Oby's turns about another Oby are marked `relayed`: when a session starts over and hears the earlier turns, or a briefing does, each is only that there was one. The friendly name is for the owner's eyes: the tool's answers name only what the brain itself gave. `oby_members` returns who is there quoted, and makes the turn untrusted; a Confirm's title goes to the owner, never back to the model.
- **The API stays add-only.** New tools, one new Confirm action (`ask_oby`, whose value the phones treat as an open string), and a reply type the protocol already had. No new value in anything the phones decode as a closed enum: a posted answer is an ordinary Oby turn.

## Interfaces and dependencies

- **Brain tools:** `oby_members`, `oby_ask`, `oby_asks` (never for a turn nobody watches).
- **obid:** `home/sharing.json` now starts with one rule; `Exchange` keeps `asked_in` and `told`, neither in the API's answer. docs/runner-api.md, "obid only: questions from other Obis" and "obid only: sharing rules".
- **The default, decided here.** An obid's first join, with no `home/sharing.json` yet, keeps one rule: `{"for": "*", "repos": "*", "tells": ["jobStatus"]}`. Joining is the owner's own act, so nothing starts sharing on an upgrade: an obid already in a Sphere with no file gets `{"rules": []}` at its next start. The limits hold (30 an hour per sender, 60 in all). The rule answers who asks, and only that: the room (docs/contracts/rooms.md) doesn't go by it, since a post goes to every member unasked, which joining doesn't agree to. The owner removes the rule like any, after which it stays removed; a file that doesn't read, or can't be read, shares nothing and is never replaced.
- **The Obysphere's policy (obi-ai-bot/obi-sphere, Dmytro's), not changed here.** A new Sphere's policy is `[]` (`spec/sphere-api.md` section 3, `server/src/routes/spheres.ts`), so every question is `policy_denied`, and only an owner token with `admin` can write it (`PUT /v0.1/spheres/{sphere}/policy`): obid holds neither. So until the Sphere starts with a rule, or its admin writes one, the owner reads the honest refusal above. What's missing, for Dmytro: a starter rule at creation, `{"id": "members-ask", "from_owner": "*", "to_owner": "*", "actions": ["question"], "repos": ["*"]}`.
- **No screen for it:** obi-sphere has no page for the policy in this version: an admin writes it with that call.

## Acceptance scenarios

- **ASK-1** In an Obysphere, a conversation's orientation tells it to ask rather than say it can't, and names no one; `oby_members` lists who is there afresh, quoted, marking the conversation.
- **ASK-2** On the owner's word, `oby_ask` sends exactly the question as a typed `question` and says it asked, marking nothing; someone who isn't a member sends nothing, and points at `oby_members`.
- **ASK-3** A question with a possible secret sends nothing, says so, doesn't repeat it, and makes no Confirm card.
- **ASK-4** Words no app signed make a Confirm card with who and every word; nothing goes before the owner's click, and the click sends it.
- **ASK-5** An answer by the other Oby posts once, after the turn's own words: its words quoted and "Asked Dima's Oby. Dima wasn't bothered." It goes by the name the owner knew when they asked, on their word or their Confirm, though the card changed since.
- **ASK-6** An answer by the person says "Dima answered this one, not its Oby."
- **ASK-7** `waiting_on_owner` says it's waiting on the person, once, marking nothing; 2 minutes with no word says it hasn't answered yet, once; an answer or expiry still comes after.
- **ASK-8** A Sphere whose rules refuse says an admin can change them, keeps no exchange, and shows nothing of what the Sphere wrote but its code. A refusal hands the brain nothing a member chose, whatever the reason. Refused only after the click, it says so in the conversation, once.
- **ASK-9** Each decline says who said no, and marks nothing unless it carries a note.
- **ASK-10** Whatever another Oby writes stays in its quote, every line; it starts no job and adds no inbox item; the conversation is marked as having read something untrusted.
- **ASK-11** After its first join an obid answers a member's question about its owner's work by itself, with its owner's inbox empty; the rule is status only, keeps its id, and stays removed once removed; a file that doesn't read shares nothing and isn't replaced; a membership from before gets no rule; a job with no title tells nothing of its prompt; the rule puts nothing in the room.
- **ASK-12** A question only the person can answer sends `waiting_on_owner`, waits in the inbox, and is said quoted, naming the owner id when the card goes by another name, in the one conversation for every member's questions, which it marks; however many members ask, that pushes out no conversation of the owner's; the person's words go back labelled as theirs; called off or run out, it says so there. `oby_inbox` shows its words, so the brain can tell two questions apart, and it wakes no event turn and makes no second item.
- **ASK-13** A later turn reads what was asked and answered with `oby_asks`, quoted; that marks the conversation with words that name no one, also with no answer yet, and what the same session, or a new one, asks next waits for the owner's Confirm.
- **ASK-14** A turn nobody watches asks no one, lists no one, and isn't told who is there.
- **ASK-15** On a Mac whose Claude Code login is only in the Keychain, the answering run still runs, on a copy of what calls the API (real-Mac step: no automated test reaches the Keychain).
- **ASK-16** A new session hears nothing a member chose, words or name, and every turn in the conversation of their questions waits for the owner's Confirm; an unsigned turn leaves the owner's next signed words theirs, in a new session too.
- **ASK-17** Only an exact owner id no other card claims goes without the owner seeing who: an Obi id reaches no one; a name, or an owner id another card claims, makes a Confirm showing who and tells the model nothing of it; a card gives a name, never a sentence or a look-alike.
- **ASK-18** A question from another Oby never decides where the app opens, on the Mac or the iPhone, or where words that name no conversation go.

### Test it when it's done

Two Obys on one Obysphere whose admin has written the rule above, on real Macs or as two scratch obids through a local obi-sphere:

1. With an empty policy, ask "What's Dima working on?": it says the Obysphere's rules don't allow questions yet, and that an admin can change that.
2. With the rule written and a job running on Dima's Oby, ask again: "I asked Dima's Oby", then the job quoted and "Asked Dima's Oby. Dima wasn't bothered." Dima's inbox stays empty.
3. Ask something only Dima knows: "It's waiting on Dima" here, "Questions from other Obys" there. Dima tells their Oby what to say, Confirms, and it comes back "Dima answered this one, not its Oby."
4. Ask again and have Dima Decline: "Dima said no to this one."
5. With Dima's obid stopped, ask: after 2 minutes, "Dima's Oby hasn't answered yet."
