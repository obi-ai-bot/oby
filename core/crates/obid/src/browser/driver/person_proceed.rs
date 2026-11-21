//! Go on anyway (BROWSER-29, the owner's "B: headless, trust key", ported from the lab's
//! `proceed` and `trusted_elsewhere`): a page that stopped at a certificate problem has its
//! certificate read by obid; the person's Go on anyway, on a tab they hold, trusts only that exact
//! certificate, read again and named by them; the job's browser starts again with its key in
//! `--ignore-certificate-errors-spki-list` (agent-browser does so by itself when its launch
//! arguments change, on the same profile) and opens the address again. Chrome trusts that key on
//! every host, so obid stops a page on another host whose server presents it.
//!
//! Who may (the holder of the tab) is the API's and the core's to check first. It owns no state:
//! the trusted keys live on the job's [`Session`] in the driver's live map, the rest in the core.

use std::time::Duration;

use super::{Call, Driver, Session, certificate};
use crate::browser::engine::{Refusal, SeenCertificate, is_certificate_error, is_error_page};

/// How long the browser may take to start again and open the address.
const RESTART_LIMIT: Duration = Duration::from_secs(60);

impl Driver {
    /// The person's Go on anyway on `tab` of `job`'s browser, naming the certificate's
    /// `fingerprint`: the page's URL and title after, and the tabs the restart closed.
    pub async fn person_proceed(
        &self,
        job: &str,
        tab: &str,
        fingerprint: &str,
    ) -> Result<(String, Option<String>, Vec<String>), Refusal> {
        let seen = {
            let core = self.core.lock().unwrap();
            core.tab(tab)
                .filter(|t| t.certificate)
                .and_then(|t| t.seen.clone())
        };
        let Some((seen, url)) = seen else {
            return Err(Refusal::NoCertificate);
        };
        let now = certificate::read(&seen.host, seen.port)
            .await
            .map_err(|e| {
                Refusal::Browser(format!(
                    "obid couldn't read the site's certificate again: {e}"
                ))
            })?;
        if now.fingerprint != seen.fingerprint || fingerprint != seen.fingerprint {
            // What the person sees next is the certificate there now.
            self.core.lock().unwrap().certificate_seen(tab, now, &url);
            return Err(Refusal::CertificateChanged);
        }
        let session = {
            let mut live = self.live.lock().unwrap();
            let l = live.get_mut(job).ok_or_else(|| {
                Refusal::Browser(
                    "This job's browser isn't running: it starts again on its agent's next call."
                        .into(),
                )
            })?;
            l.session.trusted.push((now.host.clone(), now.spki.clone()));
            l.session.clone()
        };
        let read = |call: Call| match call {
            Call::Ok(v) => Ok(v),
            Call::Failed(e) => Err(Refusal::Browser(e)),
            Call::TimedOut(ms) => Err(Refusal::TimedOut { ms }),
        };
        read(
            session
                .call(&["open".to_string(), url.clone()], RESTART_LIMIT)
                .await,
        )?;
        let limit = self.options.system_limit;
        let tabs = read(session.call(&["tab".into(), "list".into()], limit).await)?;
        let target = tabs["tabs"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|t| t["active"] == true)
            .and_then(|t| t["targetId"].as_str())
            .unwrap_or_default()
            .to_string();
        let landed = read(session.call(&["get".into(), "url".into()], limit).await)?["url"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let title = match session.call(&["get".into(), "title".into()], limit).await {
            Call::Ok(v) => v["title"].as_str().map(str::to_string),
            _ => None,
        };
        let closed = {
            let mut core = self.core.lock().unwrap();
            let closed = core.restarted(tab, &target);
            core.person_moved(tab, &landed, title.as_deref());
            closed
        };
        if is_error_page(&landed) {
            return Err(Refusal::Browser(
                "Go on anyway: the page still stops at a certificate problem.".into(),
            ));
        }
        Ok((landed, title, closed))
    }

    /// A navigation that failed with `message` going to `url`: when it stopped at a certificate
    /// problem, obid reads the certificate there for the person's Go on anyway.
    pub(super) async fn certificate_stopped(&self, tab: &str, url: Option<&String>, message: &str) {
        if !is_certificate_error(message) {
            return;
        }
        let Some(url) = url else { return };
        let Some((host, port)) = certificate::https_origin(url) else {
            return;
        };
        if let Ok(seen) = certificate::read(&host, port).await {
            self.core.lock().unwrap().certificate_seen(tab, seen, url);
        }
    }

    /// Chrome trusts a trusted key on every host: a page on `url` whose server presents a key the
    /// person trusted for another host is stopped as a certificate page. Answers the host the key
    /// was trusted for and the certificate read here.
    pub(super) async fn trusted_elsewhere(
        &self,
        session: &Session,
        url: &str,
    ) -> Option<(String, SeenCertificate)> {
        if session.trusted.is_empty() {
            return None;
        }
        let (host, port) = certificate::https_origin(url)?;
        let seen = certificate::read(&host, port).await.ok()?;
        if session
            .trusted
            .iter()
            .any(|(h, k)| *h == host && *k == seen.spki)
        {
            return None;
        }
        session
            .trusted
            .iter()
            .find(|(_, k)| *k == seen.spki)
            .map(|(h, _)| (h.clone(), seen))
    }
}
