//! The secret scan's verdict: a scanner that couldn't run is never Clean, and Clean means no
//! finding from either the built-in rules or gitleaks, so nothing goes out unscanned.
//! Proves: core/crates/obid/src/scan.rs

use crate::scan::{Finding, Verdict, verdict};

/// None or one finding, told apart by its line: none or any of 256. Its name and rule are empty,
/// so nothing is allocated: what the verdict does with findings is compare them, and the line is
/// enough to make two equal or not. Two lists of these make every case the verdict tells apart
/// (none, one from either side, the same one twice, two different).
fn any_findings() -> Vec<Finding> {
    if kani::any() {
        return Vec::new();
    }
    vec![Finding {
        name: String::new(),
        line: if kani::any() {
            None
        } else {
            Some(kani::any::<u8>() as usize)
        },
        rule: String::new(),
    }]
}

/// For any findings of the built-in rules and any gitleaks outcome: a gitleaks that couldn't run
/// is CouldNotRun, whatever the rules found; Clean only when neither found anything; Found never
/// empty.
#[kani::proof]
#[kani::unwind(4)]
fn a_scanner_that_cannot_run_is_never_clean() {
    let found = any_findings();
    let gitleaks = if kani::any() {
        Ok(any_findings())
    } else {
        Err(String::new())
    };
    let failed = gitleaks.is_err();
    let nothing = found.is_empty() && gitleaks.as_ref().is_ok_and(|more| more.is_empty());
    match verdict(found, gitleaks) {
        Verdict::CouldNotRun(why) => {
            assert!(failed);
            std::mem::forget(why);
        }
        Verdict::Clean => assert!(!failed && nothing),
        Verdict::Found(listed) => {
            assert!(!failed && !nothing && !listed.is_empty());
            std::mem::forget(listed);
        }
    }
}
