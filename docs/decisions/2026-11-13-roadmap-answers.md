# Roadmap decisions, 2026-11-13

The owner's answers to every decision the open roadmap tasks (`factory/queue/tasks/`) need, given on the decisions page on 2026-11-13. Intake and the spec writer treat each answer here as the owner's word for the tasks it names, in place of asking again. Where a task's own text says "ask the owner" or "the owner decides" about one of these, this file answers it.

## Standing decisions

- **Designs ("needs Sangam's design first")**: the owner decides designs now (2026-11-13: "forget about Sangam"). A task that waited for Sangam's screen takes the answer below as its design, using the existing screens' style; a task with no answer here proposes the smallest screen change in its spec and goes ahead.
- **Phone apps: frozen, not deleted** (2026-11-13: "Freeze, don't delete"). No work on `apps/ios`, `apps/shared` Swift or `apps/android`. These phone-only tasks are parked and not built: R06-14, R07-07, R07-08, R07-09, R07-11, R07-13, R07-14, R07-15, R07-16, R07-17. These build only their non-phone part: R02-08 (the `oby-mcp` request key), R07-01 and R08-02 (Electron only), R13-06 (`apps/electron/AGENTS.md` only). R03-07 (drop the Apple Watch target) and R03-08 (freeze Android) go ahead as answered below. obid's API stays add-only, since phones already installed still read it.
- **"Do it the way OpenBot does"**: where the owner asked how OpenBot solves a question, the answer below is OpenBot's way as the comparison report (`Oby and OpenBot Compared.md`) describes it, adapted to Oby's rules in AGENTS.md.

## Answers by task

### R01-03 (q001)
- **Question:** Will you add the two GitHub rules (one on dev, one on main) yourself once the first CI run on dev has passed?
- **Decision:** Yes, I add both rulesets after the first green CI run on dev

### R01-03 (q002)
- **Question:** Which queue settings do you want for merges into dev: group up to 5 pull requests, wait up to 15 minutes to fill a group, one test run at a time, 60-minute timeout?
- **Decision:** Use the proposed settings (min group 2, 15-minute wait, max 5, one run at a time, 60-minute timeout)

### R01-03 (q003)
- **Question:** Should you keep a bypass on both rules so you can still merge or push directly in an emergency?
- **Decision:** Keep owner bypass on both rules  *(the owner asked to do it the way OpenBot does: The report does not say whether OpenBot keeps an owner bypass.)*

### R03-01 (q004)
- **Question:** The 'memory guard' sentence tells people to 'turn on telemetry in Settings', but telemetry will no longer exist. What should that sentence say instead?
- **Decision:** Remove the clause about telemetry and keep the rest

### R02-05, R02-07, R02-08 (q067)
- **Question:** When an app re-sends a request it already sent (same request key) but the retry isn't signed the same way as the first, should obid refuse it, or treat it as the same request?
- **Decision:** Refuse it unless it's re-signed (safest: a retry can never act with less proof than the first request)

### R01-09 (q005)
- **Question:** obid is close to its size cap (257 lines left). Do you want to raise the cap for this feature, or have the builder cut other code to make room?
- **Decision:** Builder cuts lines elsewhere in the same change

### R02-12, R04-06, R02-09 (q006)
- **Question:** When one task fails again and again, should the app show one card in 'Needs you' for the whole chain instead of one card per failure? And should files obid could not read be shown somewhere?
- **Decision:** One card per chain, and a small notice in Settings listing unreadable files  *(the owner asked to do it the way OpenBot does: The report does not describe OpenBot showing one card for a chain of failures.)*

### R03-05 (q007)
- **Question:** Do you want oby-mcp (the old Swift MCP server) deleted now, or kept and repaired?
- **Decision:** Delete it now

### R03-06 (q008)
- **Question:** Do you want the old browser app (apps/web) deleted, and the six open issues that plan web features closed?
- **Decision:** Delete it and close or retarget the six issues

### R03-07 (q009)
- **Question:** Do you want the Apple Watch app removed from the iPhone build now?
- **Decision:** Remove the Watch target now

### R03-07 (q010)
- **Question:** Is the iPhone Settings screen without its Watch rows acceptable as it is, with nothing added in their place?
- **Decision:** Yes, just remove the rows

### R13-01 (q011)
- **Question:** Do routines (scheduled and triggered jobs) stay in Oby, or should they be cut?
- **Decision:** Keep routines, and mark them kept in docs/trim.md  *(the owner asked to do it the way OpenBot does: OpenBot keeps routines (a name, an instruction and a trigger) with a run record each, so a crash neither loses nor doubles a run.)*

### R13-02 (q012)
- **Question:** Is docs/decisions/ the right place for a log of dated owner decisions, one file each?
- **Decision:** Yes, docs/decisions/ with files named YYYY-MM-DD-<slug>.md

### R05-03 (q013)
- **Question:** What should Oby say in a conversation after it restarted and one of your messages was never answered? One sentence for a normal stop (an update or quit) and one for a crash.
- **Decision:** Clean stop: "Oby was updated while working on this, so your last message wasn't answered. Say 'continue' and I'll pick it up." Crash: "Oby stopped unexpectedly before answering your last message. Say 'continue' to pick it up."

### R05-04 (q014)
- **Question:** When a conversation message hits an account limit or a sign-in problem, should it wait and resume by itself, or keep today's behaviour (switch account or fail)?
- **Decision:** Make conversations wait and resume on the first account with room, like jobs  *(the owner asked to do it the way OpenBot does: When a refused turn reports a usage limit, OpenBot makes one hold per account and model and holds the queues on it, with no per-message failures.)*

### R07-01 (q015)
- **Question:** What should the app show for a task or card whose state the app does not know yet?
- **Decision:** Gray badge, no text change (the proposed default)

### R07-01 (q016)
- **Question:** Will you release the iPhone build with this change (TestFlight or App Store) before the tasks that add new job states are merged?
- **Decision:** Not now; hold the new-state tasks until I do

### R07-01,R07-08,R07-09,R07-15,R07-16 (q017)
- **Question:** Is Android frozen (no more work) or still in scope?
- **Decision:** Freeze Android: skip every Android step and close R07-15 and R07-16

### R08-11 (q018)
- **Question:** When the owner signs out an account while a task is still running on it, what should the app show and offer?
- **Decision:** Show 'Waiting for the running task to finish' on the account row, plus a 'Stop and sign out' button

### R13-03 (q019)
- **Question:** Which rules are non-negotiable, meaning even the owner's word does not change them?
- **Decision:** Use the proposed list: secret scan before anything leaves; approvals only from the owner's own apps; the API only grows (add-only); open-source Apache-2.0 licence; agents never get logins or vault secrets  *(the owner asked to do it the way OpenBot does: OpenBot's root AGENTS.md has five 'Non-negotiable' rules: migrations, released Team API adapters, the trust boundary, redaction and the licence.)*

### R05-05,R05-07,R05-09 (q020)
- **Question:** Should the app let you send words INTO a running turn or task (steering) with its own control, or keep only the current one-at-a-time queue?
- **Decision:** Add a control (for example a 'Send now' choice on a waiting message and on the task card's 'Tell it what to do') that steers when possible and falls back to waiting  *(the owner asked to do it the way OpenBot does: A busy OpenBot agent's message either steers the running turn or waits in the queue, with 'steer' the default and 'queue' optional.)*

### R07-09 (q021)
- **Question:** What should the iPhone say for each reason a phone is refused: removed, expired, needs an update of the phone, needs an update of the Mac?
- **Decision:** Parked: the owner froze all phone work on 2026-11-13 (see "Phone apps" above).

### R08-02 (q022)
- **Question:** When Oby meets an agent it does not know yet (for example Gemini), what small picture should it show next to the agent's name?
- **Decision:** A plain grey circle with the agent's first letter

### R13-10 (q023)
- **Question:** Do you want agents to start feature work only on issues you have marked `approved`, and will you create the `approved` and lane labels on GitHub yourself?
- **Decision:** Yes, adopt the rule; the owner creates the labels `approved`, `lane:agent`, `lane:dispatcher`, `lane:between-owners`  *(the owner asked to do it the way OpenBot does: OpenBot admits new work only through an approved issue with a triage score, and plans are pinned to a commit with a drift check.)*

### R13-12 (q024)
- **Question:** Will you review and approve each epic's verdict list before any of the 425 gap-map issues is closed, and how should the 63 security-labelled and 15 in-review ones be treated?
- **Decision:** Approve all epics in one go after reading the whole list

### R06-10,R06-11 (q025)
- **Question:** While your words or a click (Allow, Stop) are sent but obid has not yet confirmed them, should the app show a visible 'sending' or 'not confirmed' state?
- **Decision:** Keep today's look; show nothing extra unless the send fails

### R07-11 (q026)
- **Question:** What should the phone's connection notice say while it is cooling down after repeated failures, and when it has stopped trying (needs an update, was forgotten, or another phone replaced it)?
- **Decision:** Cooldown: 'Can't reach your Mac. Trying again in a moment.' Suspended: 'Stopped trying. Tap to reconnect.' (plus the specific reason where known)

### R03-08 (q027)
- **Question:** Do you want Android frozen: no more hand-porting of Electron or iPhone changes until generated shared models exist?
- **Decision:** Freeze Android until generated models exist

### R05-09 (q028)
- **Question:** How should a message that is waiting, and one that was not answered because Oby restarted, look in the conversation, and what should the take-back control say?
- **Decision:** Waiting: the message shows dimmed with the small label 'Waiting' and a 'Take back' link; Not answered: shows with the label 'Not answered' and the 'Continue' hint

### R07-13 (q029)
- **Question:** When words typed offline on the phone cannot be confirmed as sent, how should they look and what should the buttons say?
- **Decision:** Message stays in the thread marked 'Not confirmed' with two buttons, 'Edit' and 'Don't send'

### R07-14 (q030)
- **Question:** How should the phone mark the saved, read-only copy of your conversations it shows while connecting?
- **Decision:** A thin banner at the top: 'Last saved view, connecting...' and the message box disabled  *(the owner asked to do it the way OpenBot does: OpenBot keeps a local cache of the workspace that it shows at once and replaces when the live read arrives.)*

### R07-17 (q031)
- **Question:** Where does the attach action go in the phone's message box, and what should it offer?
- **Decision:** A paperclip button beside the message box offering Photo and File

### R07-17 (q032)
- **Question:** How large may a file attached from the phone be?
- **Decision:** 5 MB per file

### R08-16 (q033)
- **Question:** When a saved model is retired, should Oby move the account to the newest model of the same family instead of the tool's default?
- **Decision:** Yes, move to the newest model of the same family and say so

### R08-16 (q034)
- **Question:** Should the conversation's own saved model (in the settings file) also move to the newest of its family when retired?
- **Decision:** Yes, move it the same way and say so  *(the owner asked to do it the way OpenBot does: On each fresh model list, OpenBot moves an agent whose model is gone to the newest model of the same family, and never does so on a failed list.)*

### R08-16 (q035)
- **Question:** What words should Settings show when a model was moved to its successor?
- **Decision:** 'Your model <old> is gone. Oby switched to <new>.'

### R09-01 (q036)
- **Question:** Who generates the update-signing key and where is the private half kept?
- **Decision:** The owner generates the key pair on their own Mac and keeps the private file outside the repository

### R09-01 (q037)
- **Question:** What is the lowest app version every Mac must be at (the starting 'minimum version' in the signed update feed)?
- **Decision:** The current released version

### R09-03 (q038)
- **Question:** What should About's update row say while a restart waits for running work, and what should the 'Restart now' button be called?
- **Decision:** 'Waiting for a task to finish before restarting' with a button 'Restart now'

### R04-15 (q039)
- **Question:** After reading the spike's evidence, should Oby move its jobs, inbox and conversations into one SQLite database: yes, no or not yet?
- **Decision:** Yes, plan the move

### R09-12 (q040)
- **Question:** Is the Linux memory protection for obid fine with its default numbers (cap at 90% of memory, agents killed first)?
- **Decision:** Yes, use the proposed numbers

### R09-13 (q041)
- **Question:** Where should the signing credentials for releases live: GitHub secrets in CI, or on the owner's own Mac?
- **Decision:** GitHub environment secrets with a required reviewer

### R09-13 (q042)
- **Question:** How often should dev be released to main?
- **Decision:** Only when the owner calls for it

### R09-13,R09-14 (q043)
- **Question:** Will the owner let a CI runner use the Apple Developer ID certificate and add the needed secrets to GitHub?
- **Decision:** Yes, owner adds the secrets (names given by the builder)  *(the owner asked to do it the way OpenBot does: OpenBot cuts releases from a tag on main in CI: a publish job puts the build on GitHub Releases, and release-rollout.yml later changes only the staging percentage in the update manifests (report line 840).)*

### R09-14 (q044)
- **Question:** Is the first real release tag, and the test run before it, something the owner triggers?
- **Decision:** Let the builder create the test tag only, owner does the real one

### R10-01 (q045)
- **Question:** Should cancelling a teammate request need the owner's signature, or only the local token?
- **Decision:** Token alone

### R10-03 (q046)
- **Question:** May a teammate's review request be accepted straight from the Mac banner, without opening the window?
- **Decision:** Allow Accept and Decline on the banner

### R10-04 (q047)
- **Question:** What should the card for a teammate's review request show and say?
- **Decision:** One card on Home's stack with five buttons: Accept, Decline, Accept and remember for this repo, Decline and remember, Later

### R10-07 (q048)
- **Question:** What should Settings › Obysphere show about the Sphere's rules, and what should the one-click button and the 'questions refused' message say?
- **Decision:** Rules read-only for everyone; the button only in Settings

### R10-08,R10-09,R10-10 (q049)
- **Question:** What should Settings › Obysphere show for each teammate's key and look-alike warning, and what should the key-change card and the 'Did <name> read you this?' question say?
- **Decision:** Fingerprint under each member, a Verify button, a warning mark naming the look-alike, and a Home card 'New key for <name>: compare with them'

### R09-16 (q050)
- **Question:** Should Linux downloads be signed with the same key as the Mac update feed or a separate key?
- **Decision:** Same key as the Mac feed

### R09-16 (q051)
- **Question:** May the builder test the Linux installer against the tetrix-lab machine?
- **Decision:** No, use a local VM or container only

### R10-10 (q052)
- **Question:** What should the 'What your Oby does without asking' list look like, and what should Forget say?
- **Decision:** One sentence per decision ('Approves pull requests from <owner> in <repo>') with a Forget button

### R10-11 (q053)
- **Question:** How should the code moved out of the Sphere link count toward the engine's line limit?
- **Decision:** Cut the same number of lines elsewhere in the same PR  *(the owner asked to do it the way OpenBot does: OpenBot has no global line cap.)*

### R11-01,R11-06,R11-09 (q054)
- **Question:** Where in Oby should a person add their own MCP server (a tool connection of their choosing), and what should that screen show and say?
- **Decision:** A 'Custom MCP server' row in Settings > Connectors that opens the existing custom step (name, address, sign in with OAuth / API key / none), with a Remove button on each added row

### R11-01,R11-15 (q055)
- **Question:** What should the vault-health line say, and where should it show?
- **Decision:** Only show a line when something is wrong, as a banner on Home

### R11-01,R11-14 (q056)
- **Question:** When Oby needs a secret (for example a key a repository's check declares), where does the owner type it, and what does the card say?
- **Decision:** A pop-up window over whatever the owner is doing

### R11-01,R11-13 (q057)
- **Question:** Should Settings > Vault be shown to everyone once obid really keeps the secrets?
- **Decision:** Yes, show it for everyone as soon as the Vault page works against obid

### R09-10 (q058)
- **Question:** What should the start-recovery page say, and what should its five buttons be called?
- **Decision:** Same words but leave out Download previous version when none is known  *(the owner asked to do it the way OpenBot does: OpenBot opens a start-recovery window after repeated failed starts (start-recovery-window.ts).)*

### R11-07 (q059)
- **Question:** When an agent calls a tool from one of the owner's own MCP servers, should it follow the job's permission mode (Ask, Accept edits, Plan, Bypass), or be limited to read-only tools?
- **Decision:** Follow the job's mode, with the server's optional allowed-tools list (as proposed)

### R12-05 (q060)
- **Question:** How many times may one GitHub item (a pull request or issue) start a routine, and which events count?
- **Decision:** 3 runs per item per routine per 24 hours; events: reviewRequested, assigned, checksFailed, labeled (as proposed), plus one open job per item per routine at a time and no runs for events Oby's own pushes cause  *(the owner asked to do it the way OpenBot does: OpenBot funnels GitHub events into one routine start with receipts, runs one run per item at a time (a second event waits), skips events its own pushes caused (the 'self' reason), and has a chain guard (the 'chain' reason).)*

### R12-06 (q061)
- **Question:** Should the Routines group on Home show for everyone in the launch build, or stay hidden behind the 'show everything' switch?
- **Decision:** Show Routines for everyone once it is wired to real data

### R11-11 (q062)
- **Question:** What exact words should a job's line use when it moves to another tool and loses some MCP servers?
- **Decision:** Show the lost servers only in the job's details, not on its line

### R12-09 (q063)
- **Question:** What should the task menu offer for working on a job by hand, and what should the buttons and held state say?
- **Decision:** Menu items 'Take over in Terminal', 'Show in Finder', 'Open in editor'; a held task shows a badge 'You have this one' with a 'Hand back to Oby' button

### R12-15,R12-16 (q064)
- **Question:** How should a Linux obid with no Mac app renew its Sphere membership, and may a loopback call with the local token stand in for the Mac's signed approval when pairing a phone?
- **Decision:** Renewal: the Mac renews over SSH when 5 days remain, with a manual `obid sphere renew` as fallback; `headlessPairing` stays off, so phone pairing needs the Mac

### R12-15 (q065)
- **Question:** If the Mac app talks to a remote Linux obid over SSH, what should it show for adding and renewing that box?
- **Decision:** No screen; do it with a command in Terminal only

### R12-17 (q066)
- **Question:** Is there any sender outside GitHub and your own scripts that needs to start a routine through a webhook?
- **Decision:** No sender needs it: close the spike and build nothing
