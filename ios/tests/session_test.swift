// Pruebas de SessionService sin red ni Llavero: sólo sus funciones puras.
// Los secretos aquí son inventados. Compilar y correr en la Mac:
//
//   swiftc -o /tmp/session_test ios/BetterThanEminus/SessionService.swift \
//     ios/BetterThanEminus/ProfileStore.swift ios/tests/session_test.swift && /tmp/session_test

import CommonCrypto
import Foundation
import Security

func check(_ ok: Bool, _ what: String) {
    if !ok {
        print("FALLA: \(what)")
        exit(1)
    }
}

/// Un JWT de mentiras. Sin firmar: aquí sólo importa el payload.
func jwt(payload: [String: Any], padded: Bool = false) -> String {
    let data = try! JSONSerialization.data(withJSONObject: payload)
    var body = data.base64EncodedString()
        .replacingOccurrences(of: "+", with: "-")
        .replacingOccurrences(of: "/", with: "_")
    if !padded { body = body.replacingOccurrences(of: "=", with: "") }
    return "eyJhbGciOiJIUzI1NiJ9." + body + ".firma"
}

func json(_ value: Any) -> Data {
    try! JSONSerialization.data(withJSONObject: value)
}

/// Descifra base64(sal ‖ iv ‖ AES-256-CBC) con CommonCrypto, por un camino
/// distinto al de SessionService.seal, para cotejar el roundtrip.
func unseal(_ sealed: String, appKey: String) -> String? {
    guard let raw = Data(base64Encoded: sealed), raw.count >= 32 + kCCBlockSizeAES128,
        (raw.count - 32) % kCCBlockSizeAES128 == 0
    else { return nil }
    let salt = raw.prefix(16)
    let iv = raw.dropFirst(16).prefix(16)
    let cipher = raw.dropFirst(32)
    var key = Data(count: kCCKeySizeAES256)
    let keyLen = key.count
    let saltLen = salt.count
    let derived = key.withUnsafeMutableBytes { keyRaw in
        salt.withUnsafeBytes { saltRaw in
            CCKeyDerivationPBKDF(
                CCPBKDFAlgorithm(kCCPBKDF2), appKey, appKey.utf8.count,
                saltRaw.baseAddress!.assumingMemoryBound(to: UInt8.self), saltLen,
                CCPseudoRandomAlgorithm(kCCPRFHmacAlgSHA1), 100,
                keyRaw.baseAddress!.assumingMemoryBound(to: UInt8.self), keyLen
            )
        }
    }
    guard derived == kCCSuccess else { return nil }
    var out = Data(count: cipher.count)
    let outLen = out.count
    let cipherLen = cipher.count
    var moved = 0
    let crypt = out.withUnsafeMutableBytes { outRaw in
        cipher.withUnsafeBytes { cipherRaw in
            key.withUnsafeBytes { keyRaw in
                iv.withUnsafeBytes { ivRaw in
                    CCCrypt(
                        CCOperation(kCCDecrypt), CCAlgorithm(kCCAlgorithmAES), CCOptions(kCCOptionPKCS7Padding),
                        keyRaw.baseAddress, keyLen, ivRaw.baseAddress,
                        cipherRaw.baseAddress, cipherLen, outRaw.baseAddress, outLen, &moved
                    )
                }
            }
        }
    }
    guard crypt == kCCSuccess else { return nil }
    out.count = moved
    return String(data: out, encoding: .utf8)
}

@main
struct SessionTest {
    static func main() {
        let future = Int64(Date().timeIntervalSince1970) + 3600
        let now = Int64(Date().timeIntervalSince1970)

        // ───────────── identidad y expiración ─────────────

        let byNameid = jwt(payload: ["nameid": "s2024001", "sub": "999", "exp": future])
        check(SessionService.account(of: byNameid) == "s2024001", "nameid gana a sub")
        check(SessionService.expiry(of: byNameid) == future, "exp numérico")

        let bySub = jwt(payload: ["sub": "999", "email": "a@uv.mx", "exp": future])
        check(SessionService.account(of: bySub) == "999", "sub gana a email")

        let numeric = jwt(payload: ["matricula": 2024001, "exp": future])
        check(SessionService.account(of: numeric) == "2024001", "matrícula numérica en texto")

        let sameNumeric = jwt(payload: ["nameid": "123"])
        let sameString = jwt(payload: ["nameid": 123])
        check(SessionService.account(of: sameNumeric) == SessionService.account(of: sameString), "123 y \"123\" son la misma cuenta")

        let trimmed = jwt(payload: ["email": "  a@uv.mx  "])
        check(SessionService.account(of: trimmed) == "a@uv.mx", "identidad con espacios se recorta")

        check(SessionService.account(of: jwt(payload: ["exp": future])) == "", "sin identidad da vacío")
        check(SessionService.account(of: "a.b.c") == "", "payload ilegible da vacío")

        check(SessionService.expiry(of: jwt(payload: ["exp": "\(future)"])) == future, "exp en texto")
        check(SessionService.expiry(of: jwt(payload: ["exp": "1728,5"])) == 1728, "exp con coma")
        check(SessionService.expiry(of: jwt(payload: [:])) == 0, "sin exp da cero")
        check(SessionService.expiry(of: jwt(payload: ["exp": true])) == 0, "exp booleano da cero")

        check(SessionService.fresh(expiry: future), "token fresco")
        check(!SessionService.fresh(expiry: now + 60), "a un minuto ya toca renovar")
        check(!SessionService.fresh(expiry: now - 10), "vencido no es fresco")

        // ───────────── limpieza del token ─────────────

        check(SessionService.clean("  Bearer \(byNameid)  ") == byNameid, "Bearer y espacios sobran")
        check(SessionService.clean("\"\(byNameid)\"") == byNameid, "comillas sobran")
        check(SessionService.clean("'\(byNameid)'") == byNameid, "comillas simples sobran")
        check(SessionService.clean(byNameid) == byNameid, "token limpio pasa igual")
        check(SessionService.clean("a.b") == nil, "dos partes no es JWT")
        check(SessionService.clean("a.b.c.d") == nil, "cuatro partes no es JWT")
        check(SessionService.clean("a.b.c") == nil, "payload no JSON se rechaza")
        check(SessionService.clean("a. .c") == nil, "espacios dentro se rechazan")
        let oversized = byNameid + String(repeating: "a", count: 8200)
        check(oversized.count > SessionService.maxTokenSize, "el caso largo sí pasa de 8192")
        check(SessionService.clean(oversized) == nil, "más de 8192 bytes se rechaza")
        let padded = jwt(payload: ["nameid": "s1", "exp": future], padded: true)
        check(padded.contains("="), "el caso acolchado sí trae relleno")
        check(SessionService.clean(padded) != nil, "base64url con relleno se acepta")

        // ───────────── cuentas distintas no se mezclan ─────────────

        let tokenA = jwt(payload: ["nameid": "111", "exp": future])
        let tokenB = jwt(payload: ["nameid": "222", "exp": future])
        check(SessionService.account(of: tokenA) != SessionService.account(of: tokenB), "cuentas distintas no coinciden")
        check(SessionService.account(of: tokenA) == SessionService.account(of: tokenA), "la misma cuenta coincide")

        // ───────────── cifrado determinista y roundtrip ─────────────

        let salt = Data(repeating: 7, count: 16)
        let iv = Data(repeating: 9, count: 16)
        let fakePassword = "s3cr3to-mentira"
        let sealedFixed = SessionService.seal(password: fakePassword, appKey: "llave-app", salt: salt, iv: iv)
        check(sealedFixed != nil, "cifra con sal e iv fijos")
        check(sealedFixed == SessionService.seal(password: fakePassword, appKey: "llave-app", salt: salt, iv: iv), "misma sal e iv dan lo mismo")
        let sealedRaw = Data(base64Encoded: sealedFixed!)!
        check(sealedRaw.prefix(16) == salt, "los primeros 16 bytes son la sal")
        check(sealedRaw.dropFirst(16).prefix(16) == iv, "los siguientes 16 son el iv")
        check(sealedRaw.count > 32, "tras sal e iv hay texto cifrado")
        check(unseal(sealedFixed!, appKey: "llave-app") == fakePassword, "lo cifrado se descifra a la contraseña")
        check(unseal(sealedFixed!, appKey: "otra-llave") != fakePassword, "con otra llave no abre")
        check(SessionService.seal(password: fakePassword, appKey: "llave-app", salt: Data(count: 8), iv: iv) == nil, "sal corta se rechaza")
        check(SessionService.seal(password: fakePassword, appKey: "llave-app", salt: salt, iv: Data(count: 8)) == nil, "iv corto se rechaza")

        let sealedRandom1 = SessionService.seal(password: fakePassword, appKey: "llave-app")
        let sealedRandom2 = SessionService.seal(password: fakePassword, appKey: "llave-app")
        check(sealedRandom1 != nil && sealedRandom2 != nil, "cifra con sal al azar")
        check(sealedRandom1 != sealedRandom2, "la sal al azar cambia la salida")
        check(unseal(sealedRandom1!, appKey: "llave-app") == fakePassword, "lo al azar también abre")

        // ───────────── llave de login y respuesta de auth ─────────────

        let index = "<html><script src=\"polyfills.123.js\"></script><script src=\"main.abc123.js\"></script>"
        check(SessionService.loginScript(in: index) == "main.abc123.js", "encuentra el main.*.js")
        check(SessionService.loginScript(in: "<html>sin scripts</html>") == nil, "sin main.*.js no hay llave")
        check(SessionService.loginKey(in: "algo hash:{key:\"ABCDEF123\"} resto") == "ABCDEF123", "saca la llave del JS")
        check(SessionService.loginKey(in: "hash:{key:\"\"}") == nil, "llave vacía no vale")
        check(SessionService.loginKey(in: "sin llave") == nil, "sin hash no hay llave")

        check(SessionService.accessToken(from: json(["accessToken": "TOK"])) == "TOK", "objeto directo")
        check(SessionService.accessToken(from: json([["accessToken": "VIEJO"], ["accessToken": "NUEVO"]])) == "NUEVO", "del arreglo toma el último")
        check(SessionService.accessToken(from: json(["contenido": ["accessToken": "C"]])) == "C", "envuelto en contenido")
        check(SessionService.accessToken(from: json([])) == nil, "arreglo vacío no da token")
        check(SessionService.accessToken(from: json(["contenido": ["otro": 1]])) == nil, "contenido sin token no da token")
        check(SessionService.accessToken(from: json(["accessToken": ""])) == nil, "token vacío no vale")
        check(SessionService.accessToken(from: Data("no json".utf8)) == nil, "respuesta ilegible no da token")

        // ───────────── nada secreto sale ─────────────

        check(!sealedFixed!.contains(fakePassword), "lo cifrado no trae la contraseña en claro")
        let problems: [SessionError] = [
            .invalidToken, .expiredToken, .noSession, .noRememberedUser, .missingPassword,
            .invalidCredentials, .accountMismatch, .loginFormUnreadable, .network("sin conexión con eminus"),
        ]
        for problem in problems {
            let text = problem.errorDescription ?? ""
            check(!text.isEmpty, "todo error dice algo")
            check(!text.contains(fakePassword), "ningún error lleva la contraseña")
            check(!text.contains(tokenA), "ningún error lleva el token")
        }

        print("session_test: ok")
    }
}
