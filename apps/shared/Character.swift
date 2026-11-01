import Foundation
import OSLog
import WebKit

/// Sangam's character: `character/oby.html` and three.js, bundled into the app and served
/// from the `oby://` scheme so its ES modules load without file-URL tricks.
@Observable
final class ObyCharacter {
    let page: WebPage
    private let events = CharacterEvents()
    private var loaded = false
    private(set) var paused = false
    /// Where `look` last put the pointer. Not observed: it moves 60 times a second.
    @ObservationIgnored private var looking: CGPoint?
    /// Oby's first frame is drawn (its `ready` event). Show its frame from then on: shown before,
    /// it may flash while the page loads.
    private(set) var ready = false

    /// `query` is the page's: the default is a 1.25 scale; the window's Home asks for its embedded
    /// look (`embed`, no playground, a transparent page) at Sangam's size.
    init(query: String = "size=1.25") {
        var config = WebPage.Configuration()
        config.urlSchemeHandlers[URLScheme("oby")!] = CharacterFiles()
        config.userContentController.add(events, name: "oby")
        page = WebPage(configuration: config)
        events.received = { [weak self] event in
            guard event == "ready", let self else { return }
            ready = true
            // Paused before it drew: pause it now that it has.
            if paused { applyPause() }
        }
        Task {
            do {
                for try await event in page.load(URLRequest(url: URL(string: "oby://character/oby.html?\(query)")!))
                where event == .finished {
                    loaded = true
                    applyPause()
                    // Until it draws, the window keeps Oby's frame transparent: say so if it doesn't.
                    Task {
                        try? await Task.sleep(for: .seconds(5))
                        if !ready { Logger.character.notice("Oby's page loaded 5 s ago and hasn't drawn its first frame (no ready): its frame stays hidden until it does") }
                    }
                }
            } catch {
                Logger.character.error("Oby's page didn't load: \(error.localizedDescription)")
            }
        }
    }

    /// The state Oby shows whenever the window shows it: idle, listening, thinking, working,
    /// loading or quiet.
    var mood = "idle" {
        didSet { if !paused, mood != oldValue { run(Self.switching(from: oldValue, to: mood), ["state": mood]) } }
    }

    /// How Oby goes from one mood to the next: at once, except out of loading, where it finishes
    /// first, its dots running back together into Oby (`setState`'s `finish`). Not when you start
    /// talking: then it listens right away.
    static func switching(from old: String, to new: String) -> String {
        old == "loading" && new != "listening" ? "oby.setState(state, { finish: true })" : "oby.setState(state, { now: true })"
    }

    /// One expression now, then back to the state.
    func play(_ expression: String) { run("oby.play(name)", ["name": expression]) }

    /// Where the pointer is, in the page's CSS px from its top left (past its edges when it's
    /// outside the frame). Nil lets go: Oby looks around on its own.
    func look(_ point: CGPoint?) {
        looking = point
        guard let point else { return run("oby.look(null)", [:]) }
        run("oby.look(x, y)", ["x": Double(point.x), "y": Double(point.y)])
    }

    /// A glance where the pointer is, as at a click (the pointer arriving on Home's chips).
    func glance() { if let looking { notice(looking) } }

    /// A click there: Oby glances at it, then goes back to its own business.
    func notice(_ point: CGPoint) { run("oby.notice(x, y)", ["x": Double(point.x), "y": Double(point.y)]) }

    /// A click on Oby's frame, as a click in the page: a poke when it lands on Oby (the page decides).
    func poke(_ point: CGPoint) {
        run("document.getElementById('stage').dispatchEvent(new PointerEvent('pointerdown', { clientX: x, clientY: y }))", ["x": Double(point.x), "y": Double(point.y)])
    }

    /// A paused Oby draws nothing, so a closed window costs no CPU.
    func pause(_ on: Bool) {
        guard on != paused else { return }
        paused = on
        applyPause()
    }

    private func applyPause() {
        // Not drawn yet: its first frame comes first. Paused before it (under the notifications
        // prompt, say), the page never says it's ready, and Oby stays hidden.
        if paused, !ready { return }
        guard paused else {
            run("oby.pause(false); oby.setState(state, { now: true })", ["state": mood])
            return
        }
        // Look ahead and settle into a calm face first: paused mid-fidget or mid-glance,
        // Oby would freeze looking away.
        run("document.dispatchEvent(new PointerEvent('pointerleave')); oby.setState('quiet', { now: true })", [:])
        Task {
            try? await Task.sleep(for: .milliseconds(900))
            if paused { run("oby.pause(true)", [:]) }
        }
    }

    private func run(_ js: String, _ arguments: [String: Any]) {
        guard loaded else { return }
        Task {
            do {
                // The page's own world: that's where oby.html put window.oby.
                let found = try await page.callJavaScript("if (!window.oby) return false; \(js); return true", arguments: arguments, contentWorld: .page)
                if found as? Bool != true { Logger.character.error("window.oby isn't on the page") }
            } catch {
                Logger.character.error("oby call failed: \(error.localizedDescription)")
            }
        }
    }
}

/// Oby's events (ready, state, expression, end, poke) come back here, as `{ oby: name, … }`.
private final class CharacterEvents: NSObject, WKScriptMessageHandler {
    var received: (String) -> Void = { _ in }

    func userContentController(_ controller: WKUserContentController, didReceive message: WKScriptMessage) {
        Logger.character.debug("event \(String(describing: message.body), privacy: .public)")
        if let event = (message.body as? [String: Any])?["oby"] as? String { received(event) }
    }
}

/// Serves the bundled character files, and nothing outside them.
nonisolated struct CharacterFiles: URLSchemeHandler {
    func reply(for request: URLRequest) -> AsyncThrowingStream<URLSchemeTaskResult, any Error> {
        AsyncThrowingStream { stream in
            guard let url = request.url,
                  let root = Bundle.main.resourceURL?.appending(path: "Character").standardizedFileURL else {
                stream.finish(throwing: URLError(.badURL)); return
            }
            let file = root.appending(path: url.path).standardizedFileURL
            guard file.path.hasPrefix(root.path + "/"), let data = try? Data(contentsOf: file) else {
                stream.finish(throwing: URLError(.fileDoesNotExist)); return
            }
            let mime = ["html": "text/html", "js": "text/javascript", "css": "text/css"][file.pathExtension] ?? "application/octet-stream"
            stream.yield(.response(URLResponse(url: url, mimeType: mime, expectedContentLength: data.count, textEncodingName: "utf-8")))
            stream.yield(.data(data))
            stream.finish()
        }
    }
}

extension Logger {
    nonisolated static let character = Logger(subsystem: "com.deskree.oby", category: "character")
}
