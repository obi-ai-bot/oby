# Connections screens: an MCP server, vault health and a secret

This is the answer to roadmap task R11-01, for R11-06, R11-09, R11-13, R11-14 and R11-15 (the owner's MCP servers, the vault's health and a secret Oby needs). It rests on the owner's answers in [the roadmap decisions of 2026-11-13](decisions/2026-11-13-roadmap-answers.md), given there for R11-01 and the tasks that waited on it, so this task writes them down instead of asking again.

The question: which screens add an MCP server of the owner's own, show the vault's health and take a secret, and what do they show?

## Who decides

The owner decides designs now, as [the standing decision on designs](decisions/2026-11-13-roadmap-answers.md#standing-decisions) says: "A task that waited for Sangam's screen takes the answer below as its design, using the existing screens' style; a task with no answer here proposes the smallest screen change in its spec and goes ahead."

## What the app has today

In the Electron app, `apps/electron/src/renderer/src/`:

- **The first run's promise.** `FirstRun.tsx:268`: the Knowledge step says "Or add any MCP server in Settings after setup." (its comment at `FirstRun.tsx:257`). Settings doesn't keep that promise yet.
- **Settings › Connectors.** `Connectors.tsx`: on obid (`function Live`, `Connectors.tsx:97`) it shows GitHub (`Connectors.tsx:125`) and Tetrix (`Connectors.tsx:141`) only; on the mock (`function Mock`, `Connectors.tsx:62`) it shows a demo list of brands (`CONNECTORS`, `Connectors.tsx:50` to `:60`). It has no row for an MCP server of the owner's own, and no Remove for one.
- **Settings › Vault.** `SettingsView.tsx:37`: `LATER` holds `'vault'`, so the Vault page (listed at `SettingsView.tsx:46`, filtered at `:48`) is hidden unless `everything`. Its page, `function Vault` at `SettingsView.tsx:1085`, keeps its names in a local setting (`useSetting('vault', …)`, `SettingsView.tsx:1086`), not in obid.
- **The custom MCP server step.** `AddSourceModal.tsx`, used only by `KnowledgeView.tsx` (an Obysphere's Knowledge: an admin's connection for every member). Its `custom` step (`AddSourceModal.tsx:15`, `:100` to `:123`), picked from its "Custom MCP server" kind (`AddSourceModal.tsx:82`), is titled "Connect an MCP server" (`AddSourceModal.tsx:69`) and takes a Name (`:102`), an "MCP server URL" (`:104`), a Sign-in choice of OAuth / API key / None (`:108`), an API key field (`:110`) and a "Test connection" button (`:123`).

## What is missing

- A screen for the owner's own MCP servers: adding one, listing them and removing one.
- A line that shows the vault's health.
- A card, or any screen, where the owner types a secret Oby needs.

## The owner's decisions

- **The owner's own MCP server**, for R11-06 and R11-09: [q054](decisions/2026-11-13-roadmap-answers.md#r11-01r11-06r11-09-q054), "A 'Custom MCP server' row in Settings > Connectors that opens the existing custom step (name, address, sign in with OAuth / API key / none), with a Remove button on each added row". The existing custom step is `AddSourceModal.tsx`'s, above.
- **The vault's health**, for R11-15: [q055](decisions/2026-11-13-roadmap-answers.md#r11-01r11-15-q055), "Only show a line when something is wrong, as a banner on Home".
- **A secret Oby needs**, for R11-14: [q056](decisions/2026-11-13-roadmap-answers.md#r11-01r11-14-q056), "A pop-up window over whatever the owner is doing".
- **Settings › Vault**, for R11-13: [q057](decisions/2026-11-13-roadmap-answers.md#r11-01r11-13-q057), "Yes, show it for everyone as soon as the Vault page works against obid".

## What the answers leave open

- The words of the Home banner (q055) and of the secret pop-up (q056) are not in the owner's answers, and this doc proposes none.
- Each UI task's own spec proposes the smallest change in the existing screens' style, as the standing decision says, and the owner's word on it settles it.

## AGENTS.md

q057 changes AGENTS.md's line in "The launch build", "Settings › Vault is hidden for launch: under obid it does nothing.", only once the Vault page works against obid: R11-13 changes that AGENTS.md line in its own pull request, when it lands. This task (R11-01) doesn't edit AGENTS.md.
