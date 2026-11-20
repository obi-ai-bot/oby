//! A route of the browser's contract, parsed from the method and path (every route of the
//! contract's `ROUTES`, BROWSER-26's ways out among them), and which carry the owner's signed words
//! or a secret. It owns no state and writes nothing.

use crate::browser::contract as c;

/// A route of the contract, parsed from the method and path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Route {
    Tabs,
    Events,
    GetSettings,
    PutSettings,
    Downloads,
    SaveCopy(String),
    Stream(String),
    Input(String),
    Take(String),
    GiveBack(String),
    Secret(String),
    Reload(String),
    Back(String),
    Dialog(String),
    Proceed(String),
    Answer(String),
    Mcp,
    Preflight,
    NotFound,
}

/// An id in a route: letters, digits, `-` and `_`, at most 64.
fn id_ok(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
}

pub fn route(method: &str, path: &str) -> Route {
    let path = path.split('?').next().unwrap_or(path);
    if method == "OPTIONS" {
        return Route::Preflight;
    }
    let parts: Vec<&str> = path.trim_end_matches('/').split('/').collect();
    let id = |i: usize| parts.get(i).filter(|s| id_ok(s)).map(|s| s.to_string());
    match (method, parts.as_slice()) {
        ("GET", ["", "v1", "browser"]) => Route::Tabs,
        ("GET", ["", "v1", "browser", "events"]) => Route::Events,
        ("GET", ["", "v1", "browser", "settings"]) => Route::GetSettings,
        ("PUT", ["", "v1", "browser", "settings"]) => Route::PutSettings,
        ("GET", ["", "v1", "browser", "downloads"]) => Route::Downloads,
        ("POST", ["", "v1", "browser", "mcp"]) => Route::Mcp,
        ("POST", ["", "v1", "browser", "downloads", _, v]) if *v == c::VERB_SAVE_COPY => {
            id(4).map(Route::SaveCopy).unwrap_or(Route::NotFound)
        }
        ("POST", ["", "v1", "browser", "asks", _, v]) if *v == c::VERB_ANSWER => {
            id(4).map(Route::Answer).unwrap_or(Route::NotFound)
        }
        (m, ["", "v1", "browser", "tabs", _, verb]) => {
            let Some(tab) = id(4) else {
                return Route::NotFound;
            };
            match (m, *verb) {
                ("GET", v) if v == c::VERB_STREAM => Route::Stream(tab),
                ("POST", v) if v == c::VERB_INPUT => Route::Input(tab),
                ("POST", v) if v == c::VERB_TAKE => Route::Take(tab),
                ("POST", v) if v == c::VERB_GIVE_BACK => Route::GiveBack(tab),
                ("POST", v) if v == c::VERB_SECRET => Route::Secret(tab),
                ("POST", v) if v == c::VERB_RELOAD => Route::Reload(tab),
                ("POST", v) if v == c::VERB_BACK => Route::Back(tab),
                ("POST", v) if v == c::VERB_DIALOG => Route::Dialog(tab),
                ("POST", v) if v == c::VERB_PROCEED => Route::Proceed(tab),
                _ => Route::NotFound,
            }
        }
        _ => Route::NotFound,
    }
}

impl Route {
    /// The owner's own acts: they carry the device's signed words.
    pub fn signed(&self) -> bool {
        matches!(
            self,
            Route::PutSettings
                | Route::SaveCopy(_)
                | Route::Take(_)
                | Route::GiveBack(_)
                | Route::Secret(_)
                | Route::Reload(_)
                | Route::Back(_)
                | Route::Dialog(_)
                | Route::Proceed(_)
                | Route::Answer(_)
        )
    }

    /// Whether its request body may hold a secret, which no log line carries.
    pub fn carries_secret(&self) -> bool {
        matches!(self, Route::Secret(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routes_parse_as_the_contract_lists_them() {
        assert_eq!(route("GET", "/v1/browser"), Route::Tabs);
        assert_eq!(route("GET", "/v1/browser/events"), Route::Events);
        assert_eq!(route("PUT", "/v1/browser/settings"), Route::PutSettings);
        assert_eq!(
            route("POST", "/v1/browser/tabs/tab-1/take"),
            Route::Take("tab-1".into())
        );
        assert_eq!(
            route("POST", "/v1/browser/tabs/tab-1/giveBack"),
            Route::GiveBack("tab-1".into())
        );
        assert_eq!(
            route("GET", "/v1/browser/tabs/tab-1/stream"),
            Route::Stream("tab-1".into())
        );
        assert_eq!(
            route("POST", "/v1/browser/downloads/dl-2/saveCopy"),
            Route::SaveCopy("dl-2".into())
        );
        assert_eq!(
            route("POST", "/v1/browser/asks/ask-3/answer"),
            Route::Answer("ask-3".into())
        );
        assert_eq!(route("POST", "/v1/browser/mcp"), Route::Mcp);
        assert_eq!(
            route("POST", "/v1/browser/tabs/tab-1/reload"),
            Route::Reload("tab-1".into())
        );
        assert_eq!(
            route("POST", "/v1/browser/tabs/tab-1/back"),
            Route::Back("tab-1".into())
        );
        assert_eq!(
            route("POST", "/v1/browser/tabs/tab-1/dialog"),
            Route::Dialog("tab-1".into())
        );
        assert_eq!(
            route("GET", "/v1/browser/tabs/tab-1/reload"),
            Route::NotFound
        );
        for r in [
            Route::Reload("t".into()),
            Route::Back("t".into()),
            Route::Dialog("t".into()),
        ] {
            assert!(r.signed(), "{r:?} is the owner's own act");
        }
        assert_eq!(route("GET", "/v1/browser/tabs/../take"), Route::NotFound);
        assert_eq!(route("GET", "/v1/browser/tabs/tab-1/take"), Route::NotFound);
        // Every route of the contract parses to something.
        for r in c::ROUTES {
            let (m, p) = r.split_once(' ').unwrap();
            assert_ne!(route(m, &p.replace("{id}", "x-1")), Route::NotFound, "{r}");
        }
    }
}
