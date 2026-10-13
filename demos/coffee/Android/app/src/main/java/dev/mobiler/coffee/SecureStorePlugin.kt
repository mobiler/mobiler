package dev.mobiler.coffee

import dev.mobiler.coffee.shared.types.PluginResponse

import androidx.security.crypto.EncryptedSharedPreferences
import androidx.security.crypto.MasterKey
import kotlinx.coroutines.asCoroutineDispatcher
import kotlinx.coroutines.withContext
import org.json.JSONObject

// Secure key/value storage (free, bundled). Backed by EncryptedSharedPreferences (AES-256,
// keys held in the Android Keystore). For secrets — auth tokens, API keys — NOT bulk data.
// `input` is JSON:
//   set:    {"key": "...", "value": "..."}  → ok:true once the value is on disk
//   get:    {"key": "..."}                  → ok:true, output = value ("" if absent)
//   delete: {"key": "..."}                  → ok:true once the removal is on disk
// A write that fails comes back ok:false, output = the reason. Calls run one at a time in the order
// the core sent them, so a `set` then a `get` (or a `delete` then a `set`) of one key can't swap.
// Pair with the `biometric` plugin (gate a get behind an authenticate in your Rust core).
class SecureStorePlugin(private val application: android.app.Application) : MobilerPlugin {
    private companion object {
        // One thread, not the shared IO pool: the shell starts plugin calls in the order the core
        // sent them, and the pool would let two calls on one key finish the other way round. One
        // per process, so a relaunched activity's new plugin queues behind the old one's calls.
        val lane = java.util.concurrent.Executors.newSingleThreadExecutor().asCoroutineDispatcher()
    }

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
            "set" -> write { putString(key, obj.optString("value")) }
            "get" -> withContext(lane) { PluginResponse(true, prefs.getString(key, "") ?: "") }
            "delete" -> write { remove(key) }
            else -> PluginResponse(false, "unknown op '$op'")
        }
    }

    // `commit()`, not `apply()`: `apply()` returns while the change is only in memory and never
    // reports a failure, so a process killed before its background write loses a value the core was
    // told is stored (a rotated refresh token the server has already consumed the old one for).
    // `commit()` returns once the file is written, and false when it isn't. It blocks on disk I/O,
    // so it runs off the main thread. When the file write fails, SharedPreferences keeps the change
    // in memory anyway: until the app restarts, a `get` can still return the value that failed.
    private suspend fun write(change: android.content.SharedPreferences.Editor.() -> Unit): PluginResponse =
        withContext(lane) {
            val written = try {
                val editor = prefs.edit()
                editor.change()
                editor.commit()
            } catch (e: Exception) {
                // Opening the store or encrypting the value failed: nothing was written.
                return@withContext PluginResponse(false, "write failed: ${e.message ?: e.javaClass.simpleName}")
            }
            if (written) PluginResponse(true, "") else PluginResponse(false, "write failed")
        }
}
