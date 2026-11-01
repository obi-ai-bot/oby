import SwiftUI

/// Settings, opened over Home from your picture: the Electron app's pages on obid, in its order
/// (apps/electron/src/renderer/src/SettingsView.tsx, Sangam's design): General, Agents, Remote
/// access, Connectors, About, each a page of his cards (apps/shared/SettingsComponents.swift). What
/// only the Mac can do (open at login, adding or signing in an agent, connecting GitHub or Tetrix,
/// pairing another device, updating Oby) stays on the Mac, and its rows say so.
struct SettingsScreen: View {
    /// Deskree's own Macs (`showEverything`) also get Apple Watch without iPhone.
    @AppStorage(Launch.everythingKey) private var everything = false

    var body: some View {
        SettingsPageFrame(title: "Settings") {
            SetGroup {
                ForEach(PhoneSettingsPage.allCases.filter { everything || $0 != .watch }, id: \.self) { page in
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
    case general, agents, remote, connectors, about, watch

    var label: String {
        switch self {
        case .general: "General"
        case .agents: "Agents"
        case .remote: "Remote access"
        case .connectors: "Connectors"
        case .about: "About"
        case .watch: "Apple Watch"
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
            case .watch: WatchPage()
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
        NotificationsSection(allowed: { UIApplication.shared.registerForRemoteNotifications() },
                             footer: "Oby alerts you when a job needs you, like a question or a failed run. Tap an alert to open the job.")
    }
}

/// Agents (Electron's Settings › Agents): which agent new tasks go to, then a card per agent with
/// its 5-hour session and week, its model and efforts, each changed here as there. Adding, signing
/// in and removing an agent happen on the Mac, where its tool runs (the gateway carries none of
/// those: docs/runner-api.md, "obid only: accounts").
private struct AgentsPage: View {
    @Environment(Phone.self) private var phone
    @State private var options: ModelOptions?
    @State private var problem: String?

    /// The providers' order, as Electron's cards.
    private static let order = ["claude", "codex", "cursor"]

    var body: some View {
        let agents = phone.accounts.sorted { (Self.order.firstIndex(of: $0.agent) ?? 9) < (Self.order.firstIndex(of: $1.agent) ?? 9) }
        let ready = agents.filter(\.signedIn)
        let chosen = ready.first { $0.id == phone.defaultAccount } ?? ready.first
        SetGroup("Default agent", footer: "Add, sign in or remove an agent in Settings › Agents on your Mac, where it runs.", problem: problem) {
            if let chosen {
                SetRow("New tasks go to") {
                    SetSelect("Default agent", selection: Binding { chosen.id } set: { id in
                        guard id != chosen.id else { return }
                        Task { do { try await phone.setDefaultAccount(id); problem = nil } catch { problem = error.localizedDescription } }
                    }, options: ready.map { ($0.id, "\($0.name) · \(Usage.tool($0.agent))") })
                }
            } else {
                SetRow("No agents yet", quiet: true)
            }
        }
        ForEach(agents) { account in
            AgentCard(account: account, options: options, base: base(account.agent))
        }
        Color.clear.frame(height: 0).task {
            options = try? await phone.runner.modelOptions()
            // obid sends no event when another app changes it: read it as the page opens.
            await phone.readDefaultAccount()
        }
    }

    /// What the tool runs when the account chose nothing (obid's status `models`).
    private func base(_ agent: String) -> ModelChoice? {
        switch agent {
        case "claude": phone.models?.claude
        case "codex": phone.models?.codex
        case "cursor": phone.models?.cursor.map { ModelChoice(model: $0.model, effort: "") }
        default: nil
        }
    }
}

/// One agent's card: its usage (Electron's: the 5-hour session's bar, the week in words under
/// it), its model and its efforts.
private struct AgentCard: View {
    @Environment(Phone.self) private var phone
    let account: Account
    let options: ModelOptions?
    /// The tool's own model and efforts, for what the account didn't choose.
    let base: ModelChoice?
    @State private var problem: String?

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
            SetGroup(account.name, problem: problem) {
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
                                SetAbout(account.label ?? "Not signed in: sign in on your Mac")
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
                SetRow("Model") {
                    ModelMenu(title: "\(account.name) model and effort",
                              models: Self.choices((tool?.models ?? []).map { ($0.id, $0.name) }, model),
                              model: Binding { model } set: { if $0 != model { tune(["model": $0]) } },
                              efforts: account.agent == "cursor" ? [] : Self.choices(efforts.map { ($0, effortName($0)) }, effort),
                              effort: Binding { effort } set: { if $0 != effort { tune(["effort": $0]) } })
                }
                if account.agent != "cursor" {
                    SetRow("Effort on complex tasks") {
                        SetSelect("\(account.name) complex task effort", selection: Binding { complexEffort } set: { if $0 != complexEffort { tune(["complexEffort": $0]) } },
                                  options: Self.choices((tool?.complexEfforts ?? []).map { ($0, effortName($0)) }, complexEffort))
                    }
                }
            }
        }
    }

    /// What the account runs on: its own choice, else the tool's (Electron's `useModels`).
    private var model: String { account.model ?? base?.model ?? "" }
    private var effort: String { account.effort ?? base?.effort ?? "" }
    private var complexEffort: String { account.complexEffort ?? base?.complexEffort ?? effort }
    /// The efforts it takes: Codex's are its model's, the others' the tool's.
    private var efforts: [String] {
        account.agent == "codex" ? tool?.models.first { $0.id == model }?.efforts ?? [] : tool?.efforts ?? []
    }

    /// A pop-up's choices, the one set kept even when the choices don't have it (Electron's `choices`).
    static func choices(_ list: [(String, String)], _ set: String) -> [(String, String)] {
        (set.isEmpty || list.contains { $0.0 == set } ? [] : [(set, set)]) + list
    }

    private func tune(_ change: [String: String]) {
        Task { do { try await phone.tune(account.id, change); problem = nil } catch { problem = error.localizedDescription } }
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
            : a.sevenDayResets.map { "Resets \($0.formatted(.dateTime.weekday(.wide)))" } ?? "No reset reported"
        if a.sevenDay != nil, let used {
            week = "This week \(used)% · " + (resets.hasPrefix("Resets") ? "resets" + resets.dropFirst(6) : resets)
        } else if a.sevenDay == nil, session != nil {
            week = "No weekly use reported yet"
        } else {
            week = resets
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
            SetGroup("Your Mac", footer: "Unpair makes this iPhone forget your Mac. To end its pairing on the Mac too, revoke it in your Mac’s Settings › Remote access.") {
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

/// Electron's ModelPicker: one button, the model and after it in grey its effort; one menu with the
/// models, then Effort, which opens the efforts. No efforts (Cursor's): the models only.
private struct ModelMenu: View {
    let title: String
    let models: [(String, String)]
    @Binding var model: String
    let efforts: [(String, String)]
    @Binding var effort: String

    var body: some View {
        Menu {
            Picker(title, selection: $model) { ForEach(models, id: \.0) { Text($0.1).tag($0.0) } }
                .pickerStyle(.inline).labelsHidden()
            if !efforts.isEmpty {
                Picker("Effort", selection: $effort) { ForEach(efforts, id: \.0) { Text($0.1).tag($0.0) } }
                    .pickerStyle(.menu)
            }
        } label: {
            HStack(spacing: 6) {
                Text(name(model, models)).foregroundStyle(Tokens.ink)
                if !efforts.isEmpty { Text(name(effort, efforts)).foregroundStyle(Tokens.muted) }
                Image(systemName: "chevron.down").font(.system(size: 9, weight: .semibold)).foregroundStyle(Tokens.muted).padding(.leading, 4)
            }
            .font(Tokens.font(Tokens.textBase)).lineLimit(1)
            .padding(.leading, 14).padding(.trailing, 12)
            .modifier(FieldFrame(hot: false))
            .contentShape(Rectangle())
        }
        .menuStyle(.button).buttonStyle(.plain).menuIndicator(.hidden).fixedSize()
        .accessibilityLabel(title)
        .accessibilityValue(efforts.isEmpty ? name(model, models) : "\(name(model, models)), \(name(effort, efforts))")
    }

    private func name(_ id: String, _ list: [(String, String)]) -> String { list.first { $0.0 == id }?.1 ?? (id.isEmpty ? "—" : id) }
}

/// Apple Watches that reach the Mac without this iPhone (`showEverything` only, as before).
private struct WatchPage: View {
    @Environment(Phone.self) private var phone
    @State private var watchPairing = WatchPairing()
    @State private var problem: String?

    var body: some View {
        SetGroup("Apple Watch without iPhone",
                 footer: "Away from this iPhone, your watch reaches your Mac through your Cloudflare Tunnel. Paste the Cloudflare Access service token, then pair with Oby open on the watch.",
                 problem: problem) {
            SetRow("Client ID") { SetField("Client ID", text: $watchPairing.clientID, prompt: "…access", width: 180).textInputAutocapitalization(.never) }
            SetRow("Client secret") { SetField("Client secret", text: $watchPairing.clientSecret, prompt: "Secret", secret: true, width: 180) }
            SetRow(watchPairing.watches.isEmpty ? "No watch paired" : "Pair another watch") {
                Button(watchPairing.busy ? "Pairing…" : "Pair") {
                    Task {
                        problem = nil
                        do { try await watchPairing.pair(phone) } catch { problem = error.localizedDescription }
                    }
                }
                .buttonStyle(.set)
                .disabled(!watchPairing.canPair)
            }
            ForEach(watchPairing.watches) { watch in
                SetRow(watch.name, about: "Paired") {
                    Button("Remove", role: .destructive) {
                        Task { do { try await watchPairing.remove(watch.id, phone) } catch { problem = error.localizedDescription } }
                    }
                    .buttonStyle(.set)
                }
            }
        }
        .task { try? await watchPairing.load(phone) }
    }
}
