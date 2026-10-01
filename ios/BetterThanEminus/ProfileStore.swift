import Foundation
import Security

/// Perfiles de Eminus (usuario y contraseña) en el Llavero de iOS: cifrados
/// por el sistema, sólo en este dispositivo (no viajan a iCloud ni a los
/// respaldos) y disponibles sólo con el iPhone desbloqueado.
///
/// La página nunca recibe una contraseña guardada: pide «entrar con el perfil
/// X» y es la app la que llena el formulario (ver EminusViewController).
enum ProfileStore {
    struct Profile {
        let user: String
        let name: String
    }

    private static let service = "mx.hiram.BetterThanEminus.profiles"

    private static func query(_ user: String? = nil) -> [String: Any] {
        var q: [String: Any] = [kSecClass as String: kSecClassGenericPassword, kSecAttrService as String: service]
        if let user { q[kSecAttrAccount as String] = user }
        return q
    }

    static func list() -> [Profile] {
        var q = query()
        q[kSecMatchLimit as String] = kSecMatchLimitAll
        q[kSecReturnAttributes as String] = true
        var out: CFTypeRef?
        guard SecItemCopyMatching(q as CFDictionary, &out) == errSecSuccess, let items = out as? [[String: Any]] else { return [] }
        return items.compactMap { item in
            guard let user = item[kSecAttrAccount as String] as? String else { return nil }
            let name = item[kSecAttrLabel as String] as? String ?? user
            return Profile(user: user, name: name.isEmpty ? user : name)
        }.sorted { $0.name.localizedCaseInsensitiveCompare($1.name) == .orderedAscending }
    }

    static func password(for user: String) -> String? {
        var q = query(user)
        q[kSecMatchLimit as String] = kSecMatchLimitOne
        q[kSecReturnData as String] = true
        var out: CFTypeRef?
        guard SecItemCopyMatching(q as CFDictionary, &out) == errSecSuccess, let data = out as? Data else { return nil }
        return String(data: data, encoding: .utf8)
    }

    @discardableResult
    static func save(user: String, password: String, name: String) -> Bool {
        guard !user.isEmpty, !password.isEmpty, let data = password.data(using: .utf8) else { return false }
        let attrs: [String: Any] = [kSecValueData as String: data, kSecAttrLabel as String: name]
        if SecItemUpdate(query(user) as CFDictionary, attrs as CFDictionary) == errSecSuccess { return true }
        var add = query(user)
        add.merge(attrs) { _, new in new }
        add[kSecAttrAccessible as String] = kSecAttrAccessibleWhenUnlockedThisDeviceOnly
        return SecItemAdd(add as CFDictionary, nil) == errSecSuccess
    }

    @discardableResult
    static func remove(user: String) -> Bool {
        SecItemDelete(query(user) as CFDictionary) == errSecSuccess
    }
}
