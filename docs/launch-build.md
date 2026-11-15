# The launch build

The text of the root [AGENTS.md](../AGENTS.md)'s "The launch build"; its rules for obid are in [core/AGENTS.md](../core/AGENTS.md#the-launch-build).

Oby launches lean: where this differs from Sangam's design, this wins until the owner says otherwise ([why](decisions/2026-10-23-the-launch-build.md)). The plan, workstreams and checklist: https://claude.ai/artifact/GUdcsv2sLN3CqviHVDSYCE.

- What launches: Home, conversations, tasks and the sidebar; Sangam's Voice comes back later, and the microphone rule below holds.
- Mac share (Settings › Sharing), Runner share (Run on another Mac, Shared with you, Use X's share) and Remote access are hidden on the Mac and the iPhone, but for the next bullet. obid's side went with the strip to a dispatcher, and Sangam's screens stay the target when they return ([why](decisions/2026-10-27-strip-to-a-dispatcher.md)). A `showEverything` default, with no UI, shows them on Deskree's own Macs.
- Settings › Remote access shows in the Electron app with pairing an iPhone through the owner's Obysphere (Pair a device, Paired devices) ([why](decisions/2026-11-01-remote-access-in-electron.md)). "Runner" is never a word a person sees, on the Mac or the iPhone.
- Connectors show GitHub and Tetrix, and Slack is cut (docs/trim.md, [why](decisions/2026-10-25-slack-cut.md)). Sangam's first run promises GitLab, Bitbucket and any MCP server in Settings later: obid work once he designs those screens.
- Oby answers in text, never aloud, and the user's words show as they talk.
- Anyone hosts an Obysphere from a Deploy to Cloudflare button (obi-ai-bot/obi-sphere, public once its `dev` is in `main`), with a workers.dev address or their own domain.
- A new user goes from download to a first task in about 5 minutes: obid on by default, the agent logins already on the Mac picked up without a click, and every repository their GitHub login can push to usable without editing `config.json`.
- After the core audit (the plan's "Core audit" tab, [why](decisions/2026-10-23-core-audit.md)):
  - A task ends honestly: "Pull request open · waiting for your review on GitHub", only for a pull request Oby doesn't review by itself: one published by hand ([why](decisions/2026-10-24-honest-end-for-hand-published-prs.md)). Settings makes no promise about merging, and Checks on this Mac (this repository's Mac checks) is hidden and off by default.
  - Settings › Vault is hidden for launch: under obid it does nothing. General › Permissions shows and is live, with Sangam's four modes (Ask, Accept edits, Plan, Bypass) and Bypass the default.
  - The launch download leaves out cloudflared and obi-vm, strips obid and drops the bundled web app; Deskree's Macs get them in their own builds or on demand.
  - Deskree's Obysphere relays phones for its members only; anyone else pairs through their own Sphere.
