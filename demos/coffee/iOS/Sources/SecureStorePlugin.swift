import Foundation
import Security
import SharedTypes

// Secure key/value storage (free, bundled). Backed by the iOS Keychain (encrypted, app-scoped).
// For secrets — auth tokens, API keys — NOT bulk data. `input` is JSON:
//   set:    {"key": "...", "value": "..."}  → ok:true
//   get:    {"key": "..."}                  → ok:true, output = value ("" if absent)
//   delete: {"key": "..."}                  → ok:true
// Pair with the `biometric` plugin (gate a get behind an authenticate in your Rust core).
enum SecureStorePlugin {
    static func handle(op: String, input: String) async -> PluginResponse {
        guard
            let data = input.data(using: .utf8),
            let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
            let key = obj["key"] as? String, !key.isEmpty
        else { return PluginResponse(ok: false, output: "missing key") }

        switch op {
        case "set":
            let value = (obj["value"] as? String) ?? ""
            return set(key: key, value: value)
        case "get":
            return get(key: key)
        case "delete":
            delete(key: key)
            return PluginResponse(ok: true, output: "")
        default:
            return PluginResponse(ok: false, output: "unknown op '\(op)'")
        }
    }

    private static func query(_ key: String) -> [String: Any] {
        [kSecClass as String: kSecClassGenericPassword,
         kSecAttrService as String: "mobiler.securestore",
         kSecAttrAccount as String: key]
    }

    private static func set(key: String, value: String) -> PluginResponse {
        SecItemDelete(query(key) as CFDictionary) // overwrite semantics
        var attrs = query(key)
        attrs[kSecValueData as String] = Data(value.utf8)
        attrs[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
        let status = SecItemAdd(attrs as CFDictionary, nil)
        return status == errSecSuccess
            ? PluginResponse(ok: true, output: "")
            : PluginResponse(ok: false, output: "keychain set failed (\(status))")
    }

    private static func get(key: String) -> PluginResponse {
        var q = query(key)
        q[kSecReturnData as String] = true
        q[kSecMatchLimit as String] = kSecMatchLimitOne
        var item: CFTypeRef?
        let status = SecItemCopyMatching(q as CFDictionary, &item)
        if status == errSecItemNotFound { return PluginResponse(ok: true, output: "") }
        guard status == errSecSuccess, let data = item as? Data, let value = String(data: data, encoding: .utf8)
        else { return PluginResponse(ok: false, output: "keychain get failed (\(status))") }
        return PluginResponse(ok: true, output: value)
    }

    private static func delete(key: String) {
        SecItemDelete(query(key) as CFDictionary)
    }
}
