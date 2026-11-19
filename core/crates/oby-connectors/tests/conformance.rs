//! The suite against the reference host: green as written, and red for each broken fake, on the
//! case that must catch its bug and on no other (so each case is proven able to fail).

mod reference;

use oby_connectors::conformance::{self, Failed, Fixture, Subject};
use oby_connectors::{ListQuery, list_all};
use reference::{Flaw, ReferenceDefinition, ReferenceHost};

fn subject(flaw: Option<Flaw>) -> Subject<ReferenceDefinition, ReferenceHost> {
    Subject::new(
        move |names| ReferenceDefinition::new(names, flaw),
        move |fixture| ReferenceHost::new(fixture, flaw),
    )
}

// The reference host passes all of it: the one line every adapter writes.
oby_connectors::conformance_tests!(subject(None));

/// The flaw fails exactly `cases`, and the first one's failure says something containing `says`.
fn fails_only(flaw: Flaw, cases: &[&str], says: &str) {
    let failed: Vec<Failed> = conformance::run(&subject(Some(flaw)));
    let got: Vec<&str> = failed.iter().map(|f| f.case).collect();
    assert_eq!(
        got, cases,
        "{flaw:?} should fail {cases:?} alone: {failed:?}"
    );
    let text = failed[0].to_string();
    assert!(
        text.contains(says),
        "{flaw:?}'s failure should say {says:?}: {text}"
    );
}

#[test]
fn broken_urls_drops_trailing_path() {
    fails_only(
        Flaw::DropsTrailingPath,
        &["urls_round_trip"],
        "/files read back as None",
    );
}

#[test]
fn broken_remotes_misparses_ssh() {
    fails_only(
        Flaw::MisparsesSsh,
        &["remote_recognition"],
        "\"git@forge.example:acme/widgets.git\" recognized as None",
    );
}

#[test]
fn broken_remotes_ignores_ssh_host() {
    fails_only(
        Flaw::IgnoresSshHost,
        &["remote_recognition"],
        "ssh.forge.example",
    );
}

#[test]
fn broken_remotes_accepts_lookalike_host() {
    fails_only(
        Flaw::AcceptsLookalikeHost,
        &["remote_recognition"],
        "evilforge.example",
    );
}

#[test]
fn broken_states_invents_state() {
    fails_only(
        Flaw::InventsState,
        &["states_map_to_api"],
        "\"LOCKED\", which obid doesn't have",
    );
}

#[test]
fn broken_states_guesses_unknown_state() {
    // A host that guesses also reads a 1 MiB word as a state: bounded parsing catches it too.
    fails_only(
        Flaw::GuessesUnknownState,
        &["states_map_to_api", "bounded_parsing"],
        "a guess",
    );
}

#[test]
fn broken_states_merged_as_closed() {
    fails_only(
        Flaw::MergedAsClosed,
        &["states_map_to_api"],
        "no state of the host's maps to MERGED",
    );
}

#[test]
fn broken_pagination_cuts_at_100() {
    fails_only(
        Flaw::CutsAt100,
        &["pagination_past_100"],
        "100 change requests came back of 250",
    );
}

#[test]
fn broken_pagination_repeats_a_page() {
    fails_only(
        Flaw::RepeatsPage,
        &["pagination_past_100"],
        "same page twice",
    );
}

#[test]
fn broken_capabilities_attempts_unsupported() {
    fails_only(
        Flaw::AttemptsUnsupported,
        &["unsupported_capability"],
        "isn't declared but its request was made",
    );
}

#[test]
fn broken_capabilities_fails_instead_of_unsupported() {
    fails_only(
        Flaw::FailsInsteadOfUnsupported,
        &["unsupported_capability"],
        "not Unsupported",
    );
}

#[test]
fn broken_bounded_parsing_unbounded_parse() {
    fails_only(Flaw::UnboundedParse, &["bounded_parsing"], "panicked");
}

#[test]
fn broken_secrets_leaks_secret() {
    fails_only(
        Flaw::LeaksSecret,
        &["no_secret_in_output"],
        "holds the token",
    );
}

/// `list_all` stops at a page the host already gave, rather than loop forever.
#[test]
fn list_all_stops_at_a_repeated_page() {
    let names = conformance::public_names();
    let fixture = Fixture::new(&ReferenceDefinition::new(&names, None), names);
    let host = ReferenceHost::new(&fixture, Some(Flaw::RepeatsPage));
    let err = list_all(&host, &fixture.repo, &ListQuery::default()).unwrap_err();
    assert!(err.to_string().contains("partial"), "{err}");
    assert_eq!(fixture.requests.count("list"), 4);
}
