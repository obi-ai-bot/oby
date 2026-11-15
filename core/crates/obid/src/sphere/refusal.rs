//! What obid shows of a Sphere's refusal (docs/contracts/teammate-review.md, TEAMREVIEW-8; ASK-8):
//! of all the Sphere wrote, only its code, and only when it is one, `[a-z0-9_]{1,40}`. Its message
//! never reaches the owner: obid says it in its own words.

/// Whether `code` is a code obid may show: 1 to 40 bytes of `a`-`z`, `0`-`9` and `_`.
pub fn is_code(code: &[u8]) -> bool {
    let fits = (1..=40).contains(&code.len());
    fits && code
        .iter()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'_')
}

/// What obid says of a refusal with `code`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shown {
    /// `policy_denied`: the Sphere's rules, which an admin can change. obid's fixed sentence.
    PolicyDenied,
    /// Another code: it, in obid's sentence.
    Code,
    /// Not a code: that it was refused, and nothing the Sphere wrote.
    Nothing,
}

/// What obid shows of a refusal whose code is `code`.
pub fn shown(code: &[u8]) -> Shown {
    if code == b"policy_denied" {
        Shown::PolicyDenied
    } else if is_code(code) {
        Shown::Code
    } else {
        Shown::Nothing
    }
}

#[cfg(test)]
mod proofs {
    #![cfg(kani)]
    //! Kani's proof of [`shown`] (TEAMREVIEW-8): `kani::any()` stands for every code of up to 41
    //! bytes.

    use super::*;

    /// TEAMREVIEW-8: a code is shown exactly when it is 1 to 40 bytes of `a`-`z`, `0`-`9` and
    /// `_`, and the `policy_denied` sentence only for exactly `policy_denied`.
    #[kani::proof]
    #[kani::unwind(42)]
    fn teamreview_8_only_a_code_is_shown() {
        let bytes: [u8; 41] = kani::any();
        let len: usize = kani::any();
        kani::assume(len <= 41);
        let code = &bytes[..len];
        let mut code_like = len >= 1 && len <= 40;
        let policy = *b"policy_denied";
        let mut denied = len == policy.len();
        let mut at = 0;
        while at < len {
            let byte = code[at];
            code_like &= byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_';
            denied &= at < policy.len() && byte == policy[at];
            at += 1;
        }
        let said = shown(code);
        assert_eq!(said == Shown::PolicyDenied, denied);
        assert_eq!(said != Shown::Nothing, code_like);
        assert_eq!(is_code(code), code_like);
    }
}
