# The browser downloads on first use, pinned; one per task, four at once

- **Date:** 2026-11-20
- **Status:** In force
- **Replaced:** The lab's way (agent-browser installed with npm and Chrome fetched by `agent-browser install`, only agent-browser's binary hash-checked)
- **Changed by:** [2026-11-20-headless-shell.md](2026-11-20-headless-shell.md) (the browser is chrome-headless-shell, not Chrome for Testing)
- **Contracts and scenarios:** [browser.md](../contracts/browser.md), BROWSER-23, 24 and 25

## The owner's words

Asked how agent-browser and Chrome for Testing reach a person's Mac (bundled in the app, about 400 MB more per platform, or downloaded on first use into obid's home, pinned and hash-checked):

> Download on first use.

The owner's words, as the orchestrator session relayed them.

Asked where the browser runs and its limits (one agent-browser daemon and its Chrome per job as obid's child, at most 4 live, a 15 s per-command deadline, an idle browser closed after 10 minutes):

> One per task, 4 at once.

The owner's words, as the orchestrator session relayed them. Both were said on 2026-11-20 in the owner's own chat.

## Where it is stated

- [browser.md](../contracts/browser.md), "Rules": "Downloaded on first use, pinned" and "One per task, four at once"; BROWSER-23 to 25

## Notes

What each choice means, as put to the owner:

- **Download on first use.** obid downloads agent-browser (17.8 MB, darwin-arm64) and Chrome for Testing (about 377 MB on disk) itself, on the first browser call, into its home. Each is checked against a SHA-256 pinned in obid's source per platform (darwin-arm64, darwin-x64, linux-x64, and linux-arm64 where a build exists: Chrome for Testing has none). A mismatch refuses and never runs. obid downloads Chrome itself rather than through `agent-browser install`, so Chrome is hash-checked too; Google publishes no hashes for Chrome for Testing, so obid records its own. The first call runs under the 60 s start allowance and says it is starting the browser. The app stays small; a Chrome security update needs a new pin in obid.
- **One per task, four at once.** Each job gets one agent-browser daemon and its Chrome as obid's child process, started on the job's first browser call, with its own profile, session, `HOME` and socket folder under obid's home. It is killed when the job ends, and any left over are reaped when obid starts. At most 4 are live: a fifth waits as `browser_starting`, then refuses. Every command has a 15 s deadline. A browser idle for 10 minutes closes. A foreign-lane job's Chrome sits behind the box's egress proxy and can't reach loopback.
