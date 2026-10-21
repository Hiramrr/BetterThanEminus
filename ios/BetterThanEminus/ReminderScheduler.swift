import Foundation
import UserNotifications

struct DeadlineReminder {
    let id: String
    let date: Date
    let title: String
    let body: String
    let target: String

    static func plan(tasks: [AgendaTask], now: Date = Date()) -> [DeadlineReminder] {
        AgendaStore.sanitized(tasks).flatMap { task in
            [(86400.0, "Vence en un día"), (7200.0, "Vence en 2 horas"), (600.0, "Vence en 10 minutos")].compactMap { offset, label in
                let date = task.deadline.addingTimeInterval(-offset)
                guard date > now else { return nil }
                return DeadlineReminder(id: "bte-task-\(task.id)-\(Int(offset))", date: date,
                                        title: task.title, body: "\(label) · \(task.course)", target: task.target)
            }
        }.sorted { $0.date < $1.date }.prefix(60).map { $0 }
    }
}

@MainActor
enum ReminderScheduler {
    static var enabled: Bool {
        get { UserDefaults.standard.bool(forKey: "bte.reminders.enabled") }
        set { UserDefaults.standard.set(newValue, forKey: "bte.reminders.enabled") }
    }
    private static var generation = 0
    private static var work: Task<Int, Never>?

    static func synchronize(tasks: [AgendaTask]) async -> Int {
        generation += 1
        let current = generation
        let previous = work
        previous?.cancel()
        _ = await previous?.value
        guard current == generation else { return 0 }
        let task = Task { await replace(tasks: tasks) }
        work = task
        let count = await task.value
        if current == generation { work = nil }
        return count
    }

    private static func replace(tasks: [AgendaTask]) async -> Int {
        let center = UNUserNotificationCenter.current()
        let old = await center.pendingNotificationRequests().map(\.identifier).filter { $0.hasPrefix("bte-") }
        guard !Task.isCancelled else { return 0 }
        center.removePendingNotificationRequests(withIdentifiers: old)
        let status = await center.notificationSettings().authorizationStatus
        guard !Task.isCancelled, enabled, status == .authorized || status == .provisional else { return 0 }
        var count = 0
        for reminder in DeadlineReminder.plan(tasks: tasks) {
            guard !Task.isCancelled else { break }
            let content = UNMutableNotificationContent()
            content.title = reminder.title
            content.body = reminder.body
            content.sound = .default
            content.threadIdentifier = "eminus-deliveries"
            content.userInfo = ["target": reminder.target]
            let trigger = UNTimeIntervalNotificationTrigger(timeInterval: max(1, reminder.date.timeIntervalSinceNow), repeats: false)
            do {
                try await center.add(UNNotificationRequest(identifier: reminder.id, content: content, trigger: trigger))
                if Task.isCancelled {
                    center.removePendingNotificationRequests(withIdentifiers: [reminder.id])
                    break
                }
                count += 1
            } catch {
                UserDefaults.standard.set("No se pudo programar un recordatorio.", forKey: "bte.sync.error")
            }
        }
        return count
    }
}
