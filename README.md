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
> Oby runs on the Mac, hands work to Claude Code, Codex and Cursor, connects to GitHub and Tetrix, and asks your teammates' Obys for reviews.

## What it does

- **Delegates your coding work.** Ask for a review, a fix for failing CI, or release notes. Oby picks the agent, account and repository, and gives every job its own branch.
- **Uses the plans you already have.** Claude Code, Codex and Cursor sign in through their own tools. Oby never sees your passwords and shows how much of each Claude Code and Codex plan is left.
- **Cursor today.** Its file edits can't be gated yet, so Ask mode works as Accept edits on Cursor. It shows no usage meter, and Oby's review of a finished job never runs on it. Its questions reach you (#1456), its Plan mode waits for your OK (#1452), and a repository's hooks ask before they run (#1454). More in [docs/cursor.md](docs/cursor.md).
- **Keeps the work moving.** It follows each job, nudges it when it stalls, and opens the pull request once checks pass and the secret scan is clean.
- **Only interrupts you for decisions.** When something needs you, you get the finding, the evidence and a clear next step.
- **Works with your team.** Everyone has their own Oby. Your team's Obysphere is a server you host yourself, free on Cloudflare from one Deploy to Cloudflare button ([obi-sphere](https://github.com/obi-ai-bot/obi-sphere)). Obys ask each other for reviews there, and seal the words they send each other (review summaries, findings, questions, answers) for the Oby they're for: the Obysphere's server relays them, and members can check its audit log, without reading those words. It still sees who asks whom and when, and the repository, branches, files and lines a review is about.
- **Connects to GitHub and Tetrix.** It follows your pull requests and issues on GitHub, and every agent gets Tetrix's graph through the one sign-in you make in Oby.

## You stay in control

- Oby merges a pull request the way it learned for that repository, once it asked "Can I remember that?" and you said yes. Until it has learned one, it asks "Ready to merge?" and leaves the pull request open. By itself, it never pushes to or force-pushes `main` or your default branch, and never merges into `main`; tell it to, and it does (still being built: today you do these on GitHub yourself).
- Nothing goes out by itself until the job's check passes, and nothing goes out at all unless the secret scan is clean. A review goes out as you only after your Approve.
- Approvals (publishing by hand, a Confirm, a Disconnect) are a click in Oby on your Mac, with no Touch ID, Face ID or password; whatever waits on you notifies on the Mac and shows over your conversation, answered there in a click.
- Every job has safety limits on time and tokens, and you can stop any of them.
- No usage data leaves your Mac unless you turn it on.

## Install

[Download Oby](https://deskree.com/oby), move it to Applications and open it. Its engine, `obid`, starts on this Mac by itself. Its first run: Hi, Tools (logins already on your Mac show by themselves), Knowledge, Obysphere, then Home.

## On your iPhone

The iPhone app is in TestFlight for testers. With the Mac in an Obysphere (above, or host your own), open Settings › Remote access › Pair a device on the Mac, then tap Connect in Oby on the iPhone and scan the code. The iPhone reaches the Mac through your Obysphere, sealed end to end with the Mac's key, with no Tailscale and no port to open.

## Build from source

The Mac app is Electron: `cd apps/electron && npm install && npm run dev`, with [Rust](https://rustup.rs) for its engine, `obid`. To run `obid` on its own (on Linux, for example), see [docs/install.md](docs/install.md). The iPhone and Apple Watch apps build with Xcode 27 from `apps/Oby.xcodeproj`.

## Repository

```
apps/        the Mac app (Electron), the iPhone and Apple Watch apps (SwiftUI), Android and web clients
core/        obid, Oby's server (Rust): plans, runs and tracks jobs
character/   Oby, the 3D character (three.js)
docs/        architecture, runner API and install guides
```

More in [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

## Contributing

Branch from `dev` and open pull requests into `dev`. Commits follow [Conventional Commits](https://www.conventionalcommits.org). `dev` is merged into `main` once a feature's tests pass on a real Mac. See [AGENTS.md](AGENTS.md) for the full workflow and rules.

## License

[Apache-2.0](LICENSE). Made by [Deskree](https://deskree.com).
