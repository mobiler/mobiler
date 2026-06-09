package {{PACKAGE}}

import {{PACKAGE_SHARED_TYPES}}.PluginResponse

import android.app.Application
import android.os.Bundle
import com.google.firebase.FirebaseApp
import com.google.firebase.analytics.FirebaseAnalytics
import com.google.firebase.crashlytics.FirebaseCrashlytics
import org.json.JSONObject

// analytics (free, bundled, EXPERIMENTAL). Product analytics + automatic crash capture via Firebase
// Analytics + Crashlytics. Fire-and-forget request/response (no stream). Crash capture is AUTOMATIC
// once Firebase is initialized (it auto-inits on Android via google-services + the firebase-common
// ContentProvider — no launch hook needed). Ops (input is JSON unless noted):
//   logEvent {name, params?}   setUserId <id>   setUserProperty {name, value}   setEnabled "true"|"false"
//   log <message>              recordError {message, domain?}                    testCrash ""  (dev only)
//
// Needs google-services.json in Android/app/ — the google-services Gradle plugin reads it at build time
// and the build FAILS without it.
class AnalyticsPlugin(private val application: Application) : MobilerPlugin {
    private val analytics by lazy { FirebaseAnalytics.getInstance(application) }
    private val crashlytics by lazy { FirebaseCrashlytics.getInstance() }

    override suspend fun handle(op: String, input: String): PluginResponse {
        if (FirebaseApp.getApps(application).isEmpty()) {
            return PluginResponse(false, "Firebase not configured — add google-services.json")
        }
        return when (op) {
            "logEvent" -> logEvent(input)
            "setUserId" -> {
                analytics.setUserId(input.ifEmpty { null })
                PluginResponse(true, "")
            }
            "setUserProperty" -> setUserProperty(input)
            "setEnabled" -> setEnabled(input == "true")
            "log" -> {
                crashlytics.log(input)
                PluginResponse(true, "")
            }
            "recordError" -> recordError(input)
            // Deliberate crash to verify Crashlytics wiring — the report uploads on the NEXT launch.
            "testCrash" -> throw RuntimeException("analytics testCrash")
            else -> PluginResponse(false, "unknown op '$op'")
        }
    }

    private fun logEvent(input: String): PluginResponse {
        val obj = runCatching { JSONObject(input) }.getOrNull()
            ?: return PluginResponse(false, "invalid input JSON")
        val name = obj.optString("name").ifEmpty { return PluginResponse(false, "expected {name, params?}") }
        val bundle = Bundle()
        obj.optJSONObject("params")?.let { params ->
            for (key in params.keys()) {
                when (val v = params.get(key)) {
                    is String -> bundle.putString(key, v)
                    is Int -> bundle.putLong(key, v.toLong())
                    is Long -> bundle.putLong(key, v)
                    is Double -> bundle.putDouble(key, v)
                    is Boolean -> bundle.putString(key, v.toString())
                    else -> bundle.putString(key, v.toString())
                }
            }
        }
        analytics.logEvent(name, bundle)
        return PluginResponse(true, "")
    }

    private fun setUserProperty(input: String): PluginResponse {
        val obj = runCatching { JSONObject(input) }.getOrNull()
            ?: return PluginResponse(false, "invalid input JSON")
        val name = obj.optString("name").ifEmpty { return PluginResponse(false, "expected {name, value}") }
        analytics.setUserProperty(name, obj.optString("value").ifEmpty { null })
        return PluginResponse(true, "")
    }

    private fun setEnabled(on: Boolean): PluginResponse {
        analytics.setAnalyticsCollectionEnabled(on)
        crashlytics.setCrashlyticsCollectionEnabled(on)
        return PluginResponse(true, "")
    }

    private fun recordError(input: String): PluginResponse {
        val obj = runCatching { JSONObject(input) }.getOrNull()
        val message = obj?.optString("message")?.ifEmpty { null } ?: "error"
        crashlytics.recordException(RuntimeException(message))
        return PluginResponse(true, "")
    }
}
