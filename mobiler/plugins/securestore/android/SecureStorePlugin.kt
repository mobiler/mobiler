package {{PACKAGE}}

import {{PACKAGE_SHARED_TYPES}}.PluginResponse

import androidx.security.crypto.EncryptedSharedPreferences
import androidx.security.crypto.MasterKey
import org.json.JSONObject

// Secure key/value storage (free, bundled). Backed by EncryptedSharedPreferences (AES-256,
// keys held in the Android Keystore). For secrets — auth tokens, API keys — NOT bulk data.
// `input` is JSON:
//   set:    {"key": "...", "value": "..."}  → ok:true
//   get:    {"key": "..."}                  → ok:true, output = value ("" if absent)
//   delete: {"key": "..."}                  → ok:true
// Pair with the `biometric` plugin (gate a get behind an authenticate in your Rust core).
class SecureStorePlugin(private val application: android.app.Application) : MobilerPlugin {
    private val prefs by lazy {
        try {
            open()
        } catch (e: java.security.KeyStoreException) {
            // The Keystore itself failed, which can be transient: never wipe secrets for it.
            throw e
        } catch (e: java.security.GeneralSecurityException) {
            recreate()
        } catch (e: java.io.IOException) {
            recreate()
        }
    }

    // The file's keysets can't be decrypted or parsed: its Keystore key is gone (a copy restored
    // from a backup or moved to another device). Those values are unreadable anywhere, so start an
    // empty store instead of failing every call (ADR-0037).
    private fun recreate(): android.content.SharedPreferences {
        application.deleteSharedPreferences("mobiler_secure")
        return open()
    }

    private fun open() = EncryptedSharedPreferences.create(
        application,
        "mobiler_secure",
        MasterKey.Builder(application).setKeyScheme(MasterKey.KeyScheme.AES256_GCM).build(),
        EncryptedSharedPreferences.PrefKeyEncryptionScheme.AES256_SIV,
        EncryptedSharedPreferences.PrefValueEncryptionScheme.AES256_GCM,
    )

    override suspend fun handle(op: String, input: String): PluginResponse {
        val obj = runCatching { JSONObject(input) }.getOrNull()
            ?: return PluginResponse(false, "invalid input JSON")
        val key = obj.optString("key")
        if (key.isEmpty()) return PluginResponse(false, "missing key")
        return when (op) {
            "set" -> {
                prefs.edit().putString(key, obj.optString("value")).apply()
                PluginResponse(true, "")
            }
            "get" -> PluginResponse(true, prefs.getString(key, "") ?: "")
            "delete" -> {
                prefs.edit().remove(key).apply()
                PluginResponse(true, "")
            }
            else -> PluginResponse(false, "unknown op '$op'")
        }
    }
}
