import Foundation

@main
struct ReminderChecks {
    static func main() {
        let now = Date(timeIntervalSince1970: 1_000_000)
        func task(_ id: String, _ offset: Double) -> AgendaTask {
            AgendaTask(id: id, title: "Reporte", course: "Física", deadline: now.addingTimeInterval(offset), target: "{}")
        }
        let plan = DeadlineReminder.plan(tasks: [task("past", -1), task("tomorrow", 90000), task("soon", 1200)], now: now)
        assert(plan.count == 4)
        assert(plan.allSatisfy { $0.date > now })
        assert(plan.first?.id == "bte-task-soon-600")
        assert(plan.map(\.date) == plan.map(\.date).sorted())
        let many = DeadlineReminder.plan(tasks: (1...100).map { task(String($0), Double($0) * 3600 + 86400) }, now: now)
        assert(many.count == 60)
        assert(Set(many.map(\.id)).count == 60)
        print("Recordatorios: fechas, vencidas, orden y límite de 60 correctos")
    }
}
