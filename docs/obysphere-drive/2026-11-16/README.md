# Two Obys on one Obysphere, end to end (2026-11-16)

Oby A (owner `ana`) asks Oby B (owner `ben`) a question, then hands it a task; Ben answers and accepts in B's app, B runs the task in the foreign lane, and A sees the answer and the result. Contracts: [obysphere.md](../../contracts/obysphere.md), [peer-thread.md](../../contracts/peer-thread.md), [handoff.md](../../contracts/handoff.md).

## How it ran

- **The Obysphere:** a local one on `127.0.0.1`, obi-ai-bot/oby-sphere's own server (`server/scripts/local.mjs`) at branch `feat/room-on-live`, the commit Deskree's Sphere runs. Its admin made Obysphere `demo` ("Demo team"), the policy below, and one open link.
- **Two obids** of this branch, each with a home and ports of its own, on the owner's real Claude Code, Codex and Cursor logins; one scratch repository, `demo/greeter`, with a local bare remote and `node test.mjs` as its check.
- **Two Electron apps** of this branch (`npm run build`), each with its own app folder, pointed at its obid with `OBID_PORT` and `OBID_HOME`, driven through DevTools with Playwright (real clicks). Two harness stand-ins, both outside the product: the link reached main's `open-url` as macOS delivers an `oby://` link, and the native "Join Demo team?" dialog was answered Join by the harness.
- **The handoff** left A through obid's API (`POST /v1/obid/sphere/requests`, `type: "handoff"`): no screen sends one yet.

## What it showed

| | |
|---|---|
| `01-a-joined.jpg` | A joined by the app's own join: Demo team in the side panel, ben's Oby a member. |
| `02-b-question.jpg` | A asked from ben's thread; B's Home: "Question from ana", quoted, with Answer and Decline. |
| `03-a-answer.jpg` | Ben answered in B's app; A's thread shows it, "ben answered this one, not its Oby". |
| `04-b-task-card.jpg` | A handed B a task; B's Home: "Task from ana", the goal quoted, with Accept and Decline. |
| `05-b-task-ran.jpg` | Ben accepted. B ran it as an untrusted build job (boxed, foreign lane), the request only in `.obi/inputs/handoff.json`; its check passed and B pushed `oby/build-57ac187a`. |
| `06-a-result.jpg` | A's thread shows the task and B's result. The result: `outcome: done`, `tests_pass` met, `head_branch`, `head_sha`, the agent's summary through B's publish gate. |

## Tomorrow, on Deskree's Obysphere

1. **The admin (Dmytro) makes one open invite link** on `https://obysphere.deskree.com` for a Sphere whose **policy lets members send `question` and `handoff`** (and `review_request` if wanted), for example one rule `{"id": "members-ask", "from_owner": "*", "to_owner": "*", "actions": ["question", "review_request", "handoff"], "repos": ["*"]}`. A policy without `handoff` refuses the task `policy_denied`. The link's form: `https://obysphere.deskree.com/join#sphere=<id>&name=<name>&by=<admin>&id=<invitation id>&secret=<secret>`, with at least as many uses as Obys joining.
2. **Each Oby opens it:** the packaged app opens `oby://join/obysphere.deskree.com#<the same fragment>` from the join page's Open in Oby, and the owner clicks Join. An Oby that joined before this change leaves the Sphere first and opens the link again: its old card doesn't take `handoff`.
3. **Run the flow again:** ask from a member's thread; send the task with obid's API on the asking Mac:

   ```bash
   curl -s -X POST -H "authorization: Bearer $(cat "$OBID_HOME/api-token")" -H 'content-type: application/json' \
     http://127.0.0.1:7777/v1/obid/sphere/requests \
     -d '{"to": "<their obi id>", "type": "handoff", "expiresIn": 86400,
          "body": {"goal": "<one line>", "repo": "<a repository their Oby works in>", "base_branch": "main",
                   "acceptance": [{"kind": "tests_pass"}]}}'
   ```

   Their obi id is in `GET /v1/obid/sphere/members`. The receiving owner accepts on Home; the result shows in the sender's thread with them.
