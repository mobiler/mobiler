package {{PACKAGE}}

import android.content.Intent
import android.net.Uri
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.browser.customtabs.CustomTabsIntent

/** Static relay between OAuthPlugin (Application context) and the transient OAuthActivity
 *  (which owns the browser launch + the redirect intent). */
object OAuthRelay {
    var authUrl: String? = null
    var onResult: ((String?) -> Unit)? = null
}

/**
 * Transient Activity that runs the OAuth browser round-trip and relays the redirect URL back to
 * OAuthPlugin, then finishes. Shipped by the oauth plugin so it needs no edits to MainActivity.
 *
 * Declared `singleTask` with an intent-filter on `${applicationId}` (see the plugin manifest), so
 * the provider's redirect to "<applicationId>://…" is delivered here as a new VIEW intent. The
 * flow (mirrors AppAuth's management activity, which solves the "onResume fires right after we
 * launch the tab" race):
 *   1st onResume → launch the Custom Tab, set authStarted, return (don't evaluate yet).
 *   redirect    → onNewIntent stores the redirect URL; then onResume runs again.
 *   2nd onResume → if the intent now has redirect data → success; otherwise the user backed out
 *                  of the tab → cancelled.
 */
class OAuthActivity : ComponentActivity() {
    private var authStarted = false

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        if (savedInstanceState != null) {
            authStarted = savedInstanceState.getBoolean(KEY_AUTH_STARTED, false)
        }
    }

    override fun onResume() {
        super.onResume()
        if (!authStarted) {
            val url = OAuthRelay.authUrl
            if (url == null) { finishWith(null); return }
            try {
                CustomTabsIntent.Builder().build().launchUrl(this, Uri.parse(url))
                authStarted = true
            } catch (e: Exception) {
                finishWith(null)
            }
            return
        }
        // Back from the browser. A redirect arrived via onNewIntent → intent.data is set (success);
        // otherwise the user dismissed the tab without completing (cancelled).
        finishWith(intent?.data?.toString())
    }

    override fun onNewIntent(newIntent: Intent) {
        super.onNewIntent(newIntent)
        setIntent(newIntent)
    }

    override fun onSaveInstanceState(outState: Bundle) {
        super.onSaveInstanceState(outState)
        outState.putBoolean(KEY_AUTH_STARTED, authStarted)
    }

    private fun finishWith(result: String?) {
        OAuthRelay.onResult?.invoke(result)
        OAuthRelay.onResult = null
        OAuthRelay.authUrl = null
        finish()
    }

    private companion object {
        const val KEY_AUTH_STARTED = "authStarted"
    }
}
