package {{PACKAGE}}

import {{PACKAGE_SHARED_TYPES}}.PluginResponse

import android.app.Application
import android.content.Intent
import kotlinx.coroutines.suspendCancellableCoroutine
import org.json.JSONObject

/** Free bundled plugin: OAuth 2.0 / OIDC login. op "login", input = JSON
 *  {"url": "<authorize URL>", "scheme": "<callback scheme>"}. Launches a transient
 *  [OAuthActivity] (shipped with this plugin) that opens a Chrome Custom Tab and catches the
 *  provider's redirect to the app's custom scheme, then relays the redirect URL back.
 *
 *  ok:true  → output = the full redirect URL ("<scheme>://…?code=…&state=…")
 *  ok:false → "cancelled" (user dismissed the tab) or an error.
 *
 *  The `scheme` field is informational on Android — the redirect is caught by OAuthActivity's
 *  intent-filter on `${applicationId}`, so the redirect scheme must equal the application id.
 *  The Rust core builds the authorize URL and, on success, parses code/state, exchanges via
 *  cx.http, and stores tokens via the securestore plugin. */
class OAuthPlugin(private val application: Application) : MobilerPlugin {
    override suspend fun handle(op: String, input: String): PluginResponse {
        if (op != "login") return PluginResponse(false, "unknown op '$op'")
        val url = try {
            JSONObject(input).getString("url")
        } catch (e: Exception) {
            return PluginResponse(false, "input must be JSON {\"url\":…, \"scheme\":…}")
        }
        val activity = MobilerActivity.current?.get() ?: return PluginResponse(false, "no foreground activity")

        val result = suspendCancellableCoroutine<String?> { cont ->
            OAuthRelay.authUrl = url
            OAuthRelay.onResult = { r -> if (cont.isActive) cont.resumeWith(Result.success(r)) }
            activity.startActivity(Intent(activity, OAuthActivity::class.java))
        }
        return if (result != null) PluginResponse(true, result) else PluginResponse(false, "cancelled")
    }
}
