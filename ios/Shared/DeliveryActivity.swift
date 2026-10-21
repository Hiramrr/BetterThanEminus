import Foundation
#if canImport(ActivityKit)
import ActivityKit

/// Live Activity de la próxima entrega. La app la inicia y actualiza
/// (`LiveActivityManager`); el widget la dibuja en la pantalla bloqueada y
/// en la Dynamic Island.
struct DeliveryActivityAttributes: ActivityAttributes {
    struct ContentState: Codable, Hashable {
        var taskId: String
        var title: String
        var course: String
        var deadline: Date
        var target: String
        /// Otras entregas que vencen en las próximas 8 h.
        var moreSoon: Int
    }
}

extension DeliveryActivityAttributes.ContentState {
    init(_ task: AgendaTask, moreSoon: Int) {
        self.init(taskId: task.id, title: task.title, course: task.course, deadline: task.deadline, target: task.target, moreSoon: moreSoon)
    }

    var link: URL { AgendaLink.activity(target: target).url }
}
#endif
