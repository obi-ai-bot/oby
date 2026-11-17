# GitHub work: only the repositories the owner connected

The business behavior contract for which GitHub work obid keeps from each read of the owner's GitHub work ([#2402](https://github.com/obi-ai-bot/oby/issues/2402)), from [the template](../business-contract-template.md). Scenarios are numbered `GITHUBWORK-n`; the tests that prove them are named after them (`core/crates/obid/tests/github_work.rs`, obid's lib tests and Kani proofs in `core/crates/obid/src/github_work.rs`). How the feed reads and what it answers: [docs/api/github.md](../api/github.md), "obid only: GitHub work".

## Feature and status

- **Outcome:** The feed, the inbox and routines hold GitHub work only from the repositories the owner connected Oby to, even when the `gh` login sees many organizations. Work anywhere else is left out, unless the owner asks for every repository.
- **Status:** **Implemented** by B00-33 (#2402).
- **Scope:** what each read keeps, and what it then offers and closes. **Out of scope:** narrowing GitHub's search itself (the searches stay `@me` across every organization), which repositories a job may start in, and a way to change the setting in a conversation or in Settings.

## Behavior

### Actors and ownership

- **obid** decides, on each read, which items it keeps (`github_work::keep_connected`, `github_work::keeps`), before it marks, offers, keeps or sends anything.
- **The owner** connects a repository (in `repos` of obid's configuration, by a job that adds it, or by their signed Confirm of an `add_repository` item) and chooses `githubEveryRepo`.
- **The apps** show what obid answers. They decide nothing.

### Rules

- **Connected means listed.** A repository is connected when it is one of obid's listed repositories (`Repos::listed`): the configuration's `repos` and those added since, letter case aside. A repository that is only one `gh` can push to is not connected.
- **What a read keeps.** With `githubEveryRepo` off, a read keeps an item exactly when its repository is connected. With it on, a read keeps every item, as GitHub gave it.
- **The setting.** `githubEveryRepo` on `GET`/`PUT /v1/settings/jobs`, kept in obid's home across restarts. Off when unset.
- **Everything downstream sees only what was kept:** the inbox offers and those waiting for a later read, `GET /v1/github/work` (its lists and `counts`), `home/github-work.json`, `POST /v1/github/work/{id}/start` (`404` for an item left out), routines on GitHub events and the `github` event.
- **Counts.** Each list's count is GitHub's count less the items the read left out of it, so a whole list stays whole and a cut one stays cut.
- **Closing.** An open `github` offer whose item a read left out is done, with no answer, at the next read that is whole (no list cut, no errors besides the data), as an item that leaves the feed is.
- **Newly kept items.** An item a read keeps for the first time (the setting was turned on, or its repository was connected) is new, and offered at most 5 a read with the rest waiting, as any new item. The first read on record still offers nothing.
- **With `githubEveryRepo` on**, a review request in a repository obid doesn't work in is offered as before: a tap adds the repository only on the owner's signed Confirm (#1118).

### Invariants

- The rule holds on what obid keeps, not only on what it asks GitHub: no item left out reaches the inbox, the feed, a routine or an event.
- obid's API only grows: `githubEveryRepo` is a new key, and no key or value changed.

## Acceptance scenarios

- **GITHUBWORK-1:** With `githubEveryRepo` unset, a read serving a review request (#9), an assigned issue (#13) and an own red pull request (#5) in `someone/else`, beside work in `obi-e2e/fixture-app`, keeps none of them: they're in no list of `GET /v1/github/work`, `counts.reviews` excludes #9, no open `github` item is in `someone/else`, and starting #9 with the id an earlier every-repository read gave it answers `404`. (`githubwork_1_work_outside_the_connected_repositories_is_left_out`, github_work.rs; Kani `github_work::proofs::githubwork_1_off_keeps_exactly_the_connected`)
- **GITHUBWORK-2:** In the same reads, a new review request in `obi-e2e/fixture-app` is in `reviews` with `known: true`, and is offered as `Review requested: fixture-app #703` with `Review it`. (`githubwork_2_work_in_a_connected_repository_is_offered`, github_work.rs)
- **GITHUBWORK-3:** After `PUT /v1/settings/jobs {"githubEveryRepo": true}`, a new review request in `obi-e2e/other` is in the feed with `known: false`, offered with `Your review is requested on pull request #12 in obi-e2e/other. Oby doesn't work in obi-e2e/other yet.`; a tap with only the API token answers `422` (`Adding obi-e2e/other waits for your Confirm`), and the owner's signed Confirm adds the repository to `config.json` and starts a `review` job there. (`githubwork_3_reviewing_on_an_unknown_repository_adds_it_only_on_the_owners_tap`, github_work.rs; Kani `github_work::proofs::githubwork_3_on_keeps_every_item`)
- **GITHUBWORK-4:** With the setting on, a review request in `someone/else` is offered and stays open; after `PUT {"githubEveryRepo": false}` and the next whole read, that offer is done with no answer, no longer open, and the item is gone from `GET /v1/github/work`. (`githubwork_4_turning_every_repository_off_settles_its_open_offers`, github_work.rs)
- **GITHUBWORK-5:** `keeps(false, false)` is false and the other three are true; `keep_connected` with every repository off drops the unconnected items of all three lists and lowers each count by as many, a whole list staying whole and a cut one cut, and with it on changes nothing. (`githubwork_5_a_read_keeps_only_the_connected_unless_every_repository`, obid's lib tests)
- **GITHUBWORK-6:** A fresh obid answers `"githubEveryRepo": false` on `GET /v1/settings/jobs`; `PUT {"githubEveryRepo": true}` answers `200` with it true, still true after obid restarts; `PUT {"githubEveryRepo": "yes"}` answers `422` and changes nothing. (`githubwork_6_every_repository_is_a_setting_kept_across_restarts`, github_work.rs)
- **GITHUBWORK-7:** With the setting off, a review request in `obi-e2e/other` makes no offer; once obid restarts with `obi-e2e/other` in `repos`, the next read offers it (`Review requested: other #12`). (`githubwork_7_a_repository_connected_later_is_offered_at_the_next_read`, github_work.rs)
- **GITHUBWORK-8:** Each pull request in the feed carries its head's checks, `checkRuns`: each check run's name or status's context, its state as `checks` counts it (`FAILURE`, `PENDING`, `SUCCESS`) and its page, only an `https` one, at most 50, in GitHub's order; an issue has none, and an answer from before reads as none. (`githubwork_8_a_pull_request_lists_each_check_with_its_state_and_page`, github_work.rs)
- **GITHUBWORK-9:** The app shows a pull request's checks as a list, as T3 Code does: a line saying how many failed or run (`2 of 5 failed, 1 running`), then the failed ones, those running, then those that passed, each with its mark, name, state and Details (its page, https only); while any failed or runs, the passed ones wait behind Show all. The `CI failed` card (on the stack and in Waiting for you, its feed item found by repository and number) and the feed's card show it. (`githubwork_9_failed_then_running_then_passed_each_with_its_state_and_details`, `githubwork_9_all_passed_shows_them_all`, `githubwork_9_the_ci_failed_card_lists_its_pull_requests_checks`, prChecks.test.tsx)

Test it when it's done, on a real Mac: with a `gh` login that sees another organization, ask someone there to request your review on a pull request in a repository Oby isn't connected to, and see that nothing about it reaches Oby's inbox or GitHub work; turn `githubEveryRepo` on with `PUT /v1/settings/jobs` and see it offered.
