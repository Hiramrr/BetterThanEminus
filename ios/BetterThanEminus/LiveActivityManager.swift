import ActivityKit
import UIKit

/// Mantiene una sola Live Activity con la entrega que vence en las próximas
/// 8 horas. Se llama cada vez que cambia la agenda (al abrir la app, al
/// recibir pendientes de la página y desde la actualización en segundo plano).
///
/// - Con `enabled == false`, sin agenda o sin entregas en 8 h, termina la actividad.
/// - Si ya hay una actividad, la actualiza aunque la app esté en segundo plano.
/// - Sólo inicia una actividad nueva con la app en primer plano: iOS no deja
///   pedirla desde segundo plano.
@MainActor
enum LiveActivityManager {
    static func sync(snapshot: AgendaSnapshot?, enabled: Bool) async {
        let now = Date()
        let running = Activity<DeliveryActivityAttributes>.activities.filter {
            $0.activityState == .active || $0.activityState == .stale
        }

        guard enabled, let task = AgendaSelection.liveActivityTask(snapshot, now: now) else {
            for activity in running { await end(activity) }
            return
        }

        let soon = AgendaSelection.upcoming(snapshot, now: now)
            .filter { $0.deadline.timeIntervalSince(now) <= AgendaSelection.liveActivityWindow }
        let state = DeliveryActivityAttributes.ContentState(task, moreSoon: max(0, soon.count - 1))
        let content = ActivityContent(state: state, staleDate: task.deadline, relevanceScore: 100)

        // Una sola actividad: se conserva la más reciente y se cierran las demás.
        if let current = running.last {
            for extra in running.dropLast() { await end(extra) }
            if current.content.state != state || current.activityState == .stale {
                await current.update(content)
            }
            return
        }

        guard UIApplication.shared.applicationState == .active,
              ActivityAuthorizationInfo().areActivitiesEnabled else { return }
        do {
            _ = try Activity.request(attributes: DeliveryActivityAttributes(), content: content, pushType: nil)
        } catch {
            // Sin permiso o con demasiadas actividades: el widget sigue mostrando la agenda.
        }
    }

    private static func end(_ activity: Activity<DeliveryActivityAttributes>) async {
        await activity.end(nil, dismissalPolicy: .immediate)
    }
}
