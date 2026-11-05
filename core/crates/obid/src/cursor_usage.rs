//! A Cursor account's usage, from Cursor's own dashboard service (the owner's decision of
//! 2026-11-03; docs/contracts/usage.md, USAGE-6 to 9; docs/contracts/keychain.md, KEYCHAIN-8 and 9).
//! Cursor's CLI reports none, so obid asks what Cursor's own app and CLI ask, with the account's
//! access token. Only obid does this, never in a box or for an agent.
//!
//! These endpoints are undocumented and can change: an answer that doesn't read just means no
//! number. Nothing of an answer but a number and a time goes further (crate::usage::cursor), and no
//! failure says more than its status.
use crate::usage::{Asked, cursor};
use serde_json::Value;
use std::time::Duration;

/// Where the token goes, and nowhere else: this exact host, over TLS. Not settable.
const HOST: &str = "https://api2.cursor.sh";
const CURRENT: &str = "/aiserver.v1.DashboardService/GetCurrentPeriodUsage";
const PLAN: &str = "/aiserver.v1.DashboardService/GetPlanInfo";
/// How long each of the two requests gets.
const LIMIT: Duration = Duration::from_secs(10);

/// A stand-in for [`HOST`], in obid's own unit tests only: no build of obid can be pointed elsewhere.
#[cfg(test)]
pub(crate) static STAND_IN: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

fn host() -> String {
    #[cfg(test)]
    if let Some(host) = STAND_IN.lock().unwrap().clone() {
        return host;
    }
    HOST.to_owned()
}

/// What Cursor says of the account `token` signs in: a figure, or [`Asked::Silent`] for a plan or
/// an answer with none. `Err` says only what failed, never what came back.
pub async fn read_usage(token: &str) -> Result<Asked, String> {
    read_from(&host(), token, LIMIT).await
}

async fn read_from(host: &str, token: &str, limit: Duration) -> Result<Asked, String> {
    // Redirects off: one would carry the request off this host. No proxy from obid's environment.
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .timeout(limit)
        .build()
        .map_err(|_| "obid has no HTTP client".to_owned())?;
    let current = ask(&client, host, CURRENT, token).await?;
    let Some(mut usage) = cursor(&current, &Value::Null) else {
        return Ok(Asked::Silent);
    };
    // The plan only says when its included usage resets: without it, the billing cycle's end.
    let plan = ask(&client, host, PLAN, token).await.unwrap_or(Value::Null);
    usage.seven_day_resets = crate::usage::cursor_reset(&current, &plan);
    Ok(Asked::Said(usage))
}

/// One Connect call over JSON, its answer read up to [`crate::tetrix::small_json`]'s size. The
/// transport's own error text stays out.
async fn ask(
    client: &reqwest::Client,
    host: &str,
    path: &str,
    token: &str,
) -> Result<Value, String> {
    let answer = client
        .post(format!("{host}{path}"))
        .bearer_auth(token)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .header("Connect-Protocol-Version", "1")
        .body("{}")
        .send()
        .await
        .map_err(|_| "Cursor didn't answer".to_owned())?;
    let status = answer.status();
    if !status.is_success() {
        return Err(format!("Cursor answered {}", status.as_u16()));
    }
    Ok(crate::tetrix::small_json(answer).await)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::usage::Usage;
    use axum::http::{HeaderMap, StatusCode, header};
    use axum::response::IntoResponse;
    use std::sync::{Arc, Mutex};

    pub(crate) const TOKEN: &str = "cursor-test-access-token-0123456789";
    /// Set in a run of this test binary made by [`rerun`].
    pub(crate) const RERUN: &str = "OBID_TEST_RERUN";
    /// One test at a time points [`STAND_IN`] at its own stand-in.
    pub(crate) static STAND_IN_TAKEN: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

    /// Runs the test named `test` again in a process of its own, with `env` (what a test can't set
    /// in this process, where others run), and requires that it ran and passed.
    pub(crate) async fn rerun(test: &str, env: &[(&str, &str)]) {
        let output = tokio::process::Command::new(std::env::current_exe().unwrap())
            .args([test, "--exact", "--nocapture"])
            .env(RERUN, "1")
            .env_remove("NO_PROXY")
            .env_remove("no_proxy")
            .envs(env.iter().copied())
            .output()
            .await
            .unwrap();
        let said = String::from_utf8_lossy(&output.stdout);
        assert!(
            output.status.success() && said.contains("test result: ok. 1 passed"),
            "{said}\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    /// What a stand-in was asked: each request's path, headers and body.
    pub(crate) type Asks = Arc<Mutex<Vec<(String, HeaderMap, String)>>>;
    /// What a stand-in answers: a status, a redirect's `Location`, a body.
    pub(crate) type Answer = (u16, Option<String>, String);

    /// A stand-in for Cursor's host on loopback, answering by the request's path.
    pub(crate) async fn stand_in(
        answer: impl Fn(&str) -> Answer + Send + Sync + 'static,
    ) -> (String, Asks) {
        let asks: Asks = Arc::default();
        let (noted, answer) = (asks.clone(), Arc::new(answer));
        let app = axum::Router::new().fallback(
            move |uri: axum::http::Uri, headers: HeaderMap, body: String| {
                let (noted, answer) = (noted.clone(), answer.clone());
                async move {
                    noted
                        .lock()
                        .unwrap()
                        .push((uri.path().to_owned(), headers, body));
                    let (status, location, body) = answer(uri.path());
                    let mut reply = HeaderMap::new();
                    if let Some(location) = location {
                        reply.insert(header::LOCATION, location.parse().unwrap());
                    }
                    (StatusCode::from_u16(status).unwrap(), reply, body).into_response()
                }
            },
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let host = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await });
        (host, asks)
    }

    pub(crate) fn body(status: u16, body: &str) -> Answer {
        (status, None, body.to_owned())
    }

    async fn read(
        answer: impl Fn(&str) -> Answer + Send + Sync + 'static,
    ) -> Result<Asked, String> {
        let (host, _) = stand_in(answer).await;
        read_from(&host, TOKEN, LIMIT).await
    }

    fn figure(asked: Result<Asked, String>) -> Usage {
        match asked {
            Ok(Asked::Said(usage)) => usage,
            other => panic!("{other:?}"),
        }
    }

    /// The month's answer the owner's account gave on 2026-11-03, in its shape.
    const CURRENT_ANSWER: &str = r#"{"billingCycleStart":"1793000000000","billingCycleEnd":"1795000000000",
        "planUsage":{"autoPercentUsed":10.5,"apiPercentUsed":20,"totalPercentUsed":30.5},
        "spendLimitUsage":{},"displayMessage":"DISPLAY-MESSAGE-WORDS"}"#;

    #[tokio::test]
    async fn usage_7_a_monthly_plan_reads_its_total_and_its_cycles_end() {
        let usage = figure(
            read(|path| match path {
                CURRENT => body(200, CURRENT_ANSWER),
                _ => body(200, r#"{"planInfo":{"planName":"Pro"}}"#),
            })
            .await,
        );
        assert_eq!(usage.seven_day, Some(0.305));
        assert_eq!(usage.seven_day_resets, Some(1_795_000_000));
    }

    #[tokio::test]
    async fn usage_7_a_weekly_plan_reads_its_weeks_reset() {
        let usage = figure(
            read(|path| match path {
                CURRENT => body(200, CURRENT_ANSWER),
                _ => body(
                    200,
                    r#"{"planInfo":{"planName":"Ultra","includedUsagePeriod":"INCLUDED_USAGE_PERIOD_WEEKLY","includedUsageResetsAt":"1794000000000"}}"#,
                ),
            })
            .await,
        );
        assert_eq!(usage.seven_day_resets, Some(1_794_000_000));
    }

    #[test]
    fn usage_7_a_limit_above_zero_is_included_spend_over_it() {
        let current = |used: Value| serde_json::json!({ "planUsage": used });
        let read = |used| cursor(&current(used), &Value::Null).unwrap().seven_day;
        assert_eq!(
            read(serde_json::json!({"limit": 2000, "includedSpend": 500, "totalPercentUsed": 99})),
            Some(0.99)
        );
        assert_eq!(
            cursor(&current(serde_json::json!({"limit": 2000})), &Value::Null),
            None
        );
        assert_eq!(
            read(serde_json::json!({"limit": 0, "totalPercentUsed": 40})),
            Some(0.4)
        );
        // Past the included usage, the bar is full, and only that.
        assert_eq!(
            read(serde_json::json!({"limit": 100, "includedSpend": 250})),
            Some(1.0)
        );
    }

    #[tokio::test]
    async fn usage_8_no_plan_usage_is_no_number_and_no_second_request() {
        let (host, asks) = stand_in(|_| body(200, r#"{"billingCycleEnd":"1795000000000"}"#)).await;
        assert_eq!(read_from(&host, TOKEN, LIMIT).await, Ok(Asked::Silent));
        assert_eq!(asks.lock().unwrap().len(), 1);
        // A `planUsage` with no figure says none, not a made-up 0%.
        assert_eq!(
            cursor(&serde_json::json!({"planUsage": {}}), &Value::Null),
            None
        );
    }

    #[test]
    fn usage_7_the_highest_reported_share_wins_including_auto_only() {
        for (used, expected) in [
            (
                serde_json::json!({"limit": 2000, "includedSpend": 0, "autoPercentUsed": 72}),
                Some(0.72),
            ),
            (
                serde_json::json!({"limit": 2000, "autoPercentUsed": 72}),
                Some(0.72),
            ),
            (serde_json::json!({"autoPercentUsed": 72}), Some(0.72)),
            (
                serde_json::json!({"limit": 2000, "includedSpend": "bad", "totalPercentUsed": "99", "autoPercentUsed": 72}),
                Some(0.72),
            ),
            (
                serde_json::json!({"apiPercentUsed": 81, "totalPercentUsed": 20, "autoPercentUsed": 30}),
                Some(0.81),
            ),
            (
                serde_json::json!({"includedSpend": 90, "limit": 100, "totalPercentUsed": 20}),
                Some(0.9),
            ),
            (
                serde_json::json!({"limit": 100, "includedSpend": "90", "totalPercentUsed": null, "autoPercentUsed": false, "apiPercentUsed": "81"}),
                None,
            ),
            (serde_json::json!({"limit": 0, "includedSpend": 90}), None),
            (serde_json::json!({"autoPercentUsed": 0}), Some(0.0)),
        ] {
            let current = serde_json::json!({"planUsage": used});
            assert_eq!(
                cursor(&current, &Value::Null).and_then(|usage| usage.seven_day),
                expected,
                "{current}"
            );
        }
    }

    #[tokio::test]
    async fn usage_7_one_info_line_has_only_numbers_and_the_winning_field() {
        #[derive(Clone, Default)]
        struct Logged(Arc<Mutex<Vec<u8>>>);
        impl std::io::Write for Logged {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                self.0.lock().unwrap().extend_from_slice(bytes);
                Ok(bytes.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let logged = Logged::default();
        let writer = logged.clone();
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::INFO)
            .with_ansi(false)
            .without_time()
            .with_writer(move || writer.clone())
            .finish();
        let _logging = tracing::subscriber::set_default(subscriber);
        // Another test's thread may have told this callsite nobody listens.
        tracing::callsite::rebuild_interest_cache();
        let usage = figure(read(|_| body(200,
            r#"{"planUsage":{"limit":2000,"includedSpend":0,"autoPercentUsed":72},"displayMessage":"ANSWER-WORDS"}"#
        )).await);
        assert_eq!(usage.seven_day, Some(0.72));
        let said = String::from_utf8(logged.0.lock().unwrap().clone()).unwrap();
        assert_eq!(said.lines().count(), 1, "{said}");
        assert!(
            said.contains("INFO") && said.contains("winner=\"autoPercentUsed\""),
            "{said}"
        );
        assert!(
            said.contains("Some(72.0)") && said.contains("Some(2000.0)"),
            "{said}"
        );
        for words in [TOKEN, "ANSWER-WORDS", "displayMessage"] {
            assert!(!said.contains(words), "{said}");
        }
    }

    #[tokio::test]
    async fn usage_8_a_401_and_a_500_are_no_number_saying_only_the_status() {
        for status in [401, 500] {
            let said = read(move |_| body(status, "ERROR-BODY-WORDS")).await;
            assert_eq!(said, Err(format!("Cursor answered {status}")));
        }
    }

    #[tokio::test]
    async fn usage_8_garbage_is_no_number() {
        for garbage in ["<html>not json</html>", "[1,2]", r#"{"planUsage":"lots"}"#] {
            let garbage = garbage.to_owned();
            assert_eq!(read(move |_| body(200, &garbage)).await, Ok(Asked::Silent));
        }
        let not_numbers = r#"{"planUsage":{"totalPercentUsed":"a lot"}}"#;
        assert_eq!(
            read(move |_| body(200, not_numbers)).await,
            Ok(Asked::Silent)
        );
    }

    #[tokio::test]
    async fn usage_8_a_plan_read_that_fails_keeps_the_figure() {
        let usage = figure(
            read(|path| match path {
                CURRENT => body(200, CURRENT_ANSWER),
                _ => body(500, ""),
            })
            .await,
        );
        assert_eq!(usage.seven_day, Some(0.305));
        assert_eq!(usage.seven_day_resets, Some(1_795_000_000));
    }

    /// The host is exact and the request is Cursor's own; nothing else carries the token.
    #[tokio::test]
    async fn keychain_9_the_token_goes_to_cursors_host_as_cursors_own_app_sends_it() {
        assert_eq!(HOST, "https://api2.cursor.sh");
        let (host, asks) = stand_in(|_| body(200, CURRENT_ANSWER)).await;
        read_from(&host, TOKEN, LIMIT).await.unwrap();
        let asks = asks.lock().unwrap();
        assert_eq!(
            asks.iter().map(|ask| ask.0.as_str()).collect::<Vec<_>>(),
            [CURRENT, PLAN]
        );
        for (_, headers, sent) in asks.iter() {
            assert_eq!(
                headers[header::AUTHORIZATION].to_str().unwrap(),
                format!("Bearer {TOKEN}")
            );
            assert_eq!(headers[header::CONTENT_TYPE], "application/json");
            assert_eq!(headers["connect-protocol-version"], "1");
            assert_eq!(sent, "{}");
        }
    }

    /// A proxy in obid's environment sees nothing: in a run of its own with `HTTP_PROXY` at a
    /// second stand-in (the proxy for this stand-in's `http`; `HTTPS_PROXY` is the same switch for
    /// Cursor's host), the request reaches the host itself.
    #[tokio::test]
    async fn keychain_9_a_proxy_in_obids_environment_is_asked_nothing() {
        if std::env::var_os(RERUN).is_some() {
            let (host, asks) = stand_in(|_| body(200, CURRENT_ANSWER)).await;
            read_from(&host, TOKEN, LIMIT).await.unwrap();
            assert_eq!(asks.lock().unwrap().len(), 2);
            return;
        }
        let (proxy, proxied) = stand_in(|_| body(200, CURRENT_ANSWER)).await;
        rerun(
            "cursor_usage::tests::keychain_9_a_proxy_in_obids_environment_is_asked_nothing",
            &[("HTTP_PROXY", &proxy), ("http_proxy", &proxy)],
        )
        .await;
        assert!(proxied.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn keychain_9_a_redirect_is_refused_and_the_token_stays() {
        let (elsewhere, reached) = stand_in(|_| body(200, CURRENT_ANSWER)).await;
        let location = format!("{elsewhere}{CURRENT}");
        for status in [302, 307, 308] {
            let location = location.clone();
            let said = read(move |_| (status, Some(location.clone()), String::new())).await;
            assert_eq!(said, Err(format!("Cursor answered {status}")));
        }
        assert!(reached.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn keychain_9_a_host_that_doesnt_answer_times_out_saying_nothing_of_it() {
        let quiet = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let host = format!("http://{}", quiet.local_addr().unwrap());
        let at = std::time::Instant::now();
        let said = read_from(&host, TOKEN, Duration::from_millis(200)).await;
        assert_eq!(said, Err("Cursor didn't answer".to_owned()));
        assert!(at.elapsed() < Duration::from_secs(5));
    }

    /// An answer's words go nowhere: what's kept holds its numbers and nothing else.
    #[tokio::test]
    async fn keychain_9_none_of_an_answers_text_comes_through() {
        let usage = figure(
            read(|path| match path {
                CURRENT => body(200, CURRENT_ANSWER),
                _ => body(200, r#"{"planInfo":{"planName":"PLAN-NAME-WORDS"}}"#),
            })
            .await,
        );
        let kept = serde_json::to_string(&usage).unwrap();
        for words in ["DISPLAY-MESSAGE-WORDS", "PLAN-NAME-WORDS", TOKEN] {
            assert!(!kept.contains(words), "{kept}");
        }
    }
}
