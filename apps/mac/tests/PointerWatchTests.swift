import AppKit
import Testing
@testable import Oby

/// Oby on Home follows the pointer anywhere on screen and glances at clicks in the window, only
/// while it shows in a window that's on screen (#1196, Sangam's design).
@Suite(.serialized)
@MainActor
struct PointerWatchTests {
    /// A window on screen or not as the test says, whatever the display is doing. As a real one,
    /// it's still visible while it says it will close.
    private final class Shown: NSWindow {
        var shown = true
        override var isVisible: Bool { shown }
        override var occlusionState: NSWindow.OcclusionState { shown ? .visible : [] }
        override func close() { super.close(); shown = false }

        func set(_ on: Bool) {
            shown = on
            NotificationCenter.default.post(name: NSWindow.didChangeOcclusionStateNotification, object: self)
        }
    }

    /// A 400 × 300 window with Oby's 280 × 220 frame 50 from its left and 40 from its top.
    private func window(_ watch: PointerWatch) -> Shown {
        let window = Shown(contentRect: NSRect(x: 100, y: 200, width: 400, height: 300), styleMask: .borderless, backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        watch.frame = NSRect(x: 50, y: 40, width: 280, height: 220)
        window.contentView?.addSubview(watch)
        return window
    }

    @Test func watchesOnlyWhileObyShowsInAWindowOnScreen() {
        let watch = PointerWatch()
        var sights: [PointerWatch.Sight] = []
        watch.send = { sights.append($0) }
        watch.showing = true
        #expect(!watch.watching)
        let window = window(watch)
        #expect(watch.watching)
        #expect(sights.count == 1, "it says where the pointer is at once")
        // Covered, in the Dock or on another Space: it stops, and Oby lets go of the pointer.
        window.set(false)
        #expect(!watch.watching)
        #expect(sights.last == .look(nil))
        window.set(true)
        #expect(watch.watching)
        // A page over Home: Oby is paused, and it stops.
        watch.showing = false
        #expect(!watch.watching)
        watch.showing = true
        #expect(watch.watching)
        // Closed: it stops the moment the window says it will close.
        window.close()
        #expect(!watch.watching)
        #expect(sights.last == .look(nil))
        window.set(true)
        #expect(watch.watching)
        watch.removeFromSuperview()
        #expect(!watch.watching)
    }

    @Test func aPointOnScreenLandsInObysFrameAsThePageTakesIt() {
        let watch = PointerWatch()
        #expect(watch.point(fromScreen: .zero) == nil)
        let frame = window(watch).frame
        // The frame's top left, its middle, and a point far outside the window: past its edges.
        let topLeft = NSPoint(x: frame.minX + 50, y: frame.maxY - 40)
        #expect(watch.point(fromScreen: topLeft) == CGPoint(x: 0, y: 0))
        #expect(watch.point(fromScreen: NSPoint(x: topLeft.x + 140, y: topLeft.y - 110)) == CGPoint(x: 140, y: 110))
        #expect(watch.point(fromScreen: NSPoint(x: frame.minX - 300, y: frame.maxY + 500)) == CGPoint(x: -350, y: -540))
    }

    @Test func aClickInTheWindowCatchesObysEyeAndOneOnItsFrameIsAPoke() throws {
        let watch = PointerWatch()
        var sights: [PointerWatch.Sight] = []
        watch.send = { sights.append($0) }
        let window = window(watch)
        func click(_ x: CGFloat, _ y: CGFloat) throws -> NSEvent {
            try #require(NSEvent.mouseEvent(with: .leftMouseDown, location: NSPoint(x: x, y: y), modifierFlags: [], timestamp: 0,
                                            windowNumber: window.windowNumber, context: nil, eventNumber: 0, clickCount: 1, pressure: 1))
        }
        watch.clicked(try click(10, 290))
        #expect(sights.isEmpty, "not watching: nothing")
        watch.showing = true
        sights = []
        // Near the window's top left corner, outside Oby's frame; then on its frame.
        watch.clicked(try click(10, 290))
        watch.clicked(try click(190, 150))
        #expect(sights == [.notice(CGPoint(x: -40, y: -30)), .poke(CGPoint(x: 140, y: 110))])
        watch.showing = false
    }
}
