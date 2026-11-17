//! The two-reviews rule's security paths (MERGING-15): a changed file on any entry of
//! `SECURITY_PATHS` is always caught, in any letter case and at any depth its anchor allows, so a
//! pull request touching AGENTS.md, a CI workflow or the secret scan's settings never merges on
//! one review. One proof per entry, of `caught_by` (what `security_paths` asks of each entry):
//! with the entry fixed, CBMC proves each in minutes, where one proof choosing among all of them,
//! or walking all of them, doesn't end.
//! Proves: core/crates/obid/src/merge.rs

use super::{any_case, any_str};
use crate::merge::{SECURITY_PATHS, caught_by};

// A new entry fails this build until it has its own proof below.
const _: () = assert!(SECURITY_PATHS.len() == 26);

/// `entry`, in any case: a file name entry (no `/`) is caught alone or under any folder; a folder
/// entry (ending `/`) with anything inside it, at the root or under any folder; a whole path entry
/// at the root. The folder in front is any one byte (`/` too), what is inside it any byte or none.
fn caught(entry: &str) {
    let written = any_case(entry);
    let under = any_str::<1>();
    let path = if entry.ends_with('/') {
        let inside = any_str::<1>();
        let nested: bool = kani::any();
        kani::assume(!nested || !under.is_empty());
        // Built by hand: `format!` costs CBMC more than the property.
        let mut path = if nested {
            under.clone() + "/"
        } else {
            String::new()
        };
        path.push_str(&written);
        path.push_str(&inside);
        path
    } else if entry.contains('/') || kani::any() {
        written
    } else {
        kani::assume(!under.is_empty());
        under.clone() + "/" + &written
    };
    // `security_paths` keeps a path exactly when `caught_by` holds for some entry: proving it for
    // the entry itself is proving the path is kept, without walking the other 25.
    assert!(caught_by(&path, entry));
}

// Each proof's unwind is its entry's length plus 5: the longest path it builds is the entry and 3
// bytes, and every loop over it runs once more to end. Kani checks the bound: one too small fails
// its proof with an unwinding assertion, never passes it unproved.
macro_rules! each_entry {
    ($($i:literal $name:ident $unwind:literal),* $(,)?) => {
        $(
            #[kani::proof]
            #[kani::unwind($unwind)]
            fn $name() {
                caught(SECURITY_PATHS[$i]);
            }
        )*
    };
}

each_entry! {
    0 agents_md_is_caught 14,
    1 agents_override_md_is_caught 23,
    2 claude_md_is_caught 14,
    3 cursorrules_is_caught 17,
    4 mcp_json_is_caught 14,
    5 cursor_folder_is_caught 13,
    6 claude_folder_is_caught 13,
    7 codex_folder_is_caught 12,
    8 agents_folder_is_caught 13,
    9 gitleaks_toml_is_caught 19,
    10 gitleaksignore_is_caught 20,
    11 secretspec_toml_is_caught 20,
    12 codeowners_is_caught 15,
    13 github_workflows_is_caught 23,
    14 github_actions_is_caught 21,
    15 gitlab_ci_is_caught 19,
    16 circleci_is_caught 15,
    17 buildkite_is_caught 16,
    18 azure_pipelines_is_caught 24,
    19 bitbucket_pipelines_is_caught 28,
    20 jenkinsfile_is_caught 16,
    21 travis_is_caught 16,
    22 agents_rules_manifest_is_caught 34,
    23 build_and_run_is_caught 26,
    24 agent_clis_is_caught 23,
    25 publishing_contract_is_caught 33,
}
