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
> Oby 0.4.0 is out: [download it](https://downloads.oby.deskree.com/Oby-0.4.0.dmg) for macOS 26 or later. It updates itself. It runs on the Mac only, hands work to Claude Code, Codex and Cursor, connects to GitHub and Tetrix, and asks your teammates' Obys for reviews through your Obysphere.

## What it does

- **Delegates your coding work.** Ask for a review, a fix for failing CI, or release notes. Oby picks the agent, account and repository, and gives every job its own branch.
- **Uses the plans you already have.** Claude Code, Codex and Cursor sign in through their own tools. Oby never sees your passwords and shows how much of each Claude Code and Codex plan is left.
- **Cursor today.** Its file edits can't be gated yet, so Ask mode works as Accept edits on Cursor. It shows no usage meter, and Oby's review of a finished job never runs on it. Its questions reach you (#1456), its Plan mode waits for your OK (#1452), and a repository's hooks ask before they run (#1454). More in [docs/cursor.md](docs/cursor.md).
- **Keeps the work moving.** It follows each job, nudges it when it stalls, and opens the pull request once checks pass and the secret scan is clean.
- **Only interrupts you for decisions.** When something needs you, you get the finding, the evidence and a clear next step.
- **Works with your team.** Everyone has their own Oby. Your team's Obysphere is a server you host yourself, free on Cloudflare from one Deploy to Cloudflare button ([obi-sphere](https://github.com/obi-ai-bot/obi-sphere)). Obys ask each other for reviews there. Until #1233 lands, what Obys send each other (review summaries, questions, answers) isn't sealed end to end: the Obysphere's server, and members who read its audit log, can read it.
- **Connects to GitHub and Tetrix.** It follows your pull requests and issues on GitHub, and every agent gets Tetrix's graph through the one sign-in you make in Oby.

## You stay in control

- Oby merges a pull request the way it learned for that repository, once it asked "Can I remember that?" and you said yes. Until it has learned one, it asks "Ready to merge?" and leaves the pull request open. It never pushes to or force-pushes `main` or your default branch, and never merges into `main`.
- Nothing goes out until the job's check passes and the secret scan is clean. A review goes out as you only after your Approve.
- Approvals (publishing by hand, a Confirm, a Disconnect) are a click in Oby on your Mac, with no Touch ID, Face ID or password; whatever waits on you notifies on the Mac and shows on Home.
- Every job has safety limits on time and tokens, and **⌃⌥⌘.** stops them all.
- No usage data leaves your Mac unless you turn it on.

## Install

Download Oby, move it to Applications and open it. Its engine, `obid`, starts on this Mac by itself (Settings › Runner › This Mac), and first launch takes about two minutes: hello, hold fn to talk, your agents (logins already on your Mac show Connected; Connect signs in a tool that's installed but signed out, through its own login), then GitHub, after which every repository your GitHub login can push to is ready. Home opens with a first task in its box.

## Build from source

To build it yourself, you need macOS 26 or later and Xcode 27.

1. Open `apps/Oby.xcodeproj`.
2. Run the **Oby** scheme on My Mac.

The Mac app runs Oby's server, `obid`, for you (Settings › Runner › This Mac, on by default). To run `obid` on its own (on Linux, for example), see [docs/install.md](docs/install.md).

## Repository

```
apps/        the Mac app (SwiftUI); the iPhone, Apple Watch, Android and web clients wait until it's right
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
