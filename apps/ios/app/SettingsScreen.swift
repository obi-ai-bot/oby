import SwiftUI
import UserNotifications

/// Settings, opened over Home from your picture: the Electron app's pages on obid, in its order
/// (apps/electron/src/renderer/src/SettingsView.tsx, Sangam's design): General, Agents, Remote
/// access, Connectors, About, each a page of his cards (apps/shared/SettingsComponents.swift). What
/// only the Mac can do (open at login, adding or signing in an agent, connecting GitHub or Tetrix,
/// pairing another device, updating Oby) stays on the Mac, and its rows say so.
struct SettingsScreen: View {
    var body: some View {
        SettingsPageFrame(title: "Settings") {
            SetGroup {
                ForEach(PhoneSettingsPage.allCases, id: \.self) { page in
                    NavigationLink { page.screen } label: {
                        SetRow(page.label) {
                            Image(systemName: "chevron.right").font(Tokens.font(12, .semibold)).foregroundStyle(Tokens.muted)
                        }
                        .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                }
            }
        }
    }

    /// "0.9 (42)", as the bundle says it.
    static var version: String {
        let info = Bundle.main.infoDictionary
        let short = info?["CFBundleShortVersionString"] as? String ?? "?"
        return info?["CFBundleVersion"].map { "\(short) (\($0))" } ?? short
    }
}

/// Electron's Settings pages on obid, in its order.
enum PhoneSettingsPage: CaseIterable {
    case general, agents, remote, connectors, about

    var label: String {
        switch self {
        case .general: "General"
        case .agents: "Agents"
        case .remote: "Remote access"
        case .connectors: "Connectors"
        case .about: "About"
        }
    }

    @ViewBuilder var screen: some View {
        SettingsPageFrame(title: label) {
            switch self {
            case .general: GeneralPage()
            case .agents: AgentsPage()
            case .remote: RemotePage()
            case .connectors: ConnectorsPage()
            case .about: SetGroup { SetRow("Version") { SetValue(SettingsScreen.version) } }
            }
        }
    }
}

/// A Settings page: its 28/600 title over its cards.
private struct SettingsPageFrame<Content: View>: View {
    let title: String
    @ViewBuilder let content: Content

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 0) {
                Text(title).font(Tokens.font(28, .semibold)).foregroundStyle(Tokens.ink)
                    .padding(.bottom, 16).accessibilityAddTraits(.isHeader)
                content
            }
            .padding(.horizontal, Tokens.gutter).padding(.top, 8).padding(.bottom, 32)
        }
        .scrollDismissesKeyboard(.interactively)
        .environment(\.settingsCards, true)
        .obyPage()
        .navigationBarTitleDisplayMode(.inline)
    }
}

/// General: the Mac's permissions, then notifications (Electron's General on obid; Open at login
/// is the Mac's own).
private struct GeneralPage: View {
    @Environment(Phone.self) private var phone

    var body: some View {
        PermissionsSection(runner: phone.runner, defaultMode: phone.defaultMode, setDefault: phone.setDefaultMode,
                           title: "Permissions on your Mac",
                           footer: "Default mode sets how often your agents check with you before they edit files or run commands. To change it for one task, use that task’s Mode menu. Always allow adds a rule here; remove it to be asked again.")
        PhoneNotifications()
    }
}

/// Electron's Notifications on obid: Oby's alerts, and where the system keeps banners, sounds and
/// Focus. On the iPhone they're the system's: allowed or off there, asked for once.
private struct PhoneNotifications: View {
    @State private var status: UNAuthorizationStatus?
    @Environment(\.openURL) private var openURL

    var body: some View {
        SetGroup("Notifications", footer: "Oby alerts you when a job needs you, like a question or a failed run. Tap an alert to open the job.") {
            SetRow("Alerts") {
                if status == .notDetermined {
                    Button("Allow Alerts") { Task { await allow() } }.buttonStyle(.set)
                } else {
                    SetValue(status == .denied ? "Off" : status == nil ? "…" : "Allowed")
                }
            }
            SetRow("Banners, sounds and Focus") {
                Button("Open Settings") {
                    if let url = URL(string: UIApplication.openNotificationSettingsURLString) { openURL(url) }
                }
                .buttonStyle(.set)
            }
        }
        .task { await refresh() }
        .onReceive(NotificationCenter.default.publisher(for: UIApplication.didBecomeActiveNotification)) { _ in Task { await refresh() } }
    }

    private func refresh() async { status = await UNUserNotificationCenter.current().notificationSettings().authorizationStatus }

    private func allow() async {
        if (try? await UNUserNotificationCenter.current().requestAuthorization(options: [.alert, .sound, .badge])) == true {
            UIApplication.shared.registerForRemoteNotifications()
        }
        await refresh()
    }
}

/// Agents (Electron's Settings › Agents): which agent new tasks go to, then a card per agent with
/// its 5-hour session and week, its model and efforts. Adding, signing in and removing an agent
/// happen on the Mac, where its tool runs; so does changing its model (the gateway doesn't carry
/// account changes: docs/runner-api.md, "obid only: accounts").
private struct AgentsPage: View {
    @Environment(Phone.self) private var phone
    @State private var options: ModelOptions?

    /// The providers' order, as Electron's cards.
    private static let order = ["claude", "codex", "cursor"]

    var body: some View {
        let agents = phone.accounts.sorted { (Self.order.firstIndex(of: $0.agent) ?? 9) < (Self.order.firstIndex(of: $1.agent) ?? 9) }
        let ready = agents.filter(\.signedIn)
        let chosen = ready.first { $0.id == phone.defaultAccount } ?? ready.first
        SetGroup("Default agent", footer: "Add, sign in, remove or tune an agent in Settings › Agents on your Mac, where it runs.") {
            if let chosen {
                SetRow("New tasks go to") { SetValue("\(chosen.name) · \(Usage.tool(chosen.agent))") }
            } else {
                SetRow("No agents yet", quiet: true)
            }
        }
        ForEach(agents) { account in
            AgentCard(account: account, options: options)
        }
        Color.clear.frame(height: 0).task { options = try? await phone.runner.modelOptions() }
    }
}

/// One agent's card: its usage (Electron's: the 5-hour session's bar, the week in words under
/// it), its model and its efforts.
private struct AgentCard: View {
    let account: Account
    let options: ModelOptions?

    private static let efforts = ["low": "Low", "medium": "Medium", "high": "High", "xhigh": "Extra high", "max": "Max", "ultra": "Ultra"]

    private var tool: ModelOptions.Tool? {
        switch account.agent {
        case "claude": options?.claude
        case "codex": options?.codex
        case "cursor": options?.cursor
        default: nil
        }
    }

    var body: some View {
        TimelineView(.periodic(from: .now, by: 60)) { context in
            let usage = AgentUsage(account, now: context.date)
            SetGroup(account.name) {
                SetRow {
                    HStack(alignment: .top, spacing: 12) {
                        AgentMark(agent: account.agent).font(Tokens.font(13)).foregroundStyle(Tokens.muted)
                            .frame(width: 16).padding(.top, 2).accessibilityHidden(true)
                        VStack(alignment: .leading, spacing: 3) {
                            HStack {
                                Text(Usage.tool(account.agent))
                                Spacer(minLength: 8)
                                if let session = usage.session { SetValue("\(session)% used") }
                            }
                            if !account.signedIn {
                                SetAbout("\(account.label ?? "Not signed in"): sign in on your Mac")
                            } else if let session = usage.session {
                                SetAbout("5-hour session" + (usage.sessionResets.map { " · resets \($0)" } ?? ""))
                                SetMeter(used: Double(session)).padding(.vertical, 4)
                                SetAbout(usage.week)
                            } else {
                                SetAbout(usage.week)
                            }
                        }
                    }
                }
                SetRow("Model") { SetValue(modelWords) }
                if account.agent != "cursor" {
                    SetRow("Effort on complex tasks") { SetValue(account.complexEffort.map(effortName) ?? "Default") }
                }
            }
        }
    }

    /// The model and, after it, how hard it thinks (Electron's ModelPicker's button).
    private var modelWords: String {
        let model = account.model.map { id in tool?.models.first { $0.id == id }?.name ?? id } ?? "Default"
        guard account.agent != "cursor", let effort = account.effort else { return model }
        return "\(model) · \(effortName(effort))"
    }

    private func effortName(_ effort: String) -> String { Self.efforts[effort] ?? effort }
}

/// An account's usage as Electron's `accountRow` and `weekWords` say it: the 5-hour session's
/// share (signed in, not Cursor; past its reset, 0) and when it starts over, and the week in words.
struct AgentUsage {
    var session: Int?
    var sessionResets: String?
    var week: String

    init(_ a: Account, now: Date) {
        let over = a.fiveHourResets.map { $0 <= now } ?? false
        session = a.agent == "cursor" || !a.signedIn || a.fiveHour == nil ? nil : over ? 0 : Int(((a.fiveHour ?? 0) * 100).rounded())
        sessionResets = session != nil && !over ? a.fiveHourResets?.formatted(date: .omitted, time: .shortened) : nil
        let used = a.agent == "cursor" ? nil : (a.sevenDay ?? a.fiveHour).map { Int(($0 * 100).rounded()) }
        let resets = !a.signedIn ? a.label ?? "Not signed in"
            : a.agent == "cursor" ? "Cursor doesn’t report usage"
            : used == nil ? "No use reported yet"
            : a.sevenDayResets.map { "resets \($0.formatted(.dateTime.weekday(.wide)))" } ?? "no reset reported"
        if a.sevenDay != nil, let used {
            week = "This week \(used)% · \(resets)"
        } else if a.sevenDay == nil, session != nil {
            week = "No weekly use reported yet"
        } else {
            week = resets.prefix(1).uppercased() + resets.dropFirst()
        }
    }
}

/// Remote access, from the iPhone's side: the Mac it reaches, how, until when, and Unpair. Pairing
/// a device and the list of paired ones are the Mac's own (its Settings › Remote access).
private struct RemotePage: View {
    @Environment(Phone.self) private var phone
    @State private var unpairing = false

    var body: some View {
        if let link = phone.link {
            SetGroup("Your Mac", footer: "Revoke this iPhone from your Mac’s Settings › Remote access, or unpair it here. It will need to pair again before it can reach your Mac.") {
                SetRow(link.name, about: [phone.route?.label, phone.pairedUntil.map { "Paired until \($0.formatted(.dateTime.month(.abbreviated).day()))" }]
                    .compactMap(\.self).joined(separator: " · ")) {
                    SetValue(phone.notice?.title ?? (phone.connection == .live ? "Connected" : "Connecting…"), connected: phone.connection == .live)
                }
                SetRow("Unpair this iPhone") {
                    Button("Unpair", role: .destructive) { unpairing = true }.buttonStyle(.set)
                }
            }
            .confirmationDialog("Unpair from \(link.name)?", isPresented: $unpairing, titleVisibility: .visible) {
                Button("Unpair", role: .destructive) { phone.link = nil }
            } message: {
                Text("This iPhone forgets its device token. Scan the code on your Mac to connect again.")
            }
        } else {
            SetGroup("Your Mac", footer: "On your Mac, open Settings › Remote access › Pair a device, then scan its code.") {
                SetRow("No Mac paired yet", quiet: true)
            }
        }
    }
}

/// Connectors (Electron's on obid): Connected, then Available, each with its line. Connecting and
/// disconnecting happen on the Mac, whose logins they are.
private struct ConnectorsPage: View {
    @Environment(Phone.self) private var phone

    private struct Connector {
        var name: String
        /// Its mark, as Electron's BrandMark: GitHub's drawn in the text's color, Tetrix's its own.
        var mark: String
        var about: String
        var connected: Bool
    }

    var body: some View {
        // GitHub: obid's gh is signed in (its GitHub work says so, and as whom); Tetrix: the
        // accounts' agents have it.
        let github = phone.gitHubWork, tetrix = phone.accounts.contains { $0.tetrix == true }
        let all = [
            Connector(name: "GitHub", mark: "GitHub", about: github?.signedIn == true ? github?.login ?? "Signed in" : "Repos, pull requests and checks",
                      connected: github?.signedIn == true),
            Connector(name: "Tetrix", mark: "Tetrix", about: tetrix ? "For all your agents" : "Project knowledge and blast radius", connected: tetrix),
        ]
        ForEach([true, false], id: \.self) { connected in
            let items = all.filter { $0.connected == connected }
            if !items.isEmpty {
                SetGroup(connected ? "Connected" : "Available", footer: connected ? nil : "Connect them in Settings › Connectors on your Mac: their logins stay there, never on this iPhone.") {
                    ForEach(items, id: \.name) { c in
                        SetRow {
                            HStack(spacing: 12) {
                                Image(c.mark).resizable().scaledToFit().frame(width: 22, height: 22).foregroundStyle(Tokens.ink)
                                    .accessibilityHidden(true)
                                VStack(alignment: .leading, spacing: 2) {
                                    Text(c.name)
                                    SetAbout(c.about)
                                }
                            }
                        } control: {
                            SetValue(c.connected ? "Connected" : "On your Mac", connected: c.connected)
                        }
                    }
                }
            }
        }
    }
}

/// Settings › Appearance's choice, kept from before Settings followed Electron's (which has none):
/// the app still draws in it.
enum Appearance: String, CaseIterable {
    case system, light, dark

    static let key = "appearance"
    var label: String { rawValue.capitalized }
    var scheme: ColorScheme? {
        switch self {
        case .system: nil
        case .light: .light
        case .dark: .dark
        }
    }
}
