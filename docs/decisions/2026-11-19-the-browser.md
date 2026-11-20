# Oby's browser: agent-browser for every agent, on by default, the person in control

- **Date:** 2026-11-19
- **Status:** In force
- **Replaced:** "Follow OpenBot" for B00-69
- **Changed by:** [2026-11-20-browser-install-and-limits.md](2026-11-20-browser-install-and-limits.md) (adds how the browser reaches a Mac and where it runs)
- **Contracts and scenarios:** [browser.md](../contracts/browser.md), BROWSER-1 to 25

## The owner's words

The owner's decisions of 2026-11-19, as the approved plan (https://claude.ai/artifact/TVVtw8kq77vnm81B5UfcXq) and the lab's pull requests (#2593, #2599) record them:

- agent-browser by default for the browser, agent-desktop for the desktop;
- agents' access to the browser is on by default;
- `evaluate` ships, as in T3 Code, its result reaching the agent as quoted outside data;
- a password goes through a card straight into the page's field;
- an agent's downloads stay in its profile;
- Settings › General gets two switches, both on;
- a teammate's request uses the browser only in the foreign lane.

And the order of the work:

> I want everything created in RUST Nodes… I want everything working there first and only then we will work on porting.

## Where it is stated

- [browser.md](../contracts/browser.md), every rule that links here
- [proven-by-blok.md](2026-11-19-proven-by-blok.md) for the lab-first order

## Notes

T3 Code's browser is 198 fixes in 309 commits. Its bug groups (a shared debugger, focus theft, timeouts that replay, slow streams, handover between person and agent) are each a rule in browser.md with a workflow that goes red without it, proven first in the lab.
