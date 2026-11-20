// Oby Artifacts' contract, the TypeScript twin of tools/obid-spec/src/lab/artifacts_contract.rs:
// what the app talks to about artifacts, shaped as obid's future API, add-only over
// docs/api/artifacts.md. The Rust file is the source; a change there changes this file in the same
// commit, and its test `the_typescript_twin_matches` fails until they agree.
//
// The person's app calls every route with obid's owner token (`Authorization: Bearer <token>`);
// the owner's own acts (SIGNED_ROUTES) also carry the device's signed words (WORDS_HEADER).
// Errors are obid's: `{ error, reason }` with a 4xx (ApiError). Enums only grow: treat an unknown
// kind as a file to download and an unknown origin as "own", never as an error.

// ── Routes ──────────────────────────────────────────────────────────────────────────────────────

export const PATH_ARTIFACTS = "/v1/artifacts";
export const PATH_IMPORT = "/v1/artifacts/import";
/// `GET /v1/artifacts/{id}/export/{version}?as=zip|html`.
export const VERB_EXPORT = "export";
export const PATH_PLANS = "/v1/plans";
/// Inside a design's version, `{version}/_blob/<id>` serves Claude's `/_blob/<id>` upload.
export const BLOB_PREFIX = "_blob/";

export const ROUTES = [
  "GET /v1/artifacts",
  "GET /v1/artifacts/{id}",
  "PATCH /v1/artifacts/{id}",
  "DELETE /v1/artifacts/{id}",
  "GET /v1/artifacts/{id}/{version}/{*path}",
  "GET /v1/artifacts/{id}/export/{version}",
  "POST /v1/artifacts/import",
  "POST /v1/plans",
] as const;

export const SIGNED_ROUTES = [
  "PATCH /v1/artifacts/{id}",
  "DELETE /v1/artifacts/{id}",
  "POST /v1/artifacts/import",
  "POST /v1/plans",
] as const;

/// `<deviceId> <signedAt> <signature>`, the signature (P-256, DER, base64) over
/// `oby-words\n<METHOD>\n<path>\n<sha256 hex of the body>\n<signedAt>`.
export const WORDS_HEADER = "X-Oby-Words";

// ── Limits: Claude's ────────────────────────────────────────────────────────────────────────────

export const MB = 1024 * 1024;
export const MAX_TEXT_BYTES = 16 * MB;
export const MAX_BINARY_BYTES = 15 * MB;
export const MAX_SVG_BYTES = 2 * MB;
export const MAX_FILES = 512;
export const MAX_VERSION_BYTES = 256 * MB;
export const MAX_VERSIONS = 50;
export const MAX_OWNER_BYTES = 4096 * MB;
export const MAX_NOTES_BYTES = 1 * MB;

// ── Artifacts ───────────────────────────────────────────────────────────────────────────────────

export const KINDS = ["html", "markdown", "image", "pdf", "canvas", "design"] as const;
export type Kind = (typeof KINDS)[number];

export const ORIGINS = ["own", "sphere", "import"] as const;
export const IMPORT_FROM = ["claude-design-zip", "zip", "html", "markdown"] as const;

export type Origin =
  | { kind: "own" }
  | { kind: "sphere"; sphere: string; from: string; offer: string }
  | { kind: "import"; from: (typeof IMPORT_FROM)[number] };

export interface Version {
  version: number;
  created: string;
  files: string[];
  bytes: number;
  /// A design's uploads: Claude's `/_blob/<id>` → `assets/<name>.<ext>`.
  blobs?: Record<string, string>;
  /// The job that made this version, when one did.
  job?: string | null;
}

export interface ShareSummary {
  to: string;
  access: string;
  version: number;
  latest: boolean;
  state: string;
}

export interface PublicSummary {
  linkId: string;
  version: number;
  latest: boolean;
  at: string;
}

export interface Artifact {
  id: string;
  title: string;
  kind: Kind;
  conversation: string | null;
  job: string | null;
  created: string;
  versions: Version[];
  profile?: string | null;
  origin?: Origin;
  shares?: ShareSummary[];
  public?: PublicSummary | null;
}

export interface TitleBody {
  title: string;
}

/// obid's events stream, `artifact`: a version published, or a rename (`renamed: true`).
export const EVENT_TYPES = ["artifact"] as const;
export interface ArtifactEvent {
  id: string;
  version?: number;
  title: string;
  kind: Kind;
  conversation: string | null;
  job: string | null;
  renamed?: boolean;
}

export const EXPORTS = ["zip", "html"] as const;

// ── The turn's artifact ─────────────────────────────────────────────────────────────────────────

export interface TurnArtifact {
  id: string;
  version?: number | null;
}

export interface TurnBody {
  text: string;
  conversation?: string | null;
  artifact?: TurnArtifact | null;
}

export interface TurnAnswer {
  conversation: string;
  created: boolean;
  artifact: TurnArtifact | null;
}

// ── Errors ──────────────────────────────────────────────────────────────────────────────────────

export interface ApiError {
  error: string;
  reason: string;
}

export const REASONS = [
  "no_token",
  "bad_token",
  "unsigned",
  "bad_signature",
  "unknown_device",
  "not_found",
  "bad_request",
  "bad_title",
  "not_published",
  "too_big",
  "too_many",
  "bad_design",
  "bad_canvas",
  "made_elsewhere",
  "full",
  "bad_zip",
  "no_export",
] as const;
