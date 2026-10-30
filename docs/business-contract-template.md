# Business behavior contract template

Use this template in the feature issue before implementation. Scale the detail to the feature: state each applicable rule precisely and mark sections that do not apply. Link the issue from the implementation and document the implemented runner API in [runner-api.md](runner-api.md). Plans, epics and ideas not yet scheduled belong in [oby-plan](https://github.com/obi-ai-bot/oby-plan).

## Feature and status

- **Outcome:** What can the person accomplish, and why does it matter?
- **Status:** Proposed, accepted, implemented, or superseded. Link the owner's decision, design, related issue and any superseded contract. Distinguish current behavior from proposed behavior.
- **Scope:** What is included and explicitly outside this feature?

## Behavior

- **Actors and ownership:** Who initiates or answers? What does obid decide, persist and execute? What do clients display and send?
- **Triggers and prerequisites:** What starts the behavior, and what must be true first?
- **States and transitions:** What states can occur, what changes them, and what does the person see? Include choices and their consequences, defaults, cancellation and completion.
- **Failures and recovery:** What happens when a dependency fails, information is unavailable or stale, the app restarts, a device disconnects, or the same action arrives twice? Specify retries and what remains visible or actionable.
- **Invariants and boundaries:** What must always remain true, including authorization, privacy, security, compatibility, data ownership and any relevant design rules? Identify external effects and when they may happen.

## Interfaces and dependencies

Name the affected API, events, persistence, clients and external services. Describe compatibility and the data exchanged without prescribing unnecessary implementation details. Identify provider-specific capabilities and what happens when a capability is unavailable. Record unresolved questions and dependencies; do not present assumptions as established behavior.

## Acceptance scenarios

Write observable, testable scenarios in terms of a starting condition, action and expected result. Cover the normal path, each meaningful choice, errors, recovery and relevant boundary conditions. Link automated checks and the feature's real-device “Test it when it's done” steps. Record what was actually checked when the work is reviewed.
