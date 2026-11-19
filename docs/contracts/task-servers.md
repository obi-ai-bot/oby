# A task's local servers

The business behavior contract for showing the owner the local servers a task's agent started, and opening one, from [the template](../business-contract-template.md). Scenarios are numbered `TASKSERVERS-n`; the tests that prove them are named after them: obid's in `core/crates/obid/tests/task_servers.rs`, `core/crates/obid/src/listening.rs` and `core/crates/obid/src/jobs/servers.rs`, the app's in `apps/electron/src/renderer/src/test/taskServers.test.tsx`.

## Feature and status

- **Outcome:** When an agent starts a dev server (`npm run dev`, `vite`, `python -m http.server`), the owner sees it on the task, `localhost:5173 · Open`, and one click opens it in their own browser, as T3 Code's thread lists the servers a thread started. Nobody hunts for the port in the agent's output.
- **Status:** **Implemented.** Sangam has no screen for it yet: the line follows T3 Code's servers list, and its words (`localhost:<port>`, **Open**) are new, all his to change.
- **Scope:** obid's reading of what a running job's agent listens on, the job's `servers` key and its events; the Electron app's line on the task's page and in its session's header, and main's Open. **Out of scope:** a browser inside Oby, preview tunnels to another machine, stopping a server from Oby, servers of a check (the repository's tests), and the iPhone.

## Behavior

### Actors and ownership

- **obid** reads, while a job's worker runs, which TCP ports the job's own agent's processes listen on, keeps them on the job (`Job::servers`, never in its file) and sends the job when they change. It opens nothing.
- **Main** (`src/main/servers.ts`) opens `http://localhost:<port>` in the owner's default browser, right after a click main saw, only for a port obid reports for that job now.
- **The page** shows the line and asks main to open one; it sends only the job's id and the port, never a URL.

### What counts as a task's server

- A job's **agent's processes** are its agent's session leader, which obid started for it, every process descended from it, and every process still in its process group (one whose parent ended), as obid's meter counts its memory (crate::meter). Nothing else: not another job's, not the check's, not obid's, not the owner's other programs.
- A **server** is a TCP port one of those processes listens on, on a loopback address (`127.0.0.1`, `::1`) or every address (`0.0.0.0`, `::`): one the Mac reaches as `localhost`. A listen on another address alone isn't one.
- obid reads on its meter's tick, every 5 seconds and only while a job runs: one `lsof -nP -a -iTCP -sTCP:LISTEN -Fpcn -p <every running job's agent's processes>` on macOS (some 20 ms), `/proc/net/tcp{,6}` and each process's `/proc/<pid>/fd` on Linux. A read that fails changes nothing.
- A job's `servers` lists each port once, in order, with the name of the process that listens (its executable's; the lowest process id's, when several do) and when obid first saw it. A port no longer listened on goes at the next reading; all go when the job's worker ends (done, stopped, failed, waiting on the owner with its agent ended).
- **Oby's box.** On macOS the box (Seatbelt) shares the Mac's network and leaves a listen alone: a boxed job's server is reachable and listed. On Linux the box (bubblewrap) has a network namespace of its own: the host doesn't reach it, its sockets aren't in the host's table, and it isn't listed.

### The line and Open

- A task whose job has servers shows one quiet line on its page, above its card, and in its session's header: `localhost:<port> · Open` for each. A job with none shows nothing.
- **Open** asks main with the job's id and the port. Main asks obid for that job and opens `http://localhost:<port>` with the Mac's default browser only when obid lists that port on it now. Never a URL from an agent's words, a job's text or the page.

### Decisions and invariants

- **Only the job's own processes.** obid reads the listens of a running job's agent's processes alone, and lists them on that job only.
- **Only localhost, only ports obid reported.** Main builds the URL itself from a port number; the page can't name a host, a scheme or a path.
- **Add-only API.** The job gains `servers`, present only while it has one.

### Failure and recovery

- `lsof` missing, too slow (2 seconds) or failing: the job keeps what it had; the next tick reads again.
- A server that daemonizes out of the agent's process group and tree isn't the job's: not listed.
- Open for a port obid no longer lists, a job it doesn't have, or no click main saw: main opens nothing.

## Interfaces and dependencies

The job's `servers`: [docs/api/jobs.md](../api/jobs.md). The app's IPC: `open-task-server` (main).

## Acceptance scenarios

- **TASKSERVERS-1** A running job whose agent's child listens on 127.0.0.1 lists that port in `servers`, with the process's name (`node`) and `firstSeen`, within two meter ticks, and a `job` event carries it. On Linux the name is the executable's (`/proc/<pid>/exe`), else the command line's first word, else `comm`, which is the main thread's name (Node's is `MainThread`). (`task_servers::taskservers_1_and_2_a_running_jobs_server_shows_and_goes_when_it_closes`, `listening::tests::taskservers_1_a_linux_listener_is_named_after_its_executable`)
- **TASKSERVERS-2** Once that server stops listening the port goes from `servers` while the job still runs, and a `job` event says so; a port seen before keeps its `firstSeen`. (`task_servers::taskservers_1_and_2_a_running_jobs_server_shows_and_goes_when_it_closes`, `jobs::servers::tests::taskservers_2_first_seen_is_kept_and_closed_ports_go`)
- **TASKSERVERS-3** A job that ends (stopped) has no `servers`, while another still running keeps its own. (`task_servers::taskservers_3_and_4_only_its_own_and_none_once_it_ends`)
- **TASKSERVERS-4** Two jobs serving at once each list only their own agent's port, and neither lists a port another process of the Mac listens on. (`task_servers::taskservers_3_and_4_only_its_own_and_none_once_it_ends`)
- **TASKSERVERS-5** Only a listen `localhost` reaches is a server: `*`, `0.0.0.0`, `127.0.0.1`, `::1`, `::` and IPv4-in-IPv6 loopback count; `192.168.1.20`, `10.0.0.5` don't; Linux's table is read in the machine's byte order, listening sockets only. (`listening::tests::taskservers_5_only_listens_localhost_reaches`, `listening::tests::taskservers_5_reads_the_linux_table`)
- **TASKSERVERS-6** A boxed job's server (Codex in Sandbox) is listed on macOS, where the box shares the Mac's network, and never on Linux, where the box has a network namespace of its own. (`task_servers::taskservers_6_a_boxed_jobs_server_is_listed_only_where_the_mac_reaches_it`)
- **TASKSERVERS-7** A task whose job lists servers shows `localhost:<port>` and **Open** for each on its page and in its session's header; one with none shows no line. (`taskServers.test.tsx`, `TASKSERVERS-7`)
- **TASKSERVERS-8** Open sends only the job's id and the port; main opens `http://localhost:<port>` only after a click main saw and only when obid lists that port on that job now; a port obid doesn't list, a job it doesn't have, or a page's own call opens nothing. (`taskServers.test.tsx`, `TASKSERVERS-8`)
