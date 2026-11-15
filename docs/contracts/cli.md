# The `oby` command: obid from a shell, with fixed exit codes

The business behavior contract for the `oby` command, from [the template](../business-contract-template.md). Roadmap task R12-01, milestone R12 Developer workflows.

## Feature and status

- **Outcome:** A script, a CI step or a git hook runs `oby run obi-ai-bot/oby "fix the lint" --wait` and gets one exit code it can trust, with no hand-written `curl` and no token on the command line.
- **Status:** **Implemented** (task R12-01): a Rust crate of its own, `tools/oby`, beside `tools/oby-check-blok` and outside obid's workspace, as the owner chose (a Rust crate in `tools/`, outside `obid_lines_limit`, the check given a branch that tests it). CLI-1 to CLI-8 are implemented, each a test in `tools/oby/tests/cli.rs`.
- **Scope:** `oby status`, `oby jobs`, `oby job`, `oby run`, `oby wait`, `oby stop` and `oby routine run`, their output and their exit codes. Outside it: `oby inbox` and `oby answer` (R12-02), `--payload` on `oby routine run` (R12-04), pairing a phone from the shell (R12-14), an `oby mcp` command, a runner that isn't on this machine, `https`, a token given any way but obid's `api-token`, and shipping `oby` in the app's download or the Linux installer.

## Behavior

### Actors and ownership

- **The person or their script** runs `oby`. It is the same user obid runs as, on the same machine.
- **obid** owns everything `oby` shows and starts: the jobs, their states, the inbox and the routines, on its existing routes. `oby` adds no route, key, state or rule to obid; it is a client of [jobs.md](../api/jobs.md) (`POST /v1/jobs`, `GET /v1/jobs`, `GET /v1/jobs/{id}`, `/wait`, `/stop`), [inbox.md](../api/inbox.md) (`GET /v1/inbox`), [routines.md](../api/routines.md) (`POST /v1/routines/{id}/run`), [capabilities.md](../api/capabilities.md) (`GET /v1/status`) and [account-limits.md](../api/account-limits.md) (a job's `accountLimit`).
- **`oby`** decides only how its own run ends: the exit code, chosen in one pure function (`exit_for` in `tools/oby/src/exit.rs`) from the job's status, whether it waits for the owner, and whether the deadline passed.

### Triggers and prerequisites

- **Where obid is.** Its address is `OBID_URL`, else `http://127.0.0.1:7777`. Its home is `OBID_HOME`, else `~/Library/Application Support/Oby/Runner` on macOS and `~/.local/share/obid` elsewhere ([install.md](../install.md)).
- **The address** is taken only as `http://<host>[:<port>]`, then at most a `/`, where the host is exactly `127.0.0.1`, `localhost` or `[::1]`. `oby` connects to that loopback address itself (`localhost` is 127.0.0.1, never looked up), and sends that host and port as `Host`, which obid checks.
- **The token** is `<home>/api-token`, read under the client rules [configuration.md](../api/configuration.md) states ("The Mac app on the same Mac"): not a link, a regular file, owned by the current user, no group or other permission bits, at most 4096 bytes, and at least 32 visible ASCII characters with only a trailing CR/LF. It is opened without following a link and judged again as opened.
- **The command line** is judged whole first: a usage error reads no token and makes no call.

### Commands

| Command | Route | Prints | Ends |
| --- | --- | --- | --- |
| `oby status [--json]` | `GET /v1/status` | `engine=` and `version=` (`runner`'s) | 0 on a 2xx; 5 on `503` while obid starts |
| `oby jobs [--json]` | `GET /v1/jobs?limit=50` | one `id= status= repo=` line per job | 0 on a 2xx |
| `oby job <id> [--json]` | `GET /v1/jobs/{id}` | `id=`, `status=`, `repo=`, `branch=`, `pr=`, `step=` | 0 on a 2xx |
| `oby run <repo> <task\|-> […]` | `POST /v1/jobs` | `job=<id>` | 0 on `201` or `200`; with `--wait`, as `oby wait` |
| `oby wait <id> [--timeout D] [--json]` | `/wait`, `GET /v1/jobs/{id}`, `GET /v1/inbox` | `job=` and `status=`; on 3, `inbox=` and `kind=` too | below |
| `oby stop <id> [--json]` | `POST /v1/jobs/{id}/stop` | `job=` and `status=` | 0 on a 2xx |
| `oby routine run <id> [--json]` | `POST /v1/routines/{id}/run` | `job=<job.id>` | 0 on `201` |

- A value's newlines print as spaces; a missing or null value prints as nothing. With `--json`, stdout is obid's JSON as received.
- `oby run` sends `{"job": {"repo", "kind", "task", …}}`: `--kind` is `fix` unless given; `--agent`, `--account`, `--mode`, `--check` and `--title` go in only when given, so obid's own defaults apply; `--id KEY` goes as `job.clientRequestId`, obid's request key (RECEIPTS-5 to 7), so a retry with the same key gets the same job. A task of `-` is standard input, its trailing whitespace cut.
- `--timeout D` is a positive whole number and `s`, `m` or `h`. On `oby run` it needs `--wait`. With none, a wait lasts until the job ends or waits for the owner.
- `oby --help` (or `-h`) prints the usage on stdout and ends 0.

### States and exit codes

| Code | Meaning |
| --- | --- |
| 0 | Done: the job ended `done`, or the command did what it asked. |
| 1 | Failed or stopped: the job ended `failed`, `stopped` or `interrupted`, or obid answered anything else (a `4xx` but `401`, a `5xx`), its `error` on stderr. |
| 2 | Usage: the command line isn't one `oby` takes; the usage is on stderr. |
| 3 | Waits for the owner: an open inbox item of kind `permission`, `question`, `agentQuestion`, `plan`, `confirm` or `limit` names the job, or its `accountLimit.state` is `pending`. |
| 4 | Timed out: `--timeout` passed. The job goes on. |
| 5 | obid isn't reachable or no token is usable: no loopback address, no connection, a broken one, a token file the rules refuse, a second `401`, or `GET /v1/status` answering `503`. |

- **Waiting** (`oby wait`, `oby run --wait`) holds `GET /v1/jobs/{id}/wait` open on a thread of its own, its spaces ignored, and every 2 seconds reads `GET /v1/jobs/{id}` and `GET /v1/inbox`. It ends at the first of: a status that ends the job, from either; an ask of the owner's (code 3 above); the deadline.
- **`exit_for(status, asks, timed_out)`** decides it, in order: `done` is 0; `failed`, `stopped` or `interrupted` is 1, whatever else holds; any other status (`queued`, `running`, or one obid adds later, still going) is 3 when the job asks its owner, else 4 when the deadline passed, else keep waiting.
- A job that waits by itself for an account with room (`resumesAt`, no `accountLimit`) isn't waiting for the owner: only `--timeout` ends that wait.
- On an end it prints `job=` and `status=` (obid's latest status), and on 3 `inbox=<item id>` and `kind=<kind>`; for a pending `accountLimit` the item is its `decision` and the kind `limit`. With `--json`, stdout is obid's latest Job alone and the `inbox=` and `kind=` lines go to stderr.

### Failures and recovery

- **obid not listening, or a connection that breaks:** 5, saying where it looked. Run it again once obid is up.
- **obid starting:** `GET /v1/status` answers `503`, and `oby status` ends 5; every other call waits for obid as obid holds it.
- **A token refused (`401`):** every call goes through one send helper, which reads `api-token` again under the same rules and sends the same call once more, so a token obid rotated is picked up. A second `401`, or a file the rules now refuse, is 5.
- **A token file refused:** 5, naming the file and the rule it breaks (`chmod 600`, a link, missing), never what it holds.
- **The same `oby run --id KEY` sent twice:** obid answers the first `201` and the next `200` with the same job, and both print its id. `oby` itself never sends a `POST` again: the key makes the caller's own retry safe.
- **A timeout** stops nothing: the job runs on, and `oby wait <id>` picks it up again.

### Invariants and boundaries

- **The token only to loopback, never printed.** It goes only to 127.0.0.1, localhost or [::1], only in the `Authorization` header: never in an argument, an environment variable `oby` sets, stdout, stderr or `--json` output, and the type that holds it has no `Debug` or `Display`. An address that isn't loopback is refused before the token file is read or any connection is made.
- **No proxy.** `oby` connects straight to the address: it reads no `http_proxy`, `https_proxy`, `HTTP_PROXY`, `HTTPS_PROXY` or `ALL_PROXY`, and follows no redirect.
- **No signature, so no declared secrets.** `oby` sends no `X-Oby-Words`: the jobs it starts are unsigned, and get none of the owner's declared secrets ([jobs.md](../api/jobs.md), `POST /v1/jobs`). It approves nothing and answers no inbox item.
- **Nothing new in obid.** Every rule stays in obid's existing routes; `oby` neither writes nor makes `api-token`.
- **The CLI's words** are a terminal's, with no screen of Sangam's behind them.

## Acceptance scenarios

Each is a test in `tools/oby/tests/cli.rs`, run against a stub runner on loopback with a `0600` `api-token` in `OBID_HOME`.

- **CLI-1:** Given obid runs and `api-token` is a 0600 file of the user's, when `oby run <repo> <task> --wait` starts a job that ends done, then it prints the job id and exits 0. (`cli_1_a_done_job_exits_0`)
- **CLI-2:** Given a job that ends failed or stopped, when `oby wait <id>` returns, then it exits 1. (`cli_2_a_failed_or_stopped_job_exits_1`)
- **CLI-3:** Given unknown flags or a missing task, when `oby run` runs, then it prints usage and exits 2 without a call to obid. (`cli_3_a_usage_error_exits_2_without_a_call`)
- **CLI-4:** Given a job that opens a permission, question or limit card, when `oby wait` sees it, then it exits 3 and names the inbox item. (`cli_4_a_job_waiting_for_the_owner_exits_3_naming_the_item`)
- **CLI-5:** Given `--timeout` and a job still running after it, when the time ends, then it exits 4 and the job keeps running. (`cli_5_a_timeout_exits_4_and_the_job_goes_on`)
- **CLI-6:** Given obid is not listening or `api-token` is missing, readable by others or a link, when any command runs, then it exits 5 and never prints the token. (`cli_6_no_obid_or_no_usable_token_exits_5_and_never_prints_it`)
- **CLI-7:** Given `--id k1` sent twice, when both calls reach obid, then one job exists and both print its id. (`cli_7_one_key_sent_twice_makes_one_job`)
- **CLI-8:** Given a token that obid rotated while `--wait` ran, when obid answers 401, then the CLI reads the token once again and goes on. (`cli_8_a_rotated_token_is_read_again`)

`exit_for` is tested on every input (7 statuses × 2 asks × 2 timeouts) against a table in `tools/oby/src/exit.rs`; the loopback rule and each token-file rule have unit tests in `src/endpoint.rs` and `src/token.rs`. The check runs fmt, clippy and these tests whenever `tools/oby/` changes (`scripts/oby-check.sh`).

Test it when it's done, on a real Mac: `oby run obi-ai-bot/oby "say hello" --kind investigate --wait; echo $?` prints 0.
