import Foundation

/// El código web de la extensión (src/, app/, popup/) más la capa de la app
/// (web/). Se lee del paquete de la app o, en desarrollo, del servidor de la
/// Mac (variable de entorno BTE_DEV), así un cambio de CSS/JS sólo necesita
/// recargar la página.
struct WebBundle {
    /// Equivalen a los content scripts de manifest.json.
    static let startScripts = ["web/shim.js", "src/prefs.js", "web/app-prefs.js", "src/boot.js", "src/bg.js"]
    /// Sin src/styles.css: en el teléfono sólo se usa el estilo tui.
    static let styles = ["src/themes.css", "src/tui.css", "src/bg.css", "web/mobile.css"]
    static let endScripts = ["src/nav.js", "src/palette.js", "src/drafts.js", "src/app.js", "web/mobile.js"]
    /// Documentos que se arman dentro de Eminus (vista tui y ajustes).
    static let documentFiles = [
        "app/app.html", "app/app.css", "app/app.js",
        "popup/popup.html", "popup/popup.css", "popup/popup.js",
        "src/prefs.js", "src/boot.js", "src/bg.js", "src/themes.css", "src/bg.css",
        "web/shim.js", "web/app-prefs.js", "web/app-mobile.css", "web/app-mobile.js", "web/popup-mobile.css"
    ]

    let files: [String: String]
    let version: String

    static var devServer: URL? {
        guard let raw = ProcessInfo.processInfo.environment["BTE_DEV"], !raw.isEmpty else { return nil }
        return URL(string: raw)
    }

    static func load() async -> WebBundle {
        let paths = Set(startScripts + styles + endScripts + documentFiles)
        var files: [String: String] = [:]
        if let dev = devServer {
            await withTaskGroup(of: (String, String?).self) { group in
                for path in paths {
                    group.addTask {
                        let url = dev.appendingPathComponent("files").appendingPathComponent(path)
                        let data = try? await URLSession.shared.data(from: url).0
                        return (path, data.flatMap { String(data: $0, encoding: .utf8) })
                    }
                }
                for await (path, text) in group { if let text { files[path] = text } }
            }
        }
        for path in paths where files[path] == nil {
            if let url = Bundle.main.resourceURL?.appendingPathComponent(path),
               let text = try? String(contentsOf: url, encoding: .utf8) {
                files[path] = text
            }
        }
        let version = Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "?"
        return WebBundle(files: files, version: version)
    }

    private static func json(_ value: Any) -> String {
        let data = try? JSONSerialization.data(withJSONObject: value, options: [.fragmentsAllowed])
        return data.flatMap { String(data: $0, encoding: .utf8) } ?? "null"
    }

    /// Cada archivo va aislado: un error en uno no detiene a los demás.
    private func wrapped(_ paths: [String]) -> String {
        paths.map { path in
            "try {\n\(files[path] ?? "")\n} catch (e) { console.error('[bte] \(path)', e); }"
        }.joined(separator: "\n")
    }

    /// Sólo en páginas de Eminus (lo mismo que "matches" en el manifiesto);
    /// los iframes de otros sitios (videos, formularios) quedan fuera.
    private static let guardExpr =
        "location.hostname === 'eminus.uv.mx' && /^\\/(eminus4|aplicativoEminus)(\\/|$)/.test(location.pathname)"

    /// document_start: shim, estilos y tema antes del primer pintado.
    var startSource: String {
        let css = Self.styles.compactMap { files[$0] }.joined(separator: "\n")
        return """
        (function () {
          if (!(\(Self.guardExpr)) || window.__bteStarted) return;
          window.__bteStarted = true;
          window.__bteNative = { version: \(Self.json(version)), dev: \(Self.devServer != nil) };
          var css = \(Self.json(css));
          var addCss = function () {
            var s = document.getElementById('bte-css') || document.createElement('style');
            s.id = 'bte-css';
            if (s.textContent !== css) s.textContent = css;
            (document.head || document.documentElement).appendChild(s);
          };
          addCss();
          document.addEventListener('DOMContentLoaded', addCss, { once: true });
          \(wrapped(Self.startScripts))
        })();
        """
    }

    /// document_idle: navegación, paleta, borradores, vista tui y capa móvil.
    var endSource: String {
        var docs: [String: String] = [:]
        for path in Self.documentFiles { docs[path] = files[path] ?? "" }
        return """
        (function () {
          if (!(\(Self.guardExpr)) || !window.__bteStarted || window.__bteEnded) return;
          window.__bteEnded = true;
          window.__bteNative.files = \(Self.json(docs));
          \(wrapped(Self.endScripts))
        })();
        """
    }
}
