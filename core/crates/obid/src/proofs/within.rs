//! Egress host matching: a host is a tool's own service only when it is the domain or a true
//! subdomain of it, never a look-alike that merely ends with the same letters
//! (`evilanthropic.com`), and only on port 443.
//! Proves: core/crates/obid/src/egress.rs

use super::any_str;
use crate::agent::Tool;
use crate::egress::{tools_own, within};

/// `within(host, domain)` holds exactly when host is domain, or ends with `.` + domain, for any
/// two strings of up to 5 bytes.
#[kani::proof]
#[kani::unwind(8)]
fn within_is_the_domain_or_a_true_subdomain() {
    let (host, domain) = (any_str::<5>(), any_str::<5>());
    let dotted = host.len() > domain.len()
        && host.ends_with(domain.as_str())
        && host.as_bytes()[host.len() - domain.len() - 1] == b'.';
    assert_eq!(within(&host, &domain), host == domain || dotted);
}

/// `domain`, one of `tool`'s own, is its service on port 443 and on no other port, and a host one
/// byte off it (in front, without a dot: `xopenai.com`) is never the tool's, on any of its domains.
fn own_on_443_only_and_never_a_look_alike(tool: Tool, domain: &str) {
    let port: u16 = kani::any();
    assert_eq!(tools_own(tool, domain, port), port == 443);
    let byte: u8 = kani::any();
    kani::assume(byte.is_ascii() && byte != b'.');
    // Built by hand: `format!` costs CBMC more than the property.
    let mut look_alike = String::from(byte as char);
    look_alike.push_str(domain);
    assert!(!tools_own(tool, &look_alike, 443));
}

/// Every tool's own domains are the ones proved below: a new one fails here until it has its own.
#[kani::proof]
fn every_own_domain_has_its_proof() {
    assert_eq!(Tool::Claude.row().own_domains.len(), 3);
    assert_eq!(Tool::Codex.row().own_domains.len(), 2);
    assert_eq!(Tool::Cursor.row().own_domains.len(), 2);
}

// One proof per domain: with the domain fixed, CBMC proves each quickly, where one proof choosing
// among all of them ran out of memory.
macro_rules! each_domain {
    ($($tool:ident $i:literal $name:ident),* $(,)?) => {
        $(
            #[kani::proof]
            #[kani::unwind(20)]
            fn $name() {
                let domain = Tool::$tool.row().own_domains[$i];
                own_on_443_only_and_never_a_look_alike(Tool::$tool, domain);
            }
        )*
    };
}

each_domain! {
    Claude 0 claude_anthropic_com_is_its_own_on_443_only,
    Claude 1 claude_claude_ai_is_its_own_on_443_only,
    Claude 2 claude_claude_com_is_its_own_on_443_only,
    Codex 0 codex_openai_com_is_its_own_on_443_only,
    Codex 1 codex_chatgpt_com_is_its_own_on_443_only,
    Cursor 0 cursor_cursor_sh_is_its_own_on_443_only,
    Cursor 1 cursor_cursor_com_is_its_own_on_443_only,
}
