// Comprobaciones de Agenda.swift. No entra en ningún target; se corre en la Mac:
//
//   cd ios && swiftc -parse-as-library Shared/Agenda.swift Shared/Checks/AgendaChecks.swift -o /tmp/agenda-checks && /tmp/agenda-checks

import Foundation

@main
enum AgendaChecks {
    static var failures = 0

    static func check(_ ok: Bool, _ name: String) {
        print(ok ? "ok    \(name)" : "FALLA \(name)")
        if !ok { failures += 1 }
    }

    static func main() {
        let now = Date(timeIntervalSince1970: 1_800_000_000)
        func task(_ id: String, hours: Double, title: String = "Tarea") -> AgendaTask {
            AgendaTask(id: id, title: title, course: " Curso ", deadline: now.addingTimeInterval(hours * 3600),
                       target: #"{"course":"Redes & más","activity":{"id":7,"courseId":3},"q":"a+b=c"}"#)
        }

        let snapshot = AgendaSnapshot(
            tasks: [task("c", hours: 30), task("a", hours: -1), task("b", hours: 7.5, title: "  "), task("c", hours: 2), task("", hours: 1)],
            updatedAt: now,
            account: "zS00000000"
        )

        let upcoming = AgendaSelection.upcoming(snapshot, now: now)
        check(upcoming.map(\.id) == ["b", "c"], "sólo pendientes futuras, sin duplicados ni ids vacíos, en orden")
        check(upcoming.first?.title == "Actividad", "título vacío se reemplaza")
        check(upcoming.first?.course == "Curso", "curso sin espacios sobrantes")

        check(AgendaSelection.liveActivityTask(snapshot, now: now)?.id == "b", "Live Activity: vence en 7.5 h")
        check(AgendaSelection.liveActivityTask(snapshot, now: now.addingTimeInterval(-3600)) == nil, "Live Activity: nada a más de 8 h")
        check(AgendaSelection.liveActivityTask(nil, now: now) == nil, "Live Activity: sin agenda")
        check(AgendaSelection.dueSoonCount(snapshot, now: now) == 1, "una entrega en 24 h")

        let t = upcoming[0]
        let url = AgendaLink.activity(t).url
        check(url.absoluteString.hasPrefix("eminus://activity?target=%7B"), "URL de actividad codificada")
        check(AgendaLink(url: url) == .activity(target: t.target), "el target regresa intacto")
        check(AgendaLink(url: AgendaLink.agenda.url) == .agenda, "eminus://agenda")
        check(AgendaLink(url: URL(string: "eminus://activity")!) == nil, "actividad sin target se rechaza")
        check(AgendaLink(url: URL(string: "https://eminus.uv.mx")!) == nil, "otro esquema se rechaza")

        let data = try! JSONEncoder().encode(snapshot)
        let json = String(decoding: data, as: UTF8.self)
        check(!json.contains("password") && !json.contains("token"), "el modelo no tiene campos de sesión")
        let back = try! JSONDecoder().decode(AgendaSnapshot.self, from: data)
        check(back.tasks == snapshot.tasks, "ida y vuelta en JSON")

        // save() guarda esto mismo; en la Mac no hay contenedor de App Group
        // y el guardado se prueba en el simulador.
        let clean = AgendaStore.sanitized(snapshot.tasks)
        check(clean.map(\.id) == ["a", "b", "c"], "lo que se guarda: con vencidas, sin duplicados, en orden")

        print(failures == 0 ? "todo bien" : "\(failures) fallas")
        exit(failures == 0 ? 0 : 1)
    }
}
