# oby-connectors

One pattern for every connector, and the test that holds every adapter to it. It starts with code
hosts (GitHub, GitLab, Bitbucket, Forgejo…): what Oby knows about a host without asking it, what
it asks a host, and the conformance suite every adapter must pass before it merges.

No I/O and no async runtime: plain data and pure functions, with no dependencies. obid's adapters
do the I/O (a CLI, HTTP) behind these signatures. The rules it serves are in
`docs/contracts/connector-pattern.md` (proposed, lane 0.3).

## Run the suite on a new adapter: one line

```rust
oby_connectors::conformance_tests!(oby_connectors::conformance::Subject::new(MyDefinition::new, my_host_on_a_fake));
```

That writes one `#[test]` per case, each named `conformance_<case>`. `Subject::new` takes two
functions: one builds the adapter's definition for the `HostNames` the suite picks (a public host
and an Enterprise one), the other builds its `CodeHost` over a fake of its own service, seeded from
the suite's `Fixture`. The fake must behave as the service does:

- it answers in the service's own words for states, and pages as the service does, 100 at most;
- it answers a change request that doesn't exist with an error echoing `Fixture::secret`, in a URL
  and bare, as a CLI echoing a remote with a token does;
- it writes every request it gets into `Fixture::requests`, which is how the suite sees a call that
  was made.

`tests/reference/mod.rs` is a whole example: a made-up forge, its definition, and its host over a
fake service.

## What's here

| | |
|---|---|
| `CodeHostDefinition` | kind, label, nouns, sign-in, `Capabilities`, nested groups; URL builders and parsers for change requests, issues and commits; `recognize_remote`; the host's own state words and what each maps to. |
| `Capabilities` | merge queue, auto-merge, drafts, required approvals, CODEOWNERS, a reviewer account, check logs, rerun: bools, no host names. |
| `CodeHost` | viewer, can push, list (paged) and get change requests, checks, reviews, merge; and the capability-gated calls, which answer `ConnectorError::Unsupported` without reaching the host. `list_all` follows every page; `require` gates a call. |
| `words` | the only words a state may map onto: obid's own, GitHub's (`OPEN`, `CLOSED`, `MERGED`; `REVIEW_REQUIRED`, `CHANGES_REQUESTED`, `APPROVED`; merge states, checks, mergeability). The API is add-only: other hosts map onto these, never add one. A draft is `OPEN` with merge state `DRAFT`; "rebase required" is `OPEN` with `BEHIND`. |
| `remote` | a bounded reader of remotes and web URLs, and `HostNames::recognize`, which every definition can use. |
| `Detail` | a host's error text made safe: a URL's credentials taken out, cut to 2,000 bytes; `redact` takes out the token the adapter holds. |

## The cases

Each is a function that answers what was wrong, and a test. The reference host passes all of them;
each broken fake in `tests/conformance.rs` fails the case that must catch its bug, and no other.
Most are seeded with bugs T3 Code's host packages kept fixing.

| Case | What it holds | Broken fakes it rejects | Lane 0.3's draft |
|---|---|---|---|
| `urls_round_trip` | change request, issue and commit URLs parse back to what built them, with a trailing path, query or fragment, on a public and an Enterprise host; one kind's URL isn't read as another's | `DropsTrailingPath` | |
| `remote_recognition` | HTTPS, scp-style and `ssh://` remotes, an SSH host that isn't the web host, an Enterprise hostname, ports, `.git`, nested groups where the host has them; other hosts and lookalikes refused | `MisparsesSsh`, `IgnoresSshHost` (forgejo c6a120a958), `AcceptsLookalikeHost` | CONNECTOR-20, 21 |
| `states_map_to_api` | each of the host's words maps to exactly one of obid's; none invented; open, closed and merged all reachable; an unknown word maps to none, never a guess | `InventsState`, `GuessesUnknownState`, `MergedAsClosed` (azure 60e54867fc) | CONNECTOR-22 |
| `pagination_past_100` | 250 change requests come back whole and in order, filtered too; a repeated page fails as partial | `CutsAt100` (9f815c94af), `RepeatsPage` | CONNECTOR-23 |
| `unsupported_capability` | an undeclared capability answers `Unsupported` naming it, and its request is never made; a declared one is asked | `AttemptsUnsupported`, `FailsInsteadOfUnsupported` | CONNECTOR-29 |
| `bounded_parsing` | hostile input (1 MiB paths, 100,000 segments, overflowing numbers, NULs, colons) is refused or read within 1 second in all, never panics, and nothing longer than 8 KiB is read out of it | `UnboundedParse` (f1cd57bbf9); `GuessesUnknownState` too | |
| `no_secret_in_output` | the token the adapter holds is in no recognized repository, built URL, listed change request or error, even when the fake echoes it | `LeaksSecret` | CONNECTOR-26 |

Not here yet, from lane 0.3's draft: a merge parsed (CONNECTOR-24), the outgoing secret scan
(CONNECTOR-25), fenced text (CONNECTOR-27) and failure isn't emptiness (CONNECTOR-28). Issue
trackers, knowledge and agents get their own definition traits and cases when their lanes build
them.

## Where it differs from T3 Code

T3 Code (`packages/source-control-core`) has the shape this follows, and no conformance suite: its
`source-control-testing` package holds two fakes, and about half of each host package's commits are
fixes.

- **Definition.** T3's `SourceControlDefinition` is a record of functions per kind, with
  Enterprise hosts in settings. Here it is a trait over a configured value built from `HostNames`,
  so an Enterprise hostname is the same code path as the public host, never a special case.
- **Remote recognition** is written once (`HostNames::recognize`), with SSH hosts named
  explicitly, rather than re-derived per package.
- **States** map onto obid's existing upper-case words (T3 has its own lower-case literals): obid's
  API is add-only and the Swift apps decode these as closed enums. T3's `rebaseRequired` flag and
  `isDraft` become merge states.
- **Capabilities** are a struct of the bools obid acts on. T3 lists actions and merge methods and
  has clients hide what's missing; here a missing capability is a typed `Unsupported` answer, so obid
  can say what it skipped.
- **Errors** keep T3's reasons (missing tool, unauthenticated, forbidden, not found, rate limited,
  failed), add `Unsupported`, and carry a `Detail` that can't hold a URL's credentials.
- **The driver** (T3's `PullRequestProvider`, Effect and streams) is synchronous here, over plain
  data: obid wraps the I/O.
