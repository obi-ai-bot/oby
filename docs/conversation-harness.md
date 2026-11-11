# Talking to Oby: the conversation harness

The owner's decision (2026-10-17): talking to Oby works like talking to Claude Code, ChatGPT or Codex. A conversation is a real agent session with the agent's own harness (reading and searching code, the web, running things, sub-agents) plus Oby's own tools, and what it makes to be looked at is an artifact the apps show. Sangam's design stays as it is: this is the logic under it. Whichever agent runs the conversation (Claude Code, Codex or Cursor over ACP, the owner's choice at runtime: docs/contracts/conversation-agent.md), it gets the same Oby tools, so it works the same.

This replaces obid's plan loop. Conversation turns use the harness (the event orchestrator that also did is gone, #1946); side calls still use a short, separate model call.

## 1. Talking and work stay apart

As in Sangam's design (design/app `chats.ts`, `tasks.ts`):

- **A conversation is talking.** Home's message box starts one; a reply carries it on. Oby works in the open (its steps and tool calls show as it goes), its words stream in, and most turns end in an answer. It is never a task.
- **A task is work.** It exists only once Oby starts one (a job, with its card under Oby's words and its row on Home), runs in its own worktree, and has its own thread with its agent. Telling a finished task something picks it up again at once (#644).

## 2. A conversation is an agent session

- **One session per conversation, on one agent.** obid starts the conversation's agent on its first turn and resumes the same session on every later turn (`claude -p --resume <session>` with stream-json in and out, as jobs run Claude Code; Codex: `thread/start`, then `turn/start` on the same thread; Cursor: `session/new`, then `session/load` and `session/prompt`). On another agent or account a turn starts a new session with the earlier turns handed over, quoted, as after a lost one; what the conversation read stays with it. A conversation remembers what was said because the agent's session does; obid also keeps every turn (`GET /v1/conversation`), as today.
- **Its room.** `home/conversations/<id>/room/`, the agent's working folder and `HOME`-like temp, in the same sandbox as a foreign job (bubblewrap on Linux, Seatbelt on macOS) with:
  - writes: only its room;
  - reads: its room, and a read-only checkout of each configured repository at its default branch (`home/conversations/<id>/room/repos/<owner>/<name>`, made from obid's clones; refreshed at the start of a turn when the default branch moved);
  - network: the model's API and the web (web search and fetch are part of talking), as a job's in Ask mode asks before;
  - no code-host login, no vault secret, no other conversation's room, no job's worktree.
- **Its lane** (docs/contracts/lanes.md, LANES-12). The owner's Claude Code conversation runs in the own lane, as the owner's jobs do: on the account's own folder and login, which Claude Code refreshes itself, unboxed by obid, its commands run by Claude Code's own Bash in Claude Code's own sandbox, writing only the room (never `repos/`, nor the room's `.claude/`, `.mcp.json` or `CLAUDE.md`) and its temporary folder, with the owner's user-level settings, skills and MCP servers (CONVO-4, `--setting-sources user`). Reading something untrusted doesn't move it: the mark stays, and with the owner's Confirm guards what goes out (CONVO-1). A conversation with another Oby, Codex's and Cursor's (with no shell, CONVO-2) stay in the box below, with a copy of the login and `oby_shell`, and so does the brain's planner (CONVO-5).
- **Its tools.** The agent's own: read, search, list, web search, web fetch, run a command (in the room), write a file (in the room), sub-agents, a to-do list. Permission prompts are answered by obid's policy in Sandbox mode: inside the room, yes; the network for the web, yes; secrets and anything outside the room, no (a conversation never asks the owner mid-sentence: what it can't do, it says).
- **Oby's tools** (section 3), from obid's MCP endpoint for the conversation, `POST /v1/agent/mcp/conversation-<id>` with a token of the session's own, like a job's connected-app broker.
- **Tetrix**, while Oby is signed in to it: obid's Tetrix server, `POST /v1/agent/tetrix/conversation-<id>` with the same token, as every job's agent has it, but the reads only (no `save_session`: a conversation keeps no outgoing list). Each call's arguments pass the secret scan, each answer is scrubbed and quoted as data, and a call makes the turn count as having read something untrusted. Oby's Tetrix login never reaches the session. The orientation says to ask Tetrix first (code, ownership, impact, earlier sessions) and `oby_github` for live GitHub state, which Tetrix doesn't track (the owner's rule of 2026-10-27: Tetrix first, GitHub only for what it doesn't have).
- **The model.** The account's Claude Code default (Sonnet for talking unless the owner picks another in Settings › Models › Conversations); Opus when the owner asks for it in words for that conversation.
- **When it can't run** (no agent account signed in, the agent fails twice), it moves to another signed-in account of any tool and says so in a line, unless the owner keeps failover off (`accountFailover`); with nothing left, the turn says why in one line.

## 3. Oby's tools (MCP server `oby`)

The same for every agent. obid checks every call against the same action rules (`brain/actions.rs`), and anything that persists, widens trust or starts work in Bypass is a `confirm` item only the owner's answer runs.

| Tool | Input | Does |
|---|---|---|
| `oby_jobs` | `{status?, repo?, limit?}` | Lists jobs, newest first: id, title, status, step, repo, agent, account, PR. |
| `oby_job` | `{id}` | One job: what was asked, its report, files changed, what went out, its PR, Mac checks, Oby's verdict and why reviewing or merging stopped, what it's waiting for, and a note wherever anything was cut (docs/contracts/task-conversations.md, TASKCONV-16). Agent-written text is returned as quoted data. |
| `oby_dispatch` | `{repo, task, kind?, agent?, account?, mode?, issue?, pr?, title?}` | Starts a job from this conversation (`conversation` set on the job, its card on the turn). The dispatch uses obid's action validation. After the turn read something untrusted the job is marked `untrusted` and still runs in the owner's mode, Bypass too, inside Oby's box (the owner's decision of 2026-10-27); a Bypass the agent names waits for the owner's Confirm either way. |
| `oby_offer` | as `oby_dispatch` | Offers the job instead of starting it, where the agent would otherwise ask "shall I start…?": planned and checked as `oby_dispatch`'s (the turn's untrusted state, foreign words and check go with it; it counts against the turn's ten), then kept as a `start` inbox item of this conversation with `offer` (docs/runner-api.md, "Inbox item"). One turn's offers are one card under its answer, showing each job in obid's words, the task Oby planned (quoted) and whether the turn had read something untrusted: Dispatch is the owner's own `Start` of each, Skip their `Cancel`, Later puts it off an hour. Nothing starts until that tap; the agent's own `Start` (`oby_answer`) only becomes a `confirm` item. The job it starts names the turn it was offered in (`turn`), so its card goes under that answer. An offer closes by itself after 24 hours, or when its conversation is deleted. Bypass is refused (only the owner's Confirm starts one: `oby_dispatch` asks it). |
| `oby_tell_job` | `{id, words}` | The owner's words to a job (#542), the agent's quoted after them: running, at its next turn; ended, a follow-up with the same kind, agent, account, repository and conversation, the owner's words its request and what the first job did quoted in its first prompt, its card on the turn (Sangam's design, the owner's decision of 2026-10-18, which replaced #644's one more round). After the turn read something untrusted, the follow-up is a `start` item for the owner's Start. A follow-up counts against the turn's ten jobs, at most one per job a turn. |
| `oby_stop_job` | `{id}` | Stops a running job. |
| `oby_inbox` | `{}` | What waits for the owner, what needs them first, each item with its `triage`. |
| `oby_github` | `{list?}` | What GitHub wants from the owner, as obid's own `gh` login last read it (`mine`, `reviews`, `assigned`); titles quoted. |
| `oby_answer` | `{item, answer}` | Answers an inbox item the brain may answer; the owner's own decisions (permissions, confirms, publishing) are refused with why. |
| `oby_conversations` | `{query?}` | Finds past conversations by what was said (#593); with no query, lists them all with their dates (#2085). |
| `oby_delete_conversations` | `{ids}` | Deletes conversations for good, only on the owner's Confirm, one for them all (#2085). |
| `oby_room_read` | `{}` | The Obysphere's room's newest posts, in a quoted file in the room, `repos/sphere-room.json`; marks the conversation as having read something untrusted (docs/contracts/rooms.md). |
| `oby_room_post` | `{kind, text, about?, replyTo?}` | A `need` or `found` in the Obysphere's room, on the owner's word, else their Confirm. |
| `oby_artifact` | `{title, path, kind?, artifact?}` | Publishes a file or folder from the room as an artifact, or a new version of `artifact` (section 4). |
| `oby_read` | `{path, offset?, limit?}` | A text file of the room (`repos/` included), its lines numbered: at most 2,000 lines and 256 KB at a time. Quoted data. |
| `oby_search` | `{pattern, path?, glob?}` | A regex search of the room's text files under `path`: at most 200 matches. Quoted data. |
| `oby_list` | `{path?, glob?}` | A folder's entries, or with `glob` every file under it that matches: at most 500. |

The read tools are how any agent reads (Codex has no shell, so no other way): only inside the room once links are followed, never `..` or its `config/` (Codex's login copy), `login/`, `home/` or `session.json`, the same refusals and taint as Claude Code's Read, Grep and LS (`repos/` and `attachments/` taint), files opened with `O_NOFOLLOW` and checked again once open, a search held to 32 MB and 10 seconds and ended with its turn, answers and refusals fenced, and every known secret taken out.

## 4. Artifacts

What an agent makes to be looked at: a page, a doc, a chart, a canvas of design options, a PDF, an image.

- **Made** by `oby_artifact` from the conversation's room, or by a job (its `.obi/artifacts/` folder, published when the job ends). Kinds: `html` (a folder with an `index.html`), `markdown`, `image`, `pdf`, `canvas` (an artboard: a JSON list of frames, each an HTML page with a size and position).
- **Kept** in `home/artifacts/<id>/<version>/`, every version, with `artifact.json` (`id`, `title`, `kind`, `conversation`, `job`, `created`, `versions`).
- **API**: `GET /v1/artifacts?conversation=&job=`, `GET /v1/artifacts/{id}`, `GET /v1/artifacts/{id}/{version}/{path}` (the files), `PATCH /v1/artifacts/{id}` (`title`), `DELETE /v1/artifacts/{id}`; an `artifact` event when one is made or gets a version; the turn that made it carries `artifacts: [{id, version}]`. On the gateway's allowlist (the phone shows them).
- **Shown** as a card under the words that made it, opening in a viewer in Sangam's style: HTML in a web view with scripts on, Markdown rendered, images and PDFs native, a canvas as a pannable artboard of its frames. obid sends `Content-Security-Policy: default-src 'self' data: blob:; …; connect-src 'none'; …; sandbox allow-scripts` on every file it serves from an artifact (docs/runner-api.md, "obid only: artifacts"), which stops the page loading or fetching anything but its own files. It doesn't stop the page navigating itself away, a meta refresh, WebRTC or DNS prefetch: that is the client's part.
- **The client contract** for showing an artifact page (every app, the web one too):
  - Render it through a scheme handler (or a local proxy) that fetches each file from obid with the token and passes obid's `Content-Security-Policy` header on unchanged, or in a frame with `sandbox="allow-scripts"`. Never `allow-same-origin`, never with obid's API origin.
  - Never load its HTML with `blob:` URLs, `srcdoc` or `loadHTMLString` outside such a sandbox: those drop obid's header.
  - Deny every navigation in the web view's delegate (`WKNavigationDelegate` on Apple, `WebViewClient` on Android): only the artifact's own files load; a link the user taps may open outside the viewer, in the browser, after they confirm.
  - Load it with network access denied where the platform can (WebRTC off, no DNS prefetch).
- Sharing outside Oby (#387) comes after.

## 5. Events

Besides `step`, `thinking`, `reply` and `turn` (unchanged):

- `delta`: `{conversation, turn, text}`, the agent's words as they're written. Apps append them; the turn's final text replaces them.
- `step` gains `tool` (`read`, `search`, `web_search`, `web_fetch`, `bash`, `write`, `agent`, `oby_*`) and `detail` (the file, query, URL or command, secrets taken out), so the apps show what Oby is doing like Claude Code's tool lines.
- `artifact`: `{id, version, title, kind, conversation, job}`.

## 6. Guardrails (Claude Code's, kept)

- The agent never publishes, pushes, merges or holds a code-host login; Oby's publish gate is the only way out, as for jobs.
- What it reads from the web or a repository is data. It can't act past Oby's tools, and those keep obid's rules (section 3).
- No vault secret, token or agent login ever reaches the room, a tool result, an event or an artifact; obid's redaction runs on everything it passes on.
- Every turn has a budget (time and the model's turns); Stop ends it and says what was done.
- **Known gap: the web is a way out.** A conversation runs no code, and work it plans after reading anything untrusted waits for the owner, but WebFetch itself still goes anywhere: after a prompt injection (from a page, a repository's code, a job's report or an attachment) the agent could put what it read in the room or the repositories' checkouts into a URL it fetches. Secrets obid knows are redacted from what reaches the agent and from its words, but the code and text it reads are not. The next step is a domain allowlist for WebFetch (and WebSearch's results), set by the owner, with everything else refused.

## 7. Order of work

1. **Core**: the conversation session (Claude Code), its room and sandbox, `delta` and tool `step` events, Oby's tools for conversations, the fallback. API in docs/runner-api.md, "obid only: the conversation harness".
2. **Artifacts**: the store, the API and the `artifact` event, `oby_artifact`, jobs' `.obi/artifacts/`, CSP.
3. **Apps**: streamed words, tool lines, artifact cards and the viewer, on the iPhone and the Mac (web after).
4. **Codex and Cursor over ACP** as the conversation's agent, with the same Oby tools, chosen at runtime (docs/contracts/conversation-agent.md). Cursor's can't be held as the others' are, so it runs under stricter rules there; proven against a stand-in, not yet against a real `cursor-agent`.
