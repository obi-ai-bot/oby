# Alerts: what waits on the owner reaches them

The business behavior contract for Oby's alerts on the Mac and the phones, from [the template](../business-contract-template.md). The API is `docs/runner-api.md` (`POST /v1/notifications`, the events, the inbox, routines). Roadmap: OBY-70.

## Feature and status

- **Outcome:** AGENTS.md's rule: "Every request that waits on the owner (a job's permission or question, a Confirm) notifies on the Mac with its answers as buttons and shows on Home." The owner can walk away from Oby: whatever needs them reaches them, once, on the Mac and the phone alike, with nothing else in the way, and whatever obid settles by itself (a limit, a second review) never asks.
- **Status:** **Proposed.** Two gates exist today (below, "What's implemented"); ALERTS-1 to 8 make obid's the one list both alert from. The owner's decisions of 2026-11-11 (WAKE-1, ALERTS-HOLD, ALERTS-WORDS, ALERTS-FRONT) and of 2026-11-10 (account limits move or wait by themselves, [account-limits.md](account-limits.md) LIMITS-15; a merge waiting on a second review never asks, [merging.md](merging.md) MERGING-34) are quoted where they apply. Words not on a Sangam screen today are **proposed, Sangam's to change**.
- **Scope:** What alerts on the Mac (Notification Center banners from the Electron app's main process, the Dock's badge) and on the phones (the iPhone through the sealed relay, Android through FCM, the Watch as the iPhone), and the list obid serves for both. Outside it: Home's own cards and stack (the window shows everything; this contract is about what reaches the owner away from it), Live Activities, the phone's own half of ALERTS-8 (apps/ios/app/Notifications.swift keeps it), a Settings switch for WAKE-1 ("can come later"), and voice.

## What's implemented today

Two gates, built apart, that mostly agree.

**The Mac's** (`apps/electron/src/main/notify.ts`). Main follows `GET /v1/events` from launch until Oby quits, window closed too (`follow`), syncing `/v1/jobs`, `/v1/inbox`, `/v1/conversations` and `/v1/limit-decisions`. `wanted` builds one alert per thing: a job's permission or question, a review left to the owner, a merge left to the owner (`mergeNeedsYou`, titled as its card by `mergeAlert`), Ready to publish or Couldn't publish, Failed, an interrupted job not at a limit (Needs you), a `#1563` limit decision (Usage limit, with Move all, Resume all, Keep all), and from the open inbox a permission, Confirm, Start, another Oby's question, or an item sorted `needs_you`; done with nothing to decide, and Oby's answers, are FYI. `notifies(needs, focused)` is the one gate: nothing while the window is in front, and only what needs the owner. `look` compares with `said`, kept in memory only: `null` until the first look, so what's there when Oby opens is never news, and a quit and relaunch forgets what was said. Allow and Deny are buttons only when the banner shows the request whole (`fits`); Allow is signed (`notificationWords`); Confirm is never answered from a banner. An alert goes once its thing is settled anywhere; the Dock's badge counts what waits.

**obid's, for the phones** (`core/crates/obid/src/push.rs`). `spawn` looks once at start, then 2 seconds after each burst of `job`, `inbox` or `turn` events (`heard`). `candidates` builds the same kinds of alert as the Mac's, but without merges, without limit decisions, and with every interrupted job; `wanted` passes them through `notifies(Need::Owner)`. `look` takes the vault's secrets out first (`for_alerts`), then `look_fcm` and `look_relay` compare with what was said, kept in `home/push/notices.json` and `home/push/said.json`, so a restart says nothing again; every title and body passes the secret scan before it's sent. Oby's own answers go to the phone only while no paired device follows the events (`followed`). obid doesn't know whether the Mac app is following, and opens nothing.

**Where they differ**, each settled below: the words (ALERTS-WORDS), merges (ALERTS-7), limits (ALERTS-6), what waited while Oby was closed (ALERTS-2), Oby quit (ALERTS-3), a job held for a ChatGPT yes (ALERTS-4) and a routine that paused itself (ALERTS-5), which neither alerts.

## Behavior

### Actors and ownership

- **obid** decides what waits on the owner: it builds the one list (`GET /v1/alerts`, ALERTS-1), takes secrets out and scans it, pushes it to the phones, remembers what it told them, knows whether the Mac app follows, and opens Oby hidden when it doesn't (ALERTS-3). Rules decide; no model call does.
- **The Mac app** shows what obid serves: it keeps what it said across launches (ALERTS-2), keeps the window-in-front rule (ALERTS-8), draws the buttons and answers through the same endpoints the window uses, signed where the window's are. It decides nothing about what alerts.
- **The phones** show what the relay or FCM brings, as today.
- **The owner** answers from a banner, the window or the phone; an answer anywhere settles the alert everywhere.

### What alerts

Only what waits on the owner to decide or act, one alert per thing, said again only when what it needs changes, gone once it's settled anywhere:

- a job's agent asking for an OK, or asking a question;
- a Confirm, a Start, a conversation's shell command waiting for Allow, an inbox item sorted `needs_you`, another Oby's question to the owner;
- a job that failed, was interrupted (not at a limit: ALERTS-6), or is done with commits obid couldn't publish;
- Oby's review left to the owner;
- a merge that waits on the owner (ALERTS-7);
- a queued job held for the owner's yes on a ChatGPT login (ALERTS-4);
- a routine that paused itself (ALERTS-5);
- the Mac only: the answer to a question the owner asked another Oby (its card said "I'll ping you"); and, as FYI the gate keeps out, Oby's answer in a conversation while the window is away.

**Never**, by the owner's decisions: a job moving or waiting at a limit (LIMITS-15, ALERTS-6), a merge waiting for a second review or for GitHub (MERGING-34, 35; ALERTS-7), a job held only by an account's weekly reserve (its line says it waits; ALERTS-HOLD), progress, a result with nothing to decide, anything while the window is in front (ALERTS-8).

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

The owner's decision: "Mac's words, new ones proposed." obid's list serves the Mac's current words (Sangam's); where obid's push says something else today, obid's changes. A title ends `: <the job's title>` where it's about a job.

| What waits | Title (served by obid) | Body | Buttons | obid's push today |
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
| A conversation's shell command | the item's title | its text | Allow, Deny (when it fits) | same |
| Another Oby's question | `Question from <their owner>` | their words, plain | none: signed on its card | the item's title: changes |
| An item sorted `needs_you` | the item's title | its text | none | same |
| A queued job held for a ChatGPT yes (ALERTS-4) | `Needs your OK` **(proposed)** | its line, `waiting.text`: `your OK for work I run by myself to use the ChatGPT plan on <account>: you can allow it on your Mac, in Settings › Agents` | none: the yes is signed in Settings › Agents | not alerted: new |
| A routine that paused itself (ALERTS-5) | `Routine paused` **(proposed)** | `<name> failed 3 times in a row, so it's paused.` **(proposed)** | Turn back on, Dismiss **(proposed)** | not alerted: new |

The Mac-only `Usage limit` alert (Move all, Resume all when available, Keep all stopped) goes (ALERTS-6).

## Interfaces and dependencies

- **`GET /v1/alerts`** (new, obid only, add-only; loopback and the gateway, read-only): what waits on the owner now, after the gate and the scan, as `[{"key", "what", "title", "body", "category", "thread", "job", "conversation", "item", "routine", "buttons": ["Allow", "Deny"]}]`, `job`, `conversation`, `item` and `routine` each only when it's about one. `key` names the thing (an alert replaces the one with the same key), `what` changes when what it needs changes. A button's answer is the endpoint the window already uses for that thing (a job's permission, an inbox item's answer, ALERTS-5's routine); the list carries the words, never a token or a signature. The exact keys are ALERTS-1's to settle, and `docs/runner-api.md` gets them in the same change.
- **The events.** No new event: the Mac looks again on `job`, `inbox`, `turn`, `conversation` and `routine` events, as today, and reads the list.
- **Persistence.** obid: `home/push/notices.json`, `home/push/said.json` as today, and what it woke Oby for (ALERTS-3). The Mac: what it said, in its own folder beside `settings.json` (ALERTS-2).
- **The Mac app following.** obid knows the Mac app follows while its main process's own event stream is open, and for a short while after it drops (a reconnect). How the stream says it's the Mac's is ALERTS-3's to settle, add-only.
- **Opening Oby hidden** (WAKE-1): `open -g -j` on the app's bundle (`com.deskree.oby.electron`), macOS only; the app launched hidden shows no window and starts following. Off a Mac, or with no Mac app, obid does nothing more than push.
- **Answers.** Unchanged: `POST /v1/jobs/{id}/permission`, `POST /v1/inbox/{id}/answer`, `PATCH /v1/routines/{id}` (`{"enabled": true}`), Allow signed with `X-Oby-Words` as today.
- **Open:** whether Dismiss on a paused routine (ALERTS-5) is an inbox item's answer or the routine's own state is ALERTS-5's to settle, add-only either way.

## Acceptance scenarios

Test names are fixed: the tasks that build each fill them in.

- **ALERTS-1** obid's gate, `GET /v1/alerts`, is the one list the Mac and the phone alert from. It holds exactly what passes `notifies(Need::Owner)`, in the words above (ALERTS-WORDS), the relay and FCM send from it, and the Mac shows what it serves, deciding nothing itself. A vault secret in a job's title, a question or a reason never leaves: it's taken out before the list is built, and an alert the scan stops (or can't scan) isn't in the list. `runner_api::alerts_1_get_alerts_is_the_gate_as_data`, `runner_api::alerts_1_a_secret_never_leaves_in_an_alert`, `alerts.test.ts alerts_1_the_mac_alerts_what_obid_serves`.
- **ALERTS-2** The Mac keeps what it said across launches. A permission that came while Oby was quit alerts once when Oby opens (today it never does: what's there at launch isn't news); a quit and relaunch with nothing new says nothing again. With nothing kept (the first launch with this), what's there isn't news, as today. `alerts_2_what_waited_while_closed_alerts_once_at_launch`, `alerts_2_a_restart_repeats_nothing`.
- **ALERTS-3** With no Mac app following, obid opens Oby hidden, once per need (WAKE-1, the owner's "Yes, open it hidden": "with Oby quit on purpose, when something new waits on the owner obid opens Oby hidden in the background (open -g -j, no window forward) so the Mac banner with buttons appears"). Something new in the list with no Mac app following: one `open -g -j`; the app opens with no window, and its banner (ALERTS-2) shows it. The same need again, or FYI, opens nothing; a Mac app following, or one reconnecting its stream, isn't woken. `push::tests::alerts_3_a_new_need_with_no_mac_following_wakes_once`, `alerts_3_a_following_or_reconnecting_mac_app_is_not_woken`.
- **ALERTS-4** A queued job held for the owner's yes on a ChatGPT login (CHATGPT-20, `waiting.reserved` empty) alerts, once (ALERTS-HOLD, the owner's "Only the ChatGPT yes"); its yes is given in Settings › Agents, so the banner opens the window there. One held only by an account's weekly reserve (`waiting.reserved` naming accounts), and one waiting out a limit (LIMITS-5), stay silent: their line says what they wait for. `push::tests::alerts_4_a_job_held_for_the_owners_yes_alerts`, `alerts_4_a_limits_5_wait_stays_silent`.
- **ALERTS-5** A routine that paused itself (3 failed runs in a row, `failuresInARow`) raises a card in the inbox and an alert: Turn back on, Dismiss (proposed words). Turn back on resumes it, as `PATCH /v1/routines/{id}` `{"enabled": true}` does, and settles the card and the alert; Dismiss settles them and leaves it paused. A routine the owner paused raises nothing. `routines_api::alerts_5_a_paused_routine_raises_a_card`, `alerts_5_turn_back_on_resumes_it_and_settles_the_card`.
- **ALERTS-6** Changed from the old plan: #1563's cards are gone for jobs. A job that moves at a limit, or waits for room (LIMITS-5, LIMITS-15), never alerts, on the Mac or a phone, and an interrupted job at a limit isn't `Needs you`. A `limitDecision` an older release left resolves itself (LIMITS-15) without an alert; the Mac's `Usage limit` alert and its `/v1/limit-decisions` polling go. `push::tests::alerts_6_a_limit_move_or_wait_never_alerts`.
- **ALERTS-7** A merge that waits on the owner (only what MERGING-34 and 35 still leave to them: no learned workflow's `Ready to merge?`, or a real refusal's `Not merged: …`) alerts once per card, titled as its card, on the phone too (today the Mac's alone). A merge waiting for a second review (MERGING-34) or for GitHub to work out whether it can merge (MERGING-35) never alerts. `push::tests::alerts_7_a_merge_wait_alerts_once_as_its_card`.
- **ALERTS-8** Nothing alerts on the Mac while Oby's window is in front, on any page (ALERTS-FRONT, the owner's "Keep it"): what needs the owner and came meanwhile alerts once the window leaves the front, and what's on the page the window shows is withdrawn. The badge still counts. `alerts_8_nothing_alerts_while_the_window_is_in_front`.

### Test it when it's done (a real Mac)

1. Quit Oby (⌘Q) with obid running; start a job that asks for a permission from the phone. Oby opens with no window, and the banner shows Needs your OK with Allow and Deny. Allow from the banner: the job goes on, and the phone's alert goes.
2. Open Oby with the window in front; start a job that fails. No banner. Switch to another app: Failed alerts once.
3. Fill one account's limit with a job running and another account with room: the job moves, and nothing alerts anywhere.
4. Turn off ChatGPT background use on a ChatGPT login and let a routine on it come due: one Needs your OK alert; a routine held only by a weekly reserve says nothing.
5. Make a routine fail 3 times: Routine paused alerts with Turn back on and Dismiss; Turn back on resumes it.
6. Quit and reopen Oby with nothing new: no banner repeats.
