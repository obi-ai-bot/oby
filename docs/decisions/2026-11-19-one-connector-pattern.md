# One pattern every connector follows

- **Date:** 2026-11-19
- **Status:** In force
- **Replaced:** Nothing AGENTS.md names: each connector (GitHub, Tetrix, Claude Code, Codex, Cursor) had its own shape
- **Changed by:** Nothing
- **Contracts and scenarios:** [connector-pattern.md](../contracts/connector-pattern.md) (CONNECTOR-1 to 15, CONNECTOR-20 to 30)

## The owner's words

> a pattern that must be followed for all [connectors] so we always have a pattern being followed

The owner's words of 2026-11-19, as the session that started this work relayed them.

> Start phase 0

The owner's words of 2026-11-19, approving wave 3's plan, whose phase 0 writes this pattern down and points every agent at it.

## Where it is stated

- [connector-pattern.md](../contracts/connector-pattern.md): the 13 rules, what each kind offers, the conformance suite, the guards G1 to G10, and the recipe for adding a connector.
- [core/AGENTS.md](../../core/AGENTS.md#from-outside), "From outside": every connector follows that contract and passes its conformance suite before it merges.

## Notes

Like a power socket: every appliance has a different motor inside, but they all fit the same plug and are tested the same way before they are sold. Oby is about to grow from one code host (GitHub) to several, from one knowledge source (Tetrix) to any MCP server, and from three agents to any ACP agent. Without one plug, each would be built its own way and break its own way.

The shape is T3 Code's (a definition and a driver per service, with a registry). Its history is why a shape alone isn't enough: about 45 to 55% of the commits to each of its host packages are fixes, of a few repeating kinds (recognizing a host, mapping states, lists past 100, parsing merges), and nothing in it makes a new host behave like the others. So the pattern adds one conformance suite, seeded with those kinds of bug, that every adapter must pass.

The plan put the binding line in the root `AGENTS.md`. The root file is at its size limit (`agents_md_bytes_limit`), and raising it is the owner's call in a pull request of its own, so the line is in `core/AGENTS.md`, obid's rules, since every connector is obid code.

Wave 3's plan orders the work so nothing risky lands before the launch: phase 0 (the guards, GitHub's calls recorded, this pattern, the suite against a reference fake) adds only tests and documents; phase A moves GitHub behind the pattern after the launch, one pure move at a time; phase B adds new connectors, each off by default.
