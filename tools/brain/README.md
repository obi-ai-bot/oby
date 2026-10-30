# Oby Brain

A map of Oby read from its own code, for the two people who work on it from opposite ends:

- **Sangam's design** (`apps/electron`): every screen and where each piece of it comes from: obid (live, with the
  route that feeds it), the demo data (mock), the app itself (kept locally, with the obid issue that will take it
  over), or hidden in v1 behind `?everything`. Also the states to open on the mock (the first run's steps, the URL
  flags, the Control Center's scenarios) and every sentence the app shows.
- **obid** (`core/crates/obid`): every HTTP route with who may call it (the owner's token, an agent's, a paired phone)
  and what it refuses, joined to the screens that reach it; obid's eleven areas and its modules, the pairs that import
  each other; the background work it starts; the files it keeps (obid has no database); the wire types beside their
  Rust structs; and AGENTS.md's "Rules Oby never breaks", each tied to the code that keeps it and the tests that prove
  it.
- **Checks**: where the code, `docs/runner-api.md`, the phone allowlist, the config and the screens disagree.

It is local only: the server listens on `127.0.0.1:4320`, nothing is published, and the generated map isn't in git.
Nothing talks to a running obid; it's static analysis of the code. It started as the Brain of Wellington's Dieta
Flexível and keeps its interface. obid's own `src/brain/` is a different thing (the conversation's brain).

## Run it

```bash
make brain
```

or `cd tools/brain && npm install && npm run dev`, or the `oby-brain` launch configuration, then open
<http://127.0.0.1:4320>. While it runs, saving a file of obid, the app, `docs/runner-api.md` or `AGENTS.md` reads the
code again and the page updates. `⌘K` searches everything. The states it links to open on the mock, so run the
renderer too (`npm run web` in `apps/electron`, or `oby-app-web`, on port 5176).

The first run builds the Rust extractor (about 15 s); after that a full read takes about 5 s.

## How it works

- `obid-extract/` is a small Rust tool (its own Cargo project, so obid's build never sees it) that reads obid with
  `syn`: routes and their handlers' extractors and `ApiError`s, the phone allowlist, the module graph (macro bodies
  included), spawns, stored-file names, serde structs, string constants and every function and test.
- `generator/design.ts` reads the Electron app with ts-morph (syntax and imports, no type checker): the `api` object in
  `obid.ts`, which stores each screen's code reaches it through, mock imports, storage keys, gates and words.
- `generator/index.ts` joins them and runs the checks; `brain.config.ts` holds only what the code can't say (the areas,
  who owns each kept-locally choice, which code keeps each rule).
- `vite-plugin-brain.ts` serves the JSON at `/brain.json` and regenerates it on save; the React interface in `src/`
  only reads that JSON. `npm run generate` writes the same JSON to `.brain/brain.json` and prints the checks.

### Limits

- "Reaches" is static: a branch that never runs still counts. State plumbing (`live.ts`'s `set`) isn't followed, and a
  component a screen renders is listed under Renders, not as its own calls.
- A route's errors are those its handler writes; errors from the functions it calls aren't listed.
- Keys obid adds with `json!` aren't in its structs, so the wire view shows them as keys only the app declares.

## Tests

```bash
cd tools/brain && npm test
```

`test/repo.test.ts` runs the Brain on the real repository and holds what must stay true for any version of the code:
every route read from its own line and its handler found; every module in an area; every app call reaching a route;
every kept-locally choice owned, and every owed one tied to an issue; every rule tied to code that's still there and to
a test; no internal host in the open-source code. `cargo test --manifest-path obid-extract/Cargo.toml` tests the
extractor on small Rust snippets. `npm run typecheck` checks the generator and the interface.
