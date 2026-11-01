# Pairing an iPhone with the Electron app's obid

The business behavior contract for showing the pairing QR code in the Electron app, so the owner's iPhone reaches the obid it manages through their Obysphere, sealed end to end. From [the template](../business-contract-template.md). Issue: [#1711](https://github.com/obi-ai-bot/oby/issues/1711). Scenarios are numbered `PAIRING-n`; the tests that prove them are named after them (`apps/electron/src/renderer/src/test/pairing.test.ts`); the rest are real-device steps.

## Feature and status

- **Outcome:** The owner opens Settings › Runner, clicks Pair a device, scans the QR code with Oby on their iPhone, and the iPhone shows and answers their conversations and tasks from anywhere, through the Obysphere, with no port, tunnel or tailnet.
- **Status:** **Implemented** in the Electron app (`src/main/device.ts`, `obid-pair-invite`; `src/renderer/src/SettingsView.tsx`, `Pairing`). obid's side was already built (docs/runner-api.md, "LAN gateway" and "obid only: the phone relay") and is unchanged; so is the iPhone's (`apps/ios/app/ConnectScreen.swift`, `apps/shared/PhoneRelay.swift`). A port of the frozen Swift Mac app's `PairingView.swift`, through the Obysphere only.
- **Scope:** an invitation through the Obysphere, its QR code, the paired devices and Revoke. **Out of scope:** pairing over Remote access or the tailnet (hidden for launch, as Remote access is), the six-digit code (the iPhone takes it only for a Mac Bonjour finds), the Apple Watch.
- **Design:** Sangam's Settings has a Runner page (hidden for launch); on obid it shows only these two groups, in his Group and Row, with the Swift Mac app's words (which the iPhone's own words point to: "Scan the one the Mac's Settings › Runner shows."). Sangam may redraw it.

## Behavior

- **Actors and ownership:** the owner clicks Pair a device, then Pair in main's own dialog ("Pair a new device with this Mac?", which the page can't click: a phone that can approve is for good); main then signs the invitation with this Mac's device key (the one a Confirm uses); obid makes the invitation, its relay token and the Mac's relay key, and registers them with the Obysphere; the page shows the QR code and the list.
- **Triggers and prerequisites:** obid is a member of an Obysphere (`GET /v1/obid/sphere` `joined`; docs/contracts/obysphere.md).

| What the owner sees | What happens |
|---|---|
| Settings › Runner, in an Obysphere: Pair a device, "Through your Obysphere" | Nothing yet. |
| Pair a device | main's dialog: "Pair a new device with this Mac?", "The phone that scans its code can see and answer your tasks, and approve for you, until you revoke it.", Pair and Cancel (the default; Cancel shows "Not paired: you cancelled."). Pair: `POST /v1/pairing/invite` `{"name": <this Mac's name>, "approval": {device, action: "", signedAt, signature over "oby-pair-invite\n<signedAt>"}}`. The QR code shows, with "Through your Obysphere: *host*. Valid for two minutes. The code is single-use."; the button says Create new invitation. |
| Two minutes later | The QR code is gone. |
| The iPhone paired | It shows in Paired devices within 5 seconds, with when it expires, and Revoke. |
| Revoke | `DELETE /v1/pairing/devices/{id}`: the phone's session ends and its relay token opens nothing; it pairs again to reach this Mac. |
| Not in an Obysphere | "No way to reach this Mac yet", and "Join an Obysphere and your iPhone pairs through it, with nothing else to set up." No button. |

- **Failures and recovery:** obid's own line under the group (the Obysphere unreachable `504`, refusing `502`, an approval that doesn't hold `403`, the device key unavailable); Pair a device tries again.
- **Invariants:** the QR code carries the Obysphere's URL, the link (the Mac's name, the relay: the Obysphere, the relay id, the phone's relay token, the Mac's X25519 public key) and the one-time secret, never the six digits; the Mac's key is pinned on the iPhone from it, so the Obysphere passes sealed frames and never sees a request, an answer, a device token or the secret. Nothing in the page can make main sign an invitation: only Pair in main's own dialog does. obid's API is unchanged.

## Acceptance scenarios

- **PAIRING-1** In an Obysphere, Pair a device shows a QR code that holds `{"url", "link", "secret"}` as the Swift Mac app's did, the link an object with its relay, and never the code; ASCII only, so a Mac's name with a curly apostrophe or accents reads back the same on the iPhone. (`pairing_1`)
- **PAIRING-2** The invitation is signed by this Mac's registered device key only after Pair in main's own dialog, so the phone it pairs registers its own key and can Confirm and publish. (Real Mac: pair, then Confirm a delete on the iPhone.)
- **PAIRING-3** Not in an Obysphere, the group says so and offers no button. (Real Mac.)
- **PAIRING-4** The iPhone scans the QR code, pairs through the Obysphere, and shows the Mac's conversations and tasks on cellular, Wi-Fi off. (Real iPhone, TestFlight.)
- **PAIRING-5** The paired iPhone shows in Paired devices; Revoke ends its session and it must pair again. (Real iPhone.)
