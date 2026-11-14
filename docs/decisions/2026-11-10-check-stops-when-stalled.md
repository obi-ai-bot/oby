# A check is stopped only once it stalls or runs too long

- **Date:** 2026-11-10
- **Status:** In force
- **Replaced:** A check's 45-minute limit (publishing.md: "the old 45 minutes")
- **Changed by:** Nothing
- **Contracts and scenarios:** [publishing.md](../contracts/publishing.md) (PUBLISHING-29)

## The owner's words

> A check is stopped only once it stopped making progress, or ran too long (the owner's decision of 2026-11-10: under load, checks of a jobs.rs change ran past the old 45 minutes still compiling and testing, three times in one night).

publishing.md's wording, not a quote.

## Where it is stated

- [publishing.md](../contracts/publishing.md), PUBLISHING-29
