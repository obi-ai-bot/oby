//! What the certificate rule reads (BROWSER-27): a failed navigation's words naming a certificate
//! problem, the browser's own error page by its URL, and which tools work the page as it is. The
//! rule itself is [`super::Browsers::page_allows`]. It owns no state and writes nothing.

use super::{Kind, Tool};

/// A failed navigation's words say the page stopped at a certificate problem (Chrome's
/// `net::ERR_CERT_*`).
pub fn is_certificate_error(message: &str) -> bool {
    message.to_ascii_uppercase().contains("ERR_CERT_")
}

/// The browser's own error page (Chrome's interstitials, its certificate page among them, live at
/// `chrome-error://chromewebdata/`): not a page of the web, and its buttons aren't the agent's.
pub fn is_error_page(url: &str) -> bool {
    url.starts_with("chrome-error://")
}

/// Whether a tool acts on, or runs code in, the page as it is. Navigating elsewhere and closing the
/// tab leave the page instead, so they are never stopped by the page they leave.
pub fn works_the_page(tool: &Tool) -> bool {
    tool.kind == Kind::Act && !matches!(tool.name, "oby_browser_navigate" | "oby_browser_close_tab")
}

#[cfg(test)]
mod tests {
    use super::super::tool;
    use super::*;

    #[test]
    fn browser_27_what_the_rule_reads() {
        assert!(is_certificate_error(
            "Navigation failed: net::ERR_CERT_AUTHORITY_INVALID"
        ));
        assert!(is_certificate_error("net::err_cert_date_invalid"));
        assert!(!is_certificate_error("net::ERR_CONNECTION_REFUSED"));
        assert!(is_error_page("chrome-error://chromewebdata/"));
        assert!(!is_error_page("https://127.0.0.1:1/"));
        for (name, works) in [
            ("oby_browser_act", true),
            ("oby_browser_evaluate", true),
            ("oby_browser_dialog", true),
            ("oby_browser_resize", true),
            ("oby_browser_set_appearance", true),
            ("oby_browser_navigate", false),
            ("oby_browser_close_tab", false),
            ("oby_browser_open", false),
            ("oby_browser_snapshot", false),
            ("oby_browser_screenshot", false),
            ("oby_browser_wait_for", false),
            ("oby_browser_list_tabs", false),
        ] {
            assert_eq!(works_the_page(tool(name).unwrap()), works, "{name}");
        }
    }
}
