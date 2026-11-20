#!/usr/bin/env python3
"""Writes workflows/lab/artifacts/*.json (BLOK v2): edit this, not the JSON.

    python3 lab-ui/artifacts/scripts/artifacts_workflows.py workflows/lab/artifacts
"""
import json, os, sys

OUT = sys.argv[1]
os.makedirs(OUT, exist_ok=True)

LONG = "Kq7Zt2mW9xR4vB8nL3pF6hJ1sD5gA0cYeU7iO2wT"
MB = 1024 * 1024
POLICY = "default-src 'self' data: blob:; script-src 'self' 'unsafe-inline' data: blob:; style-src 'self' 'unsafe-inline' data:; img-src 'self' data: blob:; connect-src 'none'; frame-ancestors 'none'; object-src 'none'; base-uri 'none'; form-action 'none'; sandbox allow-scripts"


def step(id, node, inputs=None, proves=None, expect=None):
    s = {"id": id, "use": f"runtime.rust:lab.artifacts.{node}", "inputs": dict(inputs or {})}
    if expect is not None:
        s["inputs"]["expect"] = {"paths": expect}
    if proves:
        s["ui"] = {"proves": proves if isinstance(proves, list) else [proves]}
    return s


API = "${state.up}"
DEV = "${state.d1}"


def up():
    return [
        step("up", "up", expect={"url": {"$contains": "http://127.0.0.1:"}, "token": {"$type": "string"}}),
        step("d1", "device", {}, expect={"id": {"$type": "string"}}),
    ]


def wf(file, name, desc, steps):
    doc = {
        "schemaVersion": "2",
        "name": name,
        "version": "1.0.0",
        "description": desc,
        "trigger": {"manual": {}},
        "tags": ["lab"],
        "steps": steps,
    }
    with open(os.path.join(OUT, file), "w") as f:
        json.dump(doc, f, indent=2, ensure_ascii=False)
        f.write("\n")


def publish(id, path, proves=None, expect=None, **kw):
    inputs = {"path": path, "title": kw.pop("title", "Lab artifact")}
    inputs.update(kw)
    return step(id, "publish", inputs, proves, expect)


def call(id, method, path, proves=None, expect=None, **kw):
    inputs = {"api": API, "method": method, "path": path}
    inputs.update(kw)
    return step(id, "call", inputs, proves, expect)


def serve(id, artifact, version, path, proves=None, expect=None, **kw):
    inputs = {"api": API, "id": artifact, "version": version, "path": path}
    inputs.update(kw)
    return step(id, "serve", inputs, proves, expect)


# ── 1 ─────────────────────────────────────────────────────────────────────────────────────────
S1 = "LAB-ARTIFACTS-1"
wf(
    "design.json",
    "Lab artifacts: a Claude Design canvas publishes as design, byte for byte, and its export zip equals the import",
    f"{S1} (src/lab/artifacts.rs: check_design, ingest_blobs, export_entries, zip_of, unzip, project_of). The lab's own Claude Design project (canvas.json v3 with pages, notes, launch and a design system; three .dc.html boards; fonts, video, data; three uploads under _blob/, one whose id looks like a token) publishes from an agent's folder as the design kind: every file kept byte for byte, each upload moved under assets/ with the map in the version, the token-like id under a hashed name rather than refused. The same project as Claude Design's project zip imports (signed) as a design whose origin is the import and whose title is canvas.json's; its export zip holds exactly the import's entries, byte for byte, and a second import and export gives the same zip bytes. Its standalone HTML export holds every file and reaches nothing outside itself.",
    up()
    + [
        step("fx", "fixture", {"name": "claude-design", "zip": True}, expect={"zip": {"$type": "string"}}),
        publish(
            "1-published-as-design",
            "${state.fx.path}",
            S1,
            {
                "ok": True,
                "artifact.kind": "design",
                "artifact.origin.kind": "own",
                "artifact.versions.0.version": 1,
                "artifact.versions.0.files": {"$contains": "canvas.json"},
                f"artifact.versions.0.blobs.{LONG}": {"$matches": "^assets/[0-9a-f]{16}\\.png$"},
                "artifact.versions.0.blobs.font-7c1e": "assets/font-7c1e.woff2",
                "artifact.versions.0.blobs.clip_3": "assets/clip_3.mp4",
                "sameAsSource": True,
            },
            title="Lab design",
            conversation="c1",
            job="j1",
        ),
        publish(
            "1-no-upload-kept-at-its-claude-path",
            "${state.fx.path}",
            S1,
            {"ok": True, "version": 1, "artifact.versions.0.files": {"$none": f"_blob/{LONG}"}},
            title="Lab design",
            conversation="c1",
            job="j1",
            artifact="${state.1-published-as-design.id}",
        ),
        step(
            "1-imported-from-the-zip",
            "import",
            {"api": API, "device": DEV, "file": "${state.fx.zip}"},
            S1,
            {
                "status": 200,
                "body.kind": "design",
                "body.title": "Lab design",
                "body.origin": {"kind": "import", "from": "claude-design-zip"},
                "body.conversation": None,
                f"body.versions.0.blobs.{LONG}": {"$matches": "^assets/[0-9a-f]{16}\\.png$"},
            },
        ),
        step(
            "1-export-equals-the-import",
            "roundtrip",
            {"api": API, "device": DEV, "zip": "${state.fx.zip}"},
            S1,
            {
                "same": True,
                "onlyInSource": {"$len": 0},
                "onlyInExport": {"$len": 0},
                "changed": {"$len": 0},
                "stable": True,
                "imported.kind": "design",
            },
        ),
        step(
            "1-export-zip",
            "export",
            {"api": API, "id": "${state.1-imported-from-the-zip.body.id}", "version": 1, "as": "zip"},
            S1,
            {
                "status": 200,
                "type": "application/zip",
                "entries": {"$contains": {"name": f"project/_blob/{LONG}"}},
                "disposition": {"$contains": "Lab-design-v1.zip"},
            },
        ),
        step(
            "1-export-html-is-one-file",
            "export",
            {"api": API, "id": "${state.1-imported-from-the-zip.body.id}", "version": 1, "as": "html"},
            S1,
            {
                "status": 200,
                "type": {"$contains": "text/html"},
                "outside": {"$len": 0},
                "files": {"$contains": "Main.dc.html"},
            },
        ),
    ],
)

# ── 2 ─────────────────────────────────────────────────────────────────────────────────────────
S2 = "LAB-ARTIFACTS-2"
spoiled = [
    ("missing-board", "bad_design", "board \"Gone.dc.html\" isn't a file of the design"),
    ("bad-page", "bad_design", "page id \"not a page id\""),
    ("bad-order", "bad_design", "order names \"Elsewhere.dc.html\""),
    ("bad-launch", "bad_design", "launch names \"Elsewhere.dc.html\""),
    ("no-xdc", "bad_design", "Card.dc.html has no <x-dc>"),
    ("two-scripts", "bad_design", "Card.dc.html has 2"),
    ("no-ds", "bad_design", "design system \"absent\""),
    ("link", "not_published", "linked.css is a link"),
]
steps2 = up() + [
    step("obid", "fixture", {"name": "canvas-obid"}),
    step("swift", "fixture", {"name": "canvas-swift"}),
    publish("2-obid-canvas", "${state.obid.path}", S2, {"ok": True, "artifact.kind": "canvas"}),
    publish("2-swift-canvas", "${state.swift.path}", S2, {"ok": True, "artifact.kind": "canvas"}),
]
for how, reason, says in spoiled:
    steps2.append(step(f"fx-{how}", "fixture", {"name": "claude-design", "break": how}))
    steps2.append(
        publish(
            f"2-refused-{how}",
            f"${{state.fx-{how}.path}}",
            S2,
            {"ok": False, "reason": reason, "error": {"$contains": says}},
        )
    )
wf(
    "shapes.json",
    "Lab artifacts: obid's and Swift's canvases both publish; a design with a missing board, a bad page id or a link is refused, saying which",
    f"{S2} (src/lab/artifacts.rs: check_canvas, check_design, read_source). A canvas.json that is obid's list of frames, and one that is Swift's {{\"frames\": [...]}}, both publish as canvas. Claude Design projects spoiled one way each are refused with the reason bad_design and one line naming what is wrong: a board canvas.json names that isn't a file, a page id that isn't 1 to 40 letters, digits, _ or -, an order or a launch naming no board, a board with no <x-dc> or two logic scripts, a design system with no ds/ folder; and a project holding a link is refused naming the link, never following it.",
    steps2,
)

# ── 3 ─────────────────────────────────────────────────────────────────────────────────────────
S3 = "LAB-ARTIFACTS-3"
A = "${state.design.id}"
wf(
    "blobs.json",
    "Lab artifacts: /_blob/<id> resolves inside the version only; a blob outside the map is 404",
    f"{S3} (src/lab/artifacts.rs: resolve). A published design's uploads are served at {{version}}/_blob/<id>: the token-like id's PNG byte for byte as image/png under obid's artifact CSP, the font as font/woff2. An id the version's map doesn't hold, the same id asked of another artifact or of a version that isn't there, and a path stepping out of _blob/ are all 404.",
    up()
    + [
        step("fx", "fixture", {"name": "claude-design"}),
        publish("design", "${state.fx.path}", None, {"ok": True}),
        step("page-fx", "fixture", {"name": "page"}),
        publish("page", "${state.page-fx.path}", None, {"ok": True}),
        serve(
            "3-upload-served-by-its-claude-name",
            A,
            1,
            f"_blob/{LONG}",
            S3,
            {"status": 200, "type": "image/png", "same": True, "csp": POLICY, "nosniff": True},
            sameAs="${state.fx.blobLocal}",
        ),
        serve("3-font-upload", A, 1, "_blob/font-7c1e", S3, {"status": 200, "type": "font/woff2"}),
        serve("3-unknown-id-404", A, 1, "_blob/not-in-the-map", S3, {"status": 404, "body.reason": "not_found"}),
        serve("3-other-artifact-404", "${state.page.id}", 1, f"_blob/{LONG}", S3, {"status": 404}),
        serve("3-other-version-404", A, 2, f"_blob/{LONG}", S3, {"status": 404}),
        serve("3-no-way-out-404", A, 1, "_blob/../canvas.json", S3, {"status": 404}),
        serve("3-board-itself", A, 1, "Main.dc.html", S3, {"status": 200, "type": "text/html; charset=utf-8"}),
    ],
)

# ── 4 ─────────────────────────────────────────────────────────────────────────────────────────
S4 = "LAB-ARTIFACTS-4"
types = [
    ("fonts/Lab.ttf", "font/ttf"),
    ("fonts/Lab.otf", "font/otf"),
    ("fonts/Lab.woff2", "font/woff2"),
    ("media/loop.webm", "video/webm"),
    ("media/still.avif", "image/avif"),
    ("_blob/clip_3", "video/mp4"),
    ("data.json", "application/json"),
    ("app.js", "text/javascript; charset=utf-8"),
    ("lab.css", "text/css; charset=utf-8"),
    ("widget.jsx", "application/octet-stream"),
]
steps4 = up() + [step("fx", "fixture", {"name": "claude-design"}), publish("design", "${state.fx.path}", None, {"ok": True})]
for path, t in types:
    steps4.append(
        serve(
            "4-" + path.replace("/", "-").replace(".", "-").replace("_", ""),
            A,
            1,
            path,
            S4,
            {"status": 200, "type": t, "csp": POLICY, "nosniff": True},
        )
    )
wf(
    "types.json",
    "Lab artifacts: fonts and video are served with their types",
    f"{S4} (src/lab/artifacts.rs: type_of). A design's TrueType and OpenType fonts, WOFF2, WebM and an MP4 upload, AVIF, JSON and scripts are each served with its own media type, every one with obid's artifact CSP and nosniff; a .jsx stays application/octet-stream, as the format needs none. (Fetching a design's Google fonts into it comes with the renderer, the artifactview PR, where the board is drawn offline.)",
    steps4,
)

# ── 5 ─────────────────────────────────────────────────────────────────────────────────────────
S5 = "LAB-ARTIFACTS-5"
P = "${state.page.id}"
wf(
    "rename.json",
    "Lab artifacts: a rename sends artifact {renamed}; a delete takes it away",
    f"{S5} (src/lab/artifacts.rs: handle's Rename and Delete). Renaming an artifact, signed by the owner's device, answers it with its new title and sends the artifact event with renamed: true and the title (obid sends none today); a title that isn't one line of 1 to 100 characters is refused bad_title and sends nothing. Deleting it, signed, answers 204 and it is gone: 404 after, and again on a second delete. (Revoking its shares and public link first comes with the sharing PR.)",
    up()
    + [
        step("fx", "fixture", {"name": "page"}),
        publish("page", "${state.fx.path}", None, {"ok": True}, conversation="c1", job="j1"),
        call(
            "5-renamed",
            "PATCH",
            f"/v1/artifacts/{P}",
            S5,
            {"status": 200, "body.title": "Renamed page", "body.versions": {"$len": 1}},
            body={"title": "Renamed page"},
            device=DEV,
            **{"as": "device"},
        ),
        step(
            "5-rename-event",
            "events",
            {},
            S5,
            {
                "count": 2,
                "events": {"$any": {"type": "artifact", "data": {"id": P, "renamed": True, "title": "Renamed page", "conversation": "c1"}}},
            },
        ),
        call(
            "5-bad-title",
            "PATCH",
            f"/v1/artifacts/{P}",
            S5,
            {"status": 422, "body.reason": "bad_title"},
            body={"title": ""},
            device=DEV,
            **{"as": "device"},
        ),
        step("5-no-event-for-a-refusal", "events", {}, S5, {"count": 2}),
        call("5-deleted", "DELETE", f"/v1/artifacts/{P}", S5, {"status": 204}, device=DEV, **{"as": "device"}),
        call("5-gone", "GET", f"/v1/artifacts/{P}", S5, {"status": 404, "body.reason": "not_found"}),
        call("5-gone-again", "DELETE", f"/v1/artifacts/{P}", S5, {"status": 404}, device=DEV, **{"as": "device"}),
    ],
)

# ── 6 ─────────────────────────────────────────────────────────────────────────────────────────
S6 = "LAB-ARTIFACTS-6"
wf(
    "versions.json",
    "Lab artifacts: a follow-up job in the same conversation versions the artifact; another conversation can't",
    f"{S6} (src/lab/artifacts.rs: may_version). Job j1 of conversation c1 publishes a page. A follow-up job j2 of c1 (the artifact's chat starts a new job in its conversation) publishes a changed page as a new version of it: version 2, made by j2, the artifact still j1's and c1's. The same files again make no version. Job j3 of conversation c2 is refused made_elsewhere, and c1's own conversation tool (no job) makes version 3.",
    up()
    + [
        step("fx", "fixture", {"name": "page"}),
        publish("first", "${state.fx.path}", None, {"ok": True, "version": 1}, conversation="c1", job="j1"),
        publish(
            "6-follow-up-job-versions-it",
            "${state.fx.path}",
            S6,
            {"ok": True, "version": 2, "artifact.job": "j1", "artifact.conversation": "c1", "artifact.versions.1.job": "j2"},
            conversation="c1",
            job="j2",
            artifact="${state.first.id}",
            write={"note.txt": "the follow-up's change"},
        ),
        publish(
            "6-same-files-no-version",
            "${state.fx.path}",
            S6,
            {"ok": True, "version": 2, "artifact.versions": {"$len": 2}},
            conversation="c1",
            job="j2",
            artifact="${state.first.id}",
        ),
        publish(
            "6-other-conversation-refused",
            "${state.fx.path}",
            S6,
            {"ok": False, "reason": "made_elsewhere", "error": {"$contains": "belongs to another conversation"}},
            conversation="c2",
            job="j3",
            artifact="${state.first.id}",
            write={"note.txt": "another conversation's change"},
        ),
        publish(
            "6-the-conversations-own-tool",
            "${state.fx.path}",
            S6,
            {"ok": True, "version": 3, "artifact.versions.2.job": None},
            conversation="c1",
            artifact="${state.first.id}",
            write={"note.txt": "the conversation's own change"},
        ),
    ],
)

# ── 7 ─────────────────────────────────────────────────────────────────────────────────────────
S7 = "LAB-ARTIFACTS-7"
T = "${state.7-an-import-gets-one.body.conversation}"
I = "${state.imported.body.id}"
wf(
    "turn.json",
    "Lab artifacts: words with artifact land in its conversation even when another was open; an artifact with none gets one",
    f"{S7} (src/lab/artifacts.rs: land_words, handle's Plans). Words said while an artifact made in conversation c1 is open land in c1, though the app had c-other open; the same words with no artifact land in c-other. An imported artifact has no conversation: its first words make one and it becomes the artifact's (TASKCONV-4), and its next words land there too. A job of that conversation then versions the import. Words naming no such artifact are refused 404.",
    up()
    + [
        step("fx", "fixture", {"name": "page"}),
        publish("page", "${state.fx.path}", None, {"ok": True}, conversation="c1", job="j1"),
        step(
            "7-lands-in-the-artifacts-conversation",
            "turn",
            {"api": API, "device": DEV, "text": "Make the title bigger.", "conversation": "c-other", "artifact": {"id": "${state.page.id}", "version": 1}},
            S7,
            {"status": 200, "body.conversation": "c1", "body.created": False, "body.artifact.id": "${state.page.id}"},
        ),
        step(
            "7-no-artifact-the-open-one",
            "turn",
            {"api": API, "device": DEV, "text": "And the other thing.", "conversation": "c-other"},
            S7,
            {"status": 200, "body.conversation": "c-other", "body.created": False},
        ),
        step(
            "imported",
            "import",
            {"api": API, "device": DEV, "file": "${state.fx.path}/index.html", "contentType": "text/html"},
            None,
            {"status": 200, "body.conversation": None, "body.origin": {"kind": "import", "from": "html"}},
        ),
        step(
            "7-an-import-gets-one",
            "turn",
            {"api": API, "device": DEV, "text": "What is this?", "conversation": "c-other", "artifact": {"id": I}},
            S7,
            {"status": 200, "body.created": True, "body.conversation": {"$matches": "^conversation-[0-9a-f]{12}$"}},
        ),
        call("7-the-import-is-the-conversations", "GET", f"/v1/artifacts/{I}", S7, {"status": 200, "body.conversation": T}),
        step(
            "7-next-words-land-there-too",
            "turn",
            {"api": API, "device": DEV, "text": "Make it blue.", "artifact": {"id": I}},
            S7,
            {"status": 200, "body.created": False, "body.conversation": T},
        ),
        publish(
            "7-a-job-of-it-versions-the-import",
            "${state.fx.path}",
            S7,
            {"ok": True, "version": 2, "artifact.origin.kind": "import"},
            conversation=T,
            job="j9",
            artifact=I,
            write={"index.html": "<!doctype html><html><head><title>Blue</title></head><body><h1>Blue</h1></body></html>\n"},
        ),
        step(
            "7-no-such-artifact",
            "turn",
            {"api": API, "device": DEV, "text": "Hm.", "artifact": {"id": "00000000-0000-4000-8000-000000000000"}},
            S7,
            {"status": 404, "body.reason": "not_found"},
        ),
    ],
)

# ── 19 ────────────────────────────────────────────────────────────────────────────────────────
S19 = "LAB-ARTIFACTS-19"
steps19 = up() + [
    step("fx", "fixture", {"name": "page"}),
    publish("page", "${state.fx.path}", None, {"ok": True}, conversation="c1", job="j1"),
    step("design-fx", "fixture", {"name": "claude-design", "zip": True}),
]
for who, status, reason in [
    ("unsigned", 401, "unsigned"),
    ("badsig", 401, "bad_signature"),
    ("unknown-device", 401, "unknown_device"),
    ("none", 401, "no_token"),
    ("wrong", 401, "bad_token"),
]:
    steps19.append(
        call(f"19-rename-{who}", "PATCH", f"/v1/artifacts/{P}", S19, {"status": status, "body.reason": reason}, body={"title": "Not this"}, device=DEV, **{"as": who})
    )
steps19 += [
    call("19-delete-unsigned", "DELETE", f"/v1/artifacts/{P}", S19, {"status": 401, "body.reason": "unsigned"}, **{"as": "unsigned"}),
    step("19-import-unsigned", "import", {"api": API, "file": "${state.design-fx.zip}", "as": "unsigned"}, S19, {"status": 401, "body.reason": "unsigned"}),
    step("19-turn-unsigned", "turn", {"api": API, "text": "Hm.", "artifact": {"id": P}, "as": "unsigned"}, S19, {"status": 401, "body.reason": "unsigned"}),
    call("19-list-needs-the-token", "GET", "/v1/artifacts", S19, {"status": 401, "body.reason": "no_token"}, **{"as": "none"}),
    call("19-nothing-was-renamed", "GET", f"/v1/artifacts/{P}", S19, {"status": 200, "body.title": "Lab artifact"}),
    call("19-signed-it-is-done", "PATCH", f"/v1/artifacts/{P}", S19, {"status": 200, "body.title": "Signed"}, body={"title": "Signed"}, device=DEV, **{"as": "device"}),
]
wf(
    "signed.json",
    "Lab artifacts: every owner act is refused unsigned",
    f"{S19}, the store's part (src/lab/artifacts.rs: Words). Renaming, deleting, importing and the words of a turn each need the device's signed words over the exact body: unsigned, signed by another key, by a device never registered, with no token or a wrong one, each is refused 401 with its reason, and nothing changed. Reading needs obid's token. Signed by the owner's device, the rename is done. (Sharing's acts join this list in the sharing PR.)",
    steps19,
)

# ── 23 ────────────────────────────────────────────────────────────────────────────────────────
S23 = "LAB-ARTIFACTS-23"


def sized(id, files):
    return step(id, "fixture", {"name": "sized", "files": [{"path": p, "bytes": b} for p, b in files]})


version_files = [("index.html", MB)] + [(f"b{i:02}.bin", 15 * MB) for i in range(17)]
version_over = [("index.html", MB + 1)] + [(f"b{i:02}.bin", 15 * MB) for i in range(17)]
wf(
    "limits.json",
    "Lab artifacts: every limit refuses one byte or one file over, saying which",
    f"{S23} (src/lab/artifacts.rs: check_sizes, Claude's limits as the owner decided on 2026-11-20). At each limit it publishes, and one over is refused saying which file or what, and by how much: a text file of 16 MB, a binary of 15 MB, an SVG of 2 MB, 512 files, a version of 256 MB (17 binaries of 15 MB and a 1 MB page), and 50 versions. The files are sparse: only their length is written.",
    up()
    + [
        sized("text-at", [("index.html", 16 * MB)]),
        publish("23-text-at-16-mb", "${state.text-at.path}", S23, {"ok": True}),
        sized("text-over", [("index.html", 16 * MB + 1)]),
        publish("23-text-one-byte-over", "${state.text-over.path}", S23, {"ok": False, "reason": "too_big", "error": "Not published: index.html is 16 MB and 1 byte, and a text file may be 16 MB at most."}),
        sized("bin-at", [("index.html", 10), ("still.png", 15 * MB)]),
        publish("23-binary-at-15-mb", "${state.bin-at.path}", S23, {"ok": True}),
        sized("bin-over", [("index.html", 10), ("still.png", 15 * MB + 1)]),
        publish("23-binary-one-byte-over", "${state.bin-over.path}", S23, {"ok": False, "reason": "too_big", "error": {"$contains": "still.png is 15 MB and 1 byte, and a binary file"}}),
        sized("svg-at", [("index.html", 10), ("drawing.svg", 2 * MB)]),
        publish("23-svg-at-2-mb", "${state.svg-at.path}", S23, {"ok": True}),
        sized("svg-over", [("index.html", 10), ("drawing.svg", 2 * MB + 1)]),
        publish("23-svg-one-byte-over", "${state.svg-over.path}", S23, {"ok": False, "reason": "too_big", "error": {"$contains": "drawing.svg is 2 MB and 1 byte, and an SVG may be 2 MB at most"}}),
        step("files-at", "fixture", {"name": "many", "count": 511}),
        publish("23-512-files", "${state.files-at.path}", S23, {"ok": True, "artifact.versions.0.files": {"$len": 512}}),
        step("files-over", "fixture", {"name": "many", "count": 512}),
        publish("23-513-files", "${state.files-over.path}", S23, {"ok": False, "reason": "too_many", "error": "Not published: a version holds at most 512 files, and this holds 513."}),
        sized("version-at", version_files),
        publish("23-version-at-256-mb", "${state.version-at.path}", S23, {"ok": True, "artifact.versions.0.bytes": 256 * MB}),
        sized("version-over", version_over),
        publish("23-version-one-byte-over", "${state.version-over.path}", S23, {"ok": False, "reason": "too_big", "error": "Not published: this version is 256 MB and 1 byte, and a version may be 256 MB at most."}),
        step("small", "fixture", {"name": "page"}),
        publish("first", "${state.small.path}", None, {"ok": True, "version": 1}, conversation="c1", job="j1"),
        publish("23-50-versions", "${state.small.path}", S23, {"ok": True, "version": 50, "repeated": 49}, conversation="c1", job="j1", artifact="${state.first.id}", repeat=49),
        publish(
            "23-the-51st-refused",
            "${state.small.path}",
            S23,
            {"ok": False, "reason": "full", "error": {"$contains": "has 50 versions, the most obid keeps"}},
            conversation="c1",
            job="j1",
            artifact="${state.first.id}",
            write={"version.txt": "51"},
        ),
    ],
)

# ── 20 ────────────────────────────────────────────────────────────────────────────────────────
S20 = "LAB-ARTIFACTS-20"
wf(
    "claude-headless.json",
    "Lab artifacts: Claude Code headless, whether its Artifact tool is offered, and what it makes from Oby's guide (informational, never red)",
    f"{S20} (src/lab/artifacts.rs: claude_headless; lab-ui/artifacts/scripts/claude-headless.mjs; lab-ui/artifacts/guide.md). The owner's own Claude Code, on its own login as Oby's own lane runs it, headless in stream-json: its system/init says whether Claude's Artifact tool is offered, with and without --allowedTools Artifact (the bridge to Claude waits for it). Then it makes a design from Oby's guide alone, with Read, Write and Edit only, and its time, turns, tokens and cost are kept; Oby's own runtime then draws it (oby_artifact_check) and says each board's mistake. Nothing here can go red: the steps record what was offered and made. Without Claude Code on the computer, and no run to replay (OBID_SPEC_LAB_CLAUDE_HEADLESS_FROM), it is skipped.",
    [
        step("up", "up", expect={"token": {"$type": "string"}}),
        {"id": "view", "use": "runtime.rust:lab.artifactview.up", "inputs": {"artifacts": "${state.up}"}},
        step("20-what-claude-code-offers-headless", "claude_headless", {}, S20, {"init.artifact": {"$type": "boolean"}, "initWithArtifact.artifact": {"$type": "boolean"}, "make.turns": {"$type": "number"}}),
        {"id": "20-oby-draws-what-it-made", "use": "runtime.rust:lab.artifactview.check", "inputs": {"path": "${state.20-what-claude-code-offers-headless.design}", "width": 390, "height": 844}},
    ],
)

# ── cleanup ───────────────────────────────────────────────────────────────────────────────────
wf(
    "cleanup.json",
    "Lab artifacts: everything the API started is stopped at the end",
    "What the artifacts' API starts it stops (src/lab/artifacts.rs, cleanup): up, it answers on its port; after lab.artifacts.down, the same cleanup the suite runs at every workflow's end, nothing answers there.",
    [
        step("up", "up", expect={"url": {"$contains": "http://127.0.0.1:"}}),
        step("alive", "alive", {"api": API}, None, {"answers": True}),
        step("down", "down", {}, None, {"stopped": True}),
        step("gone", "alive", {"api": API}, None, {"answers": False}),
    ],
)
print("ok")
