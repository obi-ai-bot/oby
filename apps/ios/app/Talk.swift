import SwiftUI

/// Who Oby talks through (docs/contracts/conversation-agent.md), as the picker in the message box
/// shows it: what obid says, in words. obid decides; this only reads (Electron's talk.ts).
nonisolated enum TalkChoice {
    /// obid's word for "whichever has room".
    static let auto = "auto"
    private static let tools = ["claude", "codex", "cursor"]

    /// An account held full by obid.
    static func full(_ account: Account, now: Date = .now) -> Bool { account.fullUntil.map { $0 > now } ?? false }
    static func canRun(_ account: Account, now: Date = .now) -> Bool { account.signedIn && !full(account, now: now) }

    /// The fuller of its week and its 5-hour session (one past its reset counts 0), the limit that
    /// hits first, in percent. Nil: it reports none, as Cursor, and no number is made up.
    static func percent(_ account: Account, now: Date = .now) -> Int? {
        guard account.agent != "cursor" else { return nil }
        let week = account.sevenDay ?? account.fiveHour
        let over = account.fiveHourResets.map { $0 <= now } ?? false
        let session = !account.signedIn || account.fiveHour == nil ? nil : over ? 0 : account.fiveHour
        guard week != nil || session != nil else { return nil }
        return Int((max(week ?? 0, session ?? 0) * 100).rounded())
    }

    static func usageWords(_ account: Account, now: Date = .now) -> String {
        if full(account, now: now) { return "Full" }
        return percent(account, now: now).map { "\($0)%" } ?? "No usage"
    }

    /// The dot: past 80% yellow, past 90% red, red when obid holds it full.
    static func dot(_ account: Account, now: Date = .now) -> UsageDot {
        full(account, now: now) ? .over : UsageDot.of(weeks: [percent(account, now: now).map { Double($0) / 100 }])
    }

    static let toolNames = ["claude": "Claude", "codex": "Codex", "cursor": "Cursor"]
    static func toolName(_ agent: String) -> String { toolNames[agent] ?? agent }

    /// An account by its tool; by its name too once its tool has more than one.
    static func rowTitle(_ account: Account, among accounts: [Account]) -> String {
        accounts.count { $0.agent == account.agent && $0.signedIn } > 1 ? "\(toolName(account.agent)) · \(account.name)" : toolName(account.agent)
    }

    /// Who it is, as its tool says; else the tool.
    static func who(_ account: Account) -> String {
        let plan = account.plan.map { $0.prefix(1).uppercased() + $0.dropFirst() }
        let words = [account.email, account.org, plan].compactMap { $0 }.filter { !$0.isEmpty }.joined(separator: " · ")
        return words.isEmpty ? toolName(account.agent) : words
    }

    /// The owner's choice: the conversation's, else the setting's; none is Auto.
    static func chosen(own: String?, setting: String?) -> String? {
        let id = own ?? setting ?? auto
        return id == auto ? nil : id
    }

    /// The account the picker names: the chosen one while it can run and its turns haven't moved off
    /// it; else the one its last answer came from, else the one obid says a first turn would use,
    /// each while it can run; none can, where it last talked.
    static func shown(_ accounts: [Account], chosen: String?, talkedOn: String?, next: String?, movedOff: String?, now: Date = .now) -> Account? {
        func find(_ id: String?) -> Account? { accounts.first { $0.id == id } }
        return [chosen == movedOff ? nil : find(chosen), find(talkedOn), find(next)].compactMap { $0 }.first { canRun($0, now: now) }
            ?? accounts.first { canRun($0, now: now) } ?? find(talkedOn)
    }

    /// The list's rows: every connected account, tool by tool.
    static func list(_ accounts: [Account]) -> [Account] {
        (tools + accounts.map(\.agent).filter { !tools.contains($0) }).reduce(into: [Account]()) { rows, agent in
            if !rows.contains(where: { $0.agent == agent }) { rows += accounts.filter { $0.agent == agent && $0.signedIn } }
        }
    }
}

/// In the message box: the chosen account's mark and an arrow, or the word Auto and an arrow, with a
/// dot past 80% and past 90% or held full. A tap opens the list over it: Auto, then every connected
/// account, the chosen one checked. `conversation`: a choice is that conversation's, at its next
/// turn; nil, it's the setting new conversations follow. No Add account: a phone can't add one (the
/// gateway keeps signing in on the Mac, docs/runner-api.md "obid only: accounts").
struct TalkPicker: View {
    @Environment(Phone.self) private var phone
    let conversation: String?
    /// `-ObySamplePicker YES` opens it, for its screenshot.
    @State private var open = UserDefaults.standard.bool(forKey: "ObySamplePicker")

    private var own: RunnerConversation? { conversation.flatMap { id in phone.conversations?.first { $0.id == id } } }
    private var chosen: String? { TalkChoice.chosen(own: own?.talkAccount, setting: phone.talkSetting) }
    private var on: Account? {
        TalkChoice.shown(phone.accounts, chosen: chosen, talkedOn: own?.talkedOn, next: phone.talkNext, movedOff: own?.movedOff)
    }

    var body: some View {
        let on = on
        let dot = on.map { TalkChoice.dot($0) } ?? .none
        Button { open.toggle() } label: {
            HStack(spacing: 3) {
                if let on, chosen != nil {
                    AgentMark(agent: on.agent).font(Tokens.font(13, .medium))
                } else {
                    Text("Auto").font(Tokens.font(Tokens.textSm, .medium))
                }
                if dot != .none {
                    Circle().fill(dot == .over ? Tokens.over : Tokens.full).frame(width: 6, height: 6)
                }
                Image(systemName: "chevron.down").font(Tokens.font(9, .semibold))
            }
            .foregroundStyle(open ? Tokens.ink : Tokens.muted)
            .padding(.horizontal, 8).frame(height: 32)
            .background(open ? Tokens.active : .clear, in: Capsule())
            .contentShape(Capsule())
        }
        .buttonStyle(.plain)
        .accessibilityLabel(label(on))
        .popover(isPresented: $open, arrowEdge: .bottom) {
            list.presentationCompactAdaptation(.popover)
        }
        // As the usage panel: the list shows use read just now.
        .onChange(of: open) { if open { Task { await phone.lookAtUsage() } } }
    }

    private func label(_ on: Account?) -> String {
        guard let on else { return "Choose who Oby talks through" }
        let used = TalkChoice.full(on) ? ", full" : TalkChoice.percent(on).map { ", \($0)% used" } ?? ""
        return "Talking through \(TalkChoice.rowTitle(on, among: phone.accounts))\(used). Change"
    }

    private var list: some View {
        VStack(alignment: .leading, spacing: 0) {
            row(account: TalkChoice.auto, checked: chosen == nil, name: "Auto", about: "Whichever has room", used: nil, level: .none) { Color.clear }
            ForEach(TalkChoice.list(phone.accounts)) { account in
                row(account: account.id, checked: chosen == account.id, name: TalkChoice.rowTitle(account, among: phone.accounts),
                    about: TalkChoice.who(account), used: TalkChoice.usageWords(account), level: TalkChoice.dot(account)) {
                    AgentMark(agent: account.agent).font(Tokens.font(15)).foregroundStyle(Tokens.item)
                }
            }
        }
        .padding(6)
        .frame(width: 320)
        .fixedSize(horizontal: false, vertical: true)
        .background(Tokens.surface)
    }

    private func row<Mark: View>(account: String, checked: Bool, name: String, about: String, used: String?, level: UsageDot, @ViewBuilder mark: () -> Mark) -> some View {
        Button {
            open = false
            phone.chooseTalk(account, in: conversation)
        } label: {
            HStack(spacing: 10) {
                mark().frame(width: 20)
                VStack(alignment: .leading, spacing: 1) {
                    Text(name).font(Tokens.font(Tokens.textBase)).foregroundStyle(Tokens.ink).lineLimit(1)
                    Text(about).font(Tokens.font(Tokens.textSm)).foregroundStyle(Tokens.muted).lineLimit(1)
                }
                Spacer(minLength: 4)
                if let used {
                    Text(used).font(Tokens.font(Tokens.textSm)).monospacedDigit()
                        .foregroundStyle(level == .over ? Tokens.over : level == .high ? Tokens.full : Tokens.text)
                }
                Image(systemName: "checkmark").font(Tokens.font(12, .semibold)).foregroundStyle(Tokens.ink)
                    .frame(width: 14).opacity(checked ? 1 : 0)
            }
            .padding(.horizontal, 8).frame(minHeight: 48)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityElement(children: .combine)
        .accessibilityAddTraits(checked ? .isSelected : [])
    }
}
