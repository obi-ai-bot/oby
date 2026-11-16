# FEEDSTOP: the feeder stops a follow-up that keeps failing the same way

- **Date:** 2026-11-16
- **Status:** In force
- **Replaced:** Nothing AGENTS.md names
- **Changed by:** Nothing
- **Contracts and scenarios:** [factory.md](../contracts/factory.md) (FEEDSTOP-1 to FEEDSTOP-7)

## The owner's words

> In the feeder

The owner's words, as task B00-35 quoted them.

## Where it is stated

- [factory.md](../contracts/factory.md), "The feeder's fix rounds stop: oby-feed sends no more follow-ups for the same failure", FEEDSTOP-1 to FEEDSTOP-7

## Notes

The diagnosis of task A00-96's six failed items in Oby's factory. Each was obid's `failed` item `Oby's review still asks for changes. Take it from here.`: its own review had run out of rounds (`reviewNeedsYou`). A bridge script in the factory's home, outside the repository, answered each one with a new follow-up, up to six for the task. Nothing in the repository counted those follow-ups or stopped them. So the stop is the feeder's: it counts the follow-ups it sends itself, the lens fix round and the gate's fix message, and obid is unchanged.
