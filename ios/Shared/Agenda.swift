import Foundation
#if canImport(WidgetKit)
import WidgetKit
#endif

// Agenda de pendientes que comparten la app, los widgets y la Live Activity.
// Sólo guarda lo que se ve en pantalla: nada de tokens, cookies ni contraseñas.

struct AgendaTask: Codable, Identifiable, Equatable {
    var id: String
    var title: String
    var course: String
    var deadline: Date
    /// JSON que entiende `bte.openTarget` en la página (curso, sección y actividad).
    var target: String
}

struct AgendaSnapshot: Codable {
    var tasks: [AgendaTask]
    var updatedAt: Date
    var account: String
}

enum AgendaStore {
    static let appGroup = "group.mx.hiram.BetterThanEminus"
    /// Tope para que el archivo no crezca sin límite con cursos viejos.
    static let maxTasks = 60

    private static let fileName = "agenda.json"

    /// Carpeta del App Group. Es nil si la firma no trae el App Group
    /// (por ejemplo, una cuenta gratuita que no lo permite).
    static var sharedContainer: URL? {
        FileManager.default.containerURL(forSecurityApplicationGroupIdentifier: appGroup)
    }

    /// true si los widgets pueden leer lo que guarda la app.
    static var isShared: Bool { sharedContainer != nil }

    private static var fileURL: URL? {
        if let dir = sharedContainer { return dir.appendingPathComponent(fileName) }
        // Sin App Group la app sigue funcionando; los widgets muestran "abre Eminus".
        guard let dir = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first else { return nil }
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        return dir.appendingPathComponent(fileName)
    }

    @discardableResult
    static func save(_ snapshot: AgendaSnapshot) -> Bool {
        guard let url = fileURL else { return false }
        // Las vencidas hace más de un día ya no se muestran en ningún lado.
        let cutoff = Date.now.addingTimeInterval(-86400)
        let clean = AgendaSnapshot(
            tasks: Array(sanitized(snapshot.tasks).filter { $0.deadline > cutoff }.prefix(maxTasks)),
            updatedAt: snapshot.updatedAt,
            account: snapshot.account
        )
        guard let data = try? encoder.encode(clean) else { return false }
        // Protección por omisión de iOS (hasta el primer desbloqueo): los
        // widgets de la pantalla bloqueada pueden leerla con el teléfono bloqueado.
        do { try data.write(to: url, options: [.atomic, .completeFileProtectionUntilFirstUserAuthentication]) }
        catch { return false }
        reloadWidgets()
        return true
    }

    static func load() -> AgendaSnapshot? {
        guard let url = fileURL, let data = try? Data(contentsOf: url) else { return nil }
        return try? decoder.decode(AgendaSnapshot.self, from: data)
    }

    static func clear() {
        if let url = fileURL { try? FileManager.default.removeItem(at: url) }
        reloadWidgets()
    }

    /// Sin duplicados, sin campos vacíos y en orden de vencimiento.
    static func sanitized(_ tasks: [AgendaTask]) -> [AgendaTask] {
        var seen = Set<String>()
        return tasks
            .filter { !$0.id.isEmpty && seen.insert($0.id).inserted }
            .map { task in
                var t = task
                t.title = task.title.trimmingCharacters(in: .whitespacesAndNewlines)
                t.course = task.course.trimmingCharacters(in: .whitespacesAndNewlines)
                if t.title.isEmpty { t.title = "Actividad" }
                return t
            }
            .sorted { $0.deadline == $1.deadline ? $0.id < $1.id : $0.deadline < $1.deadline }
    }

    private static func reloadWidgets() {
        #if canImport(WidgetKit)
        WidgetCenter.shared.reloadAllTimelines()
        #endif
    }

    private static var encoder: JSONEncoder {
        let e = JSONEncoder()
        e.dateEncodingStrategy = .iso8601
        return e
    }

    private static var decoder: JSONDecoder {
        let d = JSONDecoder()
        d.dateDecodingStrategy = .iso8601
        return d
    }
}

/// Qué tareas mostrar. Las mismas reglas para widgets y Live Activity.
enum AgendaSelection {
    /// La Live Activity sólo aparece si la próxima entrega vence en este plazo.
    static let liveActivityWindow: TimeInterval = 8 * 3600

    /// Pendientes que todavía no vencen, de la más próxima a la más lejana.
    static func upcoming(_ snapshot: AgendaSnapshot?, now: Date = .now) -> [AgendaTask] {
        guard let snapshot else { return [] }
        return AgendaStore.sanitized(snapshot.tasks).filter { $0.deadline > now }
    }

    /// La entrega que va en la Live Activity, o nil si ninguna vence en 8 h.
    static func liveActivityTask(_ snapshot: AgendaSnapshot?, now: Date = .now) -> AgendaTask? {
        guard let next = upcoming(snapshot, now: now).first,
              next.deadline.timeIntervalSince(now) <= liveActivityWindow else { return nil }
        return next
    }

    /// Cuántas vencen antes de `interval` (por omisión, 24 h).
    static func dueSoonCount(_ snapshot: AgendaSnapshot?, within interval: TimeInterval = 24 * 3600, now: Date = .now) -> Int {
        upcoming(snapshot, now: now).filter { $0.deadline.timeIntervalSince(now) <= interval }.count
    }
}

/// Enlaces `eminus://` que abren la app.
///
/// - `eminus://agenda` abre la lista de pendientes.
/// - `eminus://activity?target=<JSON>` abre una actividad; `target` va
///   codificado con porcentajes y es el mismo JSON de `AgendaTask.target`.
enum AgendaLink: Equatable {
    case agenda
    case activity(target: String)

    static let scheme = "eminus"

    var url: URL {
        var c = URLComponents()
        c.scheme = Self.scheme
        switch self {
        case .agenda:
            c.host = "agenda"
        case .activity(let target):
            c.host = "activity"
            // queryItems deja pasar "+", "&" y "="; se codifica a mano para
            // que el JSON regrese intacto.
            c.percentEncodedQuery = "target=" + Self.encode(target)
        }
        return c.url ?? URL(string: "eminus://agenda")!
    }

    init?(url: URL) {
        guard url.scheme?.lowercased() == Self.scheme,
              let c = URLComponents(url: url, resolvingAgainstBaseURL: false) else { return nil }
        switch c.host?.lowercased() {
        case "agenda":
            self = .agenda
        case "activity":
            guard let target = c.queryItems?.first(where: { $0.name == "target" })?.value,
                  !target.isEmpty else { return nil }
            self = .activity(target: target)
        default:
            return nil
        }
    }

    static func activity(_ task: AgendaTask) -> AgendaLink { .activity(target: task.target) }

    private static func encode(_ value: String) -> String {
        var allowed = CharacterSet.alphanumerics
        allowed.insert(charactersIn: "-._~")
        return value.addingPercentEncoding(withAllowedCharacters: allowed) ?? ""
    }
}
