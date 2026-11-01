import SwiftUI

struct StopButton: View {
    let action: () -> Void
    @State private var hovering = false

    var body: some View {
        Button(action: action) {
            Image(systemName: "stop.fill")
                .font(.system(size: 9, weight: .semibold))
                .foregroundStyle(hovering ? OBYStyle.primary : OBYStyle.secondary)
                .frame(width: 22, height: 22)
                .background(
                    RoundedRectangle(cornerRadius: 6, style: .continuous)
                        .stroke(hovering ? OBYStyle.primary : OBYStyle.secondary, lineWidth: 1)
                )
        }
        .buttonStyle(.plain)
        .help("Stop")
        .accessibilityLabel("Stop job")
        .onHover { hovering = $0 }
    }
}
