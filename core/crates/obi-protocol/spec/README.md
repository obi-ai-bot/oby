# A copy of the Obi-to-Obi protocol v0.1's machine-readable spec

These files are copied, unchanged, from [obi-ai-bot/obi-sphere](https://github.com/obi-ai-bot/obi-sphere)'s `spec/`, which is the contract (docs/ARCHITECTURE.md). They are never edited here: a change goes to the spec first, then `sync-spec.sh` copies it again.

| Here | In obi-sphere |
|---|---|
| `schemas/v0.1/` | `spec/schemas/v0.1/`: compiled into `obi-protocol`, which checks every message, card and chain against them. |
| `conformance/fixtures/` (envelope, audit and host_approval cases), `conformance/jcs-vectors.json`, `conformance/channel-vectors.json`, `conformance/seal-vectors.json` | `spec/conformance/`: what `cargo test -p obi-protocol` judges, so the Rust core's CI runs the conformance cases without a checkout of obi-sphere. |

Copied from obi-sphere commit `faa0f54a6cbab3eeeaa25d187a345e418347b78d` (its branch `approve-owner-confirmation`, obi-ai-bot/oby-sphere#38; copied again from `dev` once that merges). With `OBI_SPHERE_DIR` set to a checkout, `cargo test -p obi-protocol` also checks that this copy still matches it.

Not copied: the wire cases (`fixtures/wire/`), which judge the Sphere's own API bodies; the agent-card cases (`fixtures/card/`) and `keys.json`. A card case carries the test owners' public keys as `owner_key`, which generic secret scanners mistake for API keys; obi-sphere allowlists exactly those values in its own `.gitleaks.toml`, and this repository has no such list. So `tests/conformance.rs` makes the same card cases at test time, from the test keys' public seed strings, as the spec's `generate.mjs` does, and judges them with the rest. With `OBI_SPHERE_DIR` set, it also checks that the cases it makes are the spec's, case for case, and that the test keys derive as `keys.json` says. The envelope and audit cases carry no owner key: they are copied as they are.
