# Linux host: a runaway build goes before obid

The business behavior contract for how a Linux obid keeps running when what it started runs the machine out of memory, from [the template](../business-contract-template.md). The installer is [install.md](../install.md) ("What it does"); the API side is `docs/runner-api.md` ("Whatever the machine's memory", LINUX-2).

## Feature and status

- **Outcome:** On a small Linux box, a runaway build is killed before obid is, and one killed agent doesn't stop the service, so the other jobs, the Sphere link and the phone relay go on.
- **Status:** **Implemented**, LINUX-1 and LINUX-2 ([why](../decisions/2026-11-13-roadmap-answers.md#r09-12-q040)).
- **Scope:** What obid owns: the `oom_score_adj` of what it starts through `obid __session`, and the systemd user unit `install/install.sh` writes. **Out of scope:** any memory admission or wait: nothing waits for memory (`docs/runner-api.md`, "Whatever the machine's memory"). The score of what obid starts without `__session` (sign-ins, `security`, the login-shell read, git, gh), and of obid itself at runtime. The Mac's launch agent and macOS's memory behaviour.

## Behavior

- **Actors and ownership:** the installer writes the unit; obid's `__session` sets the score of what it starts; the kernel picks what to kill. Clients show and send nothing new.
- **Triggers:** installing or updating obid on Linux writes the unit; obid starting a job's agent or check, a repository's setup, a conversation's turn, the brain or a Sphere answer sets the score.
- **Decisions:** none at runtime: the numbers are fixed, with no setting, switch or API field.
- **Observable results:** `systemctl --user show obid -p MemoryMax,OOMPolicy`, and `/proc/<pid>/oom_score_adj` of an agent and of everything it started.
- **Failure and recovery:** a score `__session` can't read or write leaves it as it is, and the program still runs. A user manager may refuse or clamp the unit's negative `OOMScoreAdjust`, and systemd goes on with the unit then, so obid always starts; `MemoryMax` is ignored where the user manager has no memory controller delegated.
- **Invariants:** what obid starts through `__session` sits at an `oom_score_adj` of 500 or above, above obid's own whatever the user manager made that, and nothing obid starts lowers it.

## Interfaces and dependencies

`process::exec_in_own_session` (core/crates/obid/src/process.rs), run as `obid __session` ahead of everything `Sandbox::wrap` and `Sandbox::unboxed` start; the unit's heredoc in `install/install.sh`. No API, client or settings change.

## Acceptance scenarios

- **LINUX-1:** Given the installer on Linux, when it writes the unit, its `[Service]` section has `MemoryMax=90%` (obid and everything it started share at most 90% of the machine's memory), `OOMPolicy=continue` (the kernel killing one of its processes doesn't stop the service) and `OOMScoreAdjust=-500` (obid is the kernel's last choice, where the user manager allows it), and `systemd-analyze --user verify` accepts the file where there is one. Running the installer again rewrites the unit, so an existing install gets the lines that way. The catch: an unprivileged user manager may clamp or ignore the negative score, so obid's own may stay at the manager's. (`install::linux_1_the_unit_protects_obid`.)
- **LINUX-2:** Given obid on Linux, when it starts a job's agent or check (or a setup, a conversation's turn, the brain, a Sphere answer) through `__session`, that process runs at an `oom_score_adj` of 500, raised before it starts, inside bubblewrap too, and so does everything it starts, so a runaway build is the kernel's first choice and obid its last. A score already above 500 is never lowered. The catch: an obid that already runs above 500 (a container's default, say) starts what it starts at its own score, not above it. Proven on Linux CI (`ci.yml`, `rust-test`) or tetrix-lab; on the Mac the test is compiled out. (`sandbox::linux_2_agents_are_the_first_to_go`.)

Test it when it's done, on tetrix-lab: run the installer again, then `systemctl --user show obid -p MemoryMax,OOMPolicy`, and `cat /proc/<agent pid>/oom_score_adj` while a job runs.
