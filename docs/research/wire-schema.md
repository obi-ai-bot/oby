# How obid's wire schema is made, and which tools generate client types

Research snapshot: 2026-11-15, against `dev` (66de6cb1). The spike for roadmap task R07-03. What the repository follows is in [contracts/wire.md](../contracts/wire.md), "How the schema is made (proposed, R07-03)"; this note holds the comparison and its numbers. Every prototype below was written in a scratch folder or a temporary `git worktree`, measured, and thrown away: none of it is committed.

Each number and each claim about a tool ends with how it is known: `(measured)` on this Mac at that commit, `(from its docs)`, `(from its licence file)`, `(estimated)` or `(not measured)`.

## 0. The short version

- **What obid sends is hand-built:** 2,170 lines with a `json!(` call against 179 `Serialize` derives in `core/crates/obid/src` (measured). A schema derived from the stored structs would describe stored files, not the wire.
- **(a) `schemars` DTOs** cost 264 production lines for `Job` alone (measured), about 900 for R07-04 and R07-05's objects (estimated), against 490 lines of headroom: it needs the owner's raise of `obid_lines_limit`, and 4 new crates (measured).
- **(b) hand-written JSON Schema held by tests** costs 0 production lines (measured), 18 lines of hook in `tests/support/mod.rs` and a 26-line `job.schema.json` for a few keys (measured), and one real `GET /v1/jobs` answer validated (measured). **This is the choice.**
- **(c) a schema inferred from recorded answers** (quicktype 26.0.0) emits draft-06, no enums at all, every seen key required and every seen `null` typed as `null` only (measured): it can't be the contract.
- **Open and closed enums:** the schema keeps the full list in `enum` and marks an open one with `"x-oby-open": true`; `jsonschema` 0.58.6 ignores the keyword and validates strictly (measured). Before generation, an open enum is rewritten to `anyOf: [{enum}, {type: string}]`, which every generator tried turns into an open type (measured, Swift's second from its docs).
- **Picks:** TypeScript json-schema-to-typescript 16.0.0 (MIT); Swift quicktype 26.0.0 (Apache-2.0); Kotlin quicktype 26.0.0 (Apache-2.0). Swift and Kotlin are choices for later, while the phone apps are frozen.

## 1. The premise, measured

| What | Count | Command |
|---|---|---|
| Lines with a `json!(` call | 2,170 (measured) | `grep -rn 'json!(' core/crates/obid/src \| wc -l` |
| `json!(` calls, however many share a line | 2,191 (measured) | `grep -rho 'json!(' core/crates/obid/src \| wc -l` |
| `Serialize` derives | 179 (measured) | `grep -rn 'derive(.*Serialize' core/crates/obid/src \| wc -l` |
| obid's production lines (`obid_lines`, `scripts/oby-check.sh:236`) | 100,660 (measured) | the script's own `obid_lines` function, sourced and run at 66de6cb1 |
| `obid_lines_limit` (`scripts/oby-check.sh:180`) | 101,150 (measured) | read from the script |

The three hand-built answers the task names:

- **`Job::to_json`**, `core/crates/obid/src/job.rs:1578`: one `json!` literal of 84 keys, then 31 more keys set one by one, some always and some only when present, out of the literal because of `json!`'s recursion limit: 115 keys in all (measured, by listing the keys in the function). One real job from `GET /v1/jobs`, queued by the stand-in agent, had 93 keys (measured).
- **inbox `Item::to_json`**, `core/crates/obid/src/sphere/inbox.rs:525`.
- **`Conversation::summary`**, `core/crates/obid/src/conversation.rs:310`, which feeds `GET /v1/conversations` and the `conversation` event.

Several of `Job`'s keys are a free `serde_json::Value` (`activity`, `waiting`, `pipeline`, `usage`, `scan`, `diff`, `windowShare`), others always `null` (`fromPR`, `lead`, `claim`, `question`, `xcodeCloud`, `xcodeCloudError`, `firstSwiftBuildSeconds`, `runsOn`), and others only present when set (`untrusted`, `clientRequestId`, `mergedBy`, `autoMerge`, `context`, `estimate`…) (measured, read from the function).

## 2. Option (a): `schemars` on wire DTO structs, prototyped on `Job`

**The prototype.** A scratch crate held `JobWire`: one field per key of `Job::to_json` (115), `#[serde(rename_all = "camelCase")]`, `#[serde(skip_serializing_if = "Option::is_none")]` on the keys only present when set, a `Status` enum, and 14 nested structs for the objects `to_json` builds inline (`stalled`, `moves`, `changes`, `startWhen`, `artifacts`, `routine`, `supervised`, `accountLimit`, `mergedBy`, `autoMerge`, `unjudged`, `buildFreed`, `estimate`). Keys whose value comes from another type's own `to_json` (`pullRequest`, `review`, `approvals`, `outgoing`, `tetrix`, `macCheck`, `answered`, `permission`) stayed `Value`. `schemars::schema_for!(JobWire)` printed its schema.

- **Production lines:** 264 for the struct file (measured, with the `production_lines` awk of `scripts/oby-check.sh`). `to_json` itself would then build `JobWire { … }` instead of `json!({ … })`: about the same number of lines, give or take 40 for rustfmt splitting what `json!` keeps on one line (estimated).
- **R07-04 and R07-05's whole set:** about 900 lines (estimated): `Job`'s 264, about 200 to type the eight keys above that stayed `Value`, about 60 each for the inbox item and `/v1/status`, about 150 for the event envelope and the events, about 150 for `Conversation`, `Turn` and `Account`.
- **Against the headroom:** 101,150 − 100,660 = 490 lines (measured). `Job` alone would take 264 of them; the whole set, about 900, is over by about 410 (estimated). **It needs the owner's raise of `obid_lines_limit`**, in a pull request of its own with a `# payback:` line (CI-39 to 41).
- **New dependencies:** `schemars` 1.2.3, `schemars_derive` 1.2.3, `dyn-clone` 1.0.20 and `serde_derive_internals` 0.30.0 (measured, the scratch crate's `Cargo.lock` against `core/Cargo.lock`; `ref-cast` is already in obid).
- **The free `Value` keys** come out as `true`, the schema that accepts anything (measured: `"activity": true`, `"pipeline": true`, `"waiting": true`).
- **Keys only present when set** come out exactly like keys always present and sometimes `null`: `"untrusted": {"type": ["boolean", "null"]}`, left out of `required` (measured). `schemars` can't say "present and `null`" apart from "absent", nor that `untrusted` is never `null`: of 115 keys, only the 44 non-`Option` fields were `required` (measured). Always-`null` keys typed `()` came out as `{"type": "null"}` (measured).
- **Enums** come out closed: `{"type": "string", "enum": ["queued", …]}` (measured). Marking one open needs an attribute per enum (`#[schemars(extend("x-oby-open" = true))]`) (from its docs).
- **It still describes a struct, not the wire.** The prototype's own field types were guesses, and the real answer disagreed with two of them: `commits` and `addedTo` were `null` for a queued job (measured), where the prototype said `u32` and `Vec<String>`. A DTO is only as true as the conversion into it.

## 3. Option (b): hand-written JSON Schema held by tests, prototyped on `Job`

**The prototype.** A temporary `git worktree` of 66de6cb1 held:

- `docs/contracts/wire/job.schema.json`, draft 2020-12, 26 lines: `id`, `status` (a `$ref` to `$defs/JobStatus`, with `"x-oby-open": true`), `agent` (open), `repo`, `title`, `created`, `fromPR` (`{"type": "null"}`), `waiting`, `activity`, `untrusted` (`{"const": true}`, not required);
- a hook in `core/crates/obid/tests/support/mod.rs`: `request_to`, under `call` and its variants, calls `wire_check(method, target, &body)`, which builds a `jsonschema::draft202012` validator once in a `OnceLock`, as `review.rs:344` does, and asserts every job in an answer to `GET /v1/jobs` matches, naming each error's path;
- a scratch test, `tests/wire_proto.rs`, that posts a `fix` job to the real obid with the stand-in agent, reads `GET /v1/jobs`, and checks a copy of the job with `status: "paused"` fails.

Run with `factory/bin/test-run --here -- cargo test -p obid --test wire_proto`:

- **Production lines:** 0 (measured): only `tests/` and `docs/` changed.
- **Test-code lines:** 18 added to `tests/support/mod.rs` (measured, `git diff --numstat`), plus 32 in the scratch test (measured), which R07-04 doesn't need: its checks ride on the tests that already exist.
- **A real answer validated:** yes (measured, `test a_real_job_list_matches_the_schema ... ok`). The first run failed first, rightly: the schema required `activity`, and the hook answered `GET /v1/jobs breaks job.schema.json: ["\"activity\" is a required property at "]` (measured): a key present only when set had been written as always present. So the hook can fail, and names what broke.
- **An open enum is still strict in tests:** `status: "paused"` failed with `"paused" is not one of "queued", "running" or 4 other candidates` (measured), and the unknown keyword `x-oby-open` was ignored, not rejected (measured).
- **New dependencies:** none. `jsonschema` 0.58.6 is already obid's (`core/Cargo.toml:39`), with `default-features = false`, so it resolves no file or URL by itself: a `$ref` from one schema file to another goes through a `jsonschema::Registry` of every file under `docs/contracts/wire/`, keyed by each file's `$id` (from its docs).

## 4. Option (c): a schema inferred from recorded answers

**Tried:** quicktype 26.0.0 (`quicktype --src-lang json --lang schema`), on the one recorded `GET /v1/jobs` answer from §3 (measured).

What it emits, and what it can't express (all measured on that answer):

- **No enums, open or closed:** `"status": {"type": "string"}`, `"agent": {"type": "string"}`. One sample can't tell an enum from free text, and many samples can't tell an open enum from a closed one: that is a decision, not an observation.
- **Optional against `null`:** every one of the 93 keys seen is `required`, and every key that happened to be `null` is typed `{"type": "null"}` (`pullRequest`, `commits`, `waiting`), so the next job with a pull request would break it.
- **Keys no test produces** aren't there at all: `untrusted`, `activity`, `mergedBy` and the other keys only present when set were absent from the answer, so from the schema.
- It writes draft-06 (`http://json-schema.org/draft-06/schema#`), not the draft 2020-12 `review.rs` validates with.

An inferred schema can seed a hand-written one, but it can't be the contract.

## 5. The comparison and the choice

| | Production lines | New dependencies | Describes | Open or closed enum | Drift risk |
|---|---|---|---|---|---|
| (a) `schemars` DTOs | 264 for `Job` (measured); about 900 for R07-04 and R07-05 (estimated): over the 490 of headroom, needs the owner's raise | 4 crates (measured) | A struct `to_json` fills: the wire only as far as the conversion is true | Closed by default; open needs an attribute per enum (from its docs) | Low between struct and schema, but `Value` keys are unchecked (`true`) and absent and `null` are one (measured) |
| (b) hand-written JSON Schema, held by tests | 0 (measured) | None (measured) | What obid sends, as the tests read it (measured) | Yes: `x-oby-open` on a strict `enum` (measured) | Caught by every HTTP test that reads an answer (measured on one); keys no test produces stay unchecked until a test does |
| (c) inferred from answers | 0, or a recorder in tests | quicktype at build time (measured) | The answers seen, nothing more (measured) | No enums at all (measured) | High: every new sample can change it (measured on the `null` keys) |

**The choice is (b).** It costs no obid production lines against 490 of headroom, and adds no dependency: `jsonschema` 0.58.6 is already in obid, in the dialect `review.rs` uses. It checks what obid really sends, keys present only when set and `null`s included, which (a) can't say and (c) can't know. Whether an enum is open is written down, once, where the add-only diff test (R07-04) can read it. And it is the shape R07-04's task file already presumes (`docs/contracts/wire/{job,inbox-item,status,event}.schema.json`, a hook in `tests/support/mod.rs`).

## 6. Generators

**What they read.** The schema files keep the strict list (`"enum": [...]`, plus `"x-oby-open": true` when open). No generator tried reads `x-oby-open`: given the strict form, each emits a closed type (measured for the three TypeScript generators and quicktype's Swift and Kotlin). So the generation step rewrites each open enum first, in memory, to the open pattern swift-openapi-generator documents:

```json
{"anyOf": [{"type": "string", "enum": ["queued", "running", "done"]}, {"type": "string"}]}
```

The rewritten form is for generators only: under draft 2020-12 its second branch accepts any string (from its docs), so the tests validate the strict form, which `jsonschema` 0.58.6 compiled and enforced with `x-oby-open` in it (measured, §3). The sample schema had `status` open (`queued`, `running`, `done`) and `mode` closed (`bypass`, `ask`); OpenAPI-only generators read it wrapped in an OpenAPI 3.1 document's `components.schemas`, `$defs` refs rewritten to `#/components/schemas/`.

### TypeScript

Unknown values were checked with `tsc --strict --noEmit` 5.9.3 on `const open: Job["status"] = "paused"; const closed: Job["mode"] = "auto";` (measured).

| Generator | Licence | Version | Open enum | Closed enum | Unknown value |
|---|---|---|---|---|---|
| [json-schema-to-typescript](https://github.com/bcherny/json-schema-to-typescript) | MIT, [LICENCE.md](https://github.com/bcherny/json-schema-to-typescript/blob/master/LICENCE.md) (from its licence file) | 16.0.0 (measured) | `export type JobStatus = ("queued" \| "running" \| "done") \| string;` (measured) | `export type Mode = "bypass" \| "ask";` (measured) | Open type-checks; closed is `TS2322` (measured) |
| [openapi-typescript](https://github.com/openapi-ts/openapi-typescript) | MIT, [LICENSE](https://github.com/openapi-ts/openapi-typescript/blob/main/packages/openapi-typescript/LICENSE) (from its licence file) | 7.13.0 (measured) | `JobStatus: ("queued" \| "running" \| "done") \| string;` inside `components["schemas"]` (measured) | `Mode: "bypass" \| "ask";` (measured) | Open type-checks; closed is `TS2322` (measured) |
| [quicktype](https://github.com/glideapps/quicktype) | Apache-2.0, [LICENSE](https://github.com/glideapps/quicktype/blob/master/LICENSE) (from its licence file) | 26.0.0 (measured) | `status: string;`: the known values are dropped (measured) | `export type Mode = "bypass" \| "ask";` (measured) | Open type-checks; closed is `TS2322` (measured) |

**Pick: json-schema-to-typescript** (MIT). It reads JSON Schema files as they are, `$ref`s and `$defs` included, with no OpenAPI wrapper, and keeps the known values of an open enum. Its `tsType` keyword lets the rewrite spell an open enum exactly as `obid-wire.ts` does today: `export type JobStatus = "queued" | "running" | "done" | (string & {});` (measured), which keeps the editor's completion that `| string` loses. **Runner-up: openapi-typescript** (MIT): the same output, but only from an OpenAPI document, so it needs the wrapper and nests every type under `components["schemas"]`. quicktype lost for dropping an open enum's values.

### Swift

Decoding was checked with `swiftc` (Apple Swift 6.4.2) and `JSONDecoder` on `{"status": "paused", "mode": "bypass"}` and `{"status": "done", "mode": "auto"}` (measured for quicktype).

| Generator | Licence | Version | Open enum | Closed enum | Unknown value |
|---|---|---|---|---|---|
| [quicktype](https://github.com/glideapps/quicktype) | Apache-2.0, [LICENSE](https://github.com/glideapps/quicktype/blob/master/LICENSE) (from its licence file) | 26.0.0 (measured) | `let status: String` (measured) | `enum Mode: String, Codable { case ask; case bypass }` (measured) | Open decodes (`decoded paused bypass`); closed fails, `Cannot initialize Mode from invalid String value auto` (measured) |
| [swift-openapi-generator](https://github.com/apple/swift-openapi-generator) | Apache-2.0, [LICENSE.txt](https://github.com/apple/swift-openapi-generator/blob/main/LICENSE.txt) (from its licence file) | 1.14.4 (from its docs: its repository at that tag; not built) | `public struct MyOpenEnum: Codable { @frozen public enum Value1Payload: String, … { case one; case two }; public var value1: Value1Payload?; public var value2: Swift.String? }` (from its docs: `testComponentsSchemasEnum_open_pattern` in its reference tests) | `@frozen public enum MyEnum: String, Codable, Hashable, Sendable, CaseIterable { case one = "one" … }` (from its docs) | Open decodes with `value1` nil and the raw string in `value2`; closed fails (from its docs; not measured) |

swift-openapi-generator wasn't built here: building it fetches its SwiftPM dependencies, which this session wasn't allowed to do. **Pick: quicktype** (Apache-2.0), a choice for later, while the phone apps are frozen: measured here, reads JSON Schema with no wrapper, emits plain `Codable` with no runtime library, and is the same tool as Kotlin's. An open enum becomes `String`, so the app keeps its own list of known values beside it, as `obid-wire.ts` does. **Runner-up: swift-openapi-generator** (Apache-2.0): its open enum keeps the known values, but it needs the OpenAPI wrapper, the `OpenAPIRuntime` library in the app, and couldn't be measured here.

### Kotlin

`kotlinc` isn't on this Mac and neither is a Java runtime (measured: `java -version` answered "Unable to locate a Java Runtime"), so no decoding was measured.

| Generator | Licence | Version | Open enum | Closed enum | Unknown value |
|---|---|---|---|---|---|
| [quicktype](https://github.com/glideapps/quicktype) (`--framework kotlinx`) | Apache-2.0, [LICENSE](https://github.com/glideapps/quicktype/blob/master/LICENSE) (from its licence file) | 26.0.0 (measured) | `val status: String` (measured) | `enum class Mode(val value: String) { @SerialName("ask") Ask("ask"), @SerialName("bypass") Bypass("bypass"); }` (measured) | Open: a `String` takes any value (not measured); closed: kotlinx.serialization rejects an unlisted name (from its docs; not measured) |
| [OpenAPI Generator](https://github.com/OpenAPITools/openapi-generator) (`kotlin`) | Apache-2.0, [LICENSE](https://github.com/OpenAPITools/openapi-generator/blob/master/LICENSE) (from its licence file: its npm wrapper 2.42.2 says Apache-2.0, measured) | wrapper 2.42.2 (measured); the generator jar not run | No output: it runs on Java (not measured). Its `enumUnknownDefaultCase` option adds an `unknown_default_open_api` case to every enum (from its docs) | A Kotlin `enum class` (from its docs) | With `enumUnknownDefaultCase`, an unknown value decodes to that case, for every enum at once, open or closed (from its docs; not measured) |

**Pick: quicktype** (Apache-2.0), a choice for later, while the phone apps are frozen: its output was measured here, it reads JSON Schema with no wrapper, and it is the Swift pick too. **Runner-up: OpenAPI Generator** (Apache-2.0): it needs Java and the OpenAPI wrapper, and its unknown-value switch opens every enum, closed ones too, so it can't follow the schema's mark.

## 7. For the tasks that follow

These task files are not changed here; they can take this up when they are built.

- **R07-04** proposes WIRE-5 and WIRE-6, but WIRE-5 is already R07-01's ([contracts/wire.md](../contracts/wire.md)): its scenarios need the next free numbers.
- **R07-06** applies the open-enum rewrite (to `anyOf`, with `tsType` for `(string & {})`) before json-schema-to-typescript.
- **R09-15** names "R01's fixtures and snapshot": R01's API snapshot is `docs/api/routes.txt` (paths only), and the shapes are the wire schema and its fixtures under `docs/contracts/wire/`; no `docs/api/obid-api.json` exists or is planned.
