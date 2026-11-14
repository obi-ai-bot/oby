# Logs: no secret reaches obid's log, whoever wrote the line

The business behavior contract for what obid's log writer takes out of each line, from [the template](../business-contract-template.md). The API note is `docs/runner-api.md` ("obid's own bugs (#914, #911)").

## Feature and status

- **Outcome:** A secret obid holds never reaches obid's log through its log subscriber, whoever wrote the line. Until now a line was safe only when whoever wrote it remembered to redact it.
- **Status:** **Implemented** (LOGS-1, LOGS-2), roadmap task R02-14.
- **Scope:** obid's `tracing` log on stderr (on a Mac, the launch agent sends it to `~/Library/Logs/obid.log`, docs/install.md). Lines obid writes with `eprintln!` outside tracing don't pass through the writer: the panic hook redacts its own line with `try_redact` (#911), and `fatal`, the usage text and the log rotation message don't redact. This contract doesn't promise every line on stderr.
- **Out of scope:** per-call redaction of `/v1` answers, events and pushes (unchanged); the secret scan's built-in rules applied to log lines (#1032).

## Behavior

### Actors and ownership

- **obid** owns it: its log subscriber writes through one writer, `crate::secrets::Redacting`, around stderr. No model turn and no click.
- **The apps** change nothing: they show and send nothing new.

### Rules

- The writer takes each event whole: it holds every byte the subscriber writes for one event and writes nothing until the event ends (it's flushed or dropped). A line that reaches it in several writes, or a message over several lines, is redacted as one text.
- It takes out every secret in `secrets::known()`, in every form the set knows (plain, base64 standard or URL-safe, hex in either case, URL-encoded), each replaced by `[redacted: <name>]` or the set's own marker.
- It redacts with the values the set learned last: it reads no vault and waits on no lock of the vault's or the set's, so a line logged while one is held can't hang obid. A value `obid vault add` put there joins the log's set the next time obid redacts anything else with the set (a `/v1` answer, a push), which reads the vault again.
- Bytes that aren't UTF-8 are read lossily before they're redacted.

### Failures and recovery

- **The set can't be read at once** (its lock is held, or poisoned): the event is withheld, and the log gets one fixed line in its place, `obid: a log line was withheld: obid couldn't take its secrets out of it`, holding nothing of the event. Never the event's text unredacted.
- **The writer emits no log line of its own**, so it can't recurse.

## Acceptance scenarios

- **LOGS-1.** Given a value V obid holds, when obid logs a line holding V plain, base64, hex or URL-encoded, then what reaches the log holds none of them, `[redacted: <name>]` in each one's place, and the rest of each line. Test: `logs_1_every_form_of_a_secret_is_taken_out` (core/crates/obid/src/secrets.rs).
- **LOGS-2.** Given a log line that reaches the writer in two writes with V split across them, then nothing is written before the event ends, and then the line is written once with V taken out, neither half of it left. Test: `logs_2_a_secret_split_across_writes_is_taken_out` (core/crates/obid/src/secrets.rs).
