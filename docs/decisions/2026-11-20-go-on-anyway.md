# Go on anyway trusts that certificate's key, for that host, on the person's click

- **Date:** 2026-11-20
- **Status:** In force
- **Replaced:** Nothing AGENTS.md names (the contract said going on past a certificate was the owner's decision still to make)
- **Changed by:** Nothing
- **Contracts and scenarios:** [browser.md](../contracts/browser.md), BROWSER-27 and 28

## The owner's words

Asked how the person goes on past a certificate the browser doesn't trust:

> And go on anyway to trust certificates.

The orchestrator put the meaning as the person's signed Go on anyway trusting that host plus that exact certificate, a different certificate still refused, and the agent never proceeding itself.

chrome-headless-shell ([2026-11-20-headless-shell.md](2026-11-20-headless-shell.md)) turned out to have no certificate page of its own, so Chrome's own "proceed" (its per-host exception) isn't there. Three ways were put to the owner: (A) keep Chrome for Testing for its certificate page, (B) headless shell, with the certificate's key trusted at the browser's start, or (C) no Go on anyway at launch. The owner's answer:

> B: headless, trust key

Both are the owner's words as the orchestrator session relayed them, said on 2026-11-20 in the owner's own chat.

## Where it is stated

- [browser.md](../contracts/browser.md), "Rules": "The certificate page is the person's", and BROWSER-28
- [docs/api/browser.md](../api/browser.md), `POST /v1/browser/tabs/{id}/proceed`

## Notes

What was measured on chrome-headless-shell 158.0.8127.36 with agent-browser 0.43.0, before building it:

- **No certificate page.** A certificate error leaves a blank `chrome-error://chromewebdata/`: no `certificateErrorPageController`, no Advanced button or Proceed link.
- **The key flag works.** `--ignore-certificate-errors-spki-list=<the SHA-256 of the certificate's SubjectPublicKeyInfo, base64>` lets that certificate load.
- **Chrome trusts that key on every host.** The trusted certificate also loaded on another port and as `https://localhost`, a name not even in the certificate. So obid ties the trust to the host itself: a page on another host whose server presents a trusted key is stopped as a certificate page, checked when it is opened or navigated to and right before every act. What a page loads from another host with that key (an image, a script, a frame) isn't checked.
- **A different certificate is still refused.** Another certificate on the same host stops at the certificate page again.
- **What the restart loses.** agent-browser starts the browser again by itself when its launch arguments change, on the same profile. Gone: every tab but the one reopened, and cookies kept only for the browser's session (a sign-in that doesn't "remember me"). Kept: cookies the sites kept and their local storage.
- **The trust lasts as long as that browser runs.** A browser closed after 10 minutes idle starts again without it, and the person goes on again if they need to.
- **T3 Code** (at 304cf149c8) has no Go on anyway for its agents' browser: a certificate error just fails.
