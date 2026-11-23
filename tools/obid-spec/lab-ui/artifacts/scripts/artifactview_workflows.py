#!/usr/bin/env python3
"""Writes workflows/lab/artifactview/*.json (BLOK v2): edit this, not the JSON.

    python3 lab-ui/artifacts/scripts/artifactview_workflows.py workflows/lab/artifactview lab-ui/artifacts/design/project/canvas.json
"""
import json, os, sys

OUT = sys.argv[1]
CANVAS = sys.argv[2]  # lab-ui/artifacts/design/project/canvas.json
os.makedirs(OUT, exist_ok=True)


def step(id, node, inputs=None, proves=None, expect=None):
    s = {"id": id, "use": f"runtime.rust:{node}", "inputs": dict(inputs or {})}
    if expect is not None:
        s["inputs"]["expect"] = {"paths": expect}
    if proves:
        s["ui"] = {"proves": proves if isinstance(proves, list) else [proves]}
    return s


def wf(file, name, desc, steps):
    doc = {"schemaVersion": "2", "name": name, "version": "1.0.0", "description": desc,
           "trigger": {"manual": {}}, "tags": ["lab"], "steps": steps}
    with open(os.path.join(OUT, file), "w") as f:
        json.dump(doc, f, indent=2, ensure_ascii=False)
        f.write("\n")


VIEW = "${state.view}"


def base(fixture="claude-design", title="Lab design", pid="design"):
    return [
        step("up", "lab.artifacts.up", expect={"token": {"$type": "string"}}),
        step("view", "lab.artifactview.up", {"artifacts": "${state.up}"}, expect={"port": {"$type": "number"}}),
        step("fx", "lab.artifacts.fixture", {"name": fixture}),
        step(pid, "lab.artifacts.publish", {"path": "${state.fx.path}", "title": title, "conversation": "c1", "job": "j1"},
             expect={"ok": True}),
    ]


def render(id, artifact, path, w, h, proves=None, expect=None, name="main", within=True, **kw):
    inputs = {"id": artifact, "version": 1, "path": path, "width": w, "height": h, "name": name}
    if within:
        inputs["within"] = {"width": w, "height": h}
    inputs.update(kw)
    return step(id, "lab.artifactview.render", inputs, proves, expect)


def ev(id, expression, proves=None, expect=None, name="main", **kw):
    inputs = {"name": name, "expression": expression}
    inputs.update(kw)
    return step(id, "lab.artifactview.eval", inputs, proves, expect)


S8 = "LAB-ARTVIEW-8"
D = "${state.design.id}"
C = "${state.canvas.id}"
canvas = json.load(open(CANVAS))
real = []
for board, b in canvas["boards"].items():
    fill = b.get("expand") == "fill"
    exp = {"ready": True, "errors": {"$len": 0}}
    if not fill:
        exp["fits"] = True
    real.append(render(f"8-canvas-{board.replace('.dc.html', '').lower()}", C, board, b["w"], b["h"], S8, exp,
                       name=f"real-{board}", within=not fill))

wf(
    "render.json",
    "Lab artifact view: Oby's runtime draws every board of a Claude Design canvas within its size",
    f"{S8} (src/lab/artifactview.rs: serve; core/assets/oby-dc/dc-runtime.js). Oby's own runtime of the .dc.html format, written from the format alone on React 18.3.1 vendored (no Babel, no CDN), draws the lab design's boards in chrome-headless-shell, each within its board's size and with no error: its holes filled from renderVals, a sibling board mounted by dc-import, a list from sc-for, a branch from sc-if, a link to another board played in place without the page leaving, #id scrolling, each board's data-props defaults given as its props (its tweaks, shown as they are). The canvas host shows every board of a page in its own sandboxed frame and switches pages. Then the Oby Artifacts canvas itself (its twelve boards, eight of them the interactive prototype, people's names replaced) publishes and every board draws with no error, each fixed-size one within its size, and its canvas host has every frame drawn.",
    base()
    + [
        render("8-main-drawn", D, "Main.dc.html", 800, 600, S8, {"ready": True, "errors": {"$len": 0}, "fits": True, "title": "Main"}),
        ev("8-holes-filled", "document.querySelector('[data-oby-dc-root] h1').textContent", S8, {"value": "Lab design"}),
        ev("8-dc-import-and-sc-for", "document.querySelectorAll('[data-oby-dc-root] .card p').length", S8, {"value": 2}),
        ev("8-template-hidden", "document.querySelectorAll('x-dc').length", S8, {"value": 0}),
        ev("8-hash-link-scrolls-in-place", "(() => { document.querySelector('a[href=\"#end\"]').click(); return location.pathname + location.hash; })()", S8, {"value": "/Main.dc.html"}),
        ev(
            "8-board-link-plays-in-place",
            "(() => { document.querySelector('a[href=\"Media.dc.html\"]').click(); return new Promise(r => setTimeout(() => r({playing: window.__OBY_DC.playing, at: location.pathname, video: !!document.querySelector('[data-oby-dc-root] video')}), 400)); })()",
            S8,
            {"value": {"playing": "Media.dc.html", "at": "/Main.dc.html", "video": True}},
        ),
        render("8-card-drawn", D, "Card.dc.html", 400, 300, S8, {"ready": True, "errors": {"$len": 0}, "fits": True}, name="card"),
        render("8-media-drawn", D, "Media.dc.html", 800, 600, S8, {"ready": True, "errors": {"$len": 0}}, name="media", within=False),
        ev("8-sc-if-false-draws-nothing", "document.querySelectorAll('[data-oby-dc-root] p').length", S8, {"value": 0}, name="media"),
        render(
            "8-canvas-host",
            D,
            "__canvas.html?view=canvas",
            1280,
            800,
            S8,
            {"ready": True, "errors": {"$len": 0}},
            name="host",
            within=False,
        ),
        ev(
            "8-canvas-host-frames",
            "Object.keys(window.__OBY_CANVAS.frames).sort().join(',') + ' ' + Object.values(window.__OBY_CANVAS.frames).every(f => f.ready)",
            S8,
            {"value": "Card.dc.html,Main.dc.html true"},
            name="host",
        ),
        ev(
            "8-canvas-host-pages",
            "(() => { document.querySelector('[data-page=\"notes_2\"]').click(); return new Promise(r => setTimeout(() => r(Object.keys(window.__OBY_CANVAS.frames).join(',')), 300)); })()",
            S8,
            {"value": "Media.dc.html"},
            name="host",
        ),
        ev(
            "8-canvas-host-frames-sandboxed",
            "Array.from(document.querySelectorAll('iframe')).map(f => f.getAttribute('sandbox')).join(',')",
            S8,
            {"value": "allow-scripts"},
            name="host",
        ),
        step("cfx", "lab.artifacts.fixture", {"name": "canvas"}),
        step("canvas", "lab.artifacts.publish", {"path": "${state.cfx.path}", "title": "Oby Artifacts", "conversation": "c2", "job": "j2"},
             expect={"ok": True, "artifact.kind": "design"}),
    ]
    + real
    + [
        step(
            "8-each-board-its-own-tweaks",
            "lab.artifactview.compare",
            {"a": "${state.8-canvas-prototype.picture}", "b": "${state.8-canvas-viewer.picture}", "most": 0.02},
            S8,
            {"within": False, "width": 1280},
        ),
        render("8-canvas-whole", C, "__canvas.html?view=canvas", 1440, 900, S8, {"ready": True, "errors": {"$len": 0}}, name="real-host", within=False, timeoutMs=60000),
    ],
)

# ── 10 ────────────────────────────────────────────────────────────────────────────────────────
S10 = "LAB-ARTVIEW-10"
FAR = "${state.out.url}"
wf(
    "sandbox.json",
    "Lab artifact view: a board can't fetch, open a window or navigate; a link out is asked first",
    f"{S10} (src/lab/artifactview.rs: BOARD_POLICY, navigation; the runtime's links). A board drawn on its version's origin can't fetch a server outside it, nor load a picture from one (nothing reaches that server), and can't open a window. When its script sends the page to an address outside, the app stops it and asks the person whether to open it outside Oby: the board stays where it was. Its link to an address outside does the same, asked first and never followed by the page. In the canvas host, a board's frame is sandboxed: the host can't reach into it, as it can't reach out.",
    base()
    + [
        step("out", "lab.artifactview.outside", {}, None, {"hits": 0}),
        render("main", D, "Main.dc.html", 800, 600, None, {"ready": True}),
        ev("10-fetch-refused", f"fetch('{FAR}/ping').then(() => 'reached', () => 'refused')", S10, {"value": "refused"}),
        ev("10-picture-from-outside-refused", f"new Promise(r => {{ const i = new Image(); i.onload = () => r('loaded'); i.onerror = () => r('refused'); i.src = '{FAR}/pic.png'; }})", S10, {"value": "refused"}),
        step("10-nothing-reached-outside", "lab.artifactview.outside", {"outside": "${state.out}"}, S10, {"hits": 0}),
        ev("10-no-window", "String(window.open('https://example.com/window'))", S10, {"value": "null"}),
        ev(
            "10-script-navigation-stopped",
            "(() => { location.href = 'https://example.com/away'; return new Promise(r => setTimeout(() => r(location.pathname + ' ' + !!window.__OBY_DC), 600)); })()",
            S10,
            {"value": "/Main.dc.html true"},
        ),
        ev(
            "10-link-out-stays",
            "(() => { document.querySelector('a[href=\"https://example.com/more\"]').click(); return new Promise(r => setTimeout(() => r(location.pathname + ' ' + window.__OBY_DC.asked.join(',')), 600)); })()",
            S10,
            {"value": "/Main.dc.html https://example.com/more"},
        ),
        step(
            "10-asked-before-anything-opens",
            "lab.artifactview.navigation",
            {"name": "main"},
            S10,
            {"asked": ["https://example.com/away", "https://example.com/more"], "allowed": {"$len": 0}},
        ),
        render("host", D, "__canvas.html?view=canvas", 1280, 800, None, {"ready": True}, name="host", within=False),
        ev(
            "10-host-cant-reach-a-board",
            "Array.from(document.querySelectorAll('iframe')).every(f => { try { return f.contentDocument === null; } catch (e) { return true; } })",
            S10,
            {"value": True},
            name="host",
        ),
    ],
)

# ── 9 ─────────────────────────────────────────────────────────────────────────────────────────
S9 = "LAB-ARTVIEW-9"
API = "${state.view.api}"
TAB = "${state.9-opened.body.id}"


def tabs_call(id, method, path, proves=None, expect=None, api=API, device="${state.d1}", **kw):
    inputs = {"api": api, "method": method, "path": path, "device": device, "as": "device"}
    inputs.update(kw)
    return step(id, "lab.artifactview.call", inputs, proves, expect)


wf(
    "tab.json",
    "Lab artifact view: an artifact tab opens, isn't streamed, can't be taken or driven, and keeps its place over a restart",
    f"{S9} (src/lab/artifactview.rs: ArtifactTabs; the browser contract's kind and artifact). The owner opens an artifact as a tab, signed: a tab of kind artifact, the person's, on the latest version at oby-artifact://<id>-v1/__canvas.html; opening it again brings the same tab, and unsigned it's refused. Taking it, streaming it and sending it input are each refused artifact_tab: the app draws it and nobody drives it. Another token (an agent's) reaches nothing. A new version moves it along; pinned to version 1 it stays there through version 3. Restarted, the view server has the same tab where it was; closed, it's gone.",
    base()
    + [
        step("d1", "lab.artifactview.device", {}),
        tabs_call(
            "9-opened",
            "POST",
            "/v1/browser/tabs",
            S9,
            {
                "status": 200,
                "body.kind": "artifact",
                "body.owner.kind": "person",
                "body.state": "ready",
                "body.artifact": {"id": D, "version": 1, "latest": True},
                "body.url": {"$matches": "^oby-artifact://[0-9a-f-]{36}-v1/__canvas\\.html$"},
            },
            body={"artifact": D},
        ),
        tabs_call("9-again-the-same-tab", "POST", "/v1/browser/tabs", S9, {"status": 200, "body.id": TAB}, body={"artifact": D}),
        tabs_call("9-unsigned-refused", "POST", "/v1/browser/tabs", S9, {"status": 401, "body.reason": "unsigned"}, body={"artifact": D}, **{"as": "unsigned"}),
        tabs_call("9-listed", "GET", "/v1/browser", S9, {"status": 200, "body.tabs": {"$len": 1}, "body.tabs.0.kind": "artifact"}),
        tabs_call("9-not-taken", "POST", f"/v1/browser/tabs/{TAB}/take", S9, {"status": 409, "body.reason": "artifact_tab"}),
        tabs_call("9-not-streamed", "GET", f"/v1/browser/tabs/{TAB}/stream", S9, {"status": 409, "body.reason": "artifact_tab"}),
        tabs_call("9-not-driven", "POST", f"/v1/browser/tabs/{TAB}/input", S9, {"status": 409, "body.reason": "artifact_tab"}, body={"type": "click", "x": 1, "y": 1}),
        tabs_call("9-an-agents-token-reaches-nothing", "GET", "/v1/browser", S9, {"status": 401, "body.reason": "bad_token"}, token="a-job-token", **{"as": "job"}),
        step("v2", "lab.artifacts.publish", {"path": "${state.fx.path}", "title": "Lab design", "conversation": "c1", "job": "j2", "artifact": D, "write": {"lab.css": ".card{background:#202022}\n"}}, None, {"ok": True, "version": 2}),
        tabs_call("9-follows-the-latest", "GET", "/v1/browser", S9, {"body.tabs.0.artifact.version": 2, "body.tabs.0.url": {"$contains": "-v2/"}}),
        tabs_call("9-pinned", "POST", f"/v1/browser/tabs/{TAB}/version", S9, {"status": 200, "body.artifact.version": 1, "body.artifact.latest": False}, body={"version": 1}),
        step("v3", "lab.artifacts.publish", {"path": "${state.fx.path}", "title": "Lab design", "conversation": "c1", "job": "j3", "artifact": D, "write": {"lab.css": ".card{background:#242426}\n"}}, None, {"ok": True, "version": 3}),
        tabs_call("9-pinned-stays", "GET", "/v1/browser", S9, {"body.tabs.0.artifact.version": 1}),
        step("restarted", "lab.artifactview.restart", {}, None, {"port": {"$ne": "${state.view.port}"}}),
        step("d2", "lab.artifactview.device", {}),
        tabs_call("9-kept-over-a-restart", "GET", "/v1/browser", S9, {"status": 200, "body.tabs.0.id": TAB, "body.tabs.0.artifact": {"id": D, "version": 1, "latest": False}}, api="${state.restarted.api}"),
        tabs_call("9-closed", "POST", f"/v1/browser/tabs/{TAB}/close", S9, {"status": 204}, api="${state.restarted.api}", device="${state.d2}"),
        tabs_call("9-gone", "GET", "/v1/browser", S9, {"body.tabs": {"$len": 0}}, api="${state.restarted.api}"),
    ],
)

# ── 11 ────────────────────────────────────────────────────────────────────────────────────────
S11 = "LAB-ARTVIEW-11"
wf(
    "check.json",
    "Lab artifact view: oby_artifact_check returns a picture and a board's thrown error",
    f"{S11} (src/lab/artifactview.rs: check). An agent's folder, before it is published, drawn as it would be: for a design with a board whose renderVals throws and one whose logic doesn't parse, the check answers not ok, with each mistake under its board's name and a picture of each board; the good board draws. A clean design answers ok with its pictures and height; so does a page.",
    [
        step("up", "lab.artifacts.up", expect={"token": {"$type": "string"}}),
        step("view", "lab.artifactview.up", {"artifacts": "${state.up}"}, expect={"port": {"$type": "number"}}),
        step("broken", "lab.artifacts.fixture", {"name": "broken-design"}),
        step(
            "11-mistakes-by-board",
            "lab.artifactview.check",
            {"path": "${state.broken.path}", "width": 400, "height": 300},
            S11,
            {
                "ok": False,
                "kind": "design",
                "errors": {"$any": {"board": "Throws.dc.html", "message": {"$contains": "the lab board broke"}}},
                "pictures": {"$len": 3},
                "pictures.0.bytes": {"$ne": 0},
            },
        ),
        step(
            "11-unparsed-said",
            "lab.artifactview.check",
            {"path": "${state.broken.path}", "board": "Unparsed.dc.html", "width": 400, "height": 300},
            S11,
            {"ok": False, "errors": {"$any": {"board": "Unparsed.dc.html", "message": {"$contains": "SyntaxError"}}}},
        ),
        step(
            "11-the-good-board-draws",
            "lab.artifactview.check",
            {"path": "${state.broken.path}", "board": "Good.dc.html", "width": 400, "height": 300},
            S11,
            {"ok": True, "errors": {"$len": 0}},
        ),
        step("clean", "lab.artifacts.fixture", {"name": "claude-design"}),
        step("11-a-clean-design", "lab.artifactview.check", {"path": "${state.clean.path}", "width": 800, "height": 600}, S11, {"ok": True, "errors": {"$len": 0}, "pictures": {"$len": 3}, "height": {"$ne": 0}}),
        step("page", "lab.artifacts.fixture", {"name": "page"}),
        step("11-a-page", "lab.artifactview.check", {"path": "${state.page.path}"}, S11, {"ok": True, "kind": "html", "pictures": {"$len": 1}}),
    ],
)

# ── 8, against Claude's own pictures ──────────────────────────────────────────────────────────
vs = []
for board, b in canvas["boards"].items():
    vs.append(step(f"8-like-claude-{board.replace('.dc.html', '').lower()}", "lab.artifactview.vsclaude",
                   {"id": C, "version": 1, "path": board, "width": b["w"], "height": b["h"], "most": 0.002}, S8, {"within": True}))
wf(
    "claude.json",
    "Lab artifact view: each board of the canvas looks as Claude draws it (only with Claude's runtime at hand)",
    f"{S8}, against Claude (src/lab/artifactview.rs: vs_claude). Each board of the Oby Artifacts canvas drawn by Oby's runtime and by Claude Design's own (a copy named by OBID_SPEC_CLAUDE_DC_RUNTIME, which this repository never holds and Oby's runtime was never written from), at the board's size: its picture taken once its animations have ended, at most 0.2% of the pixels differ by more than 16 in a channel. Skipped without the variable.",
    base()[:2]
    + [
        step("cfx", "lab.artifacts.fixture", {"name": "canvas"}),
        step("canvas", "lab.artifacts.publish", {"path": "${state.cfx.path}", "title": "Oby Artifacts", "conversation": "c2", "job": "j2"}, expect={"ok": True}),
    ]
    + vs,
)

# ── cleanup ───────────────────────────────────────────────────────────────────────────────────
wf(
    "cleanup.json",
    "Lab artifact view: everything the view started is stopped at the end",
    "What the view starts it stops (src/lab/artifactview.rs, cleanup): with a board drawn in Chrome and the outside server up, lab.artifactview.down (the same cleanup the suite runs at every workflow's end) stops the view server, Chrome and every process of its profile, and the outside server; the view's port answers nothing after.",
    base()
    + [
        step("out", "lab.artifactview.outside", {}),
        render("main", D, "Main.dc.html", 800, 600, None, {"ready": True}),
        step("alive", "lab.artifacts.alive", {"api": "${state.view}"}, None, {"answers": True}),
        step("down", "lab.artifactview.down", {}, None, {"stopped": True, "chrome": True, "chromeLeft": 0, "outside": True}),
        step("gone", "lab.artifacts.alive", {"api": "${state.view}"}, None, {"answers": False}),
    ],
)
print("ok")
