import UIKit
import UserNotifications

@main
final class AppDelegate: UIResponder, UIApplicationDelegate, UNUserNotificationCenterDelegate {
    func application(_ application: UIApplication, didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]?) -> Bool {
        UNUserNotificationCenter.current().delegate = self
        ProfileStore.enableBackgroundAccess()
        AgendaSync.register()
        return true
    }

    // Los avisos de entrega también se muestran con la app abierta.
    func userNotificationCenter(_ center: UNUserNotificationCenter, willPresent notification: UNNotification) async -> UNNotificationPresentationOptions {
        [.banner, .sound]
    }

    // Tocar un aviso abre la actividad en Eminus.
    func userNotificationCenter(_ center: UNUserNotificationCenter, didReceive response: UNNotificationResponse) async {
        guard let target = response.notification.request.content.userInfo["target"] as? String else { return }
        await MainActor.run {
            AppRoute.open(.activity(target: target))
        }
    }
}

final class SceneDelegate: UIResponder, UIWindowSceneDelegate {
    var window: UIWindow?

    func scene(_ scene: UIScene, willConnectTo session: UISceneSession, options connectionOptions: UIScene.ConnectionOptions) {
        guard let windowScene = scene as? UIWindowScene else { return }
        let window = UIWindow(windowScene: windowScene)
        window.rootViewController = EminusViewController()
        window.makeKeyAndVisible()
        self.window = window
        if let url = connectionOptions.urlContexts.first?.url, let link = AgendaLink(url: url) { AppRoute.open(link) }
        if let target = connectionOptions.notificationResponse?.notification.request.content.userInfo["target"] as? String {
            AppRoute.open(.activity(target: target))
        }
    }

    func scene(_ scene: UIScene, openURLContexts URLContexts: Set<UIOpenURLContext>) {
        if let url = URLContexts.first?.url, let link = AgendaLink(url: url) { AppRoute.open(link) }
    }

    func sceneDidEnterBackground(_ scene: UIScene) { AgendaSync.schedule() }

    func sceneDidBecomeActive(_ scene: UIScene) {
        ProfileStore.enableBackgroundAccess()
        Task {
            await LiveActivityManager.sync(snapshot: AgendaStore.load(), enabled: AgendaSync.liveEnabled)
            if (AgendaStore.load()?.updatedAt.timeIntervalSinceNow ?? -.infinity) < -600 { _ = await AgendaSync.refresh() }
        }
    }
}

extension Notification.Name {
    static let bteOpenTarget = Notification.Name("bteOpenTarget")
}

@MainActor
enum AppRoute {
    static var pending: AgendaLink?

    static func open(_ link: AgendaLink) {
        if case .activity(let target) = link, validatedTarget(target) == nil { return }
        pending = link
        NotificationCenter.default.post(name: .bteOpenTarget, object: nil)
    }

    static func validatedTarget(_ json: String) -> [String: Any]? {
        guard json.utf8.count <= 4096, let data = json.data(using: .utf8),
              let target = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let activity = target["activity"] as? [String: Any],
              let id = positiveID(activity["id"]),
              let courseID = positiveID(activity["courseId"]),
              let course = target["course"] as? String, course.count <= 500 else { return nil }
        return ["course": course, "section": "activities", "activity": ["id": id, "courseId": courseID]]
    }

    nonisolated static func positiveID(_ value: Any?) -> String? {
        let text = (value as? String) ?? (value as? NSNumber)?.stringValue ?? ""
        guard text.range(of: #"^[0-9]{1,19}$"#, options: .regularExpression) != nil,
              let number = Int64(text), number > 0 else { return nil }
        return text
    }
}
