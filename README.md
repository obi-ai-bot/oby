<p align="center">
  <img src="docs/images/oby-logo.png" width="128" alt="Oby">
</p>

<h1 align="center">Oby</h1>

<p align="center">
  <b>Your engineering delegate.</b><br>
  Oby hands your coding work to Claude Code, Codex and Cursor, follows up with your team,<br>
  and only calls you when a decision is yours.
</p>

<p align="center">
  <a href="https://deskree.com/oby">Website</a> · macOS 26+ · Apache-2.0
</p>

<p align="center">
  <img src="docs/images/app-chat.webp" width="820" alt="The Oby window: a conversation with Oby, with the agents working on the right">
</p>

> [!NOTE]
> Oby 0.4.0, the launch build, is out: [download it](https://downloads.oby.deskree.com/Oby-0.4.0.dmg) for macOS 26 or later. It updates itself. The iPhone app is on TestFlight.

## What it does

- **Delegates your coding work.** Ask for a review, a fix for failing CI, or release notes. Oby picks the agent, account and repository, and gives every job its own branch.
- **Uses the plans you already have.** Claude Code, Codex and Cursor sign in through their own tools. Oby never sees your passwords and shows how much of each plan is left.
- **Keeps the work moving.** It follows each job, nudges it when it stalls, and opens the pull request once checks pass and the secret scan is clean.
- **Only interrupts you for decisions.** When something needs you, you get the finding, the evidence and a clear next step.
- **Works with your team.** Everyone has their own Oby. Your team's Obysphere is a server you host yourself, free on Cloudflare from one Deploy to Cloudflare button ([obi-sphere](https://github.com/obi-ai-bot/obi-sphere)). Obys ask each other for reviews there.
- **Reaches your Mac from your iPhone.** Through your Obysphere, sealed end to end: the Sphere passes frames it can't open, and the iPhone checks the Mac's key from the QR code it paired with. The iPhone app is on TestFlight.
- **Connects to GitHub and Slack.** It follows your pull requests and issues, and can tell a pull request's author on Slack when a review is in.

## You stay in control

- Oby merges the way each repository works, as it learned from you: it asks "Can I remember that?" before it keeps a rule, and says so when it follows one. In a repository it hasn't learned yet, it opens the pull request and asks before any merge, and it never pushes to, merges into or force-pushes `main` or your default branch on its own.
- Nothing goes out until the job's check passes and the secret scan is clean. A review goes out as you only after your Approve.
- Approvals (publishing by hand, a Confirm, a Disconnect) are a click in Oby on your Mac or iPhone, with no Touch ID, Face ID or password; whatever waits on you notifies on the Mac and shows on Home.
- Every job has safety limits on time and tokens, and **⌃⌥⌘.** stops them all.
- No usage data leaves your Mac unless you turn it on.

## Install

[Download Oby](https://downloads.oby.deskree.com/Oby-0.4.0.dmg), move it to Applications and open it. Its engine, `obid`, starts on this Mac by itself, and first launch takes a few minutes:

1. **Hello**, then **hold fn to talk** (Oby asks for Accessibility, so it notices fn in any app).
2. **Your agents.** Apple's command-line tools (git comes with them): Install runs macOS's own installer. Then Claude Code, Codex and Cursor: logins already on your Mac show Connected by themselves. A tool that isn't installed has Copy Command; run it in Terminal, sign in there, and its row turns Connected without restarting anything. Connect signs in a tool that's installed but signed out, through its own login. One agent is enough.
3. **GitHub.** gh's own sign-in, with a code to paste; then every repository your GitHub login can push to is ready.

Home opens with a first task in its box. Updating from 0.3.0, Oby shows only the steps you haven't done yet, once.

## Joining Deskree

1. [Download Oby](https://downloads.oby.deskree.com/Oby-0.4.0.dmg) and go through first launch (above).
2. Open the team invite link you were sent and click **Open in Oby**: Settings › Obysphere opens on **Join Deskree**. Opened before or during first launch, it waits and opens when first launch closes. Without a link, the sidebar's + opens the same page: paste the link there.
3. On your iPhone, install Oby from the TestFlight invite, open it, tap **Connect** and scan the code from **Pair a device** in Settings › Runner on your Mac. The iPhone reaches your Mac through Deskree's Obysphere, sealed end to end.

## Build from source

To build it yourself, you need macOS 26 or later and Xcode 27.

1. Open `apps/Oby.xcodeproj`.
2. Run the **Oby** scheme on My Mac.

The Mac app runs Oby's server, `obid`, for you (Settings › Runner › This Mac, on by default). To run `obid` on its own (on Linux, for example), see [docs/install.md](docs/install.md).

## Repository

```
apps/        the Mac app (SwiftUI), plus iPhone, Apple Watch, Android and web clients
core/        obid, Oby's server (Rust): plans, runs and tracks jobs
character/   Oby, the 3D character (three.js)
plugins/     optional context sources, such as Tetrix
mcp/         Oby's MCP server
docs/        architecture, runner API and install guides
```

More in [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

## Contributing

Branch from `dev` and open pull requests into `dev`. Commits follow [Conventional Commits](https://www.conventionalcommits.org). `dev` is merged into `main` once a feature's tests pass on a real Mac. See [AGENTS.md](AGENTS.md) for the full workflow and rules.

## License

[Apache-2.0](LICENSE). Made by [Deskree](https://deskree.com).
