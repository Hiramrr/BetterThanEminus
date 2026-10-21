import WidgetKit

struct AgendaEntry: TimelineEntry {
    let date: Date
    let snapshot: AgendaSnapshot?

    var upcoming: [AgendaTask] { AgendaSelection.upcoming(snapshot, now: date) }
    var dueSoonCount: Int { AgendaSelection.dueSoonCount(snapshot, now: date) }
}

/// Los widgets no hablan con Eminus: leen la agenda que guardó la app.
/// La línea de tiempo cambia cuando vence una entrega o cuando una pasa a
/// quedar a menos de 24 h o de 2 h, así el widget se ve al día aunque la app
/// lleve horas cerrada.
struct AgendaProvider: TimelineProvider {
    func placeholder(in context: Context) -> AgendaEntry {
        AgendaEntry(date: .now, snapshot: .sample)
    }

    func getSnapshot(in context: Context, completion: @escaping (AgendaEntry) -> Void) {
        let stored = AgendaStore.load()
        let useSample = context.isPreview && AgendaSelection.upcoming(stored).isEmpty
        completion(AgendaEntry(date: .now, snapshot: useSample ? .sample : stored))
    }

    func getTimeline(in context: Context, completion: @escaping (Timeline<AgendaEntry>) -> Void) {
        let snapshot = AgendaStore.load()
        let dates = Self.entryDates(for: snapshot, now: .now)
        let entries = dates.map { AgendaEntry(date: $0, snapshot: snapshot) }
        // Después de la última fecha se pide otra línea de tiempo; sin
        // pendientes basta con revisar cada pocas horas.
        let policy: TimelineReloadPolicy = dates.count > 1 ? .atEnd : .after(.now.addingTimeInterval(4 * 3600))
        completion(Timeline(entries: entries, policy: policy))
    }

    static func entryDates(for snapshot: AgendaSnapshot?, now: Date, limit: Int = 40) -> [Date] {
        var dates: Set<Date> = [now]
        for task in AgendaSelection.upcoming(snapshot, now: now).prefix(12) {
            for offset in [0, -2 * 3600.0, -24 * 3600.0] {
                let d = task.deadline.addingTimeInterval(offset)
                if d > now { dates.insert(d) }
            }
        }
        return Array(dates.sorted().prefix(limit))
    }
}

extension AgendaSnapshot {
    /// Datos de ejemplo para la galería de widgets.
    static var sample: AgendaSnapshot {
        let now = Date.now
        func task(_ id: String, _ title: String, _ course: String, hours: Double) -> AgendaTask {
            AgendaTask(id: id, title: title, course: course, deadline: now.addingTimeInterval(hours * 3600), target: "{}")
        }
        return AgendaSnapshot(
            tasks: [
                task("1", "Reporte de práctica 3", "Estructuras de datos", hours: 5),
                task("2", "Cuestionario unidad 2", "Cálculo integral", hours: 27),
                task("3", "Ensayo final", "Ética profesional", hours: 75),
                task("4", "Mapa conceptual", "Redes", hours: 120),
            ],
            updatedAt: now,
            account: ""
        )
    }
}
