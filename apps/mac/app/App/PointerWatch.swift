import AppKit
import SwiftUI

/// Oby on Home watches the pointer anywhere on screen, and glances at each click in the window, as
/// in Sangam's design (its main process reads the cursor 60 times a second, twice Oby's frame rate,
/// and sends it only when it moved). Only while Oby shows and its window is on screen: it stops the
/// moment the window closes, goes to the Dock or out of sight, so a closed window costs nothing.
///
/// It reads `NSEvent.mouseLocation` rather than monitoring mouse moves everywhere: no permission,
/// and nothing missed while another app is in front. Clicks come from a local monitor, which only
/// sees Oby's own events and hands each one on untouched.
final class PointerWatch: NSView {
    /// What it tells Oby, in its frame's CSS px from the top left.
    enum Sight: Equatable {
        /// Where the pointer is; nil lets go.
        case look(CGPoint?)
        /// A click in the window, outside Oby's frame.
        case notice(CGPoint)
        /// A click on Oby's frame: the page tells a poke apart.
        case poke(CGPoint)
    }

    var send: (Sight) -> Void = { _ in }
    /// Oby shows: on Home, its window open (the character isn't paused).
    var showing = false { didSet { if showing != oldValue { refresh() } } }
    private(set) var watching = false
    private var timer: Timer?
    private var clicks: Any?
    private var observers: [NSObjectProtocol] = []
    private var last: CGPoint?

    override var isFlipped: Bool { true }
    // Never in the way of a click.
    override func hitTest(_ point: NSPoint) -> NSView? { nil }

    override func viewWillMove(toWindow newWindow: NSWindow?) {
        for observer in observers { NotificationCenter.default.removeObserver(observer) }
        observers = []
        if newWindow == nil { stop() }
    }

    override func viewDidMoveToWindow() {
        guard let window else { return }
        let center = NotificationCenter.default
        observers = [
            center.addObserver(forName: NSWindow.didChangeOcclusionStateNotification, object: window, queue: .main) { [weak self] _ in
                MainActor.assumeIsolated { self?.refresh() }
            },
            center.addObserver(forName: NSWindow.willCloseNotification, object: window, queue: .main) { [weak self] _ in
                MainActor.assumeIsolated { self?.stop() }
            },
        ]
        refresh()
    }

    /// Watches while Oby shows in a window that's on screen and not covered.
    func refresh() {
        let onScreen = window.map { $0.isVisible && !$0.isMiniaturized && $0.occlusionState.contains(.visible) } ?? false
        showing && onScreen ? start() : stop()
    }

    /// A point on screen (AppKit's, from the bottom left) in Oby's frame, as the page takes it.
    func point(fromScreen screen: NSPoint) -> CGPoint? {
        window.map { convert($0.convertPoint(fromScreen: screen), from: nil) }
    }

    func clicked(_ event: NSEvent) {
        guard watching, let window, event.window === window else { return }
        let p = convert(event.locationInWindow, from: nil)
        send(bounds.contains(p) ? .poke(p) : .notice(p))
    }

    private func start() {
        guard !watching else { return }
        watching = true
        let timer = Timer(timeInterval: 1.0 / 60, repeats: true) { [weak self] timer in
            MainActor.assumeIsolated { if let self { self.tick() } else { timer.invalidate() } }
        }
        // Common modes: it keeps up while the window is dragged or resized.
        RunLoop.main.add(timer, forMode: .common)
        self.timer = timer
        clicks = NSEvent.addLocalMonitorForEvents(matching: [.leftMouseDown, .rightMouseDown, .otherMouseDown]) { [weak self] event in
            MainActor.assumeIsolated { self?.clicked(event) }
            return event
        }
        tick()
    }

    private func stop() {
        guard watching else { return }
        watching = false
        timer?.invalidate()
        timer = nil
        if let clicks { NSEvent.removeMonitor(clicks) }
        clicks = nil
        last = nil
        send(.look(nil))
    }

    private func tick() {
        guard let p = point(fromScreen: NSEvent.mouseLocation), p != last else { return }
        last = p
        send(.look(p))
    }
}

/// `PointerWatch` behind Oby's frame, feeding the character.
struct PointerFeed: NSViewRepresentable {
    let character: ObyCharacter
    let showing: Bool

    func makeNSView(context: Context) -> PointerWatch {
        let watch = PointerWatch()
        watch.send = { [weak character] sight in
            switch sight {
            case .look(let p): character?.look(p)
            case .notice(let p): character?.notice(p)
            case .poke(let p): character?.poke(p)
            }
        }
        return watch
    }

    func updateNSView(_ watch: PointerWatch, context: Context) { watch.showing = showing }
}
