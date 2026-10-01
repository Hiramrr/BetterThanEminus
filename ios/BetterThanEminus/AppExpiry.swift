import Foundation
import UserNotifications

/// Con una cuenta gratuita de Apple la firma de la app dura 7 días. La fecha
/// viene en el perfil incluido en el paquete; ios/dev/renew.sh la renueva
/// desde la Mac y aquí sólo se lee para avisar a tiempo.
enum AppExpiry {
    static let date: Date? = {
        guard let url = Bundle.main.url(forResource: "embedded", withExtension: "mobileprovision"),
              let data = try? Data(contentsOf: url),
              let start = data.range(of: Data("<?xml".utf8)),
              let end = data.range(of: Data("</plist>".utf8)) else { return nil }
        let plist = data.subdata(in: start.lowerBound..<end.upperBound)
        let dict = try? PropertyListSerialization.propertyList(from: plist, format: nil) as? [String: Any]
        return dict?["ExpirationDate"] as? Date
    }()

    /// Aviso local un día antes de que deje de abrir (si ya hay permiso).
    static func scheduleWarning() async {
        let center = UNUserNotificationCenter.current()
        center.removePendingNotificationRequests(withIdentifiers: ["app-expiry"])
        guard let date, await center.notificationSettings().authorizationStatus == .authorized else { return }
        let when = date.addingTimeInterval(-24 * 3600)
        guard when > Date() else { return }
        let content = UNMutableNotificationContent()
        content.title = "Eminus vence mañana"
        content.body = "Desbloquea el iPhone cerca de tu Mac (misma red o cable) para que se renueve sola."
        content.sound = .default
        let parts = Calendar.current.dateComponents([.year, .month, .day, .hour, .minute], from: when)
        let trigger = UNCalendarNotificationTrigger(dateMatching: parts, repeats: false)
        try? await center.add(UNNotificationRequest(identifier: "app-expiry", content: content, trigger: trigger))
    }
}
