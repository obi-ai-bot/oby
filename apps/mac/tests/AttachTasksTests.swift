import AppKit
import Foundation
import SwiftUI
import Testing
@testable import Oby

/// Sangam's + › Attach tasks (#1173): the picker's search and arrows, and the words to Oby carrying
/// the tasks attached (`POST /v1/plans`'s `jobs`), shown under them at once.
@MainActor
struct AttachTasksTests {
    private func job(_ id: String, _ title: String) -> Job {
        var job = Job(id: id, request: "r", agent: "claude", account: "default", repo: "deskree-inc/oby", branch: "oby/\(id)",
                      base: "dev", pr: nil, kind: .build, task: "t", created: .now)
        job.title = title
        return job
    }

    @Test func theSearchKeepsTheTasksWhoseTitleHoldsWhatsTyped() {
        let tasks = [job("a", "Fix CI in oberon"), job("b", "Webhook retry"), job("c", "Ci dashboard")]
        #expect(TaskPicker.matches(tasks, "").map(\.id) == ["a", "b", "c"])
        #expect(TaskPicker.matches(tasks, "  ").map(\.id) == ["a", "b", "c"])
        #expect(TaskPicker.matches(tasks, " ci ").map(\.id) == ["a", "c"])
        #expect(TaskPicker.matches(tasks, "RETRY").map(\.id) == ["b"])
        #expect(TaskPicker.matches(tasks, "nothing").isEmpty)
    }

    @Test func theArrowsGoRound() {
        #expect(TaskPicker.moved(0, down: true, count: 3) == 1)
        #expect(TaskPicker.moved(2, down: true, count: 3) == 0)
        #expect(TaskPicker.moved(0, down: false, count: 3) == 2)
        // The list narrowed under the lit row: it moves from the last there is.
        #expect(TaskPicker.moved(5, down: false, count: 2) == 0)
        #expect(TaskPicker.moved(0, down: true, count: 0) == 0)
    }

    /// Keyboard first, as a person types: the search has the keys as it opens, ↓ lights the next
    /// task, going round at the end, Return attaches the lit one, typing narrows the list to what it
    /// lights first, and Esc closes it.
    @Test func theKeysPickTheLitTask() async throws {
        var picked: [String] = []
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 400, height: 300), styleMask: [.titled], backing: .buffered, defer: false)
        var closed = false
        window.contentView = NSHostingView(rootView: TaskPicker(tasks: [job("a", "One"), job("b", "Two"), job("c", "Three")], picked: [],
                                                                onPick: { picked.append($0.id) }, onClose: { closed = true }))
        window.orderFront(nil)
        defer { window.orderOut(nil) }
        for _ in 0..<100 where !(window.firstResponder is NSTextView) { try await Task.sleep(for: .milliseconds(20)) }
        #expect(window.firstResponder is NSTextView, "the search has the keys as it opens")
        func press(_ code: UInt16, _ characters: String) async throws {
            let event = try #require(NSEvent.keyEvent(with: .keyDown, location: .zero, modifierFlags: [], timestamp: 0, windowNumber: window.windowNumber,
                                                      context: nil, characters: characters, charactersIgnoringModifiers: characters,
                                                      isARepeat: false, keyCode: code))
            window.sendEvent(event)
            try await Task.sleep(for: .milliseconds(50))
        }
        let down = (UInt16(125), "\u{F701}"), enter = (UInt16(36), "\r")
        for key in [down, down, enter, down, enter] { try await press(key.0, key.1) }
        #expect(picked == ["c", "a"])
        for key in [(UInt16(17), "t"), (UInt16(13), "w"), enter] { try await press(key.0, key.1) }
        #expect(picked == ["c", "a", "b"])
        try await press(53, "\u{1B}")
        #expect(closed, "Esc closes it")
    }

    /// `jobs` goes only with tasks attached: a runner from before #1173 refuses keys it doesn't know.
    @Test func thePlanCarriesTheTasksOnlyWhenThereAreSome() async throws {
        let stub = RunnerStub { _ in (200, #"{"kind": "none", "jobs": [], "drafts": []}"#) }
        _ = try await stub.runner.planned("about these", jobs: ["j1", "j2"])
        _ = try await stub.runner.planned("nothing attached")
        let sent = stub.requests.compactMap(\.json)
        #expect(sent.first?["jobs"] as? [String] == ["j1", "j2"])
        #expect(sent.last?["said"] as? String == "nothing attached" && sent.last?["jobs"] == nil)
    }

    /// Send: the words show at once with the tasks' cards under them, the tasks go with them to
    /// obid, and the tags go.
    @Test func sendingTakesTheTasksAttached() async throws {
        let stub = RunnerStub { _ in (200, #"{"kind": "none", "jobs": [], "drafts": []}"#) }
        let feed = RunnerFeed()
        let model = IslandModel(feed: feed, availability: MacAvailability())
        model.plansOn = stub.runner
        model.tray.tasks = ["j1"]
        model.sendChat("what's left on this one?")
        #expect(model.thread.last?.jobs == ["j1"] && model.tray.tasks.isEmpty)
        for _ in 0..<250 where stub.requests.isEmpty { try await Task.sleep(for: .milliseconds(20)) }
        let sent = try #require(stub.requests.first?.json)
        #expect(sent["said"] as? String == "what's left on this one?" && sent["jobs"] as? [String] == ["j1"])
    }
}

/// A picture of the picker over a box with a task attached, for the pull request: off unless
/// `OBY_SCREENS` names a folder (`TEST_RUNNER_OBY_SCREENS=… xcodebuild test -only-testing:MacTests/AttachTasksScreens`).
@MainActor
@Suite(.enabled(if: ProcessInfo.processInfo.environment["OBY_SCREENS"] != nil))
struct AttachTasksScreens {
    @Test func picker() async throws {
        let titles = ["Fix CI in oberon", "Webhook retry fix", "Review tetrix-ee PR #389", "0.9 release notes"]
        var tasks = titles.enumerated().map { index, title in
            var job = Job(id: "j\(index)", request: "r", agent: "claude", account: "default", repo: "deskree-inc/oby",
                          branch: "oby/j\(index)", base: "dev", pr: nil, kind: .build, task: "t", created: .now)
            job.title = title
            return job
        }
        tasks[0].status = .running
        tasks[2].status = .done
        let tray = Tray()
        tray.tasks = ["j1"]
        let page = VStack(spacing: 8) {
            TaskPicker(tasks: tasks, picked: tray.tasks, onPick: { _ in }, onClose: {})
                .clipShape(RoundedRectangle(cornerRadius: 20, style: .continuous))
                .overlay { RoundedRectangle(cornerRadius: 20, style: .continuous).strokeBorder(Tokens.surfaceEdge) }
            MessageBox(text: .constant("What's left on this one?"), focused: .constant(true), tray: tray,
                       onSend: {}, onScreenshot: {}, onStop: { false }, usage: [], tasks: tasks)
        }
        .padding(24).frame(width: 528, height: 360, alignment: .bottom).background(Tokens.bg)
        let size = NSSize(width: 528, height: 360)
        let view = NSHostingView(rootView: page.environment(\.colorScheme, .dark))
        let window = NSWindow(contentRect: NSRect(origin: .zero, size: size), styleMask: [.borderless], backing: .buffered, defer: false)
        window.appearance = NSAppearance(named: .darkAqua)
        window.contentView = view
        view.frame = NSRect(origin: .zero, size: size)
        window.orderBack(nil)
        try await Task.sleep(for: .milliseconds(600))
        view.layoutSubtreeIfNeeded()
        let image = try #require(view.bitmapImageRepForCachingDisplay(in: view.bounds))
        view.cacheDisplay(in: view.bounds, to: image)
        let folder = URL(fileURLWithPath: ProcessInfo.processInfo.environment["OBY_SCREENS"] ?? NSTemporaryDirectory())
        try #require(image.representation(using: .png, properties: [:])).write(to: folder.appending(path: "mac-attach-tasks.png"))
        window.orderOut(nil)
    }
}
