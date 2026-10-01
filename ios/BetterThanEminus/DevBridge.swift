import Foundation
import WebKit

/// Sólo en desarrollo (BTE_DEV): la app pregunta a ios/dev/devserver.py si
/// hay código que evaluar en la página y le devuelve el resultado. Así se
/// inspecciona el WKWebView real del simulador desde la terminal, y
/// "__reload__" vuelve a leer el CSS/JS de la Mac sin recompilar.
@MainActor
final class DevBridge {
    private let server: URL
    private weak var webView: WKWebView?
    private let reload: () async -> Void
    private var task: Task<Void, Never>?

    init(server: URL, webView: WKWebView, reload: @escaping () async -> Void) {
        self.server = server
        self.webView = webView
        self.reload = reload
    }

    func start() {
        task = Task { [weak self] in
            while !Task.isCancelled {
                guard let self else { return }
                await self.pollOnce()
            }
        }
    }

    private func pollOnce() async {
        var request = URLRequest(url: server.appendingPathComponent("poll"))
        request.timeoutInterval = 30
        guard let (data, response) = try? await URLSession.shared.data(for: request),
              (response as? HTTPURLResponse)?.statusCode == 200,
              let job = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let id = job["id"] as? String, let code = job["code"] as? String else {
            try? await Task.sleep(nanoseconds: 700_000_000)
            return
        }
        var result: [String: Any] = ["id": id]
        if code == "__reload__" {
            await reload()
            result["value"] = "reloaded"
        } else if let webView {
            do {
                // callAsyncJavaScript: el código puede usar await y return.
                let value = try await webView.callAsyncJavaScript(code, contentWorld: .page)
                result["value"] = Self.jsonSafe(value)
            } catch {
                result["error"] = String(describing: error)
            }
        }
        var post = URLRequest(url: server.appendingPathComponent("result"))
        post.httpMethod = "POST"
        post.httpBody = try? JSONSerialization.data(withJSONObject: result)
        _ = try? await URLSession.shared.data(for: post)
    }

    private static func jsonSafe(_ value: Any?) -> Any {
        guard let value else { return NSNull() }
        if JSONSerialization.isValidJSONObject([value]) { return value }
        return String(describing: value)
    }
}
