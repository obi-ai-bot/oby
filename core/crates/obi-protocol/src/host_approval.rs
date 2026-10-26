//! Approving on the code host (spec `protocol.md` section 11, "Approving on the code host"): whether
//! a receiving Obi may post its approving review of a `review_request`'s `head_sha` now. It may
//! with its owner's confirmation of that exchange at that commit, from the owner's own device,
//! fresh and used once; or, under its owner's standing permission, next to approvals of that commit
//! by people who between them own every changed file (the repository's `CODEOWNERS`). Otherwise it
//! doesn't. Check for check as the spec's reference, `verifyHostApproval` in `lib/obi.mjs`; the
//! conformance cases of kind `host_approval` judge both.

use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::Value;

use crate::Rejection;

/// How long an owner's confirmation of one exchange stays fresh, either way of the receiver's clock.
pub const CONFIRMATION_FRESH: i64 = 120;

/// The request being answered, as the decision reads it.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Request {
    pub correlation_id: String,
    pub head_sha: String,
    /// ISO 8601, as the envelope's.
    pub expires_at: String,
}

/// Where the receiver stands (a case's `context`).
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Context {
    /// ISO 8601: the moment the receiver decides.
    pub now: String,
    pub request: Request,
    /// Whether the receiver is still an active member of its Sphere.
    pub member: bool,
    /// The receiver's owner's code-host login: the one its approval is posted with.
    pub owner_login: String,
    /// Whether the owner gave the standing permission for the sender's owner and the repository.
    pub standing_permission: bool,
}

/// An owner's yes the receiver holds, its device signature checked already.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Confirmation {
    pub correlation_id: String,
    pub head_sha: String,
    /// ISO 8601: when the device signed it.
    pub signed_at: String,
    /// False for a yes from anywhere but the owner's own device (an API token, a job, anything
    /// received).
    pub from_owner_device: bool,
    pub used_before: bool,
}

/// One review of the pull request, as the code host shows it.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Review {
    pub login: String,
    /// `human`, `bot` (the code host marks the account as one) or `obi` (an Obi's fixed words).
    pub kind: String,
    /// `APPROVED`, `CHANGES_REQUESTED`, `COMMENTED` or `DISMISSED`.
    pub state: String,
    pub commit_id: String,
}

/// What the code host shows about the pull request.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct CodeHost {
    /// The `CODEOWNERS` file on the base branch, or none.
    pub codeowners: Option<String>,
    /// Every path the pull request changes (a renamed file's old and new).
    pub changed_files: Vec<String>,
    /// Whether `changed_files` is the whole list.
    pub files_complete: bool,
    /// In the order they were made.
    pub reviews: Vec<Review>,
    /// The members of each team `CODEOWNERS` names (`org/team`).
    #[serde(default)]
    pub teams: BTreeMap<String, Vec<String>>,
}

/// A case's `input`.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Input {
    pub owner_confirmations: Vec<Confirmation>,
    pub code_host: CodeHost,
}

/// Whether the receiver may post its approving review now. Rejections: `lapsed` (the request
/// expired, or the receiver left its Sphere), `no_owner_confirmation`, `no_code_owner_approval`.
pub fn decide(context: &Context, input: &Input) -> Result<(), Rejection> {
    let now = crate::time::parse(&context.now);
    let request = &context.request;
    let open = now
        .zip(crate::time::parse(&request.expires_at))
        .is_some_and(|(now, until)| now < until);
    if !open || !context.member {
        return Err(Rejection::new(
            "lapsed",
            "the request expired, or the receiver is no longer an active member of its Sphere",
        ));
    }
    let now = now.unwrap_or_default();
    let confirmed = input.owner_confirmations.iter().any(|c| {
        c.from_owner_device
            && !c.used_before
            && c.correlation_id == request.correlation_id
            && c.head_sha == request.head_sha
            && crate::time::parse(&c.signed_at)
                .is_some_and(|at| (now - at).abs() <= CONFIRMATION_FRESH)
    });
    if confirmed {
        return Ok(());
    }
    if !context.standing_permission {
        return Err(Rejection::new(
            "no_owner_confirmation",
            "its owner didn't confirm this exchange at this commit on their own device",
        ));
    }
    if !code_owners_approved(&input.code_host, &request.head_sha, &context.owner_login) {
        return Err(Rejection::new(
            "no_code_owner_approval",
            "no person who owns every changed file approved this commit",
        ));
    }
    Ok(())
}

/// A `host_approval` conformance case (spec `conformance/README.md`).
pub fn verify(input: &Value, context: &Value) -> Result<(), Rejection> {
    let bad = |what: &str| Rejection::new("bad_request", format!("the case's {what} doesn't read"));
    let context: Context = serde_json::from_value(context.clone()).map_err(|_| bad("context"))?;
    let input: Input = serde_json::from_value(input.clone()).map_err(|_| bad("input"))?;
    decide(&context, &input)
}

/// Whether people who between them own every changed file approved `head`: each changed file has
/// owners, and one of them has an approval that counts ([`counting_approvers`]).
pub fn code_owners_approved(host: &CodeHost, head: &str, own_login: &str) -> bool {
    let Some(rules) = host.codeowners.as_deref().and_then(parse_codeowners) else {
        return false;
    };
    if host.changed_files.is_empty() || !host.files_complete {
        return false;
    }
    let approvers = counting_approvers(&host.reviews, head, own_login);
    host.changed_files.iter().all(|file| {
        let owners = owners_of(&rules, file);
        !owners.is_empty()
            && approvers
                .iter()
                .any(|login| owners.iter().any(|o| is_owner(o, login, &host.teams)))
    })
}

/// The logins (lower case) whose approval of `head` counts: each person's last approving,
/// changes-requesting or dismissed review decides; only a person's (`kind` human), and never the
/// receiver's own owner's, whose login the receiver posts with.
pub fn counting_approvers(reviews: &[Review], head: &str, own_login: &str) -> Vec<String> {
    let mut last: BTreeMap<String, &Review> = BTreeMap::new();
    for review in reviews {
        if matches!(
            review.state.as_str(),
            "APPROVED" | "CHANGES_REQUESTED" | "DISMISSED"
        ) {
            last.insert(review.login.to_lowercase(), review);
        }
    }
    let own = own_login.to_lowercase();
    last.into_iter()
        .filter(|(login, r)| {
            r.state == "APPROVED" && r.commit_id == head && r.kind == "human" && *login != own
        })
        .map(|(login, _)| login)
        .collect()
}

fn is_owner(owner: &str, login: &str, teams: &BTreeMap<String, Vec<String>>) -> bool {
    let Some(name) = owner.strip_prefix('@').map(str::to_lowercase) else {
        return false;
    };
    if !name.contains('/') {
        return name == login;
    }
    teams
        .iter()
        .find(|(team, _)| team.to_lowercase() == name)
        .is_some_and(|(_, members)| members.iter().any(|m| m.to_lowercase() == login))
}

/// One `CODEOWNERS` line: its pattern and its owners.
#[derive(Debug, Clone)]
pub struct Rule {
    pattern: Vec<Token>,
    pub owners: Vec<String>,
}

/// The rules of a `CODEOWNERS` file, in order, or none when it can't be read (a pattern with `!`,
/// `[`, `]` or `\`).
pub fn parse_codeowners(text: &str) -> Option<Vec<Rule>> {
    let mut rules = Vec::new();
    for line in text.split('\n') {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut words = line.split_whitespace();
        let pattern = words.next()?;
        if pattern.contains(['!', '[', ']', '\\']) {
            return None;
        }
        let owners = words
            .take_while(|w| !w.starts_with('#'))
            .map(str::to_owned)
            .collect();
        rules.push(Rule {
            pattern: compile(pattern),
            owners,
        });
    }
    Some(rules)
}

/// The owners of `path`: the last matching rule's.
pub fn owners_of<'a>(rules: &'a [Rule], path: &str) -> &'a [String] {
    let path: Vec<char> = path.chars().collect();
    rules
        .iter()
        .rev()
        .find(|r| matches(&r.pattern, &path))
        .map_or(&[], |r| r.owners.as_slice())
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Char(char),
    /// `?`: one character but `/`.
    One,
    /// `*`: any characters but `/`.
    Star,
    /// `**`: any characters.
    Deep,
    /// `**/`, or an unanchored pattern's start: nothing, or anything ending in `/`.
    Dirs,
    /// The end of a pattern that also covers everything under a directory it names: nothing, or
    /// `/` and anything.
    Subtree,
}

/// A pattern as tokens ("Who counts as a code owner"): anchored when it starts with `/` or has a
/// `/` before its end; a trailing `/` covers everything under the directory; any other pattern
/// also covers everything under a directory it names, unless its last part is exactly `*`.
fn compile(pattern: &str) -> Vec<Token> {
    let dir_only = pattern.ends_with('/');
    let body = if dir_only {
        &pattern[..pattern.len() - 1]
    } else {
        pattern
    };
    let anchored = body.starts_with('/') || body.contains('/');
    let body = body.strip_prefix('/').unwrap_or(body);
    let mut tokens = Vec::new();
    if !anchored {
        tokens.push(Token::Dirs);
    }
    let chars: Vec<char> = body.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '*' if chars.get(i + 1) == Some(&'*') => {
                if chars.get(i + 2) == Some(&'/') {
                    tokens.push(Token::Dirs);
                    i += 3;
                } else {
                    tokens.push(Token::Deep);
                    i += 2;
                }
                continue;
            }
            '*' => tokens.push(Token::Star),
            '?' => tokens.push(Token::One),
            c => tokens.push(Token::Char(c)),
        }
        i += 1;
    }
    if dir_only {
        tokens.extend([Token::Char('/'), Token::Deep]);
    } else if body.rsplit('/').next() != Some("*") {
        tokens.push(Token::Subtree);
    }
    tokens
}

fn matches(tokens: &[Token], path: &[char]) -> bool {
    let Some((first, rest)) = tokens.split_first() else {
        return path.is_empty();
    };
    match first {
        Token::Char(c) => path.first() == Some(c) && matches(rest, &path[1..]),
        Token::One => path.first().is_some_and(|c| *c != '/') && matches(rest, &path[1..]),
        Token::Star => {
            let mut i = 0;
            loop {
                if matches(rest, &path[i..]) {
                    return true;
                }
                if i == path.len() || path[i] == '/' {
                    return false;
                }
                i += 1;
            }
        }
        Token::Deep => (0..=path.len()).any(|i| matches(rest, &path[i..])),
        Token::Dirs => {
            matches(rest, path)
                || (0..path.len()).any(|i| path[i] == '/' && matches(rest, &path[i + 1..]))
        }
        Token::Subtree => path.is_empty() || path[0] == '/',
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn owner(text: &str, path: &str) -> Vec<String> {
        owners_of(&parse_codeowners(text).unwrap(), path).to_vec()
    }

    #[test]
    fn patterns_match_as_the_spec_says() {
        let covers = |pattern: &str, path: &str| !owner(&format!("{pattern} @x"), path).is_empty();
        assert!(covers("*", "a/b/c.js"));
        assert!(covers("*.js", "a/b/c.js"));
        assert!(!covers("*.js", "a/b/c.ts"));
        assert!(covers("/src/", "src/a/b.rs"));
        assert!(!covers("/src/", "lib/src/b.rs"));
        assert!(covers("apps/", "x/apps/y.swift"));
        assert!(covers("docs/*", "docs/a.md"));
        assert!(!covers("docs/*", "docs/b/c.md"));
        assert!(covers("**/logs", "deeply/nested/logs/x.log"));
        assert!(covers("/scripts", "scripts/release.sh"));
        assert!(covers("/build/logs/", "build/logs/a.log"));
        assert!(!covers("/build/logs/", "build/logs"));
        assert!(covers("src/a?.rs", "src/ab.rs"));
        assert!(!covers("src/a?.rs", "src/a/.rs"));
        assert!(covers("docs/**", "docs/a/b/c.md"));
        assert!(!covers("/README.md", "docs/README.md"));
        assert!(covers("README.md", "docs/README.md"));
    }

    #[test]
    fn the_last_matching_line_wins_and_comments_end_owners() {
        let text = "* @all\n/src/ @alice\n*.md @carol\n/vendor/\n/run.sh @bob # not @eve\n";
        assert_eq!(owner(text, "src/a.md"), ["@carol"]);
        assert_eq!(owner(text, "src/a.rs"), ["@alice"]);
        assert!(owner(text, "vendor/x.js").is_empty());
        assert_eq!(owner(text, "run.sh"), ["@bob"]);
        assert!(parse_codeowners("!secret @x").is_none());
        assert!(parse_codeowners("src/[ab].rs @x").is_none());
    }
}
