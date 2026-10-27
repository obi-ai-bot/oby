import AppKit
import SwiftUI
import UniformTypeIdentifiers

/// You, as Oby shows you on this Mac (Sangam's account menu and OwnerAvatar): the name you gave
/// it, else your macOS name, and your picture, else your initials on your name's colour. The name
/// you gave goes to your Obysphere in your Oby's card (`SphereModel.cardName`), your macOS name
/// never; the Obysphere still knows you by your owner id, which never changes.
// ponytail: the picture stays on this Mac; sharing it needs a card field the protocol doesn't have.
@MainActor @Observable final class Profile {
    /// Swapped only by tests and pictures of the window.
    static var shared = Profile()

    private let defaults: UserDefaults
    private let file: URL
    /// The name you gave, "" for none.
    private(set) var given: String
    private(set) var picture: NSImage?

    init(defaults: UserDefaults = .standard, file: URL = Workspace.root.appending(path: "profile-picture")) {
        (self.defaults, self.file) = (defaults, file)
        given = defaults.string(forKey: Self.key) ?? ""
        picture = (try? Data(contentsOf: file)).flatMap(NSImage.init(data:))
    }

    private static let key = "ObyProfileName"

    /// The name shown everywhere: yours, else this Mac user's full name.
    var name: String { given.isEmpty ? NSFullUserName() : given }

    /// Keeps `name` (blank goes back to your macOS name) and `picture`, cut small (`small`); nil takes it out.
    func save(name: String, picture: NSImage?) throws {
        if picture !== self.picture {
            if let picture {
                guard let data = Self.small(picture) else { throw RunnerError(message: "That image can't be read.") }
                try data.write(to: file, options: .atomic)
                self.picture = NSImage(data: data)
            } else {
                try? FileManager.default.removeItem(at: file)
                self.picture = nil
            }
        }
        given = name.trimmingCharacters(in: .whitespacesAndNewlines)
        defaults.set(given, forKey: Self.key)
    }

    /// 256 pixels a side from its middle (the Obysphere's own picture's cut), under 200 KB: a PNG,
    /// else a JPEG for a photo too busy for one.
    nonisolated static func small(_ image: NSImage) -> Data? {
        guard let png = SphereModel.picture(image) else { return nil }
        return png.count <= 200_000 ? png : NSBitmapImageRep(data: png)?.representation(using: .jpeg, properties: [.compressionFactor: 0.8])
    }
}

/// Your picture (Sangam's OwnerAvatar, without the Oby badge): the one you gave, else your initials
/// on your name's colour.
struct PersonPicture: View {
    let name: String
    var image: NSImage?
    var size: CGFloat = 24

    var body: some View {
        Group {
            if let image {
                Image(nsImage: image).resizable().scaledToFill()
            } else {
                Text(SidebarSections.initials(name)).font(Tokens.font(size * 5 / 12, .semibold)).foregroundStyle(.white.opacity(0.92))
                    .frame(maxWidth: .infinity, maxHeight: .infinity).background(Tokens.avatar(name))
            }
        }
        .frame(width: size, height: size).clipShape(Circle())
    }
}

/// Your name and picture (the account menu's name; Sangam's design has no page for them): a click
/// on the picture chooses an image, ⌘V or its menu pastes one. In the Create sheet's look.
struct ProfileSheet: View {
    let profile: Profile
    let close: () -> Void
    @State private var name: String
    @State private var image: NSImage?
    @State private var problem: String?

    init(close: @escaping () -> Void) {
        let profile = Profile.shared
        (self.profile, self.close) = (profile, close)
        // Only a name you gave: saving a new picture doesn't give your macOS name to the Obysphere.
        (_name, _image) = (State(initialValue: profile.given), State(initialValue: profile.picture))
    }

    private var shown: String {
        let typed = name.trimmingCharacters(in: .whitespaces)
        return typed.isEmpty ? NSFullUserName() : typed
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack {
                Text("Your name and picture").font(Tokens.font(17, .semibold)).foregroundStyle(Tokens.ink)
                Spacer()
                IconButton(symbol: "xmark", action: close).keyboardShortcut(.cancelAction).accessibilityLabel("Close")
            }
            .padding(.top, -4).padding(.trailing, -6).padding(.bottom, 8)
            Text("How Oby shows you. Teammates in your Obysphere see the name you give; your picture stays on this Mac.")
                .font(Tokens.font(Tokens.textBase)).foregroundStyle(Tokens.muted).padding(.bottom, 20)

            // The picture beside the name, as the Create sheet's logo: a click chooses one; its badge adds or takes it out.
            HStack(alignment: .bottom, spacing: 16) {
                Button(action: choose) { PersonPicture(name: shown, image: image, size: 64) }
                    .buttonStyle(.plain)
                    .help(image == nil ? "Add a picture" : "Change picture")
                    .contextMenu {
                        Button("Choose Image…", action: choose)
                        Button("Paste Image", action: paste)
                        if image != nil { Button("Remove Picture") { image = nil } }
                    }
                    .overlay(alignment: .bottomTrailing) {
                        Button { if image == nil { choose() } else { image = nil } } label: {
                            Image(systemName: image == nil ? "photo.badge.plus" : "xmark").font(Tokens.font(10, .semibold))
                                .foregroundStyle(Tokens.text).frame(width: 22, height: 22)
                                .background(Color(hex: 0x2C2C2B), in: Circle()).overlay(Circle().strokeBorder(Tokens.surface, lineWidth: 2))
                        }
                        .buttonStyle(.plain).offset(x: 6, y: 6)
                        .accessibilityLabel(image == nil ? "Add a picture" : "Remove picture")
                    }
                VStack(alignment: .leading, spacing: 8) {
                    Text("Name").font(Tokens.font(Tokens.textMd, .medium)).foregroundStyle(Tokens.item)
                    ModalField { TextField("Name", text: $name, prompt: Text(NSFullUserName()).foregroundStyle(Tokens.muted)).onSubmit(save) }
                        .onChange(of: name) { if name.count > 40 { name = String(name.prefix(40)) } }
                }
            }

            if let problem {
                Text(problem).font(Tokens.font(Tokens.textSm)).foregroundStyle(Tokens.danger).padding(.top, 16)
            }
            HStack {
                Spacer()
                Button(action: save) {
                    Text("Save").font(Tokens.font(Tokens.textBase, .medium)).foregroundStyle(Tokens.onInk)
                        .padding(.horizontal, 14).frame(height: 32)
                        .background(Tokens.ink, in: RoundedRectangle(cornerRadius: 8, style: .continuous))
                }
                .buttonStyle(.plain).keyboardShortcut(.defaultAction)
            }
            .padding(.top, 24)
        }
        .padding(20)
        .frame(width: 420)
        .background(Tokens.surface)
        .onPasteCommand(of: [.image, .fileURL]) { _ in paste() }
    }

    private func choose() {
        let panel = NSOpenPanel()
        panel.allowedContentTypes = [.image]
        if panel.runModal() == .OK, let url = panel.url, let picked = NSImage(contentsOf: url) { (image, problem) = (picked, nil) }
    }

    private func paste() {
        if let pasted = NSImage(pasteboard: .general) { (image, problem) = (pasted, nil) } else { problem = "There’s no image to paste." }
    }

    private func save() {
        do {
            try profile.save(name: name, picture: image)
            close()
        } catch {
            problem = error.localizedDescription
        }
    }
}
