# Oby Artifacts, in the lab

The plan: [Oby Artifacts backend plan](https://claude.ai/artifact/VrzfijvgHL6VD8jm5Hbs2k). The
design it serves: [the Oby Artifacts canvas](https://claude.ai/artifact/LVfe6UQn1VbR2d2AYVBXvD).

- `src/contract.ts`: the TypeScript twin of `src/lab/artifacts_contract.rs`, the routes and shapes
  the app will talk to. The Rust file is the source; `the_typescript_twin_matches` fails until the
  two agree.

The screens (the artifact tab, the canvas host) come with the `artifactview` lab PR.
