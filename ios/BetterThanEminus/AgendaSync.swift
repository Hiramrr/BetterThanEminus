import BackgroundTasks
import Foundation
import UIKit
import UserNotifications
import WidgetKit

@MainActor
enum AgendaSync {
    static let identifier = "mx.hiram.BetterThanEminus.refresh"
    static var liveEnabled: Bool {
        get { UserDefaults.standard.object(forKey: "bte.live.enabled") as? Bool ?? true }
        set { UserDefaults.standard.set(newValue, forKey: "bte.live.enabled") }
    }
    static var account: String { UserDefaults.standard.string(forKey: "bte.agenda.account") ?? "" }
    private static var revision = UUID()
    private static var accountChange = UUID()
    private static var refreshTask: Task<Bool, Never>?

    static func register() {
        BGTaskScheduler.shared.register(forTaskWithIdentifier: identifier, using: .main) { task in
            Task { @MainActor in
                schedule()
                let work = Task { await refresh() }
                task.expirationHandler = { work.cancel() }
                let success = await work.value
                task.setTaskCompleted(success: success)
            }
        }
    }

    static func schedule() {
        guard !account.isEmpty else { return }
        let request = BGAppRefreshTaskRequest(identifier: identifier)
        request.earliestBeginDate = Date(timeIntervalSinceNow: 30 * 60)
        do { try BGTaskScheduler.shared.submit(request) }
        catch { UserDefaults.standard.set("iOS no permitió programar la actualización en segundo plano.", forKey: "bte.sync.error") }
    }

    static func setAccount(_ value: String) async {
        let change = UUID()
        accountChange = change
        if value != account { await clearAgenda() }
        guard change == accountChange else { return }
        UserDefaults.standard.set(value, forKey: "bte.agenda.account")
        schedule()
    }

    @discardableResult
    static func accept(_ snapshot: AgendaSnapshot) async -> Bool {
        guard !snapshot.account.isEmpty, snapshot.account == account else { return false }
        if let stored = AgendaStore.load(), stored.account == snapshot.account, stored.updatedAt > snapshot.updatedAt { return true }
        guard AgendaStore.save(snapshot) else {
            UserDefaults.standard.set("No se pudo guardar la agenda en este iPhone.", forKey: "bte.sync.error")
            return false
        }
        UserDefaults.standard.removeObject(forKey: "bte.sync.error")
        _ = await ReminderScheduler.synchronize(tasks: snapshot.tasks)
        guard snapshot.account == account else { return false }
        await LiveActivityManager.sync(snapshot: snapshot, enabled: liveEnabled)
        return true
    }

    static func clear() async {
        accountChange = UUID()
        await clearAgenda()
    }

    private static func clearAgenda() async {
        let change = accountChange
        revision = UUID()
        refreshTask?.cancel()
        refreshTask = nil
        UserDefaults.standard.removeObject(forKey: "bte.agenda.account")
        BGTaskScheduler.shared.cancel(taskRequestWithIdentifier: identifier)
        AgendaStore.clear()
        _ = await ReminderScheduler.synchronize(tasks: [])
        guard change == accountChange else { return }
        let delivered = await UNUserNotificationCenter.current().deliveredNotifications()
        guard change == accountChange else { return }
        UNUserNotificationCenter.current().removeDeliveredNotifications(withIdentifiers: delivered.map(\.request.identifier).filter { $0.hasPrefix("bte-") })
        await LiveActivityManager.sync(snapshot: nil, enabled: false)
    }

    static func refresh() async -> Bool {
        if let refreshTask {
            return await withTaskCancellationHandler { await refreshTask.value } onCancel: { refreshTask.cancel() }
        }
        let current = revision
        let owner = account
        guard !owner.isEmpty else { return false }
        let task = Task { @MainActor () -> Bool in
            do {
                var token = try await SessionService.shared.validToken()
                guard SessionService.account(of: token) == owner else { return false }
                let tasks: [AgendaTask]
                do { tasks = try await fetchTasks(token: token) }
                catch SyncError.unauthorized {
                    token = try await SessionService.shared.validToken(forceRefresh: true, rejectedToken: token)
                    guard SessionService.account(of: token) == owner else { return false }
                    tasks = try await fetchTasks(token: token)
                }
                try Task.checkCancellation()
                guard revision == current, account == owner else { return false }
                return await accept(AgendaSnapshot(tasks: tasks, updatedAt: Date(), account: owner))
            } catch is CancellationError {
                return false
            } catch {
                guard revision == current, !Task.isCancelled else { return false }
                UserDefaults.standard.set("No se pudo actualizar. Se conserva la última agenda y sus recordatorios.", forKey: "bte.sync.error")
                return false
            }
        }
        refreshTask = task
        let result = await withTaskCancellationHandler { await task.value } onCancel: { task.cancel() }
        if revision == current { refreshTask = nil }
        return result
    }

    private enum SyncError: Error { case unauthorized, forbidden, invalidResponse }
    private static func rows(_ path: String, token: String) async throws -> [[String: Any]] {
        var request = URLRequest(url: URL(string: "https://eminus.uv.mx/eminusapi8/api" + path)!)
        request.timeoutInterval = 20
        request.setValue("Bearer " + token, forHTTPHeaderField: "Authorization")
        let (data, response) = try await URLSession.shared.data(for: request)
        guard let response = response as? HTTPURLResponse else { throw SyncError.invalidResponse }
        if response.statusCode == 401 { throw SyncError.unauthorized }
        if response.statusCode == 403 { throw SyncError.forbidden }
        guard response.statusCode == 200 else { throw SyncError.invalidResponse }
        let json = try JSONSerialization.jsonObject(with: data)
        if let rows = json as? [[String: Any]] { return rows }
        if let rows = (json as? [String: Any])?["contenido"] as? [[String: Any]] { return rows }
        throw SyncError.invalidResponse
    }

    private static func fetchTasks(token: String) async throws -> [AgendaTask] {
        let now = Date().timeIntervalSince1970
        let courses = try await rows("/Course/getAllCourses", token: token).compactMap { row -> (String, String)? in
            let course = row["curso"] as? [String: Any] ?? row
            let end = (course["fechaTerminoEpoch"] as? NSNumber)?.doubleValue ?? 0
            guard (course["archivado"] as? NSNumber)?.boolValue != true, end == 0 || end >= now,
                  let id = AppRoute.positiveID(course["idCurso"]) else { return nil }
            return (id, course["nombre"] as? String ?? "Curso")
        }
        var tasks: [AgendaTask] = []
        for batch in stride(from: 0, to: courses.count, by: 6) {
            try Task.checkCancellation()
            let part = Array(courses[batch..<min(batch + 6, courses.count)])
            let results = try await withThrowingTaskGroup(of: [AgendaTask].self) { group in
                for (courseID, course) in part {
                    group.addTask {
                        let activities: [[String: Any]]
                        do { activities = try await rows("/Activity/getActividadesEstudiante/" + courseID, token: token) }
                        catch SyncError.forbidden { return [] }
                        return activities.compactMap { raw in
                            let delivery = raw["fechaEntrega"] as? String ?? ""
                            let delivered = !delivery.isEmpty && !delivery.hasPrefix("0001") && delivery != (raw["fechaTermino"] as? String ?? "")
                            let extensionDate = (raw["fechaProrrogaEpoch"] as? NSNumber)?.doubleValue ?? 0
                            let deadline = extensionDate > 0 ? extensionDate : (raw["fechaTerminoEpoch"] as? NSNumber)?.doubleValue ?? 0
                            guard !delivered, deadline > now - 21 * 86400, (raw["visible"] as? NSNumber)?.intValue != 0,
                                  let id = AppRoute.positiveID(raw["idActividad"]) else { return nil }
                            let target: [String: Any] = ["course": course, "section": "activities", "activity": ["id": id, "courseId": Int(courseID) ?? 0]]
                            guard let data = try? JSONSerialization.data(withJSONObject: target), let targetString = String(data: data, encoding: .utf8) else { return nil }
                            return AgendaTask(id: courseID + "-" + id, title: raw["titulo"] as? String ?? "Actividad", course: course,
                                              deadline: Date(timeIntervalSince1970: deadline), target: targetString)
                        }
                    }
                }
                var results: [AgendaTask] = []
                for try await result in group { results += result }
                return results
            }
            tasks += results
        }
        return tasks.sorted { $0.deadline < $1.deadline }
    }
}
