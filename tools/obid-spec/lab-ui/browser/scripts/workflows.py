# Writes tools/obid-spec/workflows/lab/ui/*.json: one workflow per design board (LAB-UI-n), the five
# live ones on lane A's API (LAB-UI-LIVE-n), the cleanup's proof and board 15's real Stop. Edit here,
# then: python3 tools/obid-spec/lab-ui/browser/scripts/workflows.py
import json, os, sys
OUT = sys.argv[1] if len(sys.argv) > 1 else os.path.join(os.path.dirname(os.path.abspath(__file__)), '../../../workflows/lab/ui')
def step(id, node, inputs, proves=None, expected_red=None):
    s = {"id": id, "use": f"runtime.rust:lab.ui.{node}", "inputs": inputs}
    ui = {}
    if proves: ui["proves"] = proves if isinstance(proves, list) else [proves]
    if expected_red: ui["expectedRed"] = expected_red
    if ui: s["ui"] = ui
    return s
S = "${state.screen}"
def css(c, **k): return {"css": c, **k}
def role(r, n, **k): return {"role": r, **({"name": n} if n is not None else {}), **k}
def read(id, target, paths, proves=None, within=None, styles=None, **k):
    i = {"screen": S, "target": target, "expect": {"paths": paths}}
    if within: i["within"] = within
    if styles: i["styles"] = styles
    i.update(k)
    return step(id, "read", i, proves)
def words(id, target, text, proves=None):
    return read(id, target, {"count": 1, "text": text}, proves)
def has(id, target, n=1, proves=None, within=None):
    return read(id, target, {"count": n}, proves, within)
def gone(id, target, proves=None):
    return read(id, target, {"count": 0}, proves, within=5)
def size(id, target, box, proves=None):
    return step(id, "measure", {"screen": S, "target": target, "expect": {"paths": {f"box.{k}": v for k, v in box.items()}}}, proves)
def colour(id, target, styles, proves=None):
    return read(id, target, {f"styles.{p}.matches": True for p in styles}, proves, styles=styles)
def click(id, target, hover=False):
    i = {"screen": S, "target": target}
    if hover: i["hover"] = True
    return step(id, "click", i)
def compare(n, board, state=None, proves=True):
    i = {"screen": S, "board": board, "expect": {"paths": {"within": True, "sameWords": True}}}
    if state: i["state"] = state
    return step(f"matches-board" + (f"-{state}" if state else ""), "compare", i, f"LAB-UI-{n}" if proves else None)
def pictures(n, board, state=None, label=None):
    name = f"lab-ui-{n:02d}" + (f"-{state}" if state else "")
    i = {"screen": S, "board": board, "name": f"{name}-beside-board", "label": label or f"board {n}"}
    if state: i["state"] = state
    return [step("shot" + (f"-{state}" if state else ""), "screenshot", {"screen": S, "name": name}),
            step("beside-board" + (f"-{state}" if state else ""), "sidebyside", i)]
def head(n, live=False, api=None):
    serve = {} if not live else {"api": "${state.api.url}"}
    st = []
    if live:
        st.append({"id": "api", "use": "runtime.rust:lab.browserapi.serve", "inputs": {}})
    st.append(step("serve", "serve", serve))
    o = {"server": "${state.serve}", "board": n}
    if live: o["live"] = True
    st.append(step("screen", "open", o))
    return st
def wf(n, slug, name, desc, steps, tags=("lab",)):
    w = {"schemaVersion": "2", "name": f"Lab browser screens {n}: {name}", "version": "1.0.0", "description": desc,
         "trigger": {"manual": {}}, "tags": list(tags), "steps": steps + [step("close", "close", {"screen": S})]}
    with open(os.path.join(OUT, f"{n:02d}-{slug}.json"), "w") as f:
        json.dump(w, f, indent=2, ensure_ascii=False); f.write("\n")

# ------------------------------------------------------------------------------------------
TAKE = role("button", "Take control")
wf(2, "open", "open beside the conversation, the agent drives and you watch",
   "Board 2 (Main.dc.html): the browser runs the window's full height beside the conversation (640px of a 1280×800 window), its tabs in the title bar's row, the address, the strip saying Codex is using the tab and what it's doing, its pointer over the page. Every browser piece within 4px of the board and the same words; colours from the app's tokens; Full width and back.",
   head(2) + [
    size("panel-640", css("aside.br"), {"width": 640, "height": 800, "x": 640, "y": 0}, "LAB-UI-2"),
    size("conversation-640", css(".home"), {"width": 640}),
    size("tabs-in-title-row", css(".br-tabs"), {"height": 48, "y": 0}),
    size("address-row", css(".br-nav"), {"height": 44}),
    size("strip", css(".br-control"), {"height": 40}),
    words("strip-words", css(".br-control-text"), "Codex is using this tab · Clicking Sign in", "LAB-UI-2"),
    words("pointer-says", css(".agent-label"), "Clicking Sign in"),
    has("tabs-with-logos", css(".br-tab .logo[aria-label='Codex’s']"), 2),
    colour("on-tab-ink", css(".br-tab.is-on"), {"color": "--ink"}),
    colour("globe-lit", role("button", "Hide the browser"), {"color": "--text"}),
    compare(2, "Main"),
    *pictures(2, "Main"),
    click("full-width", role("button", "Full width")),
    size("full-width-panel", css("aside.br.is-full"), {"width": 1280, "x": 0}),
    gone("conversation-folds", css(".home")),
    words("peek", css(".peek-text"), "Sign-up flow works end to end · Signed up and in; now inviting a teammate"),
    click("back-beside", role("button", "Back beside the conversation")),
    size("back-640", css("aside.br"), {"width": 640}),
   ])

GLOBE = role("button", "Show the browser, 2 tabs, Codex is using one")
wf(1, "closed", "closed, the globe lights while an agent uses a tab, Watch opens it",
   "Board 1 (Closed.dc.html): the browser hidden, the conversation alone; the globe at the title bar's end carries a blue dot (--unread) while Codex uses a tab, its tooltip says Show the browser ⇧⌘B, and the log's running line has Watch. Watch opens the browser beside the conversation (board 2); the globe hides it again and shows it.",
   head(1) + [
    gone("no-browser", css("aside.br")),
    colour("globe-dot-unread", css(".bar-end .badge"), {"background-color": "--unread"}, "LAB-UI-1"),
    words("watch-on-running-line", css(".ss-tool-row", hasText="Clicking Sign in"), "Clicking Sign in Watch"),
    click("hover-globe", GLOBE, hover=True),
    words("tooltip", role("tooltip", None), "Show the browser⇧⌘B"),
    compare(1, "Closed"),
    *pictures(1, "Closed"),
    click("watch", role("button", "Watch")),
    size("opens-beside", css("aside.br"), {"width": 640, "x": 640}, "LAB-UI-1"),
    gone("watch-goes", role("button", "Watch")),
    compare(2, "Main", proves=False) | {"id": "now-board-2"},
    click("hide", role("button", "Hide the browser")),
    gone("hidden", css("aside.br")),
    click("globe-shows", GLOBE),
    has("shown-again", css("aside.br")),
   ])

wf(3, "together", "the side panel and the browser together, several agents' tabs",
   "Board 3 (Together.dc.html): the side panel (362px, as dragged) docked beside the conversation and the browser (600px) in a 1440×900 window; four tabs, each with its agent's logo and working dot (Claude Code, Codex, Cursor) or your mark. A click on another tab makes it the one shown.",
   head(3) + [
    size("side-panel", css(".sidebar-slot"), {"width": 362, "x": 0}),
    size("browser-600", css("aside.br"), {"width": 600, "x": 840}, "LAB-UI-3"),
    has("claude-tab", css(".br-tab .logo[aria-label='Claude Code’s']")),
    has("codex-tab", css(".br-tab .logo[aria-label='Codex’s']")),
    has("cursor-tab", css(".br-tab .logo[aria-label='Cursor’s']")),
    has("your-tab", css(".br-tab .you-mark[aria-label='Yours']")),
    has("working-dots", css(".br-tab .status-dot.is-working"), 3, "LAB-UI-3"),
    words("kid-in-browser", css(".convo-kid", hasText="Pricing page"), "Pricing page matches the plans · in the browser"),
    compare(3, "Together"),
    *pictures(3, "Together"),
    click("codex-tab-click", role("tab", "Codex’s Sign in · Acme", exact=False)),
    read("codex-tab-on", role("tab", "Codex’s Sign in · Acme", exact=False), {"attributes.aria-selected": "true"}, attributes=["aria-selected"]),
    words("codex-strip", css(".br-control-text"), "Codex is using this tab · Clicking Sign in"),
   ])

wf(4, "narrow", "a narrow window, the conversation keeps 420px",
   "Board 4 (Narrow.dc.html): the app's own 994×670 window with the browser open: the conversation keeps --view-min (420px) and the browser takes the rest (574px); the title fades rather than pushing the bar.",
   head(4) + [
    size("window", css("[data-lab-window]"), {"width": 994, "height": 670}),
    size("conversation-420", css(".home"), {"width": 420, "x": 0}, "LAB-UI-4"),
    size("browser-rest", css("aside.br"), {"width": 574, "x": 420}, "LAB-UI-4"),
    size("tabs-48", css(".br-tabs"), {"height": 48}),
    words("strip-words", css(".br-control-text"), "Codex is using this tab · Typing in Email"),
    compare(4, "Narrow"),
    *pictures(4, "Narrow"),
   ])

wf(5, "full-width", "full width, the conversation folds to one line and comes back",
   "Board 5 (Full.dc.html): the page across the whole 1280px window, the conversation folded to one line at its foot (Show the conversation); Back beside the conversation, or Show the conversation, puts the conversation back beside it at 640px.",
   head(5) + [
    size("full", css("aside.br.is-full"), {"width": 1280, "x": 0}, "LAB-UI-5"),
    gone("no-conversation", css(".home")),
    words("one-line", css(".peek"), "Sign-up flow works end to end · Signed up and in; now inviting a teammate Show the conversation", "LAB-UI-5"),
    read("back-pressed", role("button", "Back beside the conversation"), {"attributes.aria-pressed": "true"}, attributes=["aria-pressed"]),
    compare(5, "Full"),
    *pictures(5, "Full"),
    click("show-conversation", role("button", "Show the conversation")),
    size("beside-again", css("aside.br"), {"width": 640, "x": 640}, "LAB-UI-5"),
    size("conversation-back", css(".home"), {"width": 640}),
    click("full-again", role("button", "Full width")),
    size("full-again-1280", css("aside.br.is-full"), {"width": 1280}),
    click("back-beside", role("button", "Back beside the conversation")),
    size("beside-once-more", css("aside.br"), {"width": 640}),
   ])

HELD = "You have this tab. Codex waits until you give it back."
wf(6, "take-control", "take control: you hold the tab, the agent waits, its click refused, give back",
   "Board 6 (Hold.dc.html), both its states: held, the strip turns blue (--held) with your mark and Give back, the page gets a 2px ring, the tab a blue dot, the agent's next click shows Refused in its log and its card reads Waiting for you; Give back returns the tab (the log says so) and Codex goes on; Take control holds it again.",
   head(6) + [
    words("held-strip", css(".br-control-text > span:last-child"), HELD, "LAB-UI-6"),
    colour("held-blue", css(".br-control.is-held"), {"background-color": "--held"}, "LAB-UI-6"),
    has("ringed-page", css(".br-view.is-held")),
    colour("tab-dot-blue", css(".br-tab .status-dot.is-held"), {"background-color": "--unread"}),
    words("refused", css(".ss-tool.is-refused .ss-tool-row"), "Click Sign in · you have the tab Refused", "LAB-UI-6"),
    words("card-waits", css(".msg-task-meta"), "Waiting for you · Codex · acme-web"),
    words("agent-waits", css(".chat-live"), "Codex waits for the tab"),
    compare(6, "Hold", "held"),
    *pictures(6, "Hold", "held"),
    click("give-back", role("button", "Give back")),
    words("agent-again", css(".br-control-text"), "Codex is using this tab · Clicking Sign in", "LAB-UI-6"),
    words("gave-back-line", css(".lt-answered"), "You gave the tab back · 2:17 PM"),
    gone("ring-gone", css(".br-view.is-held")),
    compare(6, "Hold", "gaveBack"),
    *pictures(6, "Hold", "gaveBack"),
    click("take-again", TAKE),
    words("held-again", css(".br-control-text > span:last-child"), HELD),
   ])

SECRET = "lab-secret-value-9271"
wf(7, "asks-sign-in", "the agent asks you to sign in, and the password card",
   "Board 7 (AsksYou.dc.html): Codex asks over the message box (the decision card: Sign in, Take control, Type the password, Skip), the strip turns orange and says so, the tab's dot is orange (--needs). Type the password turns the card into board 8's password card; what is typed goes into the page's Password field on Fill in, and appears nowhere else on the screen.",
   head(7) + [
    words("ask-head", css(".tc-ask-line"), "Sign in Sign in to the staging admin · Codex 1 of 1", "LAB-UI-7"),
    words("orange-strip", css(".br-control-text"), "Codex asks you to sign in here"),
    colour("tab-dot-orange", css(".br-tab .status-dot.is-needs"), {"background-color": "--needs"}, "LAB-UI-7"),
    colour("kind-orange", css(".tc-ask-kind"), {"color": "--needs"}),
    compare(7, "AsksYou"),
    *pictures(7, "AsksYou"),
    click("type-the-password", role("button", "Type the password")),
    words("filling", css(".br-control-text"), "You’re filling the password. Codex waits.", "LAB-UI-7"),
    step("type", "type", {"screen": S, "target": css("[data-lab=secret-input]"), "text": "0123456789"}),
    compare(8, "Secret", proves=False) | {"id": "now-board-8"},
    click("fill-in", role("button", "Fill in")),
    words("page-field", css("[data-lab=page-password]"), "•••••••••• From Oby", "LAB-UI-7"),
    gone("card-gone", css(".decision-card")),
   ])

wf(8, "password", "a password goes straight into the field, never to the agent",
   "Board 8 (Secret.dc.html): the decision card becomes the field (blue Password, For staging.acme.dev · asked by Codex, Fill in, Cancel, and the line that Codex never sees it and Oby doesn't keep it), the strip says you're filling it, the page's Password field is ringed and badged From Oby. A typed value goes into the page's field on Fill in and is nowhere in the screen's own words, attributes or storage afterwards; Cancel goes back to the ask.",
   head(8) + [
    words("card-line", css(".tc-ask-line"), "Password For staging.acme.dev · asked by Codex", "LAB-UI-8"),
    words("never-sees", css(".secret-note"), "It goes straight into the page’s Password field. Codex never sees it, and Oby doesn’t keep it.", "LAB-UI-8"),
    colour("kind-blue", css(".tc-ask-kind.is-blue"), {"color": "--unread"}),
    words("page-ringed", css("[data-lab=page-password]"), "Password From Oby"),
    compare(8, "Secret"),
    *pictures(8, "Secret"),
    click("cancel", role("button", "Cancel")),
    words("back-to-ask", css(".tc-ask-line"), "Sign in Sign in to the staging admin · Codex 1 of 1"),
    click("type-the-password", role("button", "Type the password")),
    step("type-secret", "type", {"screen": S, "target": css("[data-lab=secret-input]"), "text": SECRET}),
    click("fill-in", role("button", "Fill in")),
    words("field-filled", css("[data-lab=page-password]"), "•" * len(SECRET) + " From Oby", "LAB-UI-8"),
    read("not-in-the-screen", css("html"), {"count": 1, "text": {"$not": {"$contains": SECRET}}}, "LAB-UI-8"),
    read("not-in-an-input", css("input"), {"count": 0}),
   ])

wf(9, "every-ask", "every ask: CAPTCHA, a choice, a code, a passkey; who has the tab",
   "Board 9 (Asks.dc.html): the decision cards for a CAPTCHA, a choice, a one-time code and a passkey (orange kinds the app's, Code in blue as yours to type), and the strip's five ways a tab can be held, plus the side panel's rows.",
   head(9) + [
    has("four-cards", css(".decision-card"), 4, "LAB-UI-9"),
    words("captcha", css(".decision-card", nth=0), "Check The page wants to know you’re a person · Codex Codex can’t answer this. Take control and solve it; Codex goes on when you give the tab back. Take control Skip this page"),
    has("code-boxes", css(".code-boxes > span"), 6),
    colour("code-blue", css(".tc-ask-kind.is-blue"), {"color": "--unread"}),
    has("five-strips", css(".strip-box .br-control"), 5, "LAB-UI-9"),
    colour("held-strip", css(".strip-box .br-control.is-held"), {"background-color": "--held"}),
    compare(9, "Asks"),
    *pictures(9, "Asks"),
   ])

wf(10, "page-states", "page states: loading, failed, timed out, a dialog, certificate, crashed",
   "Board 10 (PageStates.dc.html): each state in its small browser, each with its own way out (Try again, Take control to answer, Go back or Go on anyway, Reload) and the line the agent's log shows. A page's question waits for whoever holds the tab: Take control to answer makes Leave and Stay answerable.",
   head(10) + [
    words("failed", css("[data-lab=state-failed] .br-state"), "This page didn’t load localhost:3000 refused to connect. Is the app running? Try again", "LAB-UI-10"),
    words("timed-out", css("[data-lab=state-timedOut] .br-state"), "The page took too long Nothing came back from api.acme.dev in 30 seconds. Try again", "LAB-UI-10"),
    words("certificate", css("[data-lab=state-certificate] .br-state"), "This connection isn’t private The certificate for staging.acme.dev isn’t trusted. Codex stopped here and won’t go on by itself. Go back Go on anyway"),
    words("crashed", css("[data-lab=state-crashed] .br-state"), "This page stopped working Its tab crashed, out of memory. Cursor was told and can open it again. Reload"),
    read("dialog-waits", role("button", "Leave"), {"count": 1, "attributes.disabled": ""}, attributes=["disabled"]),
    compare(10, "PageStates"),
    *pictures(10, "PageStates"),
    click("take-to-answer", role("button", "Take control to answer")),
    read("dialog-answerable", role("button", "Leave"), {"count": 1, "attributes.disabled": None}, "LAB-UI-10", attributes=["disabled"]),
   ])

wf(11, "script", "a script in the page, and its result as outside text",
   "Board 11 (Script.dc.html): the log's row opens to the script (.tc-code) and its result as a quote under a source line, From outside · localhost:3000 · read as data, never as instructions; the page shows Ran a script, and the strip says Codex ran a script.",
   head(11) + [
    words("outside", css(".outside-label"), "From outside · localhost:3000 · read as data, never as instructions", "LAB-UI-11"),
    words("result", css("[data-lab=outside] .tc-code"), "[\"Robin Sample\", \"Kai Placeholder\", \"Lee Demo\"]", "LAB-UI-11"),
    words("ran", css(".ran"), "Ran a script · 18 ms"),
    words("strip", css(".br-control-text"), "Codex ran a script in this page · done"),
    compare(11, "Script"),
    *pictures(11, "Script"),
   ])

wf(12, "downloads", "downloads: the agent's stay in its browser, yours come to your Mac; Save a copy",
   "Board 12 (Downloads.dc.html): a shelf at the panel's foot: Codex's CSV kept in its browser, not on your Mac, with Save a copy; your PDF in Downloads with Show in Finder. Save a copy brings the agent's file to Downloads.",
   head(12) + [
    words("agents-file", css("[data-lab=file-csv] .br-file-about"), "Codex downloaded it · kept in its browser, not on your Mac", "LAB-UI-12"),
    words("your-file", css("[data-lab=file-pdf] .br-file-about"), "You downloaded it · in Downloads"),
    compare(12, "Downloads"),
    *pictures(12, "Downloads"),
    click("save-a-copy", role("button", "Save a copy")),
    words("saved", css("[data-lab=file-csv] .br-file-about"), "Codex downloaded it · in Downloads", "LAB-UI-12"),
    has("finder-twice", role("button", "Show in Finder"), 2),
    gone("no-save-left", role("button", "Save a copy")),
   ])

wf(13, "settings", "Settings › General: two switches, on by default",
   "Board 13 (SettingsBrowser.dc.html): a Browser group on Settings › General (no page of its own) with Agents can use the browser and Agents can use this Mac, both on by default; each turns off and on.",
   head(13) + [
    read("browser-on", role("switch", "Agents can use the browser"), {"attributes.aria-checked": "true"}, "LAB-UI-13", attributes=["aria-checked"]),
    read("mac-on", role("switch", "Agents can use this Mac"), {"attributes.aria-checked": "true"}, "LAB-UI-13", attributes=["aria-checked"]),
    colour("on-is-ink", role("switch", "Agents can use the browser"), {"background-color": "--ink"}),
    words("foot", css(".set-foot", hasText="password"), "A password you type in Oby goes straight to the page. No agent sees it."),
    compare(13, "SettingsBrowser"),
    *pictures(13, "SettingsBrowser"),
    click("browser-off", role("switch", "Agents can use the browser")),
    read("browser-is-off", role("switch", "Agents can use the browser"), {"attributes.aria-checked": "false"}, "LAB-UI-13", attributes=["aria-checked"]),
    click("browser-on-again", role("switch", "Agents can use the browser")),
    read("browser-on-again-reads", role("switch", "Agents can use the browser"), {"attributes.aria-checked": "true"}, attributes=["aria-checked"]),
   ])

wf(14, "another-computer", "another computer: the browser shows that computer's localhost",
   "Board 14 (OtherComputer.dc.html): a computer chip beside the project chip (devbox · Linux), a line under the tabs saying the browser runs on devbox and its localhost is devbox's, and an on devbox badge in the address.",
   head(14) + [
    words("chip", css(".project-chip.is-machine"), "devbox · Linux", "LAB-UI-14"),
    words("where", css("[data-lab=where]"), "This browser runs on devbox. Its localhost is devbox’s, not this Mac’s.", "LAB-UI-14"),
    words("badge", css(".br-where"), "on devbox"),
    has("new-tab-there", role("button", "New tab on devbox")),
    compare(14, "OtherComputer"),
    *pictures(14, "OtherComputer"),
   ])

wf(15, "the-mac", "an agent has the Mac: the pill, the frame, Stop and Esc twice",
   "Board 15 (Desktop.dc.html), drawn as its own page: Oby's pill over the screen (Claude Code is using your Mac · Typing in TextEdit, esc esc, Stop), an orange frame, and the card in Oby. Stop ends it (pill and frame go); so does Esc pressed twice. The real Stop, on agent-desktop, is 15-the-mac-stop.json.",
   head(15) + [
    words("pill", css("[data-lab=pill]"), "Claude Code is using your Mac · Typing in TextEdit esc esc Stop", "LAB-UI-15"),
    has("orange-frame", css("[data-lab=glow]")),
    words("card", css(".decision-card"), "Claude Code has your Mac. Press Esc twice to stop it. Stop"),
    compare(15, "Desktop"),
    *pictures(15, "Desktop"),
    click("stop", role("button", "Stop", within=css("[data-lab=pill]"))),
    gone("pill-gone", css("[data-lab=pill]"), "LAB-UI-15"),
    gone("frame-gone", css("[data-lab=glow]")),
    step("reload", "reload", {"screen": S}),
    has("pill-back", css("[data-lab=pill]")),
    step("esc-twice", "press", {"screen": S, "keys": "Escape", "times": 2}),
    gone("esc-stops", css("[data-lab=pill]"), "LAB-UI-15"),
   ])

wf(16, "first-time-mac", "the first time: why macOS will ask, once",
   "Board 16 (DesktopPermission.dc.html), drawn as its own page: Oby's modal explains (it sees the screen, clicks and types, you stop it any time) and that macOS asks next for agent-desktop; Continue and Not now each close it.",
   head(16) + [
    words("title", css(".modal-title"), "Let agents use your Mac?", "LAB-UI-16"),
    has("three-points", css(".modal-list li"), 3),
    words("macos-next", css(".steps"), "Next, macOS asks you, once. Allow agent-desktop in System Settings › Privacy & Security, under Accessibility and then Screen Recording. Oby never asks macOS for these itself.", "LAB-UI-16"),
    compare(16, "DesktopPermission"),
    *pictures(16, "DesktopPermission"),
    click("continue", role("button", "Continue")),
    gone("closed", css("[data-lab=scrim]"), "LAB-UI-16"),
    read("answered", css("[data-lab=answer]"), {"count": 1, "text": "continue"}),
    step("reload", "reload", {"screen": S}),
    click("not-now", role("button", "Not now")),
    gone("closed-again", css("[data-lab=scrim]")),
   ])

wf(17, "first-use", "first use: no pages yet, type an address",
   "Board 17 (Empty.dc.html): no tabs yet, the address says Type an address, the empty words in the panel's middle, and the tip (as Home's Settings tip) with its ×, which closes it and leaves the empty words.",
   head(17) + [
    has("no-tabs", css(".br-tab"), 0),
    words("address", css(".br-address"), "Type an address", "LAB-UI-17"),
    words("empty", css(".empty-words"), "No pages yet. When an agent opens one, you see it here, live.", "LAB-UI-17"),
    compare(17, "Empty"),
    *pictures(17, "Empty"),
    click("close-tip", role("button", "Close", within=css(".br-tip"))),
    gone("tip-gone", css(".br-tip")),
    has("empty-stays", css(".empty-words")),
   ])

# ---------------------------------------------------------------- live, against lane A's API
def api(id, node, inputs):
    return {"id": id, "use": f"runtime.rust:lab.browserapi.{node}", "inputs": inputs}
UP = "${state.up}"
def live_head(n, job="j1", agent="codex", delay=None):
    j = {"api": UP, "job": job, "agent": agent, "expect": {"paths": {"token": {"$type": "string"}}}}
    if delay: j["commitDelayMs"] = delay
    return [
        api("up", "up", {"expect": {"paths": {"url": {"$contains": "http://127.0.0.1:"}}}}),
        api("device", "device", {"expect": {"paths": {"id": {"$contains": "device-"}}}}),
        api(job, "job", j),
        api("open-site", "mcp", {"api": UP, "token": f"${{state.{job}.token}}", "tool": "oby_browser_open", "args": {"url": "${state.up.site.url}/"}, "expect": {"paths": {"isError": False}}}),
        step("serve", "serve", {"api": UP, "device": "${state.device}", "expect": {"paths": {"live": True, "signs": True}}}),
        step("screen", "open", {"server": "${state.serve}", "board": n, "live": True}),
        read("live-frame", css("[data-lab=frame]"), {"count": 1}, within=20),
    ]
TAB = "${state.open-site.structured.result.tab}"
def lwf(k, slug, name, desc, steps):
    wf(100 + k, slug, name, desc, steps + [step("shot", "screenshot", {"screen": S, "name": f"lab-ui-live-{k}"})])

def snap(id, job="j1"):
    return api(id, "mcp", {"api": UP, "token": f"${{state.{job}.token}}", "tool": "oby_browser_snapshot", "args": {}, "expect": {"paths": {"isError": False}}})

lwf(1, "live-open-and-drive", "live: the agent's page, its tab and what it's doing, on the stream",
    "Live (board 2's screen) on lane A's browser API: the agent's tab comes from GET /v1/browser, the page is agent-browser's own stream, and while the agent types into the page the strip says Typing into Name; once it's done the tab is idle.",
    live_head(2, delay=2500) + [
     words("tab-title", css(".br-tab.is-on .br-tab-name"), "Sign in to the lab", "LAB-UI-LIVE-1"),
     read("address", css(".br-address"), {"count": 1, "text": {"$contains": "127.0.0.1"}}),
     snap("snap"),
     step("meanwhile-type", "meanwhile", {"node": "lab.browserapi.mcp", "inputs": {"api": UP, "token": "${state.j1.token}", "tool": "oby_browser_act", "args": {"action": "type", "text": "a slow agent typing its words into the page, one key at a time, so the screen can watch it"}, "refFor": {"role": "textbox", "name": "Name"}, "snapshot": "${state.snap}"}}),
     read("strip-while-typing", css(".br-control-text"), {"count": 1, "text": "Codex is using this tab · Typing into Name"}, "LAB-UI-LIVE-1", within=10),
     step("typed", "wait", {"screen": S, "meanwhile": "${state.meanwhile-type}", "expect": {"paths": {"isError": False}}}),
     read("idle-after", css(".br-control-text"), {"count": 1, "text": "Codex opened this tab · idle"}, within=10),
     step("reload", "reload", {"screen": S}),
     read("same-tab-after-reload", css(".br-tab.is-on .br-tab-name"), {"count": 1, "text": "Sign in to the lab"}, within=10),
    ])

def get(id, paths, within=None, proves=None):
    i = {"api": UP, "method": "GET", "path": "/v1/browser", "expect": {"paths": paths}}
    if within: i["within"] = within
    s = api(id, "call", i)
    if proves: s["ui"] = {"proves": [proves]}
    return s
def agent_types(id, snapshot, error=None, job="j1"):
    exp = {"isError": True, "structured.error.code": error} if error else {"isError": False}
    return api(id, "mcp", {"api": UP, "token": f"${{state.{job}.token}}", "tool": "oby_browser_act", "args": {"action": "type", "text": "agent"}, "refFor": {"role": "textbox", "name": "Name"}, "snapshot": f"${{state.{snapshot}}}", "expect": {"paths": exp}})

lwf(2, "live-take-control", "live: Take control and Give back, the agent refused while you hold the tab",
    "Live (board 2's screen) on lane A's API: Take control in the strip is the owner's signed take (the screens' server signs it, as the app's main will), so the API holds the tab for this device and refuses the agent's act with human_control while the screen shows the blue strip and the ring; Give back releases it and the agent acts again.",
    live_head(2) + [
     snap("snap"),
     click("take-control", TAKE),
     read("held-strip", css(".br-control-text > span:last-child"), {"count": 1, "text": "You have this tab. Codex waits until you give it back."}, "LAB-UI-LIVE-2", within=10),
     get("api-holds", {"status": 200, "body.tabs.0.heldBy": "${state.device.id}"}, proves="LAB-UI-LIVE-2"),
     has("ring", css(".br-view.is-held")),
     agent_types("agent-refused", "snap", "human_control"),
     click("give-back", role("button", "Give back")),
     get("api-free", {"status": 200, "body.tabs.0.heldBy": None}, within=5, proves="LAB-UI-LIVE-2"),
     gone("ring-gone", css(".br-view.is-held")),
     snap("snap-again"),
     agent_types("agent-acts", "snap-again"),
    ])

LIVE_SECRET = "lab-ui-live-secret-5308"
lwf(3, "live-password", "live: the password card fills the page's field, and only the page's",
    "Live (board 7's screen) on lane A's API: the agent asks the person to sign in (oby_browser_ask_person, field #pw); the card shows; Type the password holds the tab (You're filling the password); the typed value goes through POST secret into the page's #pw field (Oby's own read of the page has it), the ask is answered and the tab given back, and the value is nowhere in the screen.",
    live_head(7) + [
     api("ask", "mcp", {"api": UP, "token": "${state.j1.token}", "tool": "oby_browser_ask_person", "args": {"kind": "signIn", "reason": "Sign in to the lab", "field": "#pw"}, "expect": {"paths": {"isError": False}}}),
     read("card", css(".tc-ask-line"), {"count": 1, "text": {"$contains": "Sign in"}}, within=10),
     read("orange-strip", css(".br-control-text"), {"count": 1, "text": "Codex asks you to sign in here"}, within=10),
     click("type-the-password", role("button", "Type the password")),
     read("filling", css(".br-control-text"), {"count": 1, "text": "You’re filling the password. Codex waits."}, "LAB-UI-LIVE-3"),
     get("held-while-filling", {"body.tabs.0.heldBy": "${state.device.id}"}, within=5),
     step("type-secret", "type", {"screen": S, "target": css("[data-lab=secret-input]"), "text": LIVE_SECRET}),
     click("fill-in", role("button", "Fill in")),
     gone("card-gone", css(".decision-card")),
     get("answered-and-free", {"body.tabs.0.ask": None, "body.tabs.0.heldBy": None}, within=10, proves="LAB-UI-LIVE-3"),
     {"id": "page-field-has-it", "use": "runtime.rust:lab.browser.read", "inputs": {"conversation": "j1", "selector": "#pw", "expect": {"paths": {"value": LIVE_SECRET}}}, "ui": {"proves": ["LAB-UI-LIVE-3"]}},
     read("not-in-the-screen", css("html"), {"count": 1, "text": {"$not": {"$contains": LIVE_SECRET}}}, "LAB-UI-LIVE-3"),
    ])

lwf(4, "live-page-states", "live: a page that didn't load, a page's question, a page that took too long",
    "Live (board 2's screen) on lane A's API, the states board 10 draws: the agent opens a port nothing listens on (127.0.0.1:2) (This page didn't load, Try again); back on a page, the page's own alert waits for whoever holds the tab (… asks, Take control to answer); a page that answers in 8 s given 2 s (The page took too long).",
    [{"id": "slow-site", "use": "runtime.rust:lab.browser.site", "inputs": {}}] + live_head(2) + [
     api("refused", "mcp", {"api": UP, "token": "${state.j1.token}", "tool": "oby_browser_navigate", "args": {"url": "http://127.0.0.1:2/", "tab": TAB}, "expect": {"paths": {"isError": True}}}),
     read("didnt-load", css("[data-lab=page-failed]"), {"count": 1, "text": {"$contains": "This page didn’t load"}}, "LAB-UI-LIVE-4", within=10),
     api("back", "mcp", {"api": UP, "token": "${state.j1.token}", "tool": "oby_browser_navigate", "args": {"url": "${state.slow-site.url}/", "tab": TAB}, "expect": {"paths": {"isError": False}}}),
     snap("snap-home"),
     api("alert", "mcp", {"api": UP, "token": "${state.j1.token}", "tool": "oby_browser_act", "args": {"action": "click"}, "refFor": {"role": "button", "name": "Alert me"}, "snapshot": "${state.snap-home}", "expect": {"paths": {"isError": False}}}),
     read("page-asks", css("[data-lab=page-dialog] .br-over-kind"), {"count": 1, "text": {"$contains": "asks"}}, "LAB-UI-LIVE-4", within=10),
     has("take-to-answer", role("button", "Take control to answer")),
     api("answer-dialog", "mcp", {"api": UP, "token": "${state.j1.token}", "tool": "oby_browser_dialog", "args": {"accept": True}, "expect": {"paths": {"isError": False}}}),
     api("slow", "mcp", {"api": UP, "token": "${state.j1.token}", "tool": "oby_browser_navigate", "args": {"url": "${state.slow-site.url}/slow?ms=8000", "deadlineMs": 2000, "tab": TAB}, "expect": {"paths": {"isError": True}}}),
     read("too-long", css("[data-lab=page-timedOut]"), {"count": 1, "text": {"$contains": "The page took too long"}}, "LAB-UI-LIVE-4", within=10),
    ])

lwf(5, "live-downloads", "live: the agent's download stays in its browser; Save a copy",
    "Live (board 12's screen) on lane A's API: the agent clicks the fixture's download link; the shelf shows its file kept in its browser, not on your Mac, with Save a copy; Save a copy is the owner's signed saveCopy, and the file is then in Downloads (the engine's delivered folder has it) while the agent's stays in its profile.",
    live_head(12) + [
     snap("snap"),
     api("agent-downloads", "mcp", {"api": UP, "token": "${state.j1.token}", "tool": "oby_browser_act", "args": {"action": "click"}, "refFor": {"role": "link", "name": "Download file"}, "snapshot": "${state.snap}", "expect": {"paths": {"isError": False}}}),
     read("kept-in-its-browser", css(".br-file-about"), {"count": 1, "text": "Codex downloaded it · kept in its browser, not on your Mac"}, "LAB-UI-LIVE-5", within=15),
     click("save-a-copy", role("button", "Save a copy")),
     read("in-downloads", css(".br-file-about"), {"count": 1, "text": "Codex downloaded it · in Downloads"}, "LAB-UI-LIVE-5", within=10),
     {"id": "delivered", "use": "runtime.rust:lab.browser.downloads", "inputs": {"conversation": "j1", "expect": {"paths": {"inProfile": {"$len": 1}, "delivered": {"$len": 1}}}}, "ui": {"proves": ["LAB-UI-LIVE-5"]}},
    ])

# ---------------------------------------------------------------- cleanup, and the Mac's real Stop
w = {"schemaVersion": "2", "name": "Lab browser screens: every process the screens started is gone at the cleanup", "version": "1.0.0",
     "description": "What lab.ui starts it stops (src/lab/ui.rs, cleanup): the screens' server, two screens' helpers and their Chrome for Testing (with its own helper processes) are alive while they work, found by PID and by the workflow's folder in their command lines, and none is left after lab.ui.down, the same code the suite's cleanup runs whatever a workflow ended in.",
     "trigger": {"manual": {}}, "tags": ["lab"], "steps": [
        step("serve", "serve", {}),
        step("screen", "open", {"server": "${state.serve}", "board": 2}),
        step("screen-2", "open", {"server": "${state.serve}", "board": 6, "name": "second"}),
        step("alive", "processes", {"expect": {"paths": {"servers": 1, "helpers": 2, "chromes": {"$ne": 0}, "any": True}}}),
        step("down", "down", {}),
        {**step("none-left", "processes", {"expect": {"paths": {"alive": 0, "any": False}}}), "ui": {"proves": ["LAB-UI-CLEANUP"]}},
     ]}
with open(os.path.join(OUT, "00-cleanup.json"), "w") as f:
    json.dump(w, f, indent=2, ensure_ascii=False); f.write("\n")

def desk(id, node, inputs, proves=None):
    s = {"id": id, "use": f"runtime.rust:lab.desktop.{node}", "inputs": inputs}
    if proves: s["ui"] = {"proves": [proves]}
    return s
SNAP = lambda id: desk(id, "tool", {"conversation": "c1", "tool": "oby_desktop_snapshot", "app": "${state.app}", "args": {}, "expect": {"paths": {"ok": True}}})
w = {"schemaVersion": "2", "name": "Lab browser screens 15: Stop takes the Mac back from the agent, on agent-desktop", "version": "1.0.0",
     "description": "Board 15's Stop, for real: a conversation drives the lab's own test app through agent-desktop (an invisible AppKit window, never activated); Stop is the person taking the desktop, after which the agent's click and keys are refused (human_control) while its reads go on; giving it back lets it act again. Skipped off macOS, without agent-desktop or without its Accessibility and Screen Recording permissions.",
     "trigger": {"manual": {}}, "tags": ["lab"], "steps": [
        desk("app", "app", {}),
        desk("grant", "grant", {"conversation": "c1", "app": "${state.app}"}),
        SNAP("snap"),
        desk("agent-acts", "tool", {"conversation": "c1", "tool": "oby_desktop_click", "app": "${state.app}", "args": {}, "refFor": {"role": "button", "name": "Count"}, "snapshot": "${state.snap}", "expect": {"paths": {"ok": True}}}),
        desk("stop", "take", {"person": "owner", "expect": {"paths": {"ok": True}}}, "LAB-UI-15"),
        SNAP("snap-stopped"),
        desk("click-refused", "tool", {"conversation": "c1", "tool": "oby_desktop_click", "app": "${state.app}", "args": {}, "refFor": {"role": "button", "name": "Count"}, "snapshot": "${state.snap-stopped}", "expect": {"paths": {"ok": False, "error.code": "human_control"}}}, "LAB-UI-15"),
        desk("keys-refused", "tool", {"conversation": "c1", "tool": "oby_desktop_press", "app": "${state.app}", "args": {"keys": "cmd+k"}, "expect": {"paths": {"ok": False, "error.code": "human_control"}}}),
        desk("give-back", "release", {"person": "owner", "expect": {"paths": {"ok": True}}}),
        SNAP("snap-again"),
        desk("acts-again", "tool", {"conversation": "c1", "tool": "oby_desktop_click", "app": "${state.app}", "args": {}, "refFor": {"role": "button", "name": "Count"}, "snapshot": "${state.snap-again}", "expect": {"paths": {"ok": True}}}),
     ]}
with open(os.path.join(OUT, "15-the-mac-stop.json"), "w") as f:
    json.dump(w, f, indent=2, ensure_ascii=False); f.write("\n")
