//! The cases every code host's adapter passes, against a fake of its own service.
//!
//! An adapter's tests run all of it with one line:
//!
//! ```ignore
//! oby_connectors::conformance_tests!(Subject::new(MyHost::definition, my_host_on_a_fake));
//! ```
//!
//! which writes one `#[test]` per case, each named `conformance_<case>`. The [`Subject`] says how
//! to build the adapter's definition for the host names the suite picks, and its [`CodeHost`] over
//! a fake of its service seeded with the suite's [`Fixture`]. That fake must answer as the service
//! does: its own words for states, its own page size (100 at most), and errors that echo
//! [`Fixture::secret`] the way a CLI echoes a remote with a token in it. It writes each request it
//! gets into [`Fixture::requests`], which is how the suite sees a call that was made.
//!
//! Each case is also a function answering `Err` with what was wrong, so a broken fake can be shown
//! to fail it (the crate's own tests do, one fake per case).

use std::cell::RefCell;
use std::fmt;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;
use std::time::{Duration, Instant};

use crate::definition::{Capability, CodeHostDefinition};
use crate::error::ConnectorError;
use crate::host::{ChangeRequest, CodeHost, ListQuery, MergeMethod, list_all};
use crate::remote::{HostNames, MAX_URL_BYTES, Repo};
use crate::words::{ChangeRequestState, Family, MergeState, Mergeable, ReviewDecision};

/// The host names the suite builds definitions for: a public host whose SSH host isn't its web
/// host (T3 Code's c6a120a958), and an Enterprise one on its own port (a774d7f473).
pub fn public_names() -> HostNames {
    HostNames::new("forge.example", &["ssh.forge.example"])
}

pub fn enterprise_names() -> HostNames {
    HostNames::new("git.corp.example:8443", &["git-ssh.corp.example"])
}

/// How many change requests the fixture's repository has: past 100, and past two pages of it.
pub const FIXTURE_CHANGE_REQUESTS: u64 = 250;
/// How long all the bounded-parsing case's hostile inputs may take together.
pub const PARSE_BUDGET: Duration = Duration::from_secs(1);

/// The requests an adapter's fake got, by name, in order.
#[derive(Debug, Clone, Default)]
pub struct RequestLog(Rc<RefCell<Vec<String>>>);

impl RequestLog {
    pub fn push(&self, request: &str) {
        self.0.borrow_mut().push(request.to_owned());
    }

    pub fn len(&self) -> usize {
        self.0.borrow().len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.borrow().is_empty()
    }

    /// How many of them were `request`.
    pub fn count(&self, request: &str) -> usize {
        self.0.borrow().iter().filter(|r| *r == request).count()
    }
}

/// What the adapter's fake serves.
#[derive(Debug, Clone)]
pub struct Fixture {
    pub names: HostNames,
    pub repo: Repo,
    pub viewer: String,
    /// The token the adapter holds for the fake. Its errors echo it; nothing it answers may.
    pub secret: String,
    /// Numbers 1 to [`FIXTURE_CHANGE_REQUESTS`], in order.
    pub change_requests: Vec<ChangeRequest>,
    pub requests: RequestLog,
}

impl Fixture {
    /// The fixture for `definition`: its states drawn only from words its host can say.
    pub fn new<D: CodeHostDefinition + ?Sized>(definition: &D, names: HostNames) -> Self {
        let repo = Repo {
            host: names.web.clone(),
            path: "acme/widgets".into(),
        };
        let reachable = |family: Family| -> Vec<&'static str> {
            let mut words: Vec<&'static str> = Vec::new();
            for native in definition.native_words(family) {
                if let Some(word) = definition.map_native(family, native)
                    && family.api_words().contains(&word)
                    && !words.contains(&word)
                {
                    words.push(word);
                }
            }
            words
        };
        let pick =
            |words: &[&'static str], i: u64| words.get(i as usize % words.len().max(1)).copied();
        let states = reachable(Family::ChangeRequestState);
        let reviews = reachable(Family::ReviewDecision);
        let merge_states = reachable(Family::MergeState);
        let mergeables = reachable(Family::Mergeable);
        let change_requests = (1..=FIXTURE_CHANGE_REQUESTS)
            .map(|n| ChangeRequest {
                number: n,
                title: format!("Change {n}"),
                url: definition.change_request_url(&repo, n),
                base: "main".into(),
                head_branch: format!("topic/{n}"),
                head: Some(format!("{n:040x}")),
                state: pick(&states, n / 3)
                    .and_then(ChangeRequestState::from_api)
                    .unwrap_or(ChangeRequestState::Open),
                review_decision: pick(&reviews, n).and_then(ReviewDecision::from_api),
                merge_state: pick(&merge_states, n).and_then(MergeState::from_api),
                mergeable: pick(&mergeables, n).and_then(Mergeable::from_api),
                author: Some(format!("dev{}", n % 3)),
            })
            .collect();
        Fixture {
            names,
            repo,
            viewer: "oby-owner".into(),
            secret: "sekret-sekret-sekret".into(),
            change_requests,
            requests: RequestLog::default(),
        }
    }
}

/// An adapter under test: its definition for some host names, and its host over a fake.
pub struct Subject<D, H> {
    definition: Box<dyn Fn(&HostNames) -> D>,
    host: Box<dyn Fn(&Fixture) -> H>,
}

impl<D: CodeHostDefinition, H: CodeHost> Subject<D, H> {
    pub fn new(
        definition: impl Fn(&HostNames) -> D + 'static,
        host: impl Fn(&Fixture) -> H + 'static,
    ) -> Self {
        Subject {
            definition: Box::new(definition),
            host: Box::new(host),
        }
    }

    fn fixture(&self) -> (Fixture, H) {
        let fixture = Fixture::new(&(self.definition)(&public_names()), public_names());
        let host = (self.host)(&fixture);
        (fixture, host)
    }
}

/// A case that didn't pass, and every way it didn't.
#[derive(Clone, PartialEq, Eq)]
pub struct Failed {
    pub case: &'static str,
    pub problems: Vec<String>,
}

impl fmt::Display for Failed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "conformance_{} failed:", self.case)?;
        for problem in &self.problems {
            writeln!(f, "  - {problem}")?;
        }
        Ok(())
    }
}

impl fmt::Debug for Failed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

/// The most problems one case lists.
const MAX_PROBLEMS: usize = 20;

struct Case {
    name: &'static str,
    problems: Vec<String>,
}

impl Case {
    fn new(name: &'static str) -> Self {
        Case {
            name,
            problems: Vec::new(),
        }
    }

    fn check(&mut self, ok: bool, problem: impl FnOnce() -> String) {
        if !ok && self.problems.len() < MAX_PROBLEMS {
            self.problems.push(problem());
        }
    }

    fn end(self) -> Result<(), Failed> {
        if self.problems.is_empty() {
            Ok(())
        } else {
            Err(Failed {
                case: self.name,
                problems: self.problems,
            })
        }
    }
}

/// Panics with what failed: for the `#[test]`s [`crate::conformance_tests`] writes.
pub fn pass(result: Result<(), Failed>) {
    if let Err(failed) = result {
        panic!("{failed}");
    }
}

/// Every case, each one's failure listed.
pub fn run<D: CodeHostDefinition, H: CodeHost>(subject: &Subject<D, H>) -> Vec<Failed> {
    [
        urls_round_trip(subject),
        remote_recognition(subject),
        states_map_to_api(subject),
        pagination_past_100(subject),
        unsupported_capability(subject),
        bounded_parsing(subject),
        no_secret_in_output(subject),
    ]
    .into_iter()
    .filter_map(Result::err)
    .collect()
}

/// Change request, issue and commit URLs parse back to what built them, with and without a
/// trailing path, query or fragment, on the public host and an Enterprise one; and one kind's URL
/// is never read as another's.
pub fn urls_round_trip<D: CodeHostDefinition, H: CodeHost>(
    subject: &Subject<D, H>,
) -> Result<(), Failed> {
    let mut case = Case::new("urls_round_trip");
    let suffixes = [
        "",
        "/",
        "/files",
        "/commits",
        "?tab=checks",
        "#note_1",
        "/files#diff-1",
    ];
    for names in [public_names(), enterprise_names()] {
        let definition = (subject.definition)(&names);
        let mut paths = vec!["acme/widgets", "acme/.github", "acme/my_repo.js", "a-b/c_d"];
        if definition.nested_groups() {
            paths.extend(["group/sub/widgets", "group/sub/deeper/widgets"]);
        }
        for path in paths {
            let repo = Repo {
                host: names.web.clone(),
                path: path.into(),
            };
            for number in [1, 42, 4_294_967_296, 999_999_999_999] {
                let url = definition.change_request_url(&repo, number);
                for suffix in suffixes {
                    let read = definition.parse_change_request_url(&format!("{url}{suffix}"));
                    case.check(
                        read.as_ref().map(|l| (&l.repo, l.number)) == Some((&repo, number)),
                        || format!("change request {url}{suffix} read back as {read:?}"),
                    );
                }
                case.check(definition.parse_issue_url(&url).is_none(), || {
                    format!("change request {url} read as an issue")
                });
                case.check(definition.parse_commit_url(&url).is_none(), || {
                    format!("change request {url} read as a commit")
                });
                if let Some(url) = definition.issue_url(&repo, number) {
                    for suffix in suffixes {
                        let read = definition.parse_issue_url(&format!("{url}{suffix}"));
                        case.check(
                            read.as_ref().map(|l| (&l.repo, l.number)) == Some((&repo, number)),
                            || format!("issue {url}{suffix} read back as {read:?}"),
                        );
                    }
                    case.check(definition.parse_change_request_url(&url).is_none(), || {
                        format!("issue {url} read as a change request")
                    });
                }
            }
            for sha in [
                "abc1234",
                "0123456789abcdef0123456789abcdef01234567",
                &"f".repeat(64),
            ] {
                let url = definition.commit_url(&repo, sha);
                for suffix in suffixes {
                    let read = definition.parse_commit_url(&format!("{url}{suffix}"));
                    case.check(
                        read.as_ref().map(|l| (&l.repo, l.sha.as_str())) == Some((&repo, sha)),
                        || format!("commit {url}{suffix} read back as {read:?}"),
                    );
                }
                case.check(definition.parse_change_request_url(&url).is_none(), || {
                    format!("commit {url} read as a change request")
                });
            }
        }
        let other = Repo {
            host: "other.example".into(),
            path: "acme/widgets".into(),
        };
        let url = definition.change_request_url(&other, 7);
        case.check(definition.parse_change_request_url(&url).is_none(), || {
            format!("{} read another host's change request {url}", names.web)
        });
    }
    case.end()
}

/// Remotes are recognized as the repository they name, every spelling alike: HTTPS, scp-style and
/// `ssh://` SSH, an SSH host that isn't the web host, an Enterprise hostname, ports, `.git`, a
/// trailing `/`, nested groups where the host has them; and every other host's remote, and a
/// lookalike, is refused.
pub fn remote_recognition<D: CodeHostDefinition, H: CodeHost>(
    subject: &Subject<D, H>,
) -> Result<(), Failed> {
    let mut case = Case::new("remote_recognition");
    let public = (subject.definition)(&public_names());
    let enterprise = (subject.definition)(&enterprise_names());
    let widgets = |host: &str| {
        Some(Repo {
            host: host.into(),
            path: "acme/widgets".into(),
        })
    };

    let mut expect = |definition: &D, remote: &str, want: Option<Repo>| {
        let got = definition.recognize_remote(remote);
        case.check(got == want, || {
            format!("{remote:?} recognized as {got:?}, not {want:?}")
        });
    };
    for remote in [
        "https://forge.example/acme/widgets",
        "https://forge.example/acme/widgets.git",
        "https://forge.example/acme/widgets/",
        "https://FORGE.Example/acme/widgets.git",
        "https://forge.example:443/acme/widgets.git",
        "https://someone@forge.example/acme/widgets.git",
        "https://forge.example/acme/widgets.git\n",
        "git@forge.example:acme/widgets.git",
        "git@forge.example:acme/widgets",
        "git@forge.example:/acme/widgets.git",
        "forge.example:acme/widgets.git",
        "ssh://git@forge.example/acme/widgets.git",
        "ssh://git@forge.example:22/acme/widgets.git",
        "ssh://git@ssh.forge.example:443/acme/widgets.git",
        "git@ssh.forge.example:acme/widgets.git",
        "git+ssh://git@forge.example/acme/widgets.git",
    ] {
        expect(&public, remote, widgets("forge.example"));
    }
    for remote in [
        "https://git.corp.example:8443/acme/widgets.git",
        "ssh://git@git.corp.example:2222/acme/widgets.git",
        "git@git.corp.example:acme/widgets.git",
        "git@git-ssh.corp.example:acme/widgets.git",
    ] {
        expect(&enterprise, remote, widgets("git.corp.example:8443"));
    }
    for (remote, path) in [
        (
            "https://forge.example/group/sub/widgets.git",
            "group/sub/widgets",
        ),
        (
            "git@forge.example:group/sub/deeper/widgets.git",
            "group/sub/deeper/widgets",
        ),
        (
            "ssh://git@ssh.forge.example:443/group/sub/widgets",
            "group/sub/widgets",
        ),
    ] {
        let want = public.nested_groups().then(|| Repo {
            host: "forge.example".into(),
            path: path.into(),
        });
        expect(&public, remote, want);
    }
    for remote in [
        "https://github.com/acme/widgets.git",
        "git@github.com:acme/widgets.git",
        "https://evilforge.example/acme/widgets",
        "https://forge.example.evil.example/acme/widgets",
        "https://forge.example@evil.example/acme/widgets",
        "https://evil.example/forge.example/acme/widgets",
        "https://evil.example#@forge.example/acme/widgets",
        "https://ssh.forge.example/acme/widgets",
        "https://forge.example:8443/acme/widgets",
        "file:///forge.example/acme/widgets",
        "/srv/forge.example/acme/widgets.git",
        "../widgets",
        "https://forge.example/acme",
        "https://forge.example/",
        "https://forge.example/acme/../widgets",
        "https://forge.example/acme//widgets",
        "git@forge.example:acme",
    ] {
        expect(&public, remote, None);
    }
    for remote in [
        "https://git.corp.example/acme/widgets",
        "https://forge.example/acme/widgets",
    ] {
        expect(&enterprise, remote, None);
    }
    case.end()
}

/// Each of the host's own words for a state maps to exactly one of obid's ([`Family::api_words`]),
/// never one it invents; every word obid has for a change request's state is reachable (a merged
/// one isn't read as closed, T3 Code's 60e54867fc); and a word the host isn't known to say maps to
/// none, never a guess.
pub fn states_map_to_api<D: CodeHostDefinition, H: CodeHost>(
    subject: &Subject<D, H>,
) -> Result<(), Failed> {
    let mut case = Case::new("states_map_to_api");
    let definition = (subject.definition)(&public_names());
    for family in Family::ALL {
        let natives = definition.native_words(family);
        let api = family.api_words();
        for (i, native) in natives.iter().enumerate() {
            case.check(!natives[..i].contains(native), || {
                format!("{family:?}: {native:?} is listed twice")
            });
            let word = definition.map_native(family, native);
            match word {
                None => case.check(false, || format!("{family:?}: the host's {native:?} maps to nothing")),
                Some(word) => case.check(api.contains(&word), || {
                    format!("{family:?}: the host's {native:?} maps to {word:?}, which obid doesn't have ({api:?})")
                }),
            }
            case.check(definition.map_native(family, native) == word, || {
                format!("{family:?}: {native:?} maps differently each time")
            });
        }
        let long = "x".repeat(10_000);
        let mut probes = vec![
            "",
            " ",
            "SOMETHING_NEW",
            "unknown_future_state",
            "open\0",
            long.as_str(),
        ];
        for word in Family::ALL.iter().flat_map(|other| other.api_words()) {
            if !probes.contains(word) {
                probes.push(word);
            }
        }
        for probe in probes {
            if natives.contains(&probe) {
                continue;
            }
            let got = definition.map_native(family, probe);
            case.check(got.is_none(), || {
                let shown: String = probe.chars().take(40).collect();
                format!("{family:?}: {shown:?}, a word the host isn't known to say, maps to {got:?}, a guess")
            });
        }
    }
    for word in ChangeRequestState::WORDS {
        let reached = (definition.native_words(Family::ChangeRequestState).iter())
            .any(|native| definition.map_native(Family::ChangeRequestState, native) == Some(word));
        case.check(reached, || format!("no state of the host's maps to {word}"));
    }
    case.end()
}

/// A repository with [`FIXTURE_CHANGE_REQUESTS`] change requests lists them all, every page
/// followed, in order and as served (T3 Code's 9f815c94af: a list cut at 100); filtered by state
/// too; and one read alone matches its listing.
pub fn pagination_past_100<D: CodeHostDefinition, H: CodeHost>(
    subject: &Subject<D, H>,
) -> Result<(), Failed> {
    let mut case = Case::new("pagination_past_100");
    let (fixture, host) = subject.fixture();
    match list_all(&host, &fixture.repo, &ListQuery::default()) {
        Err(e) => case.check(false, || format!("listing failed: {e}")),
        Ok(all) => {
            case.check(all.len() == fixture.change_requests.len(), || {
                format!(
                    "{} change requests came back of {}",
                    all.len(),
                    fixture.change_requests.len()
                )
            });
            let first_wrong = all
                .iter()
                .zip(&fixture.change_requests)
                .find(|(got, want)| got != want);
            case.check(first_wrong.is_none(), || {
                format!("listed (got, wanted) {first_wrong:?}")
            });
        }
    }
    let pages = fixture.requests.len();
    case.check(pages >= 3, || {
        format!("the fake served {pages} requests for {FIXTURE_CHANGE_REQUESTS}: it must page as the host does, 100 at most")
    });
    for state in ChangeRequestState::ALL {
        let want: Vec<&ChangeRequest> = fixture
            .change_requests
            .iter()
            .filter(|cr| cr.state == *state)
            .collect();
        match list_all(
            &host,
            &fixture.repo,
            &ListQuery {
                state: Some(*state),
            },
        ) {
            Err(e) => case.check(false, || format!("listing {state:?} failed: {e}")),
            Ok(got) => case.check(got.iter().collect::<Vec<_>>() == want, || {
                format!(
                    "{} {state:?} change requests came back of {}",
                    got.len(),
                    want.len()
                )
            }),
        }
    }
    let one = host.change_request(&fixture.repo, 137);
    case.check(
        one.as_ref().ok() == fixture.change_requests.get(136),
        || format!("change request 137 read as {one:?}"),
    );
    let viewer = host.viewer().map(|v| v.login);
    case.check(viewer.as_ref().ok() == Some(&fixture.viewer), || {
        format!("the viewer read as {viewer:?}")
    });
    case.end()
}

/// Every capability the definition doesn't declare answers [`ConnectorError::Unsupported`] naming
/// it, and its request is never made; every one it declares is asked.
pub fn unsupported_capability<D: CodeHostDefinition, H: CodeHost>(
    subject: &Subject<D, H>,
) -> Result<(), Failed> {
    let mut case = Case::new("unsupported_capability");
    let (fixture, host) = subject.fixture();
    let declared = host.definition().capabilities();
    for capability in Capability::ALL {
        let before = fixture.requests.len();
        let result = call_gated(&host, capability, &fixture.repo);
        let asked = fixture.requests.len() > before;
        if declared.has(capability) {
            case.check(
                !matches!(result, Err(ConnectorError::Unsupported { .. })),
                || format!("{capability:?} is declared but answered {result:?}"),
            );
            case.check(asked, || {
                format!("{capability:?} is declared but its request was never made")
            });
        } else {
            case.check(
                matches!(result, Err(ConnectorError::Unsupported { capability: c, .. }) if c == capability),
                || format!("{capability:?} isn't declared but answered {result:?}, not Unsupported"),
            );
            case.check(!asked, || {
                format!("{capability:?} isn't declared but its request was made")
            });
        }
    }
    case.end()
}

fn call_gated<H: CodeHost>(
    host: &H,
    capability: Capability,
    repo: &Repo,
) -> Result<(), ConnectorError> {
    match capability {
        Capability::MergeQueue => host.enqueue_merge(repo, 1),
        Capability::AutoMerge => host.enable_auto_merge(repo, 1, MergeMethod::Squash),
        Capability::Drafts => host.set_draft(repo, 1, true),
        Capability::RequiredApprovals => host.required_approvals(repo, "main").map(drop),
        Capability::Codeowners => host.code_owners(repo, "src/lib.rs").map(drop),
        Capability::ReviewerAccount => host.approve_as_reviewer(repo, 1),
        Capability::CheckLogs => host.check_log(repo, "build").map(drop),
        Capability::Rerun => host.rerun_checks(repo, 1),
    }
}

/// Huge and hostile input is refused or read within [`PARSE_BUDGET`] all together, never panics,
/// and nothing read from it is longer than [`MAX_URL_BYTES`] (T3 Code's f1cd57bbf9): input past
/// [`MAX_URL_BYTES`] is refused unread.
pub fn bounded_parsing<D: CodeHostDefinition, H: CodeHost>(
    subject: &Subject<D, H>,
) -> Result<(), Failed> {
    let mut case = Case::new("bounded_parsing");
    let definition = (subject.definition)(&public_names());
    let web = "https://forge.example/";
    let hostile: Vec<(String, &str)> = vec![
        (format!("{web}{}", "a".repeat(1 << 20)), "1 MiB path"),
        (
            format!("{web}acme/{}widgets.git", "a/".repeat(100_000)),
            "100,000 segments",
        ),
        (
            format!("git@forge.example:{}r.git", "o/".repeat(50_000)),
            "50,000 SSH segments",
        ),
        (
            format!(
                "https://{}.forge.example/acme/widgets",
                "a.".repeat(100_000)
            ),
            "a 200 KB host",
        ),
        (
            format!("{web}acme/widgets/-/pulls/99999999999999999999999999"),
            "an overflowing number",
        ),
        (
            format!("{web}acme/widgets/pull/99999999999999999999999999"),
            "an overflowing number",
        ),
        (
            format!("{web}acme/widgets/-/merge_requests/18446744073709551616"),
            "an overflowing number",
        ),
        (
            format!("{web}acme/widgets/-/issues/-1"),
            "a negative number",
        ),
        (
            format!("{web}acme/widgets/-/commit/{}", "f".repeat(100_000)),
            "a 100 KB commit",
        ),
        (format!("{web}{}", "%2e%2e/".repeat(10_000)), "encoded dots"),
        (format!("{web}acme/widgets\u{0}/-/pulls/1"), "a NUL"),
        (format!("{web}acme/widgets\r\n/-/pulls/1"), "a line break"),
        (
            format!("https://forge.example:{}/acme/widgets", "9".repeat(10_000)),
            "a long port",
        ),
        ("https://[::1]:443/acme/widgets".into(), "an IPv6 literal"),
        ("::::::::::::::::".repeat(4096), "colons"),
        ("@".repeat(1 << 16), "at signs"),
    ];
    let started = Instant::now();
    for (input, what) in &hostile {
        let outcome = catch_unwind(AssertUnwindSafe(|| {
            let mut longest = 0;
            if let Some(repo) = definition.recognize_remote(input) {
                longest = longest.max(repo.host.len() + repo.path.len());
            }
            if let Some(link) = definition.parse_change_request_url(input) {
                longest = longest.max(link.repo.host.len() + link.repo.path.len());
            }
            if let Some(link) = definition.parse_issue_url(input) {
                longest = longest.max(link.repo.host.len() + link.repo.path.len());
            }
            if let Some(link) = definition.parse_commit_url(input) {
                longest = longest.max(link.repo.host.len() + link.repo.path.len() + link.sha.len());
            }
            for family in Family::ALL {
                if definition.map_native(family, input).is_some() {
                    longest = longest.max(input.len());
                }
            }
            longest
        }));
        match outcome {
            Err(_) => case.check(false, || format!("{what}: panicked")),
            Ok(longest) => {
                case.check(longest <= MAX_URL_BYTES, || {
                    format!("{what}: read {longest} bytes out of it")
                });
                case.check(input.len() <= MAX_URL_BYTES || longest == 0, || {
                    format!(
                        "{what}: {} bytes, past {MAX_URL_BYTES}, were read, not refused",
                        input.len()
                    )
                });
            }
        }
    }
    let took = started.elapsed();
    case.check(took <= PARSE_BUDGET, || {
        format!("hostile input took {took:?}, past {PARSE_BUDGET:?}")
    });
    case.end()
}

/// The token the adapter holds is in nothing it answers: a repository recognized from a remote
/// that carries it, a URL built from that, a change request's URL, and errors, even when the fake
/// echoes it back as a CLI would.
pub fn no_secret_in_output<D: CodeHostDefinition, H: CodeHost>(
    subject: &Subject<D, H>,
) -> Result<(), Failed> {
    let mut case = Case::new("no_secret_in_output");
    let (fixture, host) = subject.fixture();
    let secret = fixture.secret.as_str();
    let clean = |case: &mut Case, what: &str, text: String| {
        case.check(!text.contains(secret), || {
            format!("{what} holds the token: {text}")
        });
    };
    let definition = host.definition();
    let remote = format!(
        "https://x-access-token:{secret}@{}/{}.git",
        fixture.names.web, fixture.repo.path
    );
    if let Some(repo) = definition.recognize_remote(&remote) {
        clean(
            &mut case,
            "the repository recognized from a remote with a token",
            format!("{repo:?}"),
        );
        clean(
            &mut case,
            "a change request URL built from it",
            definition.change_request_url(&repo, 1),
        );
        clean(
            &mut case,
            "a commit URL built from it",
            definition.commit_url(&repo, "abc1234"),
        );
        clean(
            &mut case,
            "an issue URL built from it",
            format!("{:?}", definition.issue_url(&repo, 1)),
        );
    }
    let url = definition.change_request_url(&fixture.repo, 3).replacen(
        "://",
        &format!("://u:{secret}@"),
        1,
    );
    clean(
        &mut case,
        "a change request read from a URL with a token",
        format!("{:?}", definition.parse_change_request_url(&url)),
    );
    if let Ok(page) = host.list_change_requests(&fixture.repo, &ListQuery::default(), None) {
        for cr in page.items {
            clean(&mut case, "a listed change request", format!("{cr:?}"));
        }
    }
    let before = fixture.requests.len();
    let missing = host.change_request(&fixture.repo, 999_999);
    case.check(fixture.requests.len() > before, || {
        "reading a missing change request never reached the fake".into()
    });
    match missing {
        Ok(cr) => case.check(false, || format!("a missing change request read as {cr:?}")),
        Err(e) => {
            clean(
                &mut case,
                "the error for a missing change request",
                e.to_string(),
            );
            clean(
                &mut case,
                "the error for a missing change request, debugged",
                format!("{e:?}"),
            );
        }
    }
    for capability in Capability::ALL {
        if let Err(e) = call_gated(&host, capability, &fixture.repo) {
            clean(&mut case, "a capability's error", format!("{e} {e:?}"));
        }
    }
    case.end()
}

/// One `#[test]` per case, named `conformance_<case>`, for the [`Subject`] given:
/// `oby_connectors::conformance_tests!(Subject::new(definition, host));`
#[macro_export]
macro_rules! conformance_tests {
    ($subject:expr) => {
        #[test]
        fn conformance_urls_round_trip() {
            $crate::conformance::pass($crate::conformance::urls_round_trip(&$subject));
        }
        #[test]
        fn conformance_remote_recognition() {
            $crate::conformance::pass($crate::conformance::remote_recognition(&$subject));
        }
        #[test]
        fn conformance_states_map_to_api() {
            $crate::conformance::pass($crate::conformance::states_map_to_api(&$subject));
        }
        #[test]
        fn conformance_pagination_past_100() {
            $crate::conformance::pass($crate::conformance::pagination_past_100(&$subject));
        }
        #[test]
        fn conformance_unsupported_capability() {
            $crate::conformance::pass($crate::conformance::unsupported_capability(&$subject));
        }
        #[test]
        fn conformance_bounded_parsing() {
            $crate::conformance::pass($crate::conformance::bounded_parsing(&$subject));
        }
        #[test]
        fn conformance_no_secret_in_output() {
            $crate::conformance::pass($crate::conformance::no_secret_in_output(&$subject));
        }
    };
}
