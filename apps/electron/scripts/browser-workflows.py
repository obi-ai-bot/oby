#!/usr/bin/env python3
"""Writes tools/obid-spec/workflows/app/browser-*.json: the browser's design boards, each driven in the
real app (docs/contracts/browser-app.md, BROWSERAPP-n), as the lab's lab.ui.* workflows drove its
screens (tools/obid-spec/lab-ui/browser/scripts/workflows.py). Edit this file, not the JSON:

    python3 apps/electron/scripts/browser-workflows.py

Each opens the app on its demo data at the board (?everything&chat&browser=<n>, app.launch `query`),
at the board's size; checks its words, sizes and colours against the app's tokens; drives what the
board shows; compares the window with the board (app.compare: every browser piece within 4px, the
same words); and saves a screenshot and the side-by-side PNG (OBID_SPEC_SHOTS keeps them).
"""
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
OUT = ROOT / "tools/obid-spec/workflows/app"
APP = "${state.app}"


def step(id, node, proves=None, **inputs):
    s = {"id": id, "use": f"runtime.rust:{node}", "inputs": inputs}
    if proves:
        s["ui"] = {"proves": proves if isinstance(proves, list) else [proves]}
    return s


def launch(board, width, height, sidebar=False):
    # the side panel kept open or closed as the board has it (Home.tsx, PANEL_KEPT)
    return step("app", "app.launch", mock=True, query=f"browser={board}", size={"width": width, "height": height},
                storage={"oby.chatPanelOpen": "1" if sidebar else "0"})


def read(id, target, proves=None, **expect):
    # words: the text with each run of white space one space (drive-app.mjs), as the lab reads its screens
    if isinstance(expect.get("text"), str):
        expect["words"] = " ".join(expect.pop("text").split())
    extra = {}
    if "styles" in expect:
        extra["styles"] = expect.pop("styles")
    within = expect.pop("within", None)
    s = step(id, "app.read", proves, app=APP, target=target, expect={"paths": expect}, **extra)
    if within:
        s["inputs"]["within"] = within
    return s


def measure(id, target, proves=None, **expect):
    return step(id, "app.measure", proves, app=APP, target=target, expect={"paths": expect})


def click(id, target, hover=False):
    s = step(id, "app.click", app=APP, target=target)
    if hover:
        s["inputs"]["hover"] = True
    return s


def hide_sidebar():
    # the demo opens with the side panel docked where it fits; the board has none
    return click("hide-sidebar", {"role": "button", "name": "Hide sidebar"})


def compare(board_file, proves, skip=None, state=None):
    inputs = dict(app=APP, board=board_file, expect={"paths": {"within": True, "sameWords": True}}, within=10)
    if skip:
        inputs["skip"] = skip
    if state:
        inputs["state"] = state
    return step("matches-board", "app.compare", proves, **inputs)


def pictures(n, board_file, label):
    return [
        step("shot", "app.screenshot", app=APP, name=f"browser-{n:02d}"),
        step("beside-board", "app.sidebyside", app=APP, board=board_file, name=f"browser-{n:02d}-beside-board", label=label),
    ]


def close():
    return step("close", "app.close", app=APP)


def workflow(file, name, description, steps):
    w = {
        "schemaVersion": "2",
        "name": name,
        "version": "1.0.0",
        "description": description,
        "trigger": {"manual": {}},
        "tags": ["browser"],
        "steps": steps,
    }
    (OUT / file).write_text(json.dumps(w, indent=2, ensure_ascii=False) + "\n")


TEXT = {"color": "--text"}
INK = {"color": "--ink"}


def board_02():
    workflow(
        "browser-02-open.json",
        "App: the browser open beside the conversation, the agent drives, you watch (board 2)",
        "The real app on its demo data at the design's board 2 (BROWSERAPP-2): the panel is the window's last column, 640px "
        "beside the conversation; its tabs sit in the title bar's row; the strip says who has the tab; the agent's pointer says "
        "what it's doing; the globe is lit. Every browser piece sits within 4px of the board, with its words. Full width puts "
        "the page across the window with the conversation folded to one line, and back (BROWSERAPP-5).",
        [
            launch(2, 1280, 800),
            measure("panel-640", {"css": "aside.br"}, "BROWSERAPP-2", **{"box.width": 640, "box.height": 800, "box.x": 640, "box.y": 0}),
            measure("conversation-640", {"css": ".app > .home"}, None, **{"box.width": 640}),
            # the title bar's row: --bar, from where macOS put the traffic lights (52 on macOS 26; the board draws 48)
            measure("tabs-in-title-row", {"css": ".br-tabs"}, None, **{"box.height": {"$oneOf": [48, 52]}, "box.y": 0}),
            measure("address-row", {"css": ".br-nav"}, None, **{"box.height": 44}),
            measure("strip", {"css": ".br-control"}, None, **{"box.height": 40}),
            read("strip-words", {"css": ".br-control-text"}, "BROWSERAPP-2", count=1, text="Codex is using this tab · Clicking Sign in"),
            read("pointer-says", {"css": ".agent-label"}, None, count=1, text="Clicking Sign in"),
            read("tabs-with-logos", {"css": ".br-tab .logo[aria-label='Codex’s']"}, None, count=2),
            read("on-tab-ink", {"css": ".br-tab.is-on"}, None, styles=INK, **{"styles.color.matches": True}),
            read("globe-lit", {"role": "button", "name": "Hide the browser"}, "BROWSERAPP-1", styles=TEXT, **{"styles.color.matches": True}),
            compare("Main", "BROWSERAPP-2"),
            *pictures(2, "Main", "board 2"),
            click("full-width", {"role": "button", "name": "Full width"}),
            measure("full-width-panel", {"css": "aside.br.is-full"}, "BROWSERAPP-5", **{"box.width": 1280, "box.x": 0}),
            read("conversation-folds", {"css": ".app > .home"}, None, visible=False),
            read("peek", {"css": ".peek-text"}, "BROWSERAPP-5", count=1, text="Sign-up flow works end to end · Signed up and in; now inviting a teammate"),
            click("back-beside", {"role": "button", "name": "Back beside the conversation"}),
            measure("back-640", {"css": "aside.br"}, None, **{"box.width": 640}),
            close(),
        ],
    )


def board_01():
    globe = {"role": "button", "name": "Show the browser, 2 tabs, Codex is using one"}
    workflow(
        "browser-01-closed.json",
        "App: the browser closed, a globe in the title bar opens it (board 1)",
        "The real app on its demo data at the design's board 1 (BROWSERAPP-1): an agent works in the browser while it's "
        "closed; the globe at the title bar's end carries the blue dot and says so; pointed at, it says Show the browser and "
        "its keys; clicked, the panel opens beside the conversation as board 2 shows, and the lit globe hides it again.",
        [
            launch(1, 1280, 800),
            read("no-browser", {"css": "aside.br"}, None, count=0),
            read("globe-dot-unread", {"css": ".bar-end .badge"}, "BROWSERAPP-1", styles={"background-color": "--unread"}, **{"styles.background-color.matches": True}),
            click("hover-globe", globe, hover=True),
            read("tooltip", {"role": "tooltip"}, "BROWSERAPP-1", count=1, text="Show the browser⇧⌘B"),
            compare("Closed", "BROWSERAPP-1"),
            *pictures(1, "Closed", "board 1"),
            click("globe-opens", globe),
            measure("opens-beside", {"css": "aside.br"}, "BROWSERAPP-1", **{"box.width": 640, "box.x": 640}),
            step("now-board-2", "app.compare", None, app=APP, board="Main", expect={"paths": {"within": True, "sameWords": True}}, within=10),
            click("hide", {"role": "button", "name": "Hide the browser"}),
            read("hidden", {"css": "aside.br"}, None, count=0, within=5),
            click("globe-shows", globe),
            read("shown-again", {"css": "aside.br"}, None, count=1),
            close(),
        ],
    )


def board_03():
    workflow(
        "browser-03-together.json",
        "App: the side panel and the browser together, tabs from several agents (board 3)",
        "The real app on its demo data at the design's board 3 (BROWSERAPP-3, BROWSERAPP-4): at 1440px the side panel stays "
        "docked at 362px, the browser takes 600px and the conversation keeps the rest; the tabs carry each agent's logo, yours "
        "the Y, and a working dot while an agent uses one; a tab clicked is the one shown, its strip naming its agent.",
        [
            launch(3, 1440, 900, sidebar=True),
            measure("side-panel", {"css": ".sidebar-slot"}, None, **{"box.width": 362, "box.x": 0}),
            measure("browser-600", {"css": "aside.br"}, "BROWSERAPP-3", **{"box.width": 600, "box.x": 840}),
            measure("conversation-rest", {"css": ".app > .home"}, "BROWSERAPP-3", **{"box.width": 478, "box.x": 362}),
            read("claude-tab", {"css": ".br-tab .logo[aria-label='Claude Code’s']"}, "BROWSERAPP-4", count=1),
            read("codex-tab", {"css": ".br-tab .logo[aria-label='Codex’s']"}, None, count=1),
            read("cursor-tab", {"css": ".br-tab .logo[aria-label='Cursor’s']"}, None, count=1),
            read("your-tab", {"css": ".br-tab .you-mark[aria-label='Yours']"}, "BROWSERAPP-4", count=1),
            read("working-dots", {"css": ".br-tab .status-dot.is-working"}, "BROWSERAPP-4", count=3),
            compare("Together", "BROWSERAPP-4", skip=BOARD_SKIPS[3]),
            *pictures(3, "Together", "board 3"),
            click("codex-tab-click", {"role": "tab", "name": "Codex’s Sign in · Acme", "exact": False}),
            step("codex-tab-on", "app.read", None, app=APP, target={"role": "tab", "name": "Codex’s Sign in · Acme", "exact": False},
                 attributes=["aria-selected"], expect={"paths": {"attributes.aria-selected": "true"}}),
            read("codex-strip", {"css": ".br-control-text"}, "BROWSERAPP-4", count=1, text="Codex is using this tab · Clicking Sign in"),
            close(),
        ],
    )


def board_04():
    workflow(
        "browser-04-narrow.json",
        "App: a narrow window, the conversation keeps 420px (board 4)",
        "The real app on its demo data at the design's board 4 (BROWSERAPP-3): at 994px the conversation keeps 420px and the "
        "browser takes the rest, 574px; its strip still says what the agent is doing.",
        [
            launch(4, 994, 670),
            measure("conversation-420", {"css": ".app > .home"}, "BROWSERAPP-3", **{"box.width": 420, "box.x": 0}),
            measure("browser-rest", {"css": "aside.br"}, "BROWSERAPP-3", **{"box.width": 574, "box.x": 420}),
            read("strip-words", {"css": ".br-control-text"}, None, count=1, text="Codex is using this tab · Typing in Email"),
            compare("Narrow", "BROWSERAPP-3", skip=BOARD_SKIPS[4]),
            *pictures(4, "Narrow", "board 4"),
            close(),
        ],
    )


def board_05():
    workflow(
        "browser-05-full-width.json",
        "App: full width, the page across the window (board 5)",
        "The real app on its demo data at the design's board 5 (BROWSERAPP-5): the page across the window, the conversation "
        "folded to one line over it with Show the conversation; Back beside the conversation is pressed; either puts the "
        "panel back at 640px beside the conversation.",
        [
            launch(5, 1280, 800),
            measure("full", {"css": "aside.br.is-full"}, "BROWSERAPP-5", **{"box.width": 1280, "box.x": 0}),
            read("no-conversation", {"css": ".app > .home"}, None, visible=False),
            read("one-line", {"css": ".peek"}, "BROWSERAPP-5", count=1, text="Sign-up flow works end to end · Signed up and in; now inviting a teammate\nShow the conversation"),
            step("back-pressed", "app.read", None, app=APP, target={"role": "button", "name": "Back beside the conversation"},
                 attributes=["aria-pressed"], expect={"paths": {"attributes.aria-pressed": "true"}}),
            compare("Full", "BROWSERAPP-5", skip=BOARD_SKIPS[5]),
            *pictures(5, "Full", "board 5"),
            click("show-conversation", {"role": "button", "name": "Show the conversation"}),
            measure("beside-again", {"css": "aside.br"}, "BROWSERAPP-5", **{"box.width": 640, "box.x": 640}),
            measure("conversation-back", {"css": ".app > .home"}, None, **{"box.width": 640}),
            click("full-again", {"role": "button", "name": "Full width"}),
            measure("full-again-1280", {"css": "aside.br.is-full"}, None, **{"box.width": 1280}),
            click("back-beside", {"role": "button", "name": "Back beside the conversation"}),
            measure("beside-once-more", {"css": "aside.br"}, None, **{"box.width": 640}),
            close(),
        ],
    )


def board_17():
    workflow(
        "browser-17-first-use.json",
        "App: first use, no pages yet (board 17)",
        "The real app on its demo data at the design's board 17 (BROWSERAPP-6): the browser with no tabs says so, its address "
        "asks for one, and its first-time note says what the browser is for; closed, the note goes and the empty words stay.",
        [
            launch(17, 1280, 800),
            measure("panel-beside", {"css": "aside.br"}, "BROWSERAPP-6", **{"box.width": 600, "box.x": 680, "box.y": 0}),
            read("no-tabs", {"css": ".br-tab"}, None, count=0),
            read("address", {"css": ".br-address"}, "BROWSERAPP-6", count=1, text="Type an address"),
            read("empty", {"css": ".empty-words"}, "BROWSERAPP-6", count=1, text="No pages yet.\nWhen an agent opens one, you see it here, live."),
            compare("Empty", "BROWSERAPP-6"),
            *pictures(17, "Empty", "board 17"),
            click("close-tip", {"role": "button", "name": "Close", "within": {"css": ".br-tip"}}),
            read("tip-gone", {"css": ".br-tip"}, None, count=0, within=5),
            read("empty-stays", {"css": ".empty-words"}, None, count=1),
            close(),
        ],
    )


def board_06():
    held = "You have this tab. Codex waits until you give it back."
    workflow(
        "browser-06-hold.json",
        "App: Take control, you hold the tab and the agent waits; Give back (board 6)",
        "The real app on its demo data at the design's board 6 (BROWSERAPP-9): you hold the tab, the strip says so in the "
        "held blue (--held), the page is ringed and the tab's dot is blue (--unread); Give back returns it to the agent and "
        "Take control holds it again. Within 4px of the board in both its states. Signing these on obid: browserLive.test.ts.",
        [
            launch(6, 1280, 800),
            read("held-strip", {"css": ".br-control-text > span:last-child"}, "BROWSERAPP-9", count=1, text=held),
            read("held-blue", {"css": ".br-control.is-held"}, "BROWSERAPP-9", styles={"background-color": "--held"}, **{"styles.background-color.matches": True}),
            measure("ringed-page", {"css": ".br-view.is-held"}, "BROWSERAPP-9", **{"box.width": 639, "box.x": 641}),
            read("tab-dot-blue", {"css": ".br-tab .status-dot.is-held"}, None, styles={"background-color": "--unread"}, **{"styles.background-color.matches": True}),
            compare("Hold", "BROWSERAPP-9", state="held"),
            *pictures(6, "Hold", "board 6, held"),
            click("give-back", {"role": "button", "name": "Give back"}),
            read("agent-again", {"css": ".br-control-text"}, "BROWSERAPP-9", count=1, text="Codex is using this tab · Clicking Sign in"),
            read("ring-gone", {"css": ".br-view.is-held"}, None, count=0, within=5),
            step("matches-board-gave-back", "app.compare", None, app=APP, board="Hold", state="gaveBack", expect={"paths": {"within": True, "sameWords": True}}, within=10),
            step("beside-board-gave-back", "app.sidebyside", app=APP, board="Hold", state="gaveBack", name="browser-06-gave-back-beside-board", label="board 6, given back"),
            click("take-again", {"role": "button", "name": "Take control"}),
            read("held-again", {"css": ".br-control-text > span:last-child"}, None, count=1, text=held),
            close(),
        ],
    )


ASK_LINE = "Sign in Sign in to the staging admin · Codex 1 of 1"
CARD = {"css": "[data-browser=asks] .decision-card"}


def board_07():
    workflow(
        "browser-07-asks-sign-in.json",
        "App: the agent asks you to sign in; Type the password fills the page (board 7)",
        "The real app on its demo data at the design's board 7 (BROWSERAPP-11, BROWSERAPP-12): the agent's sign-in ask "
        "sits over the message box, its heading the agent's first sentence; the strip turns orange and the tab's dot "
        "orange (--needs). Type the password opens the password card (board 8); Fill in puts the dots in the page's "
        "field, marked From Oby, and the card goes.",
        [
            launch(7, 1280, 800),
            read("ask-head", {"css": "[data-browser=asks] .tc-ask-line"}, "BROWSERAPP-11", count=1, text=ASK_LINE),
            read("ask-words", {"css": "[data-browser=asks] .tc-ask-text"}, "BROWSERAPP-11", count=1,
                 text="The mail log is behind a sign-in. Take control and sign in yourself, or type the password here: it goes straight into the page, never to Codex."),
            read("orange-strip", {"css": ".br-control-text"}, None, count=1, text="Codex asks you to sign in here"),
            read("tab-dot-orange", {"css": ".br-tab .status-dot.is-needs"}, "BROWSERAPP-11", styles={"background-color": "--needs"}, **{"styles.background-color.matches": True}),
            read("kind-orange", {"css": "[data-browser=asks] .tc-ask-kind"}, None, styles={"color": "--needs"}, **{"styles.color.matches": True}),
            step("over-the-box", "app.measure", "BROWSERAPP-11", app=APP, target=CARD,
                 against={"box": {"css": ".chat-layer:not(.is-covered) .composer"}}, expect={"paths": {"box.overlap": False}}),
            compare("AsksYou", "BROWSERAPP-11", skip=BOARD_SKIPS[7]),
            *pictures(7, "AsksYou", "board 7"),
            click("type-the-password", {"role": "button", "name": "Type the password"}),
            read("filling", {"css": ".br-control-text"}, "BROWSERAPP-12", count=1, text="You’re filling the password. Codex waits."),
            step("type", "app.type", app=APP, target={"css": "[data-browser=secret-input]"}, text="0123456789"),
            step("now-board-8", "app.compare", None, app=APP, board="Secret", skip=BOARD_SKIPS[8], expect={"paths": {"within": True, "sameWords": True}}, within=10),
            click("fill-in", {"role": "button", "name": "Fill in"}),
            read("page-field", {"css": "[data-browser=page-password]"}, "BROWSERAPP-12", count=1, text="••••••••••\nFrom Oby"),
            read("card-gone", CARD, None, count=0, within=5),
            close(),
        ],
    )


def board_08():
    secret = "app-secret-value-9271"
    workflow(
        "browser-08-password.json",
        "App: a password goes straight into the page's field, never to the agent (board 8)",
        "The real app on its demo data at the design's board 8 (BROWSERAPP-12): the password card says the value goes into "
        "the page's Password field, that the agent never sees it and Oby doesn't keep it; Cancel goes back to the ask; typed "
        "and filled, the page's field gets the dots and the value is nowhere in the window, nor left in any box.",
        [
            launch(8, 1280, 800),
            read("card-line", {"css": "[data-browser=asks] .tc-ask-line"}, "BROWSERAPP-12", count=1, text="Password For staging.acme.dev · asked by Codex"),
            read("never-sees", {"css": ".secret-note"}, "BROWSERAPP-12", count=1, text="It goes straight into the page’s Password field. Codex never sees it, and Oby doesn’t keep it."),
            read("kind-blue", {"css": ".tc-ask-kind.is-blue"}, None, styles={"color": "--unread"}, **{"styles.color.matches": True}),
            read("page-ringed", {"css": "[data-browser=page-password]"}, None, count=1, text="Password\nFrom Oby"),
            step("over-the-box", "app.measure", "BROWSERAPP-12", app=APP, target=CARD,
                 against={"box": {"css": ".chat-layer:not(.is-covered) .composer"}}, expect={"paths": {"box.overlap": False}}),
            compare("Secret", "BROWSERAPP-12", skip=BOARD_SKIPS[8]),
            *pictures(8, "Secret", "board 8"),
            click("cancel", {"role": "button", "name": "Cancel"}),
            read("back-to-ask", {"css": "[data-browser=asks] .tc-ask-line"}, None, count=1, text=ASK_LINE),
            click("type-the-password", {"role": "button", "name": "Type the password"}),
            step("type-secret", "app.type", app=APP, target={"css": "[data-browser=secret-input]"}, text=secret),
            click("fill-in", {"role": "button", "name": "Fill in"}),
            read("field-filled", {"css": "[data-browser=page-password]"}, "BROWSERAPP-12", count=1, text="•" * len(secret) + "\nFrom Oby"),
            read("not-in-the-window", {"css": "html"}, "BROWSERAPP-12", count=1, text={"$not": {"$contains": secret}}),
            read("no-box-left", {"css": "[data-browser=secret-input]"}, None, count=0),
            close(),
        ],
    )


def board_09():
    nxt = {"role": "button", "name": "Next decision"}
    line = {"css": "[data-browser=asks] .tc-ask-line"}
    words = {"css": "[data-browser=asks] .tc-ask-text"}
    workflow(
        "browser-09-every-ask.json",
        "App: every ask over the message box, one at a time: a CAPTCHA, a choice, a code, a passkey (board 9)",
        "The real app on its demo data with the design's board 9 asks (BROWSERAPP-13), one at a time with 1 of 4 and the "
        "next arrow, each in the board's words; a choice's heading is the agent's first sentence and its choices its buttons; "
        "the code card has six boxes in blue. Board 9 is a sheet of the four cards side by side: here they're checked word "
        "by word and over the message box, not by the sheet's places.",
        [
            launch(9, 1280, 800),
            read("captcha", line, "BROWSERAPP-13", count=1, text="Check The page wants to know you’re a person · Codex 1 of 4"),
            read("captcha-words", words, None, count=1, text="Codex can’t answer this. Take control and solve it; Codex goes on when you give the tab back."),
            step("over-the-box", "app.measure", "BROWSERAPP-13", app=APP, target=CARD,
                 against={"box": {"css": ".chat-layer:not(.is-covered) .composer"}}, expect={"paths": {"box.overlap": False}}),
            *pictures(9, "Asks", "board 9 (a sheet): the CAPTCHA card"),
            click("to-choice", nxt),
            read("choice", line, "BROWSERAPP-13", count=1, text="Question Which plan should I pick at checkout? · Claude Code 2 of 4"),
            read("choice-words", words, None, count=1, text="The test card works with both. Business adds SSO, which the next step checks."),
            read("choices", {"css": "[data-browser=asks] .tc-options"}, "BROWSERAPP-13", count=1, text="Team · $12\nBusiness · $29\nI’ll choose in the page"),
            click("to-code", nxt),
            read("code", line, "BROWSERAPP-13", count=1, text="Code The code sent to your phone, for staging.acme.dev 3 of 4"),
            read("code-boxes", {"css": ".code-boxes > span"}, None, count=6),
            read("code-blue", {"css": ".tc-ask-kind.is-blue"}, None, styles={"color": "--unread"}, **{"styles.color.matches": True}),
            click("to-passkey", nxt),
            read("passkey", line, "BROWSERAPP-13", count=1, text="Passkey staging.acme.dev asks for your passkey · Codex 4 of 4"),
            read("passkey-words", words, None, count=1, text="Only you can do this. Take control; your Mac asks for Touch ID."),
            click("skip-passkey", {"role": "button", "name": "Skip"}),
            # the last one answered, the one before it shows: three left
            read("three-left", line, None, count=1, text="Code The code sent to your phone, for staging.acme.dev 3 of 3", within=5),
            close(),
        ],
    )


def board_10():
    tab = lambda i: {"css": ".br-tab", "nth": i}
    steps = [launch(10, 1280, 880)]
    steps += [
        read("loading-strip", {"css": ".br-control-text"}, "BROWSERAPP-14", count=1, text="Codex · Waiting for the page"),
        read("loading-stop", {"role": "button", "name": "Stop loading"}, None, count=1),
    ]
    for i, (state, words, proves) in enumerate([
        ("failed", "This page didn’t load localhost:3000 refused to connect. Is the app running? Try again", "BROWSERAPP-14"),
        ("timedOut", "The page took too long Nothing came back from api.acme.dev in 30 seconds. Try again", "BROWSERAPP-14"),
    ], start=1):
        steps += [click(f"tab-{state}", tab(i)), read(state, {"css": f"[data-browser=page-{state}]"}, proves, count=1, text=words)]
    steps += [
        click("tab-dialog", tab(3)),
        read("dialog", {"css": "[data-browser=page-dialog]"}, "BROWSERAPP-14", count=1,
             text="localhost:3000 asks Leave this page? Changes you made may not be saved. Leave Stay Take control to answer"),
        step("dialog-waits", "app.read", None, app=APP, target={"role": "button", "name": "Leave"}, attributes=["disabled"], expect={"paths": {"count": 1, "attributes.disabled": ""}}),
        step("dialog-over-the-page", "app.measure", "BROWSERAPP-14", app=APP, target={"css": "[data-browser=page-dialog]"},
             against={"view": {"css": ".br-view"}}, expect={"paths": {"view.overlap": True}}),
        click("take-to-answer", {"role": "button", "name": "Take control to answer"}),
        step("dialog-answerable", "app.read", "BROWSERAPP-14", app=APP, target={"role": "button", "name": "Leave"}, attributes=["disabled"], expect={"paths": {"count": 1, "attributes.disabled": None}}),
        click("tab-certificate", tab(4)),
        read("certificate", {"css": "[data-browser=page-certificate]"}, "BROWSERAPP-14", count=1,
             text="This connection isn’t private The certificate for staging.acme.dev isn’t trusted. Codex stopped here and won’t go on by itself. Go back Go on anyway"),
        read("certificate-address", {"css": ".br-address.is-warn"}, None, count=1),
        click("tab-crashed", tab(5)),
        read("crashed", {"css": "[data-browser=page-crashed]"}, "BROWSERAPP-14", count=1,
             text="This page stopped working Its tab crashed, out of memory. Cursor was told and can open it again. Reload"),
        *pictures(10, "PageStates", "board 10 (a sheet): the crashed page"),
        close(),
    ]
    workflow(
        "browser-10-page-states.json",
        "App: a page that doesn't simply load: loading, failed, timed out, a dialog, a certificate, crashed (board 10)",
        "The real app on its demo data with the design's board 10 tabs (BROWSERAPP-14), each state in the panel with the "
        "board's words and its way out: loading's strip and Stop; failed and timed out, Try again; the page's own dialog "
        "over the page, answerable only once you take control; an untrusted certificate, its address marked; a crashed "
        "tab, Reload. Board 10 is a sheet of six small browsers: here each is checked in the panel, word by word, not by "
        "the sheet's places. On obid the ways out wait for routes obid doesn't have yet (the contract's Open list).",
        steps,
    )


def board_11():
    workflow(
        "browser-11-script.json",
        "App: a script the agent ran in the page (board 11)",
        "The real app on its demo data at the design's board 11 (BROWSERAPP-15): the page carries the agent's mark, Ran a "
        "script and how long it took, and the strip says the agent ran a script in this page and is done. Its result as "
        "outside text is the agent's log, under its card in the conversation (TaskLog), not the panel's.",
        [
            launch(11, 1280, 800),
            read("strip", {"css": ".br-control-text"}, "BROWSERAPP-15", count=1, text="Codex ran a script in this page · done"),
            read("ran", {"css": ".ran"}, "BROWSERAPP-15", count=1, text="Ran a script · 18 ms"),
            step("mark-on-the-page", "app.measure", "BROWSERAPP-15", app=APP, target={"css": ".ran"}, against={"view": {"css": ".br-view"}}, expect={"paths": {"view.overlap": True}}),
            compare("Script", "BROWSERAPP-15", skip=BOARD_SKIPS[11]),
            *pictures(11, "Script", "board 11"),
            close(),
        ],
    )


def board_12():
    workflow(
        "browser-12-downloads.json",
        "App: downloads, the agent's stay in its browser, yours come to your Mac (board 12)",
        "The real app on its demo data at the design's board 12 (BROWSERAPP-16): the shelf at the panel's foot says whose "
        "each file is and where it is; Save a copy puts the agent's into your Downloads and its row says so, with Show in "
        "Finder.",
        [
            launch(12, 1280, 800),
            read("agents-file", {"css": "[data-browser=file-csv] .br-file-about"}, "BROWSERAPP-16", count=1, text="Codex downloaded it · kept in its browser, not on your Mac"),
            read("your-file", {"css": "[data-browser=file-pdf] .br-file-about"}, None, count=1, text="You downloaded it · in Downloads"),
            step("shelf-at-the-foot", "app.measure", "BROWSERAPP-16", app=APP, target={"css": ".br-shelf"}, against={"view": {"css": ".br-view"}}, expect={"paths": {"view.overlap": False, "view.gap": 0}}),
            compare("Downloads", "BROWSERAPP-16", skip=BOARD_SKIPS[12]),
            *pictures(12, "Downloads", "board 12"),
            click("save-a-copy", {"role": "button", "name": "Save a copy"}),
            read("saved", {"css": "[data-browser=file-csv] .br-file-about"}, "BROWSERAPP-16", count=1, text="Codex downloaded it · in Downloads"),
            read("finder-twice", {"role": "button", "name": "Show in Finder"}, None, count=2),
            read("no-save-left", {"role": "button", "name": "Save a copy"}, None, count=0, within=5),
            close(),
        ],
    )


def board_14():
    workflow(
        "browser-14-another-computer.json",
        "App: another computer, the browser shows that computer's localhost (board 14)",
        "The real app on its demo data at the design's board 14 (BROWSERAPP-17): under the tabs the panel says the browser "
        "runs on devbox and whose localhost it shows; the address says on devbox; New tab opens one there.",
        [
            launch(14, 1280, 800),
            read("where", {"css": "[data-browser=where]"}, "BROWSERAPP-17", count=1, text="This browser runs on devbox. Its localhost is devbox’s, not this Mac’s."),
            read("badge", {"css": ".br-where"}, "BROWSERAPP-17", count=1, text="on devbox"),
            read("new-tab-there", {"role": "button", "name": "New tab on devbox"}, None, count=1),
            step("where-under-the-tabs", "app.measure", "BROWSERAPP-17", app=APP, target={"css": "[data-browser=where]"}, against={"tabs": {"css": ".br-tabs"}}, expect={"paths": {"tabs.overlap": False, "tabs.gap": 0}}),
            compare("OtherComputer", "BROWSERAPP-17", skip=BOARD_SKIPS[14]),
            *pictures(14, "OtherComputer", "board 14"),
            close(),
        ],
    )


def board_13():
    browser = {"role": "switch", "name": "Agents can use the browser"}
    mac = {"role": "switch", "name": "Agents can use this Mac"}
    checked = lambda id, target, value, proves=None: step(id, "app.read", proves, app=APP, target=target, attributes=["aria-checked"], expect={"paths": {"attributes.aria-checked": value}})
    workflow(
        "browser-13-settings.json",
        "App: Settings › General, two switches for the browser, on by default (board 13)",
        "The real app on its demo data at the design's board 13 (BROWSERAPP-18): Settings › General has a Browser group, "
        "marked New, after Permissions on this Mac, with Agents can use the browser and Agents can use this Mac, both on, "
        "each with the board's words, and its footer; turned off, the browser's globe goes from the title bar; on again, "
        "it's back. The page is today's Settings, with more groups than the board draws: the group is checked by its words, "
        "switches and place after Permissions, not by the board's places.",
        [
            launch(13, 1200, 860, sidebar=True),
            # Settings by its own shortcut (⌘,: on the demo data it sits in the account menu)
            step("settings", "app.press", app=APP, keys="Meta+Comma"),
            click("general", {"css": ".sidebar-item", "hasText": "General"}),
            read("head", {"css": "[data-browser=settings] .set-head"}, "BROWSERAPP-18", count=1, text="BrowserNew", within=10),
            read("rows", {"css": "[data-browser=settings] .set-card"}, "BROWSERAPP-18", count=1,
                 text="Agents can use the browser They open pages, click and type; you watch beside the conversation and take control any time. "
                      "Agents can use this Mac Other apps too, through agent-desktop. macOS asks you once."),
            checked("browser-on", browser, "true", "BROWSERAPP-18"),
            checked("mac-on", mac, "true"),
            read("on-is-ink", browser, None, styles={"background-color": "--ink"}, **{"styles.background-color.matches": True}),
            read("foot", {"css": "[data-browser=settings] .set-foot"}, None, count=1, text="A password you type in Oby goes straight to the page. No agent sees it."),
            step("after-permissions", "app.measure", "BROWSERAPP-18", app=APP, target={"css": "[data-browser=settings]"},
                 against={"permissions": {"css": ".set-group", "hasText": "Permissions on this Mac"}}, expect={"paths": {"permissions.overlap": False}}),
            *pictures(13, "SettingsBrowser", "board 13"),
            read("globe-there", {"css": "[data-browser=globe]"}, None, count=1),
            click("browser-off", browser),
            checked("browser-is-off", browser, "false", "BROWSERAPP-18"),
            read("globe-gone", {"css": "[data-browser=globe]"}, "BROWSERAPP-18", count=0, within=5),
            click("browser-on-again", browser),
            checked("browser-on-again-reads", browser, "true"),
            read("globe-back", {"css": "[data-browser=globe]"}, None, count=1, within=5),
            close(),
        ],
    )


# obid's browser routes aren't served yet: they come with browser port 1's routes pull request
# (4b; docs/contracts/browser.md, BROWSER-1 to 25; its contract #2608 and core #2610 merged). Every step that
# needs them is expected red until then; the pull request that lands them removes these marks.
PORT_1 = "browser port 1's routes PR (4b): obid doesn't serve /v1/browser yet (contract #2608, core #2610 merged)"


def red(s):
    s.setdefault("ui", {})["expectedRed"] = PORT_1
    return s


def live_101():
    workflow(
        "browser-101-live-on-obid.json",
        "App live: on a real obid, the globe opens the browser, no pages yet (board 17, live)",
        "The real app on a scratch obid (BROWSERAPP-7, BROWSERAPP-8): once obid answers GET /v1/browser, main follows its "
        "browser and the globe shows at the title bar's end; opened, the panel has no tabs and says so, as board 17. Expected "
        "red until obid serves the browser's routes (browser port 1).",
        [
            step("obid", "obid-start", agent="fake-harness-jobs"),
            step("app", "app.launch", obid="${state.obid}", size={"width": 1280, "height": 800}),
            red(step("globe", "app.read", "BROWSERAPP-8", app=APP, target={"role": "button", "name": "Show the browser, 0 tabs"},
                     within=20, expect={"paths": {"count": 1}})),
            red(click("open", {"role": "button", "name": "Show the browser, 0 tabs"})),
            red(measure("panel", {"css": "aside.br"}, "BROWSERAPP-8", **{"box.width": 640, "box.x": 640})),
            red(read("no-pages", {"css": ".empty-words"}, "BROWSERAPP-8", count=1, text="No pages yet.\nWhen an agent opens one, you see it here, live.")),
            close(),
        ],
    )


# What a board draws that the app doesn't, piece by piece, and why (docs/contracts/browser-app.md, "Not matched
# within 4px"): left out of that board's compare, never of its other checks.
BOARD_SKIPS = {
    # the board leaves Full width out of a panel beside a docked side panel; the app keeps one set of buttons on every
    # tab (board 2's), so the tabs' end has one more button and the tabs one button's room less
    3: [".br-tabs-end .icon-button", ".br-tab"],
    # the board draws a narrower set (no page menu); the app keeps board 2's set, so the address is 30px shorter
    4: [".br-address"],
    # across the window the tabs start after the traffic lights (--after-lights), where macOS put them: 5px right of the
    # board's, as the app's own panel toggle is
    5: [".br-tab"],
    # the decision card over the message box sits where the app's composer is (52px title bar, the app's thread): its
    # boxes are held to the board in their words, their places reported (today's composer, not the board's)
    7: [".decision-card", ".tc-ask-line", ".tc-ask-text"],
    8: [".decision-card", ".tc-ask-line", ".secret-row", ".secret-note"],
    # the board draws a smaller set for a tab nobody is using (no Full width, no page menu, Take control without its
    # hand); the app keeps board 2's set: one more button at the tabs' end, the address 30px shorter, the strip's
    # Take control 20px wider and its words 20px narrower
    11: [".br-tabs-end .icon-button", ".br-address", ".br-control-text", ".br-control .tc-option"],
    12: [".br-tabs-end .icon-button", ".br-address", ".br-control-text", ".br-control .tc-option"],
    14: [".br-tabs-end .icon-button", ".br-address", ".br-control-text", ".br-control .tc-option"],
}


if __name__ == "__main__":
    OUT.mkdir(parents=True, exist_ok=True)
    board_01()
    board_02()
    board_03()
    board_04()
    board_05()
    board_06()
    board_07()
    board_08()
    board_09()
    board_10()
    board_11()
    board_12()
    board_13()
    board_14()
    board_17()
    live_101()
