import AppKit
import SwiftUI

/// The text's layout controls growth, including wrapped and pasted lines; the viewport caps it.
nonisolated enum ComposerMetrics {
    static let lineHeight: CGFloat = 22
    /// The composer's height with one line, and the buttons in it: the + and the microphone (or Send)
    /// share one size, and sit at the bottom, centered on a single line of text (#322).
    static let rowHeight: CGFloat = 52
    static let button: CGFloat = 34
    static var buttonInset: CGFloat { (rowHeight - button) / 2 }
    /// Above and below the text, so one line fills the row and sits at its middle.
    static var textInset: CGFloat { (rowHeight - lineHeight) / 2 }
    static func height(lines: Int, maximum: CGFloat) -> CGFloat {
        min(max(lineHeight, maximum), CGFloat(max(1, lines)) * lineHeight)
    }

    @MainActor static func lines(_ text: String, width: CGFloat) -> Int {
        guard !text.isEmpty else { return 1 }
        let storage = NSTextStorage(string: text, attributes: [.font: NSFont.systemFont(ofSize: 15)])
        let layout = NSLayoutManager()
        let container = NSTextContainer(containerSize: NSSize(width: max(1, width), height: .greatestFiniteMagnitude))
        container.lineFragmentPadding = 0
        storage.addLayoutManager(layout)
        layout.addTextContainer(container)
        layout.ensureLayout(for: container)
        var count = 0
        layout.enumerateLineFragments(forGlyphRange: NSRange(location: 0, length: layout.numberOfGlyphs)) { _, _, _, _, _ in count += 1 }
        // AppKit lays out the empty line after a final newline separately.
        if text.hasSuffix("\n") { count += 1 }
        return max(1, count)
    }
}

struct ComposerEditor: NSViewRepresentable {
    @Binding var text: String
    @Binding var focused: Bool
    var focusRequest = 0
    let onSend: () -> Void
    let onStop: () -> Bool
    /// The window's message box sets its text on a taller line, as Sangam's design does.
    var lineHeight: CGFloat? = nil
    var textColor: NSColor = .white
    var label = "Message Oby"
    /// Esc with nothing to stop: the window goes back a page. Nil leaves Esc to the text view.
    var onEscape: (() -> Void)? = nil

    final class Editor: NSTextView {
        var send: () -> Void = {}
        var stop: () -> Bool = { false }
        var escape: (() -> Void)?
        var wantsFocus: () -> Bool = { false }
        var focusWhenAttached = false

        func requestComposerFocus() {
            guard wantsFocus() else { focusWhenAttached = false; return }
            guard let window else { focusWhenAttached = true; return }
            focusWhenAttached = false
            window.initialFirstResponder = self
            window.makeFirstResponder(self)
        }

        override func viewDidMoveToWindow() {
            super.viewDidMoveToWindow()
            if focusWhenAttached {
                DispatchQueue.main.async { [weak self] in self?.requestComposerFocus() }
            }
        }

        override func keyDown(with event: NSEvent) {
            // Let the input method finish composing before interpreting Return as Send.
            if event.keyCode == 36, !hasMarkedText(), !event.modifierFlags.contains(.shift) { send(); return }
            if event.keyCode == 53, !hasMarkedText() {
                if stop() { return }
                if let escape { escape(); return }
            }
            super.keyDown(with: event)
        }
    }

    final class Coordinator: NSObject, NSTextViewDelegate {
        var parent: ComposerEditor
        var requestedFocus = false
        var focusRequest = 0
        init(_ parent: ComposerEditor) { self.parent = parent }
        func textDidChange(_ notification: Notification) {
            guard let view = notification.object as? NSTextView else { return }
            parent.text = view.string
        }
        func textDidBeginEditing(_ notification: Notification) { parent.focused = true }
        func textDidEndEditing(_ notification: Notification) { parent.focused = false }
    }
    func makeCoordinator() -> Coordinator { Coordinator(self) }

    func makeNSView(context: Context) -> NSScrollView {
        let scroll = NSScrollView()
        scroll.drawsBackground = false
        scroll.hasVerticalScroller = true
        scroll.autohidesScrollers = true
        let editor = Editor(frame: .zero)
        editor.delegate = context.coordinator
        editor.isRichText = false
        editor.allowsUndo = true
        editor.font = .systemFont(ofSize: 15)
        editor.textColor = textColor
        editor.insertionPointColor = textColor
        if let lineHeight {
            let paragraph = NSMutableParagraphStyle()
            paragraph.minimumLineHeight = lineHeight
            paragraph.maximumLineHeight = lineHeight
            editor.defaultParagraphStyle = paragraph
            // The extra room goes above the letters: half of it back under them, so they sit in the line's middle.
            let offset = (lineHeight - NSFont.systemFont(ofSize: 15).boundingRectForFont.height) / 4
            editor.typingAttributes = [.font: NSFont.systemFont(ofSize: 15), .foregroundColor: textColor,
                                       .paragraphStyle: paragraph, .baselineOffset: max(0, offset)]
        }
        editor.drawsBackground = false
        editor.textContainerInset = .zero
        editor.textContainer?.lineFragmentPadding = 0
        editor.isVerticallyResizable = true
        editor.isHorizontallyResizable = false
        editor.autoresizingMask = [.width]
        editor.minSize = .zero
        editor.maxSize = NSSize(width: CGFloat.greatestFiniteMagnitude, height: CGFloat.greatestFiniteMagnitude)
        editor.textContainer?.widthTracksTextView = true
        editor.textContainer?.containerSize = NSSize(width: 300, height: CGFloat.greatestFiniteMagnitude)
        editor.setAccessibilityLabel(label)
        scroll.documentView = editor
        return scroll
    }

    func updateNSView(_ scroll: NSScrollView, context: Context) {
        guard let editor = scroll.documentView as? Editor else { return }
        context.coordinator.parent = self
        editor.send = onSend
        editor.stop = onStop
        editor.escape = onEscape
        editor.wantsFocus = { context.coordinator.parent.focused }
        if editor.string != text {
            editor.string = text
            editor.setSelectedRange(NSRange(location: text.utf16.count, length: 0))
            editor.scrollRangeToVisible(editor.selectedRange())
        }
        if focused && (!context.coordinator.requestedFocus || focusRequest != context.coordinator.focusRequest) {
            editor.focusWhenAttached = true
            DispatchQueue.main.async { [weak editor] in editor?.requestComposerFocus() }
        }
        context.coordinator.requestedFocus = focused
        context.coordinator.focusRequest = focusRequest
    }
}
