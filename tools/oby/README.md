# oby

The `oby` command: obid's loopback API from a shell, a script, a CI step or a git hook, with one exit
code a script can trust. Its contract is [docs/contracts/cli.md](../../docs/contracts/cli.md), and how to
use it is in [docs/install.md](../../docs/install.md#command-line), "Command line".

```sh
cargo install --locked --path tools/oby
oby run obi-ai-bot/oby "fix the lint" --wait; echo $?
```

- Exit codes: 0 done, 1 failed or stopped, 2 usage, 3 waits for the owner, 4 timed out (the job goes
  on), 5 obid not reachable or no usable token. `exit_for` in `src/exit.rs` picks them.
- It reads obid's token from `api-token` in obid's home (`OBID_HOME`, else obid's default) under the
  client rules obid documents, and sends it only to `OBID_URL` (`http://127.0.0.1:7777` unless set),
  which must be 127.0.0.1, localhost or [::1]. No proxy, no redirect, and it never prints the token.
- It is a client of obid's existing routes and nothing more: it signs nothing, approves nothing and
  answers nothing.

A crate of its own, outside obid's workspace and `obid_lines_limit`. `scripts/oby-check.sh` runs its fmt,
clippy and tests (`tests/cli.rs`, CLI-1 to CLI-8 against a stub runner) whenever `tools/oby/` changes.
