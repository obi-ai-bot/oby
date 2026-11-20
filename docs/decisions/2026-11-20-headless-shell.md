# The browser is chrome-headless-shell, not Chrome for Testing

- **Date:** 2026-11-20
- **Status:** In force
- **Replaced:** Chrome for Testing as the browser obid downloads and drives ([2026-11-20-browser-install-and-limits.md](2026-11-20-browser-install-and-limits.md), its engine only; downloading on first use, pinned, and the limits stay)
- **Changed by:** Nothing
- **Contracts and scenarios:** [browser.md](../contracts/browser.md), BROWSER-23 and 27

## The owner's words

Asked which browser agents drive: Chrome for Testing (about 191 MB to download), or chrome-headless-shell (about 100 MB, the build T3 Code uses for agents on servers), with the person's panel showing the page's stream either way:

> Let's do the headless then.

Asked whether to keep the headless shell now and maybe add a browser inside Oby's own window later (for Mac tasks while the window is open):

> Okay let's do it!

The owner's words, as the orchestrator session relayed them, said on 2026-11-20 in the owner's own chat.

## Where it is stated

- [browser.md](../contracts/browser.md), "Rules": "Downloaded on first use, pinned"; BROWSER-23
- The lab: `tools/obid-spec/src/lab/browser.rs`, `SHELL_VERSION` and `SHELL_PINNED`

## Notes

- **The pins.** chrome-headless-shell 158.0.8127.36, the version agent-browser 0.43.0 installs as Chrome for Testing. Its zip comes from Google's Chrome for Testing bucket, `https://storage.googleapis.com/chrome-for-testing-public/158.0.8127.36/<platform>/chrome-headless-shell-<platform>.zip`. Google publishes no hashes, so these are ours, taken over HTTPS on 2026-11-20:
  - mac-arm64: `c24d40a3786322ff3a5e3ac6eed162226cbaf07ebe468715407827226b0f8897`, 105,057,133 bytes
  - mac-x64: `e0c5d07d8f5bcc5bdd5ef370e7daf3770856b585fcc9eecf441c7e3bb2e3d1cf`, 112,306,247 bytes
  - linux64: `2d3a23017323c54fd457a678e4351f895c4076d8050e39d5aa6247f93448b73a`, 132,087,165 bytes
  - There is no linux-arm64 build, as with Chrome for Testing.
- **What the person sees doesn't change.** The panel shows the page's stream either way, and agent-browser drives both the same way.
- **The in-window browser is deferred.** That would be Electron's own webview, with no download. It may come after the launch for Mac tasks while the window is open.
- **No certificate page.** chrome-headless-shell has no certificate warning page of its own: a certificate error leaves a blank `chrome-error://chromewebdata/`, and `certificateErrorPageController` is `undefined` there. The rule that agents can't act on the browser's error pages (BROWSER-27) holds as it is. But Chrome's own "proceed" can't carry the person's "Go on anyway", which the owner also decided on 2026-11-20 and which isn't built yet.
