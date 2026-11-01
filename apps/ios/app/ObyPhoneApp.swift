import SwiftUI

/// Oby on iPhone: a client of the runner (docs/runner-api.md), so work goes on with the Mac closed.
/// Sangam's design, light or dark as the system is (Settings › Appearance): Home, with his side
/// panel for the tasks and the conversations, and no tab bar (PhoneRoot).
@main
struct ObyPhoneApp: App {
    /// Holds the phone, so a notification tapped before Home appears still opens its job.
    @UIApplicationDelegateAdaptor(Notifications.self) private var notifications
    /// Oby on Home, and on the unpaired screen: a page each, as a WebPage shows in one WebView at
    /// a time and, as pairing starts or ends, both screens are up for a moment (WebKit stops the app).
    @State private var home = StageCharacter()
    @State private var unpaired = StageCharacter()
    @AppStorage(Appearance.key) private var appearance = Appearance.system
    @Environment(\.scenePhase) private var scenePhase

    private var phone: Phone { notifications.phone }

    var body: some Scene {
        WindowGroup {
            PhoneRoot(home: home, unpaired: unpaired)
            .tint(Tokens.ink)
            .foregroundStyle(Tokens.text)
            .background(Tokens.bg)
            .preferredColorScheme(appearance.scheme)
            .environment(phone)
            .onChange(of: scenePhase, initial: true) { _, phase in
                // In the background: no connection and no drawing, so no battery spent.
                phase == .active ? phone.start() : phone.stop()
            }
            // Oby draws only while Home's stage shows, or the unpaired screen's.
            .onChange(of: scenePhase == .active && phone.homePath.isEmpty && !phone.needsPairing, initial: true) { _, shown in
                home.made?.pause(!shown)
            }
            .onChange(of: scenePhase == .active && phone.needsPairing, initial: true) { _, shown in
                unpaired.made?.pause(!shown)
            }
            // Opening the side panel mid-hold: the mic stops, as leaving Home always did.
            .onChange(of: phone.panel) { phone.cancelVoice() }
        }
        // A job ended while Oby was suspended (JobWaiter): say so on the Live Activity.
        .backgroundTask(.urlSession(JobWaiter.identifier)) { [phone] in await phone.catchUp() }
    }
}

/// A screen's Oby, made the first time that screen shows it: a page loaded with no WebView
/// showing it never draws, so Home's, made while the iPhone was unpaired, stayed blank.
@MainActor final class StageCharacter {
    private(set) var made: ObyCharacter?
    var character: ObyCharacter {
        if let made { return made }
        let new = ObyCharacter(query: "embed&size=1.23")
        made = new
        return new
    }
}
