import ActivityKit
import SwiftUI
import WidgetKit

/// La próxima entrega en la pantalla bloqueada y en la Dynamic Island.
struct DeliveryLiveActivity: Widget {
    var body: some WidgetConfiguration {
        ActivityConfiguration(for: DeliveryActivityAttributes.self) { context in
            DeliveryLockScreenView(state: context.state, isStale: context.isStale)
                .environment(\.locale, DueFormat.locale)
                .activityBackgroundTint(WidgetPalette.background)
                .activitySystemActionForegroundColor(WidgetPalette.text)
                .widgetURL(context.state.link)
        } dynamicIsland: { context in
            let state = context.state
            let expired = context.isStale || state.deadline <= .now
            return DynamicIsland {
                DynamicIslandExpandedRegion(.leading) {
                    Label("Entrega", systemImage: "hourglass")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .padding(.leading, 4)
                }
                DynamicIslandExpandedRegion(.trailing) {
                    Countdown(deadline: state.deadline, expired: expired)
                        .font(.title3.weight(.semibold).monospacedDigit())
                        .foregroundStyle(expired ? WidgetPalette.urgent : WidgetPalette.accent)
                        .multilineTextAlignment(.trailing)
                        .frame(maxWidth: 110, alignment: .trailing)
                        .padding(.trailing, 4)
                }
                DynamicIslandExpandedRegion(.bottom) {
                    VStack(alignment: .leading, spacing: 2) {
                        Text(state.title)
                            .font(.headline)
                            .lineLimit(2)
                        Text(subtitle(state))
                            .font(.caption)
                            .foregroundStyle(.secondary)
                            .lineLimit(1)
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, 4)
                }
            } compactLeading: {
                Image(systemName: "hourglass")
                    .foregroundStyle(WidgetPalette.accent)
            } compactTrailing: {
                Countdown(deadline: state.deadline, expired: expired)
                    .font(.caption.weight(.semibold).monospacedDigit())
                    .multilineTextAlignment(.trailing)
                    .frame(width: 56, alignment: .trailing)
            } minimal: {
                Image(systemName: expired ? "exclamationmark" : "hourglass")
                    .foregroundStyle(expired ? WidgetPalette.urgent : WidgetPalette.accent)
            }
            .widgetURL(state.link)
            .keylineTint(WidgetPalette.accent)
        }
    }

    private func subtitle(_ state: DeliveryActivityAttributes.ContentState) -> String {
        "\(state.course) · vence \(DueFormat.label(state.deadline, now: .now))"
    }
}

/// Cuenta regresiva que el sistema actualiza solo; al vencer dice "Venció".
private struct Countdown: View {
    let deadline: Date
    let expired: Bool

    var body: some View {
        if expired {
            Text("Venció")
        } else {
            Text(timerInterval: Date.now...deadline, countsDown: true)
        }
    }
}

private struct DeliveryLockScreenView: View {
    let state: DeliveryActivityAttributes.ContentState
    let isStale: Bool

    var body: some View {
        let expired = isStale || state.deadline <= .now
        VStack(alignment: .leading, spacing: 6) {
            HStack(alignment: .firstTextBaseline) {
                Text(state.course)
                    .font(.caption)
                    .foregroundStyle(WidgetPalette.secondary)
                    .lineLimit(1)
                Spacer()
                Text("vence \(DueFormat.label(state.deadline, now: .now))")
                    .font(.caption.monospacedDigit())
                    .foregroundStyle(WidgetPalette.secondary)
            }
            Text(state.title)
                .font(.headline)
                .foregroundStyle(WidgetPalette.text)
                .lineLimit(2)
            HStack(alignment: .firstTextBaseline) {
                Countdown(deadline: state.deadline, expired: expired)
                    .font(.title2.weight(.semibold).monospacedDigit())
                    .foregroundStyle(expired ? WidgetPalette.urgent : WidgetPalette.text)
                Spacer()
                if state.moreSoon > 0 {
                    Text(state.moreSoon == 1 ? "1 entrega más en 8 h" : "\(state.moreSoon) entregas más en 8 h")
                        .font(.caption)
                        .foregroundStyle(WidgetPalette.secondary)
                }
            }
            if !expired {
                ProgressView(timerInterval: state.deadline.addingTimeInterval(-AgendaSelection.liveActivityWindow)...state.deadline, countsDown: true) {
                    EmptyView()
                } currentValueLabel: {
                    EmptyView()
                }
                .progressViewStyle(.linear)
                .tint(Urgency(state.deadline, now: .now).color)
            }
        }
        .padding(16)
    }
}

#Preview("Bloqueo", as: .content, using: DeliveryActivityAttributes()) {
    DeliveryLiveActivity()
} contentStates: {
    DeliveryActivityAttributes.ContentState(taskId: "1", title: "Reporte de práctica 3", course: "Estructuras de datos", deadline: .now.addingTimeInterval(3 * 3600), target: "{}", moreSoon: 1)
}

#Preview("Isla", as: .dynamicIsland(.expanded), using: DeliveryActivityAttributes()) {
    DeliveryLiveActivity()
} contentStates: {
    DeliveryActivityAttributes.ContentState(taskId: "1", title: "Reporte de práctica 3", course: "Estructuras de datos", deadline: .now.addingTimeInterval(3 * 3600), target: "{}", moreSoon: 0)
}
