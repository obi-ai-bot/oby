# A change is proven by driving the real thing: BLOK workflows for obid, Playwright and agent-desktop for the window

- **Date:** 2026-11-19
- **Status:** In force for obid's workflows, the launch gate and the real-Mac steps; proposed for the `app.*` and `desktop.*` nodes until they're built
- **Replaced:** Nothing
- **Changed by:** Nothing
- **Contracts and scenarios:** [verification.md](../verification.md), [obid-spec.md](../contracts/obid-spec.md), [test-rig.md](../contracts/test-rig.md)

## The owner's words

> And add to it or to our AGENTS.md the BLOK check of the things that are not UI related but also we can build workflows to use Playwright or Agent Desktop and test them as well! Go!

The owner's words of 2026-11-19, in the session planning wave 3, right after asking for a plan that wouldn't make the platform less stable.

## Where it is stated

- [verification.md](../verification.md), "What proves what" and "The rules every workflow follows"
- [AGENTS.md](../../AGENTS.md), "How we work", the line after "Done"

## Notes

On 2026-11-19 four things reached dev or the merge queue that unit tests didn't catch: two contracts' new scenarios unlisted for the coverage gate (#2557, #2563), a flaky workflow step (#2558), a Copy button laid over a task card (#2572), and two Obysphere launch checks red since they were built (#2567, #2568). Each was found by driving the real thing, by the merge queue, the owner's own eyes or the launch gate, after it had already merged or nearly so.

The BLOK suite (`tools/obid-spec`, 180 workflows, the real obid binary driven through its API) already proves obid's routes and triggers; this makes it the expected proof for any obid change, not only routes. The window had no equivalent: the human drive (`.claude/skills/human-drive/`) drives it with Playwright, but by hand and not on every change. The `app.*` nodes put the same driver into workflows. The `desktop.*` nodes cover what Playwright can't reach, the macOS menu bar and system dialogs, with agent-desktop, which the 2026-11-11 spike (`spikes/computer/`) showed reads and drives macOS apps.

The suite's freeze until the launch (2026-11-18) still holds for new workflows beyond bug fixes and launch flows; this decision doesn't lift it.
