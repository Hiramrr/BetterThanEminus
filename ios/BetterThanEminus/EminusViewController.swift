import QuickLook
import SafariServices
import UIKit
import UserNotifications
import WebKit

/// Eminus a pantalla completa con BetterThanEminus inyectado. La página va
/// de borde a borde (viewport-fit=cover); el CSS usa env(safe-area-inset-*)
/// para el dock inferior y la barra de estado queda del color del tema.
final class EminusViewController: UIViewController {
    static let home = URL(string: "https://eminus.uv.mx/eminus4/")!

    private var webView: WKWebView!
    private let refresh = UIRefreshControl()
    private var bundle: WebBundle?
    private var darkTheme = true
    private var previewURL: URL?
    private var downloads: [WKDownload: URL] = [:]
    private var dev: DevBridge?
    private var stash: Stash?

    override var preferredStatusBarStyle: UIStatusBarStyle { darkTheme ? .lightContent : .darkContent }

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = UIColor(red: 0x1d / 255, green: 0x20 / 255, blue: 0x21 / 255, alpha: 1)

        let config = WKWebViewConfiguration()
        config.websiteDataStore = .default() // la sesión y los borradores se quedan entre aperturas
        config.allowsInlineMediaPlayback = true
        config.defaultWebpagePreferences.preferredContentMode = .mobile
        config.userContentController.addScriptMessageHandler(BridgeProxy(self), contentWorld: .page, name: "bte")

        webView = WKWebView(frame: .zero, configuration: config)
        webView.translatesAutoresizingMaskIntoConstraints = false
        webView.isOpaque = false
        webView.backgroundColor = view.backgroundColor
        webView.scrollView.backgroundColor = view.backgroundColor
        webView.scrollView.contentInsetAdjustmentBehavior = .never
        webView.allowsBackForwardNavigationGestures = true
        webView.allowsLinkPreview = false
        webView.navigationDelegate = self
        webView.uiDelegate = self
        #if DEBUG
        if #available(iOS 16.4, *) { webView.isInspectable = true }
        #endif
        view.addSubview(webView)
        NSLayoutConstraint.activate([
            webView.topAnchor.constraint(equalTo: view.safeAreaLayoutGuide.topAnchor),
            webView.bottomAnchor.constraint(equalTo: view.bottomAnchor),
            webView.leadingAnchor.constraint(equalTo: view.leadingAnchor),
            webView.trailingAnchor.constraint(equalTo: view.trailingAnchor)
        ])

        refresh.addTarget(self, action: #selector(pullToRefresh), for: .valueChanged)
        webView.scrollView.refreshControl = refresh

        NotificationCenter.default.addObserver(forName: .bteOpenTarget, object: nil, queue: .main) { [weak self] note in
            guard let target = note.object as? String else { return }
            self?.openTarget(target)
        }

        Task { await start() }
        Task { await AppExpiry.scheduleWarning() }
    }

    private func start() async {
        let bundle = await WebBundle.load()
        self.bundle = bundle
        installScripts(bundle)
        if let server = WebBundle.devServer {
            dev = DevBridge(server: server, webView: webView) { [weak self] in await self?.reloadCode() }
            dev?.start()
        }
        webView.load(URLRequest(url: Self.home))
    }

    private func installScripts(_ bundle: WebBundle) {
        let ucc = webView.configuration.userContentController
        ucc.removeAllUserScripts()
        ucc.addUserScript(WKUserScript(source: bundle.startSource, injectionTime: .atDocumentStart, forMainFrameOnly: false, in: .page))
        ucc.addUserScript(WKUserScript(source: bundle.endSource, injectionTime: .atDocumentEnd, forMainFrameOnly: false, in: .page))
    }

    /// Desarrollo: vuelve a leer el código de la Mac y recarga.
    func reloadCode() async {
        let bundle = await WebBundle.load()
        self.bundle = bundle
        installScripts(bundle)
        webView.reload()
    }

    @objc private func pullToRefresh() {
        webView.reload()
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.6) { self.refresh.endRefreshing() }
    }

    private func openTarget(_ json: String) {
        let js = "window.bte && bte.goToTarget && bte.goToTarget(\(json));"
        webView.evaluateJavaScript(js)
    }

    fileprivate func openExternal(_ url: URL) {
        if ["http", "https"].contains(url.scheme?.lowercased() ?? "") {
            let safari = SFSafariViewController(url: url)
            safari.dismissButtonStyle = .close
            present(safari, animated: true)
        } else if UIApplication.shared.canOpenURL(url) {
            UIApplication.shared.open(url)
        }
    }

    private static func isEminus(_ url: URL?) -> Bool {
        guard let url else { return false }
        if url.scheme == "about" || url.scheme == "blob" || url.scheme == "data" { return true }
        return url.host == "eminus.uv.mx"
    }

    // MARK: puente con la página

    fileprivate func handle(_ body: Any, mainFrame: Bool) async -> Any? {
        guard let msg = body as? [String: Any], let type = msg["type"] as? String else { return nil }
        // Los perfiles sólo se manejan desde la página principal de Eminus,
        // no desde sus iframes.
        if type.hasPrefix("profile.") { return mainFrame ? await handleProfile(type, msg) : nil }
        switch type {
        case "theme":
            if let hex = msg["bg"] as? String, let color = UIColor(hex: hex) {
                view.backgroundColor = color
                webView.backgroundColor = color
                webView.scrollView.backgroundColor = color
                refresh.tintColor = (msg["fg"] as? String).flatMap(UIColor.init(hex:))
            }
            darkTheme = msg["dark"] as? Bool ?? true
            setNeedsStatusBarAppearanceUpdate()
        case "haptic":
            switch msg["style"] as? String {
            case "success": UINotificationFeedbackGenerator().notificationOccurred(.success)
            case "warning": UINotificationFeedbackGenerator().notificationOccurred(.warning)
            case "selection": UISelectionFeedbackGenerator().selectionChanged()
            default: UIImpactFeedbackGenerator(style: .light).impactOccurred()
            }
        case "refreshable":
            webView.scrollView.refreshControl = (msg["on"] as? Bool ?? true) ? refresh : nil
        case "open":
            if let raw = msg["url"] as? String, let url = URL(string: raw) { openExternal(url) }
        case "share":
            var items: [Any] = []
            if let text = msg["text"] as? String { items.append(text) }
            if let raw = msg["url"] as? String, let url = URL(string: raw) { items.append(url) }
            let sheet = UIActivityViewController(activityItems: items, applicationActivities: nil)
            sheet.popoverPresentationController?.sourceView = view
            present(sheet, animated: true)
        case "notify":
            return await scheduleReminders(msg["items"] as? [[String: Any]] ?? [], ask: msg["ask"] as? Bool ?? false)
        case "notifyStatus":
            let settings = await UNUserNotificationCenter.current().notificationSettings()
            return settings.authorizationStatus == .authorized ? "on" : settings.authorizationStatus == .denied ? "denied" : "off"
        case "version":
            return bundle?.version
        case "font":
            // Bytes de una fuente empaquetada (ABC Areal), en base64: la página
            // la registra con FontFace. Registrar la fuente en la app no basta
            // (el proceso web no la ve) y un esquema propio se bloquea en https.
            guard let name = msg["file"] as? String,
                  name.range(of: #"^[A-Za-z0-9-]+\.ttf$"#, options: .regularExpression) != nil,
                  let url = Bundle.main.resourceURL?.appendingPathComponent("fonts").appendingPathComponent(name),
                  let data = try? Data(contentsOf: url) else { return nil }
            return data.base64EncodedString()
        case "appExpiry":
            return AppExpiry.date.map { $0.timeIntervalSince1970 }
        default:
            break
        }
        return nil
    }

    // MARK: perfiles

    /// Lo que se escribió al iniciar sesión a mano, sólo en memoria, hasta
    /// saber si entró y si se quiere guardar.
    private struct Stash { let user: String; let password: String }

    private func handleProfile(_ type: String, _ msg: [String: Any]) async -> Any? {
        let user = (msg["user"] as? String ?? "").trimmingCharacters(in: .whitespaces)
        switch type {
        case "profile.list":
            return ProfileStore.list().map { ["user": $0.user, "name": $0.name] }
        case "profile.stash":
            let password = msg["pass"] as? String ?? ""
            stash = user.isEmpty || password.isEmpty ? nil : Stash(user: user, password: password)
        case "profile.stashInfo":
            // Nunca devuelve la contraseña: sólo de quién es y si ya está guardada.
            guard let stash else { return nil }
            let saved = ProfileStore.password(for: stash.user)
            return ["user": stash.user, "saved": saved != nil, "changed": saved != nil && saved != stash.password]
        case "profile.commit":
            guard let stash else { return false }
            let name = (msg["name"] as? String ?? "").trimmingCharacters(in: .whitespaces)
            let known = ProfileStore.list().first { $0.user == stash.user }?.name
            let ok = ProfileStore.save(user: stash.user, password: stash.password, name: name.isEmpty ? (known ?? stash.user) : name)
            self.stash = nil
            return ok
        case "profile.discard":
            stash = nil
        case "profile.remove":
            return ProfileStore.remove(user: user)
        case "profile.login":
            return await login(with: user)
        default:
            break
        }
        return nil
    }

    /// Llena y envía el formulario de Eminus con un perfil guardado. La
    /// contraseña pasa del Llavero al campo del formulario; no se le entrega
    /// a ningún script de la página.
    private func login(with user: String) async -> String {
        guard webView.url?.host == "eminus.uv.mx" else { return "fuera" }
        guard let password = ProfileStore.password(for: user) else { return "sin-perfil" }
        let js = """
        const auth = document.querySelector("m-auth");
        if (!auth) return "sin-login";
        const u = auth.querySelector('input[name="username"], input[type="text"]');
        const p = auth.querySelector('input[type="password"]');
        const b = auth.querySelector('button[type="submit"]');
        if (!u || !p || !b) return "sin-formulario";
        const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value").set;
        const fill = (el, value) => {
          setter.call(el, value);
          for (const type of ["input", "change", "blur"]) el.dispatchEvent(new Event(type, { bubbles: true }));
        };
        fill(u, user);
        fill(p, pass);
        await new Promise((r) => setTimeout(r, 120));
        b.click();
        return "ok";
        """
        let result = try? await webView.callAsyncJavaScript(js, arguments: ["user": user, "pass": password], in: nil, contentWorld: .page)
        return result as? String ?? "error"
    }

    /// Avisos locales antes de cada entrega pendiente. Se reemplazan todos
    /// cada vez: la vista tui manda la lista completa al actualizar.
    private func scheduleReminders(_ items: [[String: Any]], ask: Bool) async -> String {
        let center = UNUserNotificationCenter.current()
        var status = await center.notificationSettings().authorizationStatus
        if status == .notDetermined && ask {
            _ = try? await center.requestAuthorization(options: [.alert, .sound, .badge])
            status = await center.notificationSettings().authorizationStatus
        }
        guard status == .authorized else { return status == .denied ? "denied" : "off" }
        // Sólo los de entregas: el aviso de vencimiento de la app se queda.
        let old = await center.pendingNotificationRequests().map(\.identifier).filter { $0.hasPrefix("bte-") }
        center.removePendingNotificationRequests(withIdentifiers: old)
        await AppExpiry.scheduleWarning()
        var count = 0
        for item in items.prefix(60) {
            guard let id = item["id"] as? String, let at = item["at"] as? Double,
                  let title = item["title"] as? String else { continue }
            let date = Date(timeIntervalSince1970: at)
            guard date > Date() else { continue }
            let content = UNMutableNotificationContent()
            content.title = title
            content.body = item["body"] as? String ?? ""
            content.sound = .default
            if let target = item["target"] as? String { content.userInfo = ["target": target] }
            let parts = Calendar.current.dateComponents([.year, .month, .day, .hour, .minute], from: date)
            let trigger = UNCalendarNotificationTrigger(dateMatching: parts, repeats: false)
            try? await center.add(UNNotificationRequest(identifier: id, content: content, trigger: trigger))
            count += 1
        }
        return "on:\(count)"
    }
}

// MARK: navegación

extension EminusViewController: WKNavigationDelegate, WKUIDelegate, WKDownloadDelegate, QLPreviewControllerDataSource {
    func webView(_ webView: WKWebView, decidePolicyFor action: WKNavigationAction, preferences: WKWebpagePreferences) async -> (WKNavigationActionPolicy, WKWebpagePreferences) {
        if action.shouldPerformDownload { return (.download, preferences) }
        let url = action.request.url
        // Enlaces a otros sitios (Teams, YouTube, Drive…) en una hoja de
        // Safari; los iframes de otros sitios sí se cargan dentro.
        if action.targetFrame?.isMainFrame != false, !Self.isEminus(url), let url {
            openExternal(url)
            return (.cancel, preferences)
        }
        return (.allow, preferences)
    }

    func webView(_ webView: WKWebView, decidePolicyFor response: WKNavigationResponse) async -> WKNavigationResponsePolicy {
        // Archivos (PDF, Word, imágenes…) en el marco principal: se descargan
        // y se abren en vista previa con compartir, en vez de dejarte
        // atrapado en el visor sin volver a Eminus.
        guard response.isForMainFrame else { return .allow }
        let mime = response.response.mimeType ?? ""
        let disposition = (response.response as? HTTPURLResponse)?.value(forHTTPHeaderField: "Content-Disposition") ?? ""
        if disposition.lowercased().hasPrefix("attachment") || (!mime.hasPrefix("text/html") && !response.canShowMIMEType) || mime == "application/pdf" {
            return .download
        }
        return .allow
    }

    func webView(_ webView: WKWebView, navigationAction: WKNavigationAction, didBecome download: WKDownload) { download.delegate = self }
    func webView(_ webView: WKWebView, navigationResponse: WKNavigationResponse, didBecome download: WKDownload) { download.delegate = self }

    func download(_ download: WKDownload, decideDestinationUsing response: URLResponse, suggestedFilename: String) async -> URL? {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("descargas", isDirectory: true)
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        let url = dir.appendingPathComponent(suggestedFilename.isEmpty ? "archivo" : suggestedFilename)
        try? FileManager.default.removeItem(at: url)
        downloads[download] = url
        return url
    }

    func downloadDidFinish(_ download: WKDownload) {
        guard let url = downloads.removeValue(forKey: download) else { return }
        previewURL = url
        let preview = QLPreviewController()
        preview.dataSource = self
        present(preview, animated: true)
    }

    func download(_ download: WKDownload, didFailWithError error: Error, resumeData: Data?) {
        downloads.removeValue(forKey: download)
        webView.evaluateJavaScript("window.bte && bte.toast && bte.toast('no se pudo descargar el archivo')")
    }

    func numberOfPreviewItems(in controller: QLPreviewController) -> Int { previewURL == nil ? 0 : 1 }
    func previewController(_ controller: QLPreviewController, previewItemAt index: Int) -> QLPreviewItem { previewURL! as NSURL }

    // target="_blank" y window.open: Eminus en la misma vista, lo demás fuera.
    func webView(_ webView: WKWebView, createWebViewWith configuration: WKWebViewConfiguration, for action: WKNavigationAction, windowFeatures: WKWindowFeatures) -> WKWebView? {
        if let url = action.request.url {
            if Self.isEminus(url) && url.scheme != "about" { webView.load(action.request) } else { openExternal(url) }
        }
        return nil
    }

    func webViewWebContentProcessDidTerminate(_ webView: WKWebView) { webView.reload() }

    // Diálogos de JavaScript nativos.
    func webView(_ webView: WKWebView, runJavaScriptAlertPanelWithMessage message: String, initiatedByFrame frame: WKFrameInfo) async {
        await withCheckedContinuation { done in
            let alert = UIAlertController(title: nil, message: message, preferredStyle: .alert)
            alert.addAction(UIAlertAction(title: "Aceptar", style: .default) { _ in done.resume() })
            present(alert, animated: true)
        }
    }

    func webView(_ webView: WKWebView, runJavaScriptConfirmPanelWithMessage message: String, initiatedByFrame frame: WKFrameInfo) async -> Bool {
        await withCheckedContinuation { done in
            let alert = UIAlertController(title: nil, message: message, preferredStyle: .alert)
            alert.addAction(UIAlertAction(title: "Cancelar", style: .cancel) { _ in done.resume(returning: false) })
            alert.addAction(UIAlertAction(title: "Aceptar", style: .default) { _ in done.resume(returning: true) })
            present(alert, animated: true)
        }
    }

    func webView(_ webView: WKWebView, runJavaScriptTextInputPanelWithPrompt prompt: String, defaultText: String?, initiatedByFrame frame: WKFrameInfo) async -> String? {
        await withCheckedContinuation { done in
            let alert = UIAlertController(title: nil, message: prompt, preferredStyle: .alert)
            alert.addTextField { $0.text = defaultText }
            alert.addAction(UIAlertAction(title: "Cancelar", style: .cancel) { _ in done.resume(returning: nil) })
            alert.addAction(UIAlertAction(title: "Aceptar", style: .default) { _ in done.resume(returning: alert.textFields?.first?.text) })
            present(alert, animated: true)
        }
    }
}

/// WKUserContentController retiene a su manejador: el proxy evita el ciclo.
private final class BridgeProxy: NSObject, WKScriptMessageHandlerWithReply {
    weak var target: EminusViewController?
    init(_ target: EminusViewController) { self.target = target }

    @MainActor
    func userContentController(_ ucc: WKUserContentController, didReceive message: WKScriptMessage) async -> (Any?, String?) {
        // Sólo desde páginas de Eminus.
        guard message.frameInfo.securityOrigin.host == "eminus.uv.mx" else { return (nil, "origen no permitido") }
        return (await target?.handle(message.body, mainFrame: message.frameInfo.isMainFrame), nil)
    }
}

extension UIColor {
    convenience init?(hex: String) {
        var s = hex.trimmingCharacters(in: .whitespaces)
        if s.hasPrefix("#") { s.removeFirst() }
        guard s.count == 6, let v = UInt32(s, radix: 16) else { return nil }
        self.init(red: CGFloat((v >> 16) & 255) / 255, green: CGFloat((v >> 8) & 255) / 255, blue: CGFloat(v & 255) / 255, alpha: 1)
    }
}
