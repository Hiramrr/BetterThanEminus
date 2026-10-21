import CommonCrypto
import Foundation
import Security

/// Lo que puede salir mal al pedir un token. Los mensajes se muestran tal
/// cual y nunca llevan el token ni la contraseña.
enum SessionError: Error, LocalizedError, Sendable {
    case invalidToken
    case expiredToken
    case noSession
    case noRememberedUser
    case missingPassword
    case invalidCredentials
    case accountMismatch
    case loginFormUnreadable
    case keychain
    case network(String)

    var errorDescription: String? {
        switch self {
        case .invalidToken: return "esa sesión no se entiende, entra de nuevo"
        case .expiredToken: return "esa sesión ya venció, entra de nuevo"
        case .noSession: return "sin sesión, entra con tu cuenta"
        case .noRememberedUser: return "la sesión venció y no hay un usuario recordado, entra de nuevo"
        case .missingPassword: return "este perfil ya no tiene contraseña en el llavero"
        case .invalidCredentials: return "usuario o contraseña incorrectos"
        case .accountMismatch: return "eminus devolvió otra cuenta al renovar, entra de nuevo"
        case .loginFormUnreadable: return "no se pudo leer el formulario de eminus, entra a mano"
        case .keychain: return "no se pudo guardar la sesión en el llavero"
        case .network(let message): return message
        }
    }
}

/// La sesión de Eminus: guarda el token en el Llavero y lo renueva solo.
///
/// La página avisa con `capture` cuando entra. Quien necesite llamar a la
/// API pide `validToken` y recibe un token vigente, renovado si hace falta.
/// El token se guarda por cuenta (la identidad sale del propio JWT, como
/// `account_of` en Rust), así dos cuentas nunca se mezclan. El usuario
/// recordado es sólo el login con el que se entra, que puede diferir de la
/// identidad numérica del token.
actor SessionService {
    static let shared = SessionService()

    /// El token más largo que se acepta, en bytes.
    static let maxTokenSize = 8192
    /// Cuánto antes de vencer se renueva, en segundos.
    static let renewBefore: Int64 = 300

    private static let origin = "https://eminus.uv.mx"
    private static let service = "mx.hiram.BetterThanEminus.session"
    private static let pointerService = "mx.hiram.BetterThanEminus.session.current"
    private static let pointerAccount = "current"

    private struct Record: Codable {
        var token: String
        var user: String
    }

    private let session: URLSession = {
        let config = URLSessionConfiguration.default
        config.timeoutIntervalForRequest = 15
        config.timeoutIntervalForResource = 30
        return URLSession(configuration: config)
    }()

    private var token = ""
    private var account = ""
    private var user = ""
    private var expiry: Int64 = 0
    private var loaded = false
    private var generation: UInt64 = 0
    private var inflightID: UInt64 = 0
    private var inflight: Task<String, Error>?
    private var credentialsRejected = false

    /// Guarda el token que consiguió la página. El usuario puede venir vacío
    /// si el login aún no ofrece perfil; en ese caso se conserva el anterior
    /// de la misma cuenta. Si el token es de otra cuenta, cambia a ella.
    func capture(token rawToken: String, user: String) async throws {
        ensureLoaded()
        guard let cleaned = Self.clean(rawToken) else { throw SessionError.invalidToken }
        let identity = Self.account(of: cleaned)
        guard !identity.isEmpty else { throw SessionError.invalidToken }
        let exp = Self.expiry(of: cleaned)
        // La página puede conservar un token anterior al que renovó iOS.
        if identity == account, expiry > exp {
            if !user.isEmpty, user != self.user {
                self.user = user
                credentialsRejected = false
                try Self.save(Record(token: token, user: user), account: account)
            }
            return
        }
        guard exp > Self.now() else { throw SessionError.expiredToken }
        if cleaned != token || (!user.isEmpty && user != self.user) { credentialsRejected = false }
        generation &+= 1
        inflight?.cancel()
        inflight = nil
        if identity != account {
            if !account.isEmpty { Self.delete(account: account) }
            account = identity
            self.user = user
        } else if !user.isEmpty {
            self.user = user
        }
        token = cleaned
        expiry = exp
        loaded = true
        try Self.save(Record(token: cleaned, user: self.user), account: identity)
        try Self.savePointer(identity)
    }

    /// Un token vigente para llamar a la API. Renueva si vence en menos de
    /// cinco minutos o si se pide a la fuerza. Las llamadas concurrentes
    /// comparten una sola renovación.
    func validToken(forceRefresh: Bool = false, rejectedToken: String? = nil) async throws -> String {
        try Task.checkCancellation()
        ensureLoaded()
        if token.isEmpty { throw SessionError.noSession }
        if forceRefresh, let rejectedToken, rejectedToken != token, Self.fresh(expiry: expiry) { return token }
        if !forceRefresh, Self.fresh(expiry: expiry) { return token }
        guard !credentialsRejected else { throw SessionError.invalidCredentials }
        if let running = inflight {
            return try await withTaskCancellationHandler { try await running.value } onCancel: { running.cancel() }
        }
        inflightID &+= 1
        let id = inflightID
        let gen = generation
        let task = Task<String, Error> { try await self.refreshed(id: id, generation: gen) }
        inflight = task
        return try await withTaskCancellationHandler { try await task.value } onCancel: { task.cancel() }
    }

    /// Cierra la sesión: olvida el token en memoria y en el Llavero. Lo que
    /// siga en vuelo se descarta y no revive la sesión.
    func clear() async {
        ensureLoaded()
        generation &+= 1
        inflight?.cancel()
        inflight = nil
        let old = account
        token = ""
        account = ""
        user = ""
        expiry = 0
        credentialsRejected = false
        loaded = true
        Self.deletePointer()
        if !old.isEmpty { Self.delete(account: old) }
    }

    func credentialsChanged() { credentialsRejected = false }

    // MARK: - renovación

    private func refreshed(id: UInt64, generation gen: UInt64) async throws -> String {
        do {
            let fresh = try await performRefresh(generation: gen)
            if id == inflightID { inflight = nil }
            return fresh
        } catch {
            if gen == generation, let error = error as? SessionError {
                if case .invalidCredentials = error { credentialsRejected = true }
                if case .accountMismatch = error { credentialsRejected = true }
            }
            if id == inflightID { inflight = nil }
            throw error
        }
    }

    /// Un solo intento, sin reintentos: con credenciales incorrectas se avisa
    /// una vez y no se insiste. El token renovado se guarda sólo si la sesión
    /// no cambió mientras tanto.
    private func performRefresh(generation gen: UInt64) async throws -> String {
        try Task.checkCancellation()
        let who = user
        let whose = account
        guard !who.isEmpty else { throw SessionError.noRememberedUser }
        guard let password = ProfileStore.password(for: who) else { throw SessionError.missingPassword }
        guard gen == generation else { throw CancellationError() }
        let key = try await loginKey()
        try Task.checkCancellation()
        guard gen == generation else { throw CancellationError() }
        guard let sealed = Self.seal(password: password, appKey: key) else {
            throw SessionError.network("no se pudo cifrar la contraseña")
        }
        let fresh = try await signIn(user: who, sealed: sealed)
        try Task.checkCancellation()
        guard gen == generation else { throw CancellationError() }
        guard let cleaned = Self.clean(fresh) else { throw SessionError.invalidToken }
        guard !whose.isEmpty, Self.account(of: cleaned) == whose else { throw SessionError.accountMismatch }
        let exp = Self.expiry(of: cleaned)
        guard exp > Self.now() else { throw SessionError.expiredToken }
        token = cleaned
        expiry = exp
        try Self.save(Record(token: cleaned, user: who), account: whose)
        try Self.savePointer(whose)
        return cleaned
    }

    private func ensureLoaded() {
        guard !loaded else { return }
        loaded = true
        guard let identity = Self.loadPointer(), !identity.isEmpty else { return }
        guard let record = Self.load(account: identity),
            let cleaned = Self.clean(record.token),
            Self.account(of: cleaned) == identity
        else {
            Self.deletePointer()
            Self.delete(account: identity)
            return
        }
        token = cleaned
        account = identity
        user = record.user
        expiry = Self.expiry(of: cleaned)
    }

    // MARK: - login con contraseña

    /// La llave con la que el formulario cifra la contraseña, leída de la
    /// propia página de Eminus igual que hace Rust.
    private func loginKey() async throws -> String {
        let base = Self.origin + "/eminus4/"
        let index: String
        do {
            index = try await getText(base)
        } catch {
            if error is CancellationError { throw error }
            throw SessionError.network("sin conexión con eminus")
        }
        guard let script = Self.loginScript(in: index) else { throw SessionError.loginFormUnreadable }
        let code: String
        do {
            code = try await getText(base + script)
        } catch {
            if error is CancellationError { throw error }
            throw SessionError.loginFormUnreadable
        }
        guard let key = Self.loginKey(in: code) else { throw SessionError.loginFormUnreadable }
        return key
    }

    /// `sealed` ya viene cifrada como la manda el formulario.
    private func signIn(user: String, sealed: String) async throws -> String {
        guard let url = URL(string: Self.origin + "/eminusapi/api/auth") else {
            throw SessionError.network("sin conexión con eminus")
        }
        var request = URLRequest(url: url)
        request.httpMethod = "POST"
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.setValue("application/json", forHTTPHeaderField: "Accept")
        request.timeoutInterval = 15
        request.httpBody = try? JSONSerialization.data(withJSONObject: ["username": user, "password": sealed])
        let data: Data
        let response: URLResponse
        do {
            (data, response) = try await session.data(for: request)
        } catch {
            if error is CancellationError { throw error }
            throw SessionError.network("sin conexión con eminus")
        }
        guard let http = response as? HTTPURLResponse else { throw SessionError.network("sin conexión con eminus") }
        switch http.statusCode {
        case 200 ..< 300: break
        case 429: throw SessionError.network("eminus recibió demasiadas solicitudes, inténtalo más tarde")
        case 400 ... 499: throw SessionError.invalidCredentials
        default: throw SessionError.network("HTTP \(http.statusCode)")
        }
        guard let token = Self.accessToken(from: data) else { throw SessionError.invalidCredentials }
        return token
    }

    private func getText(_ url: String) async throws -> String {
        guard let requestURL = URL(string: url) else { throw SessionError.network("sin conexión con eminus") }
        var request = URLRequest(url: requestURL)
        request.timeoutInterval = 15
        let (data, response) = try await session.data(for: request)
        guard let http = response as? HTTPURLResponse, (200 ..< 300).contains(http.statusCode) else {
            throw SessionError.network("sin conexión con eminus")
        }
        guard let text = String(data: data, encoding: .utf8) ?? String(data: data, encoding: .isoLatin1) else {
            throw SessionError.loginFormUnreadable
        }
        return text
    }

    // MARK: - JWT y respuestas

    static func now() -> Int64 { Int64(Date().timeIntervalSince1970) }

    static func fresh(expiry: Int64) -> Bool { expiry - now() > renewBefore }

    /// Acepta lo que se suele copiar: con comillas o con "Bearer " delante.
    /// Rechaza lo que pase de 8192 bytes, lo que no tenga tres partes y lo
    /// que no traiga un payload JSON legible.
    static func clean(_ raw: String) -> String? {
        var token = raw.trimmingCharacters(in: .whitespacesAndNewlines)
        token = token.trimmingCharacters(in: CharacterSet(charactersIn: "\"'"))
        token = token.trimmingCharacters(in: .whitespacesAndNewlines)
        if token.hasPrefix("Bearer ") { token = String(token.dropFirst("Bearer ".count)) }
        token = token.trimmingCharacters(in: .whitespacesAndNewlines)
        guard token.utf8.count <= maxTokenSize else { return nil }
        guard token.split(separator: ".", omittingEmptySubsequences: false).count == 3 else { return nil }
        guard !token.contains(where: \.isWhitespace) else { return nil }
        guard claims(of: token) != nil else { return nil }
        return token
    }

    /// El payload del token. Tolera base64url con o sin relleno, pero nada más.
    static func claims(of token: String) -> [String: Any]? {
        let parts = token.split(separator: ".", omittingEmptySubsequences: false)
        guard parts.count == 3 else { return nil }
        var payload = String(parts[1])
            .replacingOccurrences(of: "-", with: "+")
            .replacingOccurrences(of: "_", with: "/")
        let rest = payload.count % 4
        if rest == 1 { return nil }
        if rest > 0 { payload += String(repeating: "=", count: 4 - rest) }
        guard let data = Data(base64Encoded: payload) else { return nil }
        return (try? JSONSerialization.jsonObject(with: data)) as? [String: Any]
    }

    /// La cuenta del token, en el mismo orden que Rust: nameid, unique_name,
    /// sub, email, idPersona, matricula. Contra esto se comparan los tokens
    /// nuevos, nunca contra el usuario recordado.
    static func account(of token: String) -> String {
        guard let claims = claims(of: token) else { return "" }
        for key in ["nameid", "unique_name", "sub", "email", "idPersona", "matricula"] {
            let value = string(claims[key])
            if !value.isEmpty { return value }
        }
        return ""
    }

    static func expiry(of token: String) -> Int64 {
        guard let claims = claims(of: token) else { return 0 }
        return number(claims["exp"])
    }

    /// Como model::string de Rust: texto tal cual, números en texto, booleanos
    /// en true o false, nulos vacíos y arreglos u objetos en su JSON.
    static func string(_ value: Any?) -> String {
        switch value {
        case let text as String:
            return text.trimmingCharacters(in: .whitespacesAndNewlines)
        case let number as NSNumber:
            if CFGetTypeID(number) == CFBooleanGetTypeID() { return number.boolValue ? "true" : "false" }
            return number.stringValue
        case is NSNull, nil:
            return ""
        default:
            guard let value, JSONSerialization.isValidJSONObject(value),
                let data = try? JSONSerialization.data(withJSONObject: value),
                let text = String(data: data, encoding: .utf8)
            else { return "" }
            return text
        }
    }

    /// Como model::int de Rust: números tal cual, texto numérico con coma o
    /// punto, lo demás cero.
    static func number(_ value: Any?) -> Int64 {
        let raw: Double?
        if let number = value as? NSNumber, CFGetTypeID(number) != CFBooleanGetTypeID() {
            raw = number.doubleValue
        } else if let text = value as? String {
            raw = Double(text.trimmingCharacters(in: .whitespacesAndNewlines).replacingOccurrences(of: ",", with: "."))
        } else {
            raw = nil
        }
        guard let parsed = raw, parsed.isFinite else { return 0 }
        if parsed >= 9.223372036854776e18 { return .max }
        if parsed < -9.223372036854776e18 { return .min }
        return Int64(parsed)
    }

    /// El main.*.js del índice: el primer texto entre comillas que empiece
    /// con main. y mencione .js, igual que Rust.
    static func loginScript(in index: String) -> String? {
        index.split(separator: "\"", omittingEmptySubsequences: false)
            .first { $0.hasPrefix("main.") && $0.contains(".js") }
            .map(String.init)
    }

    /// La llave dentro del JS: hash:{key:"..."}.
    static func loginKey(in code: String) -> String? {
        guard let rest = code.components(separatedBy: "hash:{key:\"").dropFirst().first else { return nil }
        let key = rest.split(separator: "\"", omittingEmptySubsequences: false).first.map(String.init) ?? ""
        return key.isEmpty ? nil : key
    }

    /// El accessToken de la respuesta: arreglo, objeto o envuelto en
    /// contenido. Del arreglo toma el último, como Rust.
    static func accessToken(from data: Data) -> String? {
        guard let json = try? JSONSerialization.jsonObject(with: data) else { return nil }
        return accessToken(from: json)
    }

    static func accessToken(from json: Any) -> String? {
        var data = json
        if let rows = json as? [Any] { data = rows.last ?? NSNull() }
        if let dict = data as? [String: Any], let inner = dict["contenido"] as? [String: Any] { data = inner }
        guard let dict = data as? [String: Any],
            let token = dict["accessToken"] as? String, !token.isEmpty
        else { return nil }
        return token
    }

    // MARK: - cifrado de la contraseña

    /// Cifra como el formulario: base64(sal ‖ iv ‖ AES-256-CBC con PKCS7), con
    /// la llave derivada por PBKDF2-HMAC-SHA1 de 100 vueltas. Sal e iv nuevos
    /// en cada llamada.
    static func seal(password: String, appKey: String) -> String? {
        var random = Data(count: 32)
        let status = random.withUnsafeMutableBytes { raw in
            guard let base = raw.baseAddress else { return errSecParam }
            return SecRandomCopyBytes(kSecRandomDefault, 32, base)
        }
        guard status == errSecSuccess else { return nil }
        return seal(password: password, appKey: appKey, salt: random.prefix(16), iv: random.suffix(16))
    }

    /// Núcleo determinista: con la misma sal e iv da la misma salida.
    static func seal(password: String, appKey: String, salt: Data, iv: Data) -> String? {
        guard salt.count == kCCBlockSizeAES128, iv.count == kCCBlockSizeAES128 else { return nil }
        guard let plain = password.trimmingCharacters(in: .whitespacesAndNewlines).data(using: .utf8) else { return nil }
        let input = plain.isEmpty ? Data([0]) : plain
        var key = Data(count: kCCKeySizeAES256)
        let keyLen = key.count
        let derived = key.withUnsafeMutableBytes { keyRaw in
            salt.withUnsafeBytes { saltRaw in
                guard let keyBase = keyRaw.baseAddress, let saltBase = saltRaw.baseAddress else { return Int32(kCCParamError) }
                return CCKeyDerivationPBKDF(
                    CCPBKDFAlgorithm(kCCPBKDF2), appKey, appKey.utf8.count,
                    saltBase.assumingMemoryBound(to: UInt8.self), salt.count,
                    CCPseudoRandomAlgorithm(kCCPRFHmacAlgSHA1), 100,
                    keyBase.assumingMemoryBound(to: UInt8.self), keyLen
                )
            }
        }
        guard derived == kCCSuccess else { return nil }
        var out = Data(count: input.count + kCCBlockSizeAES128)
        let outLen = out.count
        let plainLen = plain.count
        var moved = 0
        let crypt = out.withUnsafeMutableBytes { outRaw in
            input.withUnsafeBytes { inputRaw in
                key.withUnsafeBytes { keyRaw in
                    iv.withUnsafeBytes { ivRaw in
                        guard let outBase = outRaw.baseAddress, let inputBase = inputRaw.baseAddress,
                            let keyBase = keyRaw.baseAddress, let ivBase = ivRaw.baseAddress
                        else { return Int32(kCCParamError) }
                        var done = 0
                        let status = CCCrypt(
                            CCOperation(kCCEncrypt), CCAlgorithm(kCCAlgorithmAES), CCOptions(kCCOptionPKCS7Padding),
                            keyBase, keyLen, ivBase, inputBase, plainLen, outBase, outLen, &done
                        )
                        moved = done
                        return status
                    }
                }
            }
        }
        guard crypt == kCCSuccess else { return nil }
        out.count = moved
        return (salt + iv + out).base64EncodedString()
    }

    // MARK: - Llavero

    /// Tras el primer desbloqueo: la renovación en segundo plano lo necesita.
    private static let accessible = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly

    private static func baseQuery(service: String, account: String) -> [String: Any] {
        [kSecClass as String: kSecClassGenericPassword,
         kSecAttrService as String: service,
         kSecAttrAccount as String: account]
    }

    private static func saveData(_ data: Data, service: String, account: String) throws {
        let attrs: [String: Any] = [kSecValueData as String: data, kSecAttrAccessible as String: accessible]
        let status = SecItemUpdate(baseQuery(service: service, account: account) as CFDictionary, attrs as CFDictionary)
        if status == errSecSuccess { return }
        guard status == errSecItemNotFound else { throw SessionError.keychain }
        var add = baseQuery(service: service, account: account)
        add.merge(attrs) { _, new in new }
        guard SecItemAdd(add as CFDictionary, nil) == errSecSuccess else { throw SessionError.keychain }
    }

    private static func loadData(service: String, account: String) -> Data? {
        var query = baseQuery(service: service, account: account)
        query[kSecMatchLimit as String] = kSecMatchLimitOne
        query[kSecReturnData as String] = true
        var out: CFTypeRef?
        guard SecItemCopyMatching(query as CFDictionary, &out) == errSecSuccess else { return nil }
        return out as? Data
    }

    private static func deleteData(service: String, account: String) {
        _ = SecItemDelete(baseQuery(service: service, account: account) as CFDictionary)
    }

    private static func save(_ record: Record, account: String) throws {
        let data = try JSONEncoder().encode(record)
        try saveData(data, service: service, account: account)
    }

    private static func load(account: String) -> Record? {
        guard let data = loadData(service: service, account: account) else { return nil }
        return try? JSONDecoder().decode(Record.self, from: data)
    }

    private static func delete(account: String) {
        deleteData(service: service, account: account)
    }

    private static func savePointer(_ identity: String) throws {
        guard let data = identity.data(using: .utf8) else { throw SessionError.keychain }
        try saveData(data, service: pointerService, account: pointerAccount)
    }

    private static func loadPointer() -> String? {
        guard let data = loadData(service: pointerService, account: pointerAccount) else { return nil }
        return String(data: data, encoding: .utf8)
    }

    private static func deletePointer() {
        deleteData(service: pointerService, account: pointerAccount)
    }
}
