# Artifact sharing: one format for what leaves, and the recipient's checks

The business behavior contract for sharing Oby Artifacts between Obys, from [the template](../business-contract-template.md). It follows the Obysphere's own spec for artifacts (obi/0.1, protocol.md 5.20 to 5.24 and section 11, "Receiving an artifact"; `oby-sphere#47`). Proven first in the Blok lab (`tools/obid-spec/src/lab/artifactsphere.rs`; #2709), then ported piece by piece as [the port plan](../plans/artifacts-port.md) orders it (P9, P10, P11). Its store is [artifacts.md](artifacts.md); its runner API, once it has routes (P10), is [docs/api/artifacts.md](../api/artifacts.md).

## Feature and status

- **Outcome:** The owner can show what an agent made to the members of their Obysphere, each opening it on their own Oby, without the Sphere ever reading it; and nothing another Oby sends reaches the owner's disk or agents before obid has checked it.
- **Status:** **Partly implemented.** ARTSHARE-1 to 3, the format and the recipient's checks (the port plan's P9), are implemented in obid as a module with no route: `core/crates/obid/src/artifacts/seal.rs`. Nothing is offered or received over the Sphere yet: the offers, updates, answers and edits back (P10) and public links (P11) are proposed in the port plan, and add their scenarios here as they land. The `age` crate was decided in the port plan's "Decided" (2026-11-20).
- **Scope:** What a shared version is on the wire, how it is sealed for a group, and what a recipient checks, keeps and sets apart. Outside it: the Sphere's own storage and policy (Dmytro's server), the actions that carry an offer (P10), and how the apps show what was shared (P12).

## Behavior

### Actors and ownership

- **The sharing Oby's obid** packs and seals a version, once for every member it goes to.
- **A recipient's obid** checks what came before it keeps anything, and sets apart the files that instruct agents. It owns `home/artifacts/quarantine/<id>/`, written only by `seal::set_apart`.
- **The Sphere** carries ciphertext only: it never holds a file key, a secret key or a file's text.

### Rules

- **One format.** A version is one tar, the same bytes every time (names sorted, mode 0644, owner 0, time 0). It is encrypted once with age (age-encryption.org/v1): a fresh file key, wrapped to each recipient's card seal key (the X25519 `seal_public_key` its owner already signs on its card), written as age's X25519 recipient (`age1…`), so a group of any size is one upload and any member's own seal key, which never leaves their Mac, opens it. No other key is made for artifacts, and none travels.
- **Parts of 64 MiB.** Ciphertext past 64 MiB (67,108,864 bytes) is cut into parts of at most that, in order; the offer names the whole one's size and SHA-256 (`blob`) and, when cut, each part's (`blobs`). A version obid takes is at most 5 parts (Claude's 256 MB with room for the headers; the Obysphere takes up to 8). (The port plan's "Open", option B, as the Obysphere built it.)
- **The recipient's checks, in order, before anything is kept:** the parts are as many as offered and each has its offered size and hash; the joined ciphertext has the whole one's; it decrypts with this Oby's own seal key; the tar has the offered hash; the tar holds only plain files at plain paths (no link of either kind, no `..`, no absolute path, no `.` part, no `\`, no NUL, no device or pipe, no path twice in any letter case, no path that is both a file and a folder, at most 2000 entries, each read to one byte past its limit at most); no more files or bytes than the offer says; Claude's limits hold (a text file 16 MB, a binary 15 MB, an SVG 2 MB, 512 files, 256 MB); and obid's secret scan finds nothing in any file's text or name, the files set apart included. The first that fails is the refusal; a scan that can't run is one too, and nothing is kept.
- **Files that could instruct an agent are set apart.** A file named `AGENTS.md`, `CLAUDE.md`, `GEMINI.md` or `.mcp.json` (any letter case, in any folder) and anything under a `.claude/`, `.cursor/`, `.codex/`, `.gemini/`, `.github/`, `hooks/` or `skills/` folder are kept out of the version and written under `home/artifacts/quarantine/<id>/` (folders 0700, files 0600), where the owner decides about them; the rest is kept.
- **What another Oby sent reaches an agent only quoted.** Text of a shared artifact or an answer is given to an agent fenced as data from another Oby, with a marker it can't guess that nothing inside can close; reading it marks the turn as having read something untrusted (where an agent reads it: P5's `oby_artifact_read`, P10's received artifacts).

### Failures and recovery

- A refusal is one stable `reason` (`bad_blob`, `too_big`, `secret_found`, `scan_failed`) and one line starting `Not kept: ` that says why, naming the file but never a secret it holds.
- Nothing partial is kept: the checks run on what is in memory, and only a version that passed every one is handed to the store.

### Invariants and boundaries

- The secret scan's non-negotiable holds both ways: nothing is kept from a scan that found something or couldn't run.
- obid never follows a link it received and never writes outside `home/artifacts`.
- A seal key's private half never leaves the Mac it was made on; the Sphere and every other Oby see only cards' public keys and ciphertext.

## Interfaces and dependencies

- **Module:** `core/crates/obid/src/artifacts/seal.rs`: `pack`, `unpack`, `seal`, `open`, `sealed` (pack, seal and cut into blobs), `receive` (the checks), `split_instructions`, `set_apart`, `quote`. No route, event or persistence but the quarantine folder until P10.
- **Dependencies:** the `age` crate (0.11, decided over writing its format by hand), `bech32` (0.9, age's own, to write a seal key as age's text) and `tar` (0.4).
- **The card's seal key:** `obi_protocol::seal::SealKey` (identity.md section 2), the one the Oby's words are already sealed to.
- **Persistence:** `home/artifacts/quarantine/<id>/`, never listed as an artifact.

## Acceptance scenarios

Each is a test named after it, in `core/crates/obid/src/artifacts/seal/tests.rs`. Until P10 gives obid a Sphere to receive from, they are obid's backlog in `tools/obid-spec/scope.json`: P10's pull request carries the lab's forged offers (`workflows/lab/artifactsphere/receive.json`: a wrong hash, a link in the tar, a path out, files that instruct agents, the scanner down) as Blok workflows against the real Sphere.

- **ARTSHARE-1** (implemented). Given a version shared with a group of three, then it is one blob whose header holds a wrapped key per member, each to its card's seal key as age's recipient; each member's seal key opens it to every file as sent, and a stranger's doesn't (`Not kept: it can't be decrypted …`). The tar is the same bytes every time. A version past 64 MiB is cut into parts of at most 64 MiB, joined in order; a part out of order or missing, or parts whose whole isn't the offered one, is refused. Lab: LAB-ARTSPHERE-12. Tests: `artshare_1_one_blob_for_a_group_opened_by_each_member_and_nobody_else`, `artshare_1_the_cards_seal_key_is_ages_recipient`, `artshare_1_a_version_packs_to_the_same_tar_every_time`, `artshare_1_a_large_version_is_cut_into_blobs_joined_in_order`.
- **ARTSHARE-2** (implemented). Given what another Oby sent, a blob whose hash differs (refused before it is decrypted), a tar whose hash differs (after decrypting, before unpacking), more files or bytes than offered, a link of either kind, a path out, a path twice or both a file and a folder, anything but a file or a folder, a file over one of Claude's limits or one file too many, a found secret, or a scan that can't run, is refused saying why, and nothing is kept. Lab: LAB-ARTSPHERE-13. Tests: `artshare_2_a_blob_whose_hash_differs_is_refused`, `artshare_2_a_tar_whose_hash_differs_is_refused_after_decrypting`, `artshare_2_more_files_or_bytes_than_offered_is_refused`, `artshare_2_a_link_is_refused`, `artshare_2_a_path_out_is_refused`, `artshare_2_a_path_twice_or_both_file_and_folder_is_refused`, `artshare_2_anything_but_a_file_or_folder_is_refused`, `artshare_2_a_file_over_a_limit_is_refused`, `artshare_2_a_found_secret_is_refused`, `artshare_2_a_scan_that_cant_run_keeps_nothing`, `artshare_2_the_scan_reads_every_file_and_name`.
- **ARTSHARE-3** (implemented). Given a version holding `CLAUDE.md`, `docs/agents.md`, `site/GEMINI.md`, `.mcp.json`, `.claude/settings.json`, `.cursor/rules`, `.codex/config.toml`, `site/.gemini/settings.json`, `.github/workflows/ci.yml`, `hooks/pre-commit` and `tools/skills/deploy/SKILL.md`, then those are set apart in `home/artifacts/quarantine/<id>/` (0700, 0600), never listed as an artifact, and the rest is kept, `CLAUDE.md.txt` and `hooks.html` too; what another Oby sent is quoted, its fence one nothing inside can close. Lab: LAB-ARTSPHERE-13. Tests: `artshare_3_files_that_instruct_agents_are_set_apart`, `artshare_3_set_apart_files_wait_in_quarantine`, `artshare_3_what_another_oby_sent_is_quoted`.
