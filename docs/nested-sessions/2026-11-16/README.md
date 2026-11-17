# Nested sessions: the design beside the build

Each image puts a screen of page 2 of the canvas "Oby Factory On and Off" (left, rendered from its own files) beside this branch's app (right), at 1280×800.

- `01`, `02`, `03-*`, `07`, `18`: the built app on a scratch obid (its own home and port) with the owner's real agents. The task in `03-conversation` ran on Claude Code in a private scratch repository and opened a pull request; its address is blacked out in `03-done`, because it names the repository's owner.
- `04-needs-you`: the built app on its demo data (`?everything&chat`): a real permission ask wasn't raised in the scratch run. The question stack at the top is the demo's own.
- Screen 17 (Waiting for you) has no image: nothing waited in the scratch run. Its rows are covered by `waiting.test.tsx`.
- Screen 16 (a project's settings) isn't built: see the pull request.
