import Foundation
import Security
import SharedTypes

// Secure key/value storage (free, bundled). Backed by the iOS Keychain (encrypted, app-scoped).
// For secrets — auth tokens, API keys — NOT bulk data. `input` is JSON:
//   set:    {"key": "...", "value": "..."}  → ok:true once the Keychain holds it
//   get:    {"key": "..."}                  → ok:true, output = value ("" if absent)
//   delete: {"key": "..."}                  → ok:true once the key is gone (or was never there)
// A Keychain call that fails comes back ok:false, output = its OSStatus.
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
            return delete(key: key)
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
        // Update in place, and add only a key that isn't there yet. Deleting first and then adding
        // would lose the old value whenever the add fails.
        let attrs: [String: Any] = [
            kSecValueData as String: Data(value.utf8),
            kSecAttrAccessible as String: kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly,
        ]
        var status = SecItemUpdate(query(key) as CFDictionary, attrs as CFDictionary)
        if status == errSecItemNotFound {
            status = SecItemAdd(query(key).merging(attrs) { $1 } as CFDictionary, nil)
            // Another `set` of the same new key added it between our update and add: update that one.
            if status == errSecDuplicateItem {
                status = SecItemUpdate(query(key) as CFDictionary, attrs as CFDictionary)
            }
        }
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

    private static func delete(key: String) -> PluginResponse {
        let status = SecItemDelete(query(key) as CFDictionary)
        // A key that was never stored is absent either way: that is a successful delete.
        return status == errSecSuccess || status == errSecItemNotFound
            ? PluginResponse(ok: true, output: "")
            : PluginResponse(ok: false, output: "keychain delete failed (\(status))")
    }
}
