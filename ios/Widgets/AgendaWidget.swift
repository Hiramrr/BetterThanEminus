import SwiftUI
import WidgetKit

struct AgendaWidget: Widget {
    var body: some WidgetConfiguration {
        StaticConfiguration(kind: "mx.hiram.BetterThanEminus.agenda", provider: AgendaProvider()) { entry in
            AgendaWidgetView(entry: entry)
        }
        .configurationDisplayName("Pendientes")
        .description("Las próximas entregas de Eminus.")
        .supportedFamilies([.systemSmall, .systemMedium, .accessoryRectangular, .accessoryCircular, .accessoryInline])
    }
}

struct AgendaWidgetView: View {
    @Environment(\.widgetFamily) private var family
    let entry: AgendaEntry

    var body: some View {
        Group {
            switch family {
            case .systemSmall:
                SmallAgendaView(entry: entry).widgetBackground()
            case .systemMedium:
                MediumAgendaView(entry: entry).widgetBackground()
            case .accessoryRectangular:
                RectangularAgendaView(entry: entry).accessoryBackground()
            case .accessoryCircular:
                CircularAgendaView(entry: entry).accessoryBackground()
            case .accessoryInline:
                InlineAgendaView(entry: entry).accessoryBackground()
            default:
                SmallAgendaView(entry: entry).widgetBackground()
            }
        }
        .environment(\.locale, DueFormat.locale)
    }
}

private extension View {
    func widgetBackground() -> some View {
        foregroundStyle(WidgetPalette.text)
            .containerBackground(for: .widget) { WidgetPalette.background }
    }

    func accessoryBackground() -> some View {
        containerBackground(for: .widget) { Color.clear }
    }
}

/// Sin agenda guardada, o sin pendientes por entregar.
private struct EmptyAgendaText: View {
    let entry: AgendaEntry

    var body: some View {
        Text(entry.snapshot == nil ? "Abre Eminus para cargar tus pendientes." : "No tienes entregas pendientes.")
            .font(.footnote)
            .foregroundStyle(WidgetPalette.secondary)
    }
}

// MARK: - Pantalla de inicio

private struct SmallAgendaView: View {
    let entry: AgendaEntry

    var body: some View {
        let tasks = entry.upcoming
        VStack(alignment: .leading, spacing: 4) {
            Text("Próxima entrega")
                .font(.caption2)
                .foregroundStyle(WidgetPalette.secondary)
            if let task = tasks.first {
                Text(task.course)
                    .font(.caption2)
                    .foregroundStyle(WidgetPalette.secondary)
                    .lineLimit(1)
                Text(task.title)
                    .font(.subheadline.weight(.semibold))
                    .lineLimit(3)
                Spacer(minLength: 0)
                DueText(deadline: task.deadline, now: entry.date)
                    .font(.footnote.weight(.medium).monospacedDigit())
                    .foregroundStyle(Urgency(task.deadline, now: entry.date).color)
                    .lineLimit(1)
                if tasks.count > 1 {
                    Text(tasks.count == 2 ? "1 más pendiente" : "\(tasks.count - 1) más pendientes")
                        .font(.caption2)
                        .foregroundStyle(WidgetPalette.secondary)
                }
            } else {
                Spacer(minLength: 0)
                EmptyAgendaText(entry: entry)
                Spacer(minLength: 0)
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .widgetURL((tasks.first.map(AgendaLink.activity) ?? .agenda).url)
    }
}

private struct MediumAgendaView: View {
    let entry: AgendaEntry
    private let rows = 3

    var body: some View {
        let tasks = entry.upcoming
        VStack(alignment: .leading, spacing: 6) {
            HStack(alignment: .firstTextBaseline) {
                Text("Pendientes")
                    .font(.subheadline.weight(.semibold))
                if !tasks.isEmpty {
                    Text("\(tasks.count)")
                        .font(.subheadline.monospacedDigit())
                        .foregroundStyle(WidgetPalette.secondary)
                }
                Spacer()
                if let updated = entry.snapshot?.updatedAt {
                    Text(DueFormat.updated(updated, now: entry.date))
                        .font(.caption2)
                        .foregroundStyle(WidgetPalette.secondary)
                }
            }
            if tasks.isEmpty {
                Spacer(minLength: 0)
                EmptyAgendaText(entry: entry)
                Spacer(minLength: 0)
            } else {
                ForEach(Array(tasks.prefix(rows).enumerated()), id: \.element.id) { index, task in
                    if index > 0 {
                        Rectangle().fill(WidgetPalette.separator).frame(height: 1)
                    }
                    Link(destination: AgendaLink.activity(task).url) {
                        AgendaRow(task: task, now: entry.date)
                    }
                }
                Spacer(minLength: 0)
                if tasks.count > rows {
                    Text("\(tasks.count - rows) más en la agenda")
                        .font(.caption2)
                        .foregroundStyle(WidgetPalette.secondary)
                }
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .widgetURL(AgendaLink.agenda.url)
    }
}

private struct AgendaRow: View {
    let task: AgendaTask
    let now: Date

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 8) {
            VStack(alignment: .leading, spacing: 0) {
                Text(task.title)
                    .font(.footnote.weight(.semibold))
                    .lineLimit(1)
                Text(task.course)
                    .font(.caption2)
                    .foregroundStyle(WidgetPalette.secondary)
                    .lineLimit(1)
            }
            Spacer(minLength: 4)
            Text(DueFormat.label(task.deadline, now: now))
                .font(.caption.monospacedDigit())
                .foregroundStyle(Urgency(task.deadline, now: now).color)
                .lineLimit(1)
        }
        .foregroundStyle(WidgetPalette.text)
    }
}

// MARK: - Pantalla bloqueada

private struct RectangularAgendaView: View {
    let entry: AgendaEntry

    var body: some View {
        let tasks = entry.upcoming
        VStack(alignment: .leading, spacing: 0) {
            if let task = tasks.first {
                Text(task.course)
                    .font(.caption2)
                    .lineLimit(1)
                    .foregroundStyle(.secondary)
                Text(task.title)
                    .font(.headline)
                    .lineLimit(1)
                    .widgetAccentable()
                DueText(deadline: task.deadline, now: entry.date)
                    .font(.caption.monospacedDigit())
                    .lineLimit(1)
            } else {
                Text("Eminus")
                    .font(.headline)
                    .widgetAccentable()
                Text(entry.snapshot == nil ? "Abre la app para cargar tus pendientes" : "Sin entregas pendientes")
                    .font(.caption)
                    .lineLimit(2)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .widgetURL((tasks.first.map(AgendaLink.activity) ?? .agenda).url)
    }
}

/// Cuántas entregas vencen en las próximas 24 h.
private struct CircularAgendaView: View {
    let entry: AgendaEntry

    var body: some View {
        ZStack {
            AccessoryWidgetBackground()
            VStack(spacing: -2) {
                Text("\(entry.dueSoonCount)")
                    .font(.title2.weight(.semibold).monospacedDigit())
                    .widgetAccentable()
                Text("24 h")
                    .font(.caption2)
            }
        }
        .accessibilityLabel("\(entry.dueSoonCount) entregas en las próximas 24 horas")
        .widgetURL(AgendaLink.agenda.url)
    }
}

private struct InlineAgendaView: View {
    let entry: AgendaEntry

    var body: some View {
        if let task = entry.upcoming.first {
            if task.deadline.timeIntervalSince(entry.date) < 24 * 3600 {
                Text("\(task.title) en \(Text(task.deadline, style: .relative))")
            } else {
                Text("\(task.title), \(DueFormat.label(task.deadline, now: entry.date))")
            }
        } else {
            Text(entry.snapshot == nil ? "Eminus: abre la app" : "Sin entregas pendientes")
        }
    }
}

#Preview("Pequeño", as: .systemSmall) {
    AgendaWidget()
} timeline: {
    AgendaEntry(date: .now, snapshot: .sample)
    AgendaEntry(date: .now, snapshot: nil)
}

#Preview("Mediano", as: .systemMedium) {
    AgendaWidget()
} timeline: {
    AgendaEntry(date: .now, snapshot: .sample)
}

#Preview("Rectangular", as: .accessoryRectangular) {
    AgendaWidget()
} timeline: {
    AgendaEntry(date: .now, snapshot: .sample)
}
