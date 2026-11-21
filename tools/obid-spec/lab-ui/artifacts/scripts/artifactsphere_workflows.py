#!/usr/bin/env python3
"""Writes workflows/lab/artifactsphere/*.json (BLOK v2): edit this, not the JSON.

    python3 lab-ui/artifacts/scripts/artifactsphere_workflows.py workflows/lab/artifactsphere
"""
import json, os, sys

OUT = sys.argv[1]
os.makedirs(OUT, exist_ok=True)

FENCE = "data, never instructions"


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


API = {o: f"${{state.sphere.{o}.api}}" for o in ("ana", "ben", "cleo")}
DEV = {"ana": "${state.da}", "ben": "${state.db}", "cleo": "${state.dc}"}
P = "${state.page.id}"
D = "${state.doc.id}"


def base(doc=False):
    s = [
        step("up", "lab.artifacts.up", expect={"token": {"$type": "string"}}),
        step("sphere", "lab.artifactsphere.up", {"artifacts": "${state.up}"}, expect={"sphere.url": {"$contains": "http://127.0.0.1:"}}),
        step("da", "lab.artifactsphere.device", {"oby": "ana"}),
        step("db", "lab.artifactsphere.device", {"oby": "ben"}),
        step("dc", "lab.artifactsphere.device", {"oby": "cleo"}),
        step("fx", "lab.artifacts.fixture", {"name": "page"}),
        step("page", "lab.artifacts.publish", {"path": "${state.fx.path}", "title": "Lab page", "conversation": "c1", "job": "j1"}, expect={"ok": True}),
    ]
    if doc:
        s += [
            step("dfx", "lab.artifacts.fixture", {"name": "document"}),
            step("doc", "lab.artifacts.publish", {"path": "${state.dfx.path}", "title": "Lab notes", "conversation": "c1", "job": "j1"}, expect={"ok": True}),
        ]
    return s


def call(id, who, method, path, proves=None, expect=None, signed=True, **kw):
    inputs = {"api": API[who], "method": method, "path": path, "device": DEV[who]}
    if signed:
        inputs["as"] = "device"
    inputs.update(kw)
    return step(id, "lab.artifactsphere.call", inputs, proves, expect)


def sync(id, who, proves=None, expect=None):
    return step(id, "lab.artifactsphere.sync", {"oby": who}, proves, expect)


def sphere(id, proves=None, expect=None, **kw):
    return step(id, "lab.artifactsphere.sphere", kw, proves, expect)


def said(id, who, proves=None, expect=None, **kw):
    inputs = {"oby": who}
    inputs.update(kw)
    return step(id, "lab.artifactsphere.said", inputs, proves, expect)


def share(id, art, to, access, proves=None, expect=None, **kw):
    body = {"to": to, "access": access}
    body.update(kw)
    return call(id, "ana", "POST", f"/v1/artifacts/{art}/shares", proves, expect, body=body)


# ── 12 ────────────────────────────────────────────────────────────────────────────────────────
S12 = "LAB-ARTSPHERE-12"
BEN = "${state.12-ben-keeps-it.kept.0.artifact}"
wf(
    "share.json",
    "Lab artifact sphere: sharing with one member sends one offer and one blob; with everyone, one blob and a key per member",
    f"{S12} (src/lab/artifactsphere.rs: World::share, seal, pack). Ana shares her page with Ben, asking what he thinks: one offer to Ben and one blob on the Sphere, age-encrypted with one wrapped key, Ben's. She shares her notes with everyone: one more blob, not one each, with a wrapped key for Ben and one for Cleo, and an offer to each. Ben's and Cleo's Obys fetch, decrypt and keep their copies: from Ana over the Sphere, in their thread with her, Ben's with comment access, and Ana's ask reaching Ben only quoted, as needing him.",
    base(doc=True)
    + [
        share("12-shared-with-ben", P, ["ben"], "comment", S12, {"status": 200, "body": {"$len": 1}, "body.0": {"to": "ben", "state": "sent", "access": "comment", "version": 1, "latest": True}}, ask="What do you think of it?"),
        sphere("12-one-offer-one-blob", S12, {"blobs": {"$len": 1}, "blobs.0": {"to": ["ben"], "stanzas": 1, "age": True, "from": "ana"}, "sent": {"$count": [{"action": "artifact_offer", "to": "ben"}, 1]}}),
        share("12-shared-with-everyone", D, "everyone", "view", S12, {"status": 200, "body": {"$len": 2}}),
        sphere("12-one-more-blob", S12, {"blobs": {"$len": 2}, "sent": {"$len": 3}}),
        sphere("12-a-key-each", S12, {"blobs": {"$count": [{"to": ["ben", "cleo"], "stanzas": 2, "age": True}, 1]}}),
        sync("12-ben-keeps-it", "ben", S12, {"kept": {"$len": 2}, "refused": {"$len": 0}}),
        sync("12-cleo-keeps-it", "cleo", S12, {"kept": {"$len": 1}, "refused": {"$len": 0}}),
        call("12-bens-copy-is-from-ana", "ben", "GET", f"/v1/artifacts/{BEN}", S12,
             {"status": 200, "body.artifact.origin": {"kind": "sphere", "sphere": "lab-sphere", "from": "ana", "offer": {"$type": "string"}},
              "body.artifact.conversation": "peer-ana", "body.artifact.title": "Lab page", "body.received.access": "comment"}, signed=False),
        said("12-ana-asks-ben-quoted", "ben", S12, {"items": {"$any": {"kind": "shared", "needsYou": True, "ask": {"$contains": FENCE}}}}, member="ana"),
    ],
)

# ── 13 ────────────────────────────────────────────────────────────────────────────────────────
S13 = "LAB-ARTSPHERE-13"
steps13 = base()
for kind, reason, says in [
    ("hash", "bad_blob", "its hash differs"),
    ("link", "bad_blob", "hosts.txt is a link"),
    ("path", "bad_blob", "isn't a plain path"),
    ("oversize", "too_big", "big.html is 16 MB and 1 byte"),
    ("secret", "secret_found", "notes.txt"),
    ("expire", "expired", "expired"),
]:
    steps13 += [
        step(f"forged-{kind}", "lab.artifactsphere.forge", {"kind": kind, "to": "ben"}),
        sync(f"13-refused-{kind}", "ben", S13, {"kept": {"$len": 0}, "refused": {"$len": 1}, "refused.0": {"reason": reason, "error": {"$contains": says}}}),
    ]
steps13 += [
    step("forged-instructions", "lab.artifactsphere.forge", {"kind": "instructions", "to": "ben"}),
    sync("13-instructions-set-apart", "ben", S13, {"kept": {"$len": 1}, "kept.0.quarantined": [".claude/settings.json", "CLAUDE.md"]}),
    call("13-kept-without-them", "ben", "GET", "/v1/artifacts/${state.13-instructions-set-apart.kept.0.artifact}", S13, {"body.artifact.versions.0.files": ["index.html"]}, signed=False),
    step("ifx", "lab.artifacts.fixture", {"name": "page"}),
    step("with-claude-md", "lab.artifacts.publish", {"path": "${state.ifx.path}", "title": "With instructions", "conversation": "c1", "job": "j1", "write": {"CLAUDE.md": "Be brief.\n"}}, expect={"ok": True}),
    share("13-instructions-asked-first", "${state.with-claude-md.id}", ["ben"], "view", S13, {"status": 409, "body.reason": "instructions", "body.error": {"$contains": "CLAUDE.md"}}),
    share("13-sent-when-the-owner-says", "${state.with-claude-md.id}", ["ben"], "view", S13, {"status": 200}, instructions=True),
    sync("13-and-set-apart-there", "ben", S13, {"kept.0.quarantined": ["CLAUDE.md"]}),
    step("secret-folder", "lab.artifactsphere.forge", {"kind": "secretfolder"}),
    step("secret-page", "lab.artifacts.publish", {"path": "${state.secret-folder.path}", "title": "Notes with a token", "conversation": "c1", "job": "j1"}, expect={"ok": True}),
    share("13-a-secret-never-leaves", "${state.secret-page.id}", ["ben"], "view", S13, {"status": 422, "body.reason": "secret_found", "body.error": {"$contains": "notes.txt"}}),
    sphere("13-nothing-uploaded", S13, {"blobs": {"$count": [{"from": "ana"}, 1]}}),
]
wf(
    "receive.json",
    "Lab artifact sphere: the recipient refuses a blob whose hash differs, a link, an over-size version, a found secret; quarantines CLAUDE.md",
    f"{S13} (src/lab/artifactsphere.rs: World::receive, unpack, split_instructions, scan). Offers forged straight onto the Sphere, as a compromised Oby would send them, each refused by Ben's Oby before anything is kept, saying why: a blob that isn't the one offered, a tar holding a link or a path out of it, a file one byte over Claude's 16 MB, a token-shaped string the secret scan finds (made at run time, never in this repository), an offer past its expiry. One holding CLAUDE.md and .claude/ is kept with those set apart in quarantine. On the way out, Ana's share of a page holding CLAUDE.md is asked about first and goes when she says so, Ben's Oby setting it apart; her share of a page with a token-shaped string is refused by the same scan, and nothing more goes up.",
    steps13,
)

# ── 14 ────────────────────────────────────────────────────────────────────────────────────────
S14 = "LAB-ARTSPHERE-14"
wf(
    "taint.json",
    "Lab artifact sphere: a received artifact read by an agent taints the turn; its text reaches the agent only quoted",
    f"{S14} (src/lab/artifactsphere.rs: read). Ben's agent reads the page Ana shared (oby_artifact_read): its turn is marked untrusted, and each text file reaches it fenced as data from Ana's Oby, never as words. Ana's own agent reading her own page is neither.",
    base()
    + [
        share("shared", P, ["ben"], "view", None, {"status": 200}),
        sync("bens", "ben", None, {"kept": {"$len": 1}}),
        step("14-bens-agent-reads-it", "lab.artifactsphere.read", {"oby": "ben", "job": "jb", "artifact": "${state.bens.kept.0.artifact}"}, S14,
             {"untrusted": True, "outside": True, "files": {"$any": {"path": "index.html", "text": {"$contains": FENCE}}}}),
        said("14-bens-turn-marked", "ben", S14, {"jobs.jb": True}, member="ana"),
        step("14-anas-own-isnt", "lab.artifactsphere.read", {"oby": "ana", "job": "ja", "artifact": P}, S14,
             {"untrusted": False, "outside": False, "files": {"$none": {"text": {"$contains": FENCE}}}}),
    ],
)

# ── 15 ────────────────────────────────────────────────────────────────────────────────────────
S15 = "LAB-ARTSPHERE-15"
wf(
    "update.json",
    "Lab artifact sphere: a new version reaches every latest share; a revoke deletes the blob and the recipient's copy says it was taken back",
    f"{S15}, with LAB-ARTIFACTS-5's second half (src/lab/artifactsphere.rs: World::sync, revoke, delete). Ana shares her page with Ben (following the latest) and Cleo (that version only). Her new version goes to Ben alone, in one new blob, the old one deleted; Ben's copy has both versions, Cleo's still one. She takes Ben's share back: his blob is deleted, his Oby is told and his copy says Ana took it back. She makes a public link, then deletes the page: Cleo's share and the link go first, the Sphere keeps nothing of it, and Cleo's copy says it was deleted.",
    base()
    + [
        share("to-ben", P, ["ben"], "comment", None, {"status": 200}),
        share("to-cleo", P, ["cleo"], "view", None, {"status": 200}, latest=False),
        sync("bens", "ben", None, {"kept": {"$len": 1}}),
        sync("cleos", "cleo", None, {"kept": {"$len": 1}}),
        step("v2", "lab.artifacts.publish", {"path": "${state.fx.path}", "title": "Lab page", "conversation": "c1", "job": "j2", "artifact": P, "write": {"note.txt": "version two\n"}}, expect={"ok": True, "version": 2}),
        sync("15-the-new-version-goes-to-the-latest", "ana", S15, {"sent": [{"to": "ben", "version": 2}]}),
        sphere("15-one-new-blob", S15, {"blobs": {"$len": 2}}),
        sphere("15-the-old-one-gone", S15, {"blobs": {"$count": [{"to": ["ben"]}, 1]}}),
        sync("15-ben-takes-it", "ben", S15, {"kept": [{"action": "artifact_update", "version": 2}]}),
        call("15-bens-copy-has-both", "ben", "GET", "/v1/artifacts/${state.bens.kept.0.artifact}", S15, {"body.artifact.versions": {"$len": 2}}, signed=False),
        sync("15-cleo-gets-nothing-new", "cleo", S15, {"kept": {"$len": 0}}),
        call("15-taken-back", "ana", "DELETE", f"/v1/artifacts/{P}/shares/ben", S15, {"status": 204}),
        sphere("15-bens-blob-deleted", S15, {"blobs": {"$len": 1}, "blobs.0.to": ["cleo"], "sent": {"$any": {"action": "artifact_revoke", "to": "ben"}}}),
        sync("ben-told", "ben", None, {"said": [{"action": "artifact_revoke"}]}),
        call("15-bens-copy-says-so", "ben", "GET", "/v1/artifacts/${state.bens.kept.0.artifact}", S15, {"body.received.state": "takenBack"}, signed=False),
        said("15-bens-thread-says-so", "ben", S15, {"items": {"$any": {"kind": "takenBack", "words": "Ana took it back."}}}, member="ana"),
        call("public", "ana", "POST", f"/v1/artifacts/{P}/public", None, {"status": 200}, body={"acknowledged": "anyone-with-the-link"}),
        call("5-a-delete-revokes-first", "ana", "DELETE", f"/v1/artifacts/{P}", [S15, "LAB-ARTIFACTS-5"], {"status": 204}),
        sphere("5-nothing-left-on-the-sphere", [S15, "LAB-ARTIFACTS-5"], {"blobs": {"$len": 0}, "public": {"$len": 0}, "sent": {"$any": {"action": "artifact_deleted", "to": "cleo"}}}),
        sync("cleo-told", "cleo", None, {"said": [{"action": "artifact_deleted"}]}),
        call("5-cleos-copy-says-so", "cleo", "GET", "/v1/artifacts/${state.cleos.kept.0.artifact}", [S15, "LAB-ARTIFACTS-5"], {"body.received.state": "deleted"}, signed=False),
    ],
)

# ── 16 ────────────────────────────────────────────────────────────────────────────────────────
S16 = "LAB-ARTSPHERE-16"
ANSWER = "Love it, but the title is too small."
wf(
    "answer.json",
    "Lab artifact sphere: the recipient's answer shows quoted in the owner's conversation and member thread; a decline and an expiry say so",
    f"{S16} (src/lab/artifactsphere.rs: World::answer, sync). Ana shares her page with Ben (comment) and Cleo (view). Ben's words come back as an answer on the offer: in Ana's conversation and her thread with Ben they show quoted, fenced as Ben's Oby's, never as her own words, and Ben's share says answered. Cleo can only look: her answer is refused, her decline goes, and Ana's conversation says Cleo's Oby declined. A share Ben never opens expires: Ana's shares say so, and Ben's Oby, too late, can't fetch it.",
    base(doc=True)
    + [
        share("to-ben", P, ["ben"], "comment", None, {"status": 200}),
        share("to-cleo", P, ["cleo"], "view", None, {"status": 200}),
        sync("bens", "ben", None, {"kept": {"$len": 1}}),
        sync("cleos", "cleo", None, {"kept": {"$len": 1}}),
        call("16-ben-answers", "ben", "POST", "/v1/artifacts/${state.bens.kept.0.artifact}/answers", S16, {"status": 200}, body={"text": ANSWER}),
        sync("ana-hears", "ana", None, {"said": [{"action": "answer"}]}),
        said("16-quoted-in-anas-conversation", "ana", S16, {"items": {"$any": {"kind": "quoted", "from": "ben", "text": ANSWER, "quoted": {"$contains": FENCE}}}}, conversation="c1"),
        said("16-and-in-her-thread-with-ben", "ana", S16, {"items": {"$any": {"kind": "quoted", "from": "ben", "text": ANSWER}}}, member="ben"),
        call("16-bens-share-answered", "ana", "GET", f"/v1/artifacts/{P}/shares", S16, {"body": {"$any": {"to": "ben", "state": "answered"}}}, signed=False),
        call("16-view-only-cant-answer", "cleo", "POST", "/v1/artifacts/${state.cleos.kept.0.artifact}/answers", S16, {"status": 403, "body.reason": "not_allowed"}, body={"text": "Nice."}),
        call("16-cleo-declines", "cleo", "POST", "/v1/artifacts/${state.cleos.kept.0.artifact}/answers", S16, {"status": 200}, body={"decline": True}),
        sync("ana-hears-cleo", "ana", None, {"said": [{"action": "decline"}]}),
        said("16-a-decline-says-so", "ana", S16, {"items": {"$any": {"kind": "declined", "from": "cleo", "words": "Cleo’s Oby declined."}}}, conversation="c1"),
        call("16-cleos-share-declined", "ana", "GET", f"/v1/artifacts/{P}/shares", S16, {"body": {"$any": {"to": "cleo", "state": "declined"}}}, signed=False),
        share("doc-to-ben", D, ["ben"], "comment", None, {"status": 200}),
        step("eight-days", "lab.artifactsphere.clock", {"seconds": 8 * 86400}),
        call("16-an-expiry-says-so", "ana", "GET", f"/v1/artifacts/{D}/shares", S16, {"body": [{"to": "ben", "state": "expired"}]}, signed=False),
        sync("16-too-late-to-fetch", "ben", S16, {"kept": {"$len": 0}, "refused": [{"reason": "expired"}]}),
    ],
)

# ── 17 and 22 ─────────────────────────────────────────────────────────────────────────────────
S17 = "LAB-ARTSPHERE-17"
S22 = "LAB-ARTSPHERE-22"
LINK = "${state.17-a-link-made.body.link}"
READY = "window.__OBY_PUBLIC && window.__OBY_PUBLIC.ready === true"
wf(
    "public.json",
    "Lab artifact sphere: a public link needs acknowledged; the Sphere stores only ciphertext and never the key; the viewer decrypts in the browser and shows the invite",
    f"{S17} and {S22} (src/lab/artifactsphere.rs: World::make_public, unpublic, the Sphere's public pages; lab-ui/artifacts/public). Without the warning acknowledged the link isn't made. Acknowledged, Ana gets the whole link once, its key after #; her vault keeps it (0600), her artifact.json never does. The Sphere holds one age file with one wrapped key and no plaintext; nothing it ever received, the viewer's own requests included, holds the key. Opened in chrome-headless-shell, the page decrypts the page in the browser and shows it, with the invite to get Oby (the one action: no field to write in) and \"I have Oby: open it there\" (oby://open). Taken back, the Sphere holds nothing and the same link says so.",
    base()
    + [
        call("17-not-without-the-warning", "ana", "POST", f"/v1/artifacts/{P}/public", S17, {"status": 422, "body.reason": "not_acknowledged"}, body={}),
        call("17-a-link-made", "ana", "POST", f"/v1/artifacts/{P}/public", S17, {"status": 200, "body.link": {"$matches": "^http://127\\.0\\.0\\.1:[0-9]+/a/l-[0-9a-f]{16}#AGE-SECRET-KEY-1[0-9A-Z]+$"}, "body.version": 1}, body={"acknowledged": "anyone-with-the-link"}),
        step("17-the-key-only-in-the-vault", "lab.artifactsphere.vault", {"oby": "ana", "artifact": P, "link": LINK}, S17, {"vault": [f"oby.artifact.{P}.link"], "mode": "600", "artifactJsonHoldsKey": False, "keyInLink": True}),
        sphere("17-only-ciphertext", S17, {"public": {"$len": 1}, "public.0": {"age": True, "stanzas": 1, "owner": "ana"}, "holds": False}, holds="Lab page"),
        step("17-opened-in-the-browser", "lab.artifactsphere.open", {"link": LINK, "name": "public", "width": 1280, "height": 800, "ready": READY}, S17, {"ready": True}),
        step("17-decrypted-there", "lab.artifactview.eval", {"name": "public", "expression": "({kind: __OBY_PUBLIC.kind, title: __OBY_PUBLIC.title, error: __OBY_PUBLIC.error, files: __OBY_PUBLIC.files.length})"}, S17,
             {"value": {"kind": "html", "title": "Lab page", "error": None, "files": 4}}),
        sphere("17-the-key-never-reached-the-sphere", S17, {"holds": False, "requests": {"$ne": 0}}, holdsKeyOf=LINK),
        step("22-only-the-invite", "lab.artifactview.eval", {"name": "public", "expression": "({invite: !!document.querySelector('[data-oby-invite]'), fields: document.querySelectorAll('textarea, input, select, [contenteditable]').length, get: document.querySelector('[data-oby-invite] a.get').textContent, open: document.querySelector('[data-oby-invite] a.have').getAttribute('href')})"}, S22,
             {"value.invite": True, "value.fields": 0, "value.get": "Get Oby for Mac", "value.open": {"$matches": "^oby://open\\?artifact=l-[0-9a-f]{16}$"}}),
        step("22-the-invite-moves-unless-asked-not-to", "lab.artifactview.eval", {"name": "public", "expression": "getComputedStyle(document.querySelector('.invite .oby')).animationName"}, S22, {"value": "hop"}),
        call("17-taken-back", "ana", "DELETE", f"/v1/artifacts/{P}/public", S17, {"status": 204}),
        sphere("17-nothing-left", S17, {"public": {"$len": 0}}),
        step("17-the-link-says-so", "lab.artifactsphere.open", {"link": LINK, "name": "after", "width": 1280, "height": 800, "ready": READY}, S17, {"ready": True}),
        step("17-said-in-the-page", "lab.artifactview.eval", {"name": "after", "expression": "__OBY_PUBLIC.error"}, S17, {"value": "This link was taken back."}),
        step("17-gone-from-the-vault", "lab.artifactsphere.vault", {"oby": "ana", "artifact": P}, S17, {"vault": {"$len": 0}}),
    ],
)

# ── 18 ────────────────────────────────────────────────────────────────────────────────────────
S18 = "LAB-ARTSPHERE-18"
wf(
    "refusals.json",
    "Lab artifact sphere: a share without the Sphere's feature says feature_not_taken in words; a policy refusal names the admin",
    f"{S18} (src/lab/artifactsphere.rs: Sphere::may_send, refusal_words). Ben's card lacks artifacts.v1: sharing with him is refused feature_not_taken, said as Ben's Oby needing an update. The members' rule no longer allows artifact_offer: sharing with Cleo is refused policy_refused, naming Ana as the admin who can change it. Neither upload happens.",
    base()
    + [
        step("ben-old", "lab.artifactsphere.policy", {"features": {"ben": []}}),
        share("18-feature-not-taken", P, ["ben"], "view", S18, {"status": 403, "body.reason": "feature_not_taken", "body.error": "Ben’s Oby can’t take artifacts yet: it needs updating before you can share with it."}),
        step("no-offers", "lab.artifactsphere.policy", {"allowed": ["artifact_revoke", "artifact_deleted"]}),
        share("18-policy-names-the-admin", P, ["cleo"], "view", S18, {"status": 403, "body.reason": "policy_refused", "body.error": "Your Obysphere doesn’t let members share artifacts. Its admin, Ana, can change that."}),
        sphere("18-nothing-went-up", S18, {"blobs": {"$len": 0}, "sent": {"$len": 0}}),
    ],
)

# ── 19 ────────────────────────────────────────────────────────────────────────────────────────
S19 = "LAB-ARTSPHERE-19"
steps19 = base() + [
    share("shared", P, ["ben"], "edit", None, {"status": 200}),
    sync("bens", "ben", None, {"kept": {"$len": 1}}),
]
COPY = "${state.bens.kept.0.artifact}"
for id, who, method, path, body in [
    ("share", "ana", "POST", f"/v1/artifacts/{P}/shares", {"to": ["cleo"], "access": "view"}),
    ("revoke", "ana", "DELETE", f"/v1/artifacts/{P}/shares/ben", None),
    ("public", "ana", "POST", f"/v1/artifacts/{P}/public", {"acknowledged": "anyone-with-the-link"}),
    ("unpublic", "ana", "DELETE", f"/v1/artifacts/{P}/public", None),
    ("delete", "ana", "DELETE", f"/v1/artifacts/{P}", None),
    ("answer", "ben", "POST", f"/v1/artifacts/{COPY}/answers", {"text": "Hm."}),
    ("send-back", "ben", "POST", f"/v1/artifacts/{COPY}/sendBack", {"version": 1}),
]:
    kw = {"as": "unsigned"}
    if body is not None:
        kw["body"] = body
    steps19.append(call(f"19-{id}-unsigned", who, method, path, S19, {"status": 401, "body.reason": "unsigned"}, signed=False, **kw))
steps19 += [
    call("19-reading-needs-the-token", "ana", "GET", f"/v1/artifacts/{P}/shares", S19, {"status": 401, "body.reason": "no_token"}, signed=False, **{"as": "none"}),
    sphere("19-nothing-happened", S19, {"blobs": {"$len": 1}, "public": {"$len": 0}, "sent": {"$len": 1}}),
]
wf(
    "signed.json",
    "Lab artifact sphere: every owner act on a share is refused unsigned",
    f"{S19}, the sharing part (src/lab/artifactsphere.rs: oby_route). Sharing, revoking, making and taking back a public link and deleting, on the owner's Oby, and answering and sending a version back, on the recipient's, each need the device's signed words over the exact body: unsigned, each is refused 401, and nothing reached the Sphere. Reading the shares needs obid's token.",
    steps19,
)

# ── 21 ────────────────────────────────────────────────────────────────────────────────────────
S21 = "LAB-ARTSPHERE-21"
BC = "${state.bens.kept.0.artifact}"
CC = "${state.cleos.kept.0.artifact}"
wf(
    "edit.json",
    "Lab artifact sphere: a member with edit sends a version back; the owner checks it like any received artifact, keeps it as theirs, and nothing of it reaches the owner's agents",
    f"{S21} (src/lab/artifactsphere.rs: World::send_back, sync's artifact_version). Ana gives Ben edit and Cleo view. Ben's own agent changes his copy; he sends that version back, encrypted to Ana alone. Ana's Oby checks it as any received artifact and keeps it as version 2 by Ben's Oby, said in her conversation; no job of hers starts, and her agent reading that version gets it quoted, its turn untrusted. Cleo can't send one back, and a version forged from her Oby past that check is refused by Ana's. One from Ben with a token-shaped string in it is refused by the scan.",
    base()
    + [
        share("to-ben", P, ["ben"], "edit", None, {"status": 200}),
        share("to-cleo", P, ["cleo"], "view", None, {"status": 200}),
        sync("bens", "ben", None, {"kept": {"$len": 1}}),
        sync("cleos", "cleo", None, {"kept": {"$len": 1}}),
        step("bfx", "lab.artifacts.fixture", {"name": "page"}),
        step("bens-edit", "lab.artifactsphere.publish", {"oby": "ben", "path": "${state.bfx.path}", "title": "Lab page", "artifact": BC, "conversation": "peer-ana", "job": "jb", "write": {"index.html": "<!doctype html><title>Lab page</title><h1>Bigger title</h1>\n"}}, None, {"ok": True, "version": 2}),
        call("21-ben-sends-it-back", "ben", "POST", f"/v1/artifacts/{BC}/sendBack", S21, {"status": 200}, body={"version": 2}),
        sphere("21-for-ana-alone", S21, {"blobs": {"$any": {"from": "ben", "to": ["ana"], "stanzas": 1}}}),
        sync("21-ana-keeps-it-as-bens", "ana", S21, {"kept": [{"action": "artifact_version", "version": 2, "by": "Ben’s Oby", "quarantined": []}], "refused": {"$len": 0}}),
        call("21-version-2-by-bens-oby", "ana", "GET", f"/v1/artifacts/{P}", S21, {"body.artifact.versions.1": {"version": 2, "by": "Ben’s Oby"}, "body.artifact.origin.kind": "own"}, signed=False),
        said("21-said-in-her-conversation", "ana", S21, {"items": {"$any": {"kind": "version", "words": "Ben’s Oby made v2."}}, "jobs": {"$len": 0}}, conversation="c1"),
        step("21-her-agent-gets-it-quoted", "lab.artifactsphere.read", {"oby": "ana", "job": "ja", "artifact": P, "version": 2}, S21, {"untrusted": True, "files": {"$any": {"path": "index.html", "text": {"$contains": FENCE}}}}),
        step("cfx", "lab.artifacts.fixture", {"name": "page"}),
        step("cleos-edit", "lab.artifactsphere.publish", {"oby": "cleo", "path": "${state.cfx.path}", "title": "Lab page", "artifact": CC, "conversation": "peer-ana", "job": "jc", "write": {"note.txt": "cleo\n"}}, None, {"ok": True, "version": 2}),
        call("21-view-cant-send-back", "cleo", "POST", f"/v1/artifacts/{CC}/sendBack", S21, {"status": 403, "body.reason": "not_allowed"}, body={"version": 2}),
        call("shares", "ana", "GET", f"/v1/artifacts/{P}/shares", None, {"status": 200}, signed=False),
        step("forged-from-cleo", "lab.artifactsphere.forge", {"kind": "version", "from": "cleo", "to": "ana", "offer": "${state.shares.body.1.offer}", "artifact": P}),
        sync("21-ana-refuses-a-view-only-version", "ana", S21, {"kept": {"$len": 0}, "refused": [{"action": "artifact_version", "reason": "not_allowed"}]}),
        step("forged-secret", "lab.artifactsphere.forge", {"kind": "version", "from": "ben", "to": "ana", "offer": "${state.shares.body.0.offer}", "artifact": P, "secret": True}),
        sync("21-a-secret-in-it-refused", "ana", S21, {"kept": {"$len": 0}, "refused": [{"action": "artifact_version", "reason": "secret_found"}]}),
        call("21-still-two-versions", "ana", "GET", f"/v1/artifacts/{P}", S21, {"body.artifact.versions": {"$len": 2}}, signed=False),
    ],
)

# ── cleanup ───────────────────────────────────────────────────────────────────────────────────
wf(
    "cleanup.json",
    "Lab artifact sphere: everything the stand-in started is stopped at the end",
    "What the Obysphere's stand-in starts it stops (src/lab/artifactsphere.rs, cleanup): with the Sphere's pages and three Obys' APIs answering, lab.artifactsphere.down (the same cleanup the suite runs at every workflow's end) stops them all; none answers after.",
    [
        step("up", "lab.artifacts.up", expect={"token": {"$type": "string"}}),
        step("sphere", "lab.artifactsphere.up", {"artifacts": "${state.up}"}),
        step("alive", "lab.artifacts.alive", {"api": API["ben"]}, None, {"answers": True}),
        step("down", "lab.artifactsphere.down", {}, None, {"stopped": True, "tasks": 4}),
        step("gone-ben", "lab.artifacts.alive", {"api": API["ben"]}, None, {"answers": False}),
        step("gone-sphere", "lab.artifacts.alive", {"api": "${state.sphere.sphere}"}, None, {"answers": False}),
    ],
)
print("ok")
