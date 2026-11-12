# Alerts: what waits on the owner reaches them

The business behavior contract for Oby's alerts on the Mac and the phones, from [the template](../business-contract-template.md). The API is `docs/runner-api.md` (`POST /v1/notifications`, the events, the inbox, routines). Roadmap: OBY-70.

## Feature and status

- **Outcome:** AGENTS.md's rule: "Every request that waits on the owner (a job's permission or question, a Confirm) notifies on the Mac with its answers as buttons and shows on Home." The owner can walk away from Oby: whatever needs them reaches them, once, on the Mac and the phone alike, with nothing else in the way, and whatever obid settles by itself (a limit, a second review) never asks.
- **Status:** **Implemented** (ALERTS-1 to 8; OBY-85 moved the Mac onto obid's list): obid's gate is the one list the Mac and the phones alert from (below, "What's implemented"). The owner's decisions of 2026-11-11 (WAKE-1, ALERTS-HOLD, ALERTS-WORDS, ALERTS-FRONT) and of 2026-11-10 (account limits move or wait by themselves, [account-limits.md](account-limits.md) LIMITS-15; a merge waiting on a second review never asks, [merging.md](merging.md) MERGING-34) are quoted where they apply. Words not on a Sangam screen today are **proposed, Sangam's to change**.
- **Scope:** What alerts on the Mac (Notification Center banners from the Electron app's main process, the Dock's badge) and on the phones (the iPhone through the sealed relay, Android through FCM, the Watch as the iPhone), and the list obid serves for both. Outside it: Home's own cards and stack (the window shows everything; this contract is about what reaches the owner away from it), Live Activities, the phone's own half of ALERTS-8 (apps/ios/app/Notifications.swift keeps it), a Settings switch for WAKE-1 ("can come later"), and voice.

## What's implemented today

One gate, obid's, that the Mac and the phones alert from.

**The Mac** (`apps/electron/src/main/notify.ts`). Main follows `GET /v1/events` from launch until Oby quits, window closed too (`follow`), syncing `/v1/alerts`, `/v1/inbox`, `/v1/conversations` and `/v1/limit-decisions`, and reads `GET /v1/alerts` again 2 seconds after each burst of `job` or `inbox` events (`refresh`); a read obid can't serve keeps the last list and is tried again by itself, 2 seconds later, then twice as long each time, at most a minute apart (`again`). It draws each alert obid serves, deciding nothing about what alerts: obid's title and body as plain text (`plain`, control and format characters out) cut to 100 and 200 characters (`cut`); obid's `answers` as buttons, each answering `POST /v1/inbox/{item}/answer` as the window does, Allow and Deny signed (`notificationWords`); a `confirm` with no button, whatever comes; a `limitDecision` with its choices from `GET /v1/limit-decisions` (Move all, Resume all, Keep all), answered through `answerLimit`, which signs its revision (obid serves none since ALERTS-6); another Oby's question as its card shows it (`questionOf`, `Question from <their owner>`, PEER-30), answered on its card; and a click on anything opens its job or conversation. It keeps what only it says: the window-in-front rule (`notifies`, ALERTS-8), the Dock's badge (what obid serves), the answer to a question the owner asked another Oby (`pingOf`), and Oby's answers as FYI the gate keeps in. `look` compares obid's `key` and `what` with `said`, kept across launches in `alerts-said-v2.json` in the app's own folder beside `settings.json`, read without following a link (ALERTS-2): what waited while Oby was closed alerts once at launch, and a relaunch repeats nothing; with nothing kept yet, the first look's is the baseline, and a file that can't be read or parsed (a link among them) keeps nothing quiet. The file was `alerts-said.json` under the Mac's own keys: renamed when the keys became obid's, so the first launch with them is a baseline and nothing alerts twice. An alert goes once obid stops serving it.

**obid's, for the phones and the Mac** (`core/crates/obid/src/push.rs`). `spawn` looks once at start, then 2 seconds after each burst of `job`, `inbox` or `turn` events (`heard`). `candidates` builds the same kinds of alert as the Mac's, but without merges, without limit decisions, and with every interrupted job; `wanted` passes them through `notifies(Need::Owner)`. `look` takes the vault's secrets out first (`for_alerts`), then `look_fcm` and `look_relay` compare with what was said, kept in `home/push/notices.json` and `home/push/said.json`, so a restart says nothing again; every title and body passes the secret scan before it's sent. Since ALERTS-1 the same list, in the Mac's words, is `GET /v1/alerts` (`listed`), on the loopback. Oby's own answers go to the phone only while no paired device follows the events (`followed`). The Mac app's main process follows them with `?alerts=1` (`mac_followed`, ALERTS-3): after each look, a need new this run (`to_wake`) with no Mac app following opens it hidden (`wake`); one that came within 60 seconds of a drop is held, and looked at again when the minute is up (`Wake::After`).

**Where the two gates differed** before the Mac read obid's list, each settled below: the words (ALERTS-WORDS), merges (ALERTS-7), limits (ALERTS-6), Oby quit (ALERTS-3), a job held for a ChatGPT yes (ALERTS-4) and a routine that paused itself (ALERTS-5).

## Behavior

### Actors and ownership

- **obid** decides what waits on the owner: it builds the one list (`GET /v1/alerts`, ALERTS-1), takes secrets out and scans it, pushes it to the phones, remembers what it told them, knows whether the Mac app follows, and opens Oby hidden when it doesn't (ALERTS-3). Rules decide; no model call does.
- **The Mac app** shows what obid serves: it keeps what it said across launches (ALERTS-2), keeps the window-in-front rule (ALERTS-8), draws the buttons and answers through the same endpoints the window uses, signed where the window's are. It decides nothing about what alerts.
- **The phones** show what the relay or FCM brings, as today.
- **The owner** answers from a banner, the window or the phone; an answer anywhere settles the alert everywhere.

### What alerts

Only what waits on the owner to decide or act, one alert per thing, said again only when what it needs changes, gone once it's settled anywhere:

- a job's agent asking for an OK, or asking a question;
- a Confirm, a Start, an inbox item sorted `needs_you`, another Oby's question to the owner;
- a job that failed, was interrupted (not at a limit: ALERTS-6), or is done with commits obid couldn't publish;
- Oby's review left to the owner;
- a merge that waits on the owner (ALERTS-7);
- a queued job held for the owner's yes on a ChatGPT login (ALERTS-4);
- a routine that paused itself (ALERTS-5);
- the Mac only: the answer to a question the owner asked another Oby (its card said "I'll ping you"); and, as FYI the gate keeps out, Oby's answer in a conversation while the window is away.

**Never**, by the owner's decisions: a job moving or waiting at a limit (LIMITS-15, ALERTS-6), a merge waiting for a second review or for GitHub (MERGING-34, 35; ALERTS-7), a job held only by an account's weekly reserve (its line says it waits; ALERTS-HOLD), a check that judged nothing, while obid runs it again or once it stopped trying with Publish kept on the job's page (`unjudged`, [publishing.md](publishing.md) PUBLISHING-31, the owner's ASK-LESS rule of 2026-11-11; **implemented** in both gates), progress, a result with nothing to decide, anything while the window is in front (ALERTS-8).

### States and transitions

An alert is **wanted** (in obid's list), **told** (the Mac or a phone showed it; remembered), **replaced** (what it needs changed: its key's `what` changed, the old one goes), or **settled** (gone from the list: the Mac withdraws it, the phone gets a clear or a "Settled"). An answer from a banner that isn't taken (answered elsewhere, obid away) opens the window on the thing.

### Failures and recovery

- obid restarts: what it told is on disk; nothing is said twice (today's behavior, kept).
- The Mac app quits and relaunches: what it said is on disk (ALERTS-2); what came meanwhile alerts once.
- Oby quit, obid running: obid opens it hidden (ALERTS-3). `open` failing (the app moved, not installed) is logged and nothing else changes; the phones still alert.
- The vault can't be read, or the scan can't run or finds a secret: that alert isn't in the list nor sent (today's behavior for the phones, kept for the list, ALERTS-1).
- The window's answer from a locked Mac's banner is never sent: the window opens behind the login (today's behavior, kept).

### Invariants and boundaries

- One list: what the Mac and the phones alert from is obid's (ALERTS-1). A change to what alerts is made once, in obid.
- Secrets never leave in an alert: the vault's taken out before anything's built, and the scan on every title and body (ALERTS-1).
- Text from GitHub, other Obys, agents and tools is data: plain text in an alert, control and format characters out, never a link the banner follows.
- Approvals stay the owner's: Allow is signed by the Mac's key as the window's is; Confirm, a ChatGPT yes and anything the banner can't show whole are answered in the window, never from a banner.
- Oby forbids nothing and asks only when something needs the owner: what obid settles by itself never alerts.
- API add-only: `GET /v1/alerts` and its keys are new; nothing existing is renamed or removed, and no enum the Swift apps decode gains a value (the phones' `category` values stay the ones they know; a new kind uses `job`, opening the app).

## Words (ALERTS-WORDS)

The owner's decision: "Mac's words, new ones proposed." obid's list serves the Mac's current words (Sangam's); where obid's push said something else, obid's changed with ALERTS-1 (the last column is what it said before). A title ends `: <the job's title>` where it's about a job.

| What waits | Title (served by obid) | Body | Buttons | obid's push before ALERTS-1 |
|---|---|---|---|---|
| A job's agent asks for an OK | `Needs your OK` | the request, whole | Allow, Deny (when it fits) | `<permission title>?`, body `<title>: <detail>`: changes |
| A job's agent asks a question | `Question` | the question | none: answered in the window | `<permission title>?`: changes |
| Done with commits obid didn't publish, no error | `Ready to publish` | the summary's first line, else `Done` | none | `Couldn't publish`, with no error: changes |
| The same, with a publish error | `Couldn't publish` | `publishError`'s first line | none | same |
| `failed` | `Failed` | the reason, else the summary | none | same |
| `interrupted`, not at a limit | `Needs you` | the reason, else `Interrupted` | none | `Interrupted`: changes |
| Oby's review couldn't run | `Couldn't review` | `reviewError` | none | same |
| Oby's review still asks for changes | `Changes requested` | `Oby's review still asks for changes. Take it from here.` | none | same |
| A merge waits on the owner | its card's title (`Ready to merge`, `Not merged`) | `mergeError`'s first line, else `Not merged` | none | not alerted: new on the phone |
| A Confirm | `Confirm` | its title, then its text | none: signed in the window | the item's title: changes |
| A Start | the item's title | its text | Start (one job, shown whole, read nothing untrusted) | same |
| One turn's offer of several jobs | `Oby offers <n> jobs` | the first's text | none | same |
| Another Oby's question | `Question from <their owner>` | their words, plain | none: signed on its card | the item's title: changes |
| An item sorted `needs_you` | the item's title | its text | none | same |
| A queued job held for a ChatGPT yes (ALERTS-4) | `Needs your OK` **(proposed)** | its line, `waiting.text`: `your OK for work I run by myself to use the ChatGPT plan on <account>: you can allow it on your Mac, in Settings › Agents` | none: the yes is signed in Settings › Agents | not alerted: new |
| A routine that paused itself (ALERTS-5) | `Routine paused` **(proposed)** | `<name> failed 3 times in a row, so it's paused.` **(proposed)** | Turn back on, Dismiss **(proposed)** | **implemented**: an inbox card (kind `routine`) sorted `needs_you`, alerted as such |

The Mac-only `Usage limit` alert (Move all, Resume all when available, Keep all stopped) goes (ALERTS-6).

## Interfaces and dependencies

- **`GET /v1/alerts`** (**implemented**, obid only, add-only; loopback only, read-only, not on the gateway): what waits on the owner now, after the gate and the scan, as `[{"key", "what", "title", "body", "category", "thread", "job", "conversation", "item", "urgent", "answers": ["Allow", "Deny"]}]`, `job`, `conversation` and `item` each only when it's about one (a paused routine is its inbox card's `item`, ALERTS-5). `key` is the alert's collapse id and names the thing (an alert replaces the one with the same key), `what` changes when what it needs changes. `answers` are decided once in obid (`push.rs`, `answers`): Allow and Deny for a permission whose command or edit the banner shows whole (notify.ts's `fits`), Start for a single Start item from words that read nothing untrusted that fits, else none. An answer goes to the endpoint the window already uses for that thing (a job's permission, an inbox item's answer); the list carries the words, never a token or a signature. `docs/runner-api.md`, `GET /v1/alerts`.
- **The events.** No new event: the Mac looks again on `job`, `inbox`, `turn`, `conversation` and `routine` events, as today, and reads the list.
- **Persistence.** obid: `home/push/notices.json`, `home/push/said.json` as today; what it woke Oby for is kept in memory for the run only (ALERTS-3: obid's start counts as a drop, so what waits then wakes the app a minute later only if it doesn't follow by then). The Mac: what it said, in its own folder beside `settings.json` (ALERTS-2).
- **The Mac app following** (settled by ALERTS-3). obid knows the Mac app follows while its main process's own event stream is open, `GET /v1/events?alerts=1` on loopback (a paired device's through the gateway never counts), and for 60 seconds after it drops (a reconnect; obid's own start counts as a drop).
- **Opening Oby hidden** (WAKE-1, settled by ALERTS-3): `/usr/bin/open -g -j -a <the .app obid runs from> --args --background`, once per look that finds a need new this obid run with no Mac app following; what came while the app followed is no news. A need that comes within 60 seconds of a drop (or of obid's start) isn't lost: it's held, and obid looks again when the minute is up, waking the app then if it didn't come back. macOS only, and only for an obid inside the app's bundle (`Contents/Helpers/obid`). The window opens only while Oby is the active app: launched with `--background`, the app creates none, and the activate (a reopen) that `open` brings, to that launch or to an Oby already running with its window closed, comes while Oby isn't active and opens nothing; the owner's Dock click, ⌘-Tab or a click on its banner makes Oby active, which opens it. Nothing else on that start shows the window: a membership another key signed (SPHERE-6) is told on the window once the owner opens it. A reopen while Oby is already the active app with no window opens it, as a Dock click then would. Off a Mac, or with no Mac app, obid does nothing more than push.
- **Answers.** Unchanged: `POST /v1/jobs/{id}/permission`, `POST /v1/inbox/{id}/answer`, `PATCH /v1/routines/{id}` (`{"enabled": true}`), Allow signed with `X-Oby-Words` as today.
- **Settled by ALERTS-5:** Dismiss on a paused routine is its inbox card's answer (`POST /v1/inbox/{id}/answer`), and so is Turn back on; the routine keeps no new state.

## Acceptance scenarios

Test names are fixed: the tasks that build each fill them in.

- **ALERTS-1** **Implemented** (`core/crates/obid/src/push.rs`, `listed` and `answers`; `GET /v1/alerts` in the Mac's words, the phones' pushes too; the Mac's half in `apps/electron/src/main/notify.ts`, OBY-85). obid's gate, `GET /v1/alerts`, is the one list the Mac and the phone alert from. It holds exactly what passes `notifies(Need::Owner)`, in the words above (ALERTS-WORDS), the relay and FCM send from it, and the Mac shows what it serves, deciding nothing itself. A vault secret in a job's title, a question or a reason never leaves: it's taken out before the list is built, and an alert the scan stops (or can't scan) isn't in the list. A permission the banner can't show whole gets no answers. `runner_api::alerts_1_get_alerts_is_the_gate_as_data`, `runner_api::alerts_1_a_secret_never_leaves_in_an_alert`, `runner_api::alerts_1_a_long_permission_gets_no_answers`, `alerts.test.ts alerts_1_the_mac_alerts_what_obid_serves`, `alerts_1_a_limit_decision_keeps_its_signed_choices`, `alerts_1_a_list_obid_couldnt_serve_is_read_again_by_itself`.
- **ALERTS-2** **Implemented** (`apps/electron/src/renderer/src/test/alerts.test.ts`). The Mac keeps what it said across launches, by obid's `key` and `what`. A permission that came while Oby was quit alerts once when Oby opens (before ALERTS-2 it never did: what was there at launch wasn't news); an alert already shown isn't posted again after a quit and relaunch, and a relaunch with nothing new says nothing. With nothing kept (the first launch with this, or with obid's keys), what's there isn't news. The file is read without following a link, and one that can't be read keeps nothing quiet. `alerts_2_what_waited_while_closed_alerts_once_at_launch`, `alerts_2_a_restart_repeats_nothing`, `alerts_2_an_alert_seen_before_a_relaunch_is_not_posted_again`, `alerts_2_the_said_file_is_read_without_following_a_link`, `alerts_2_what_the_mac_said_under_its_own_keys_is_no_news`.
- **ALERTS-3** **Implemented** (`core/crates/obid/src/push.rs`, `routes::events`, `apps/electron/src/main/index.ts`). With no Mac app following, obid opens Oby hidden, once per need (WAKE-1, the owner's "Yes, open it hidden": "with Oby quit on purpose, when something new waits on the owner obid opens Oby hidden in the background (open -g -j, no window forward) so the Mac banner with buttons appears"). Something new in the list with no Mac app following: one `open -g -j`; the app opens with no window, and its banner (ALERTS-2) shows it. The same need again, or FYI, opens nothing; a Mac app following, or one reconnecting its stream, isn't woken. No window comes forward for it, whether Oby was quit or running with its window closed. `push::tests::alerts_3_a_new_need_with_no_mac_following_wakes_once`, `alerts_3_a_following_or_reconnecting_mac_app_is_not_woken`, `wake.test.ts alerts_3_a_running_oby_obid_reopens_shows_no_window`, `alerts_3_a_membership_error_waits_for_the_owner_to_open_oby`.
- **ALERTS-4** **Implemented** (`core/crates/obid/src/push.rs`, the one gate, the Mac's alerts too since ALERTS-1). A queued job held for the owner's yes on a ChatGPT login (CHATGPT-20, `waiting.reserved` empty) alerts, once (ALERTS-HOLD, the owner's "Only the ChatGPT yes"); its yes is given in Settings › Agents, so the banner opens the window there. One held only by an account's weekly reserve (`waiting.reserved` naming accounts), and one waiting out a limit (LIMITS-5), stay silent: their line says what they wait for. `push::tests::alerts_4_a_job_held_for_the_owners_yes_alerts`, `alerts_4_a_limits_5_wait_stays_silent`.
- **ALERTS-5** **Implemented** (`core/crates/obid/src/routines.rs`, the inbox card). A routine that paused itself (3 failed runs in a row, `failuresInARow`) raises a card in the inbox and an alert: Turn back on, Dismiss (proposed words). Turn back on resumes it, as `PATCH /v1/routines/{id}` `{"enabled": true}` does, and settles the card and the alert; Dismiss settles them and leaves it paused. A routine the owner paused raises nothing. `routines_api::alerts_5_a_paused_routine_raises_a_card`, `alerts_5_turn_back_on_resumes_it_and_settles_the_card`.
- **ALERTS-6** **Implemented** (`core/crates/obid/src/push.rs`, the one gate, the Mac's alerts too since ALERTS-1). Changed from the old plan: #1563's cards are gone for jobs. A job that moves at a limit, or waits for room (LIMITS-5, LIMITS-15), never alerts, on the Mac or a phone, and an interrupted job at a limit isn't `Needs you`. A `limitDecision` an older release left resolves itself (LIMITS-15) without an alert; the Mac's own `Usage limit` alert went with ALERTS-1 (it keeps reading `/v1/limit-decisions` only for the choices of a `limitDecision` alert, which obid doesn't serve). `push::tests::alerts_6_a_limit_move_or_wait_never_alerts`.
- **ALERTS-7** **Implemented** (`core/crates/obid/src/push.rs`, the one gate, the Mac's alerts too since ALERTS-1). A merge that waits on the owner (only what MERGING-34 and 35 still leave to them: no learned workflow's `Ready to merge?`, or a real refusal's `Not merged: …`) alerts once per card, titled as its card, on the Mac and the phone alike. A merge waiting for a second review (MERGING-34) or for GitHub to work out whether it can merge (MERGING-35) never alerts. `push::tests::alerts_7_a_merge_wait_alerts_once_as_its_card`.
- **ALERTS-8** **Implemented** (`apps/electron/src/main/notify.ts`, `notifies`). Nothing alerts on the Mac while Oby's window is in front, on any page (ALERTS-FRONT, the owner's "Keep it"): what needs the owner and came meanwhile alerts once the window leaves the front, and what's on the page the window shows is withdrawn. The badge still counts. `alerts_8_nothing_alerts_while_the_window_is_in_front`.

### Test it when it's done (a real Mac)

1. Quit Oby (⌘Q) with obid running; start a job that asks for a permission from the phone. Oby opens with no window, and the banner shows Needs your OK with Allow and Deny. Allow from the banner: the job goes on, and the phone's alert goes.
2. Open Oby with the window in front; start a job that fails. No banner. Switch to another app: Failed alerts once.
3. Fill one account's limit with a job running and another account with room: the job moves, and nothing alerts anywhere.
4. Turn off ChatGPT background use on a ChatGPT login and let a routine on it come due: one Needs your OK alert; a routine held only by a weekly reserve says nothing.
5. Make a routine fail 3 times: Routine paused alerts with Turn back on and Dismiss; Turn back on resumes it.
6. Quit and reopen Oby with nothing new: no banner repeats.
