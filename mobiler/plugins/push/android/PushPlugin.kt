package {{PACKAGE}}

import {{PACKAGE_SHARED_TYPES}}.PluginResponse

import android.app.Activity
import android.content.Intent
import android.os.Build
import android.os.Bundle
import com.google.firebase.messaging.FirebaseMessaging
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.channels.awaitClose
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.callbackFlow

// Remote push (free, bundled). Firebase Cloud Messaging — the only way to get a device token on
// stock Android. Two surfaces:
//   cx.plugin("push", "register", "", |r| Msg::PushToken(r))          // → {"token":"…","platform":"fcm"}
//   cx.subscribe("push", "push", "events", "", |r| Msg::PushEvent(r)) // tagged payloads + token refreshes
//
// `register` requests POST_NOTIFICATIONS (API 33+) and resolves with the FCM token. The events
// stream attaches to `PushBus`. Every notification event is the payload object plus a reserved
// "mobiler_push" key:
//   "received" — arrived while the app was in the foreground (live only, never buffered);
//   "opened"   — the user tapped the notification (buffered until the core subscribes, so a tap that
//                launched a dead process still arrives).
// Token rotations stay {"type":"token_refresh","token":"…"} (buffered).
//
// Needs google-services.json in Android/app/ (carries your Firebase keys; can't be bundled) + the
// google-services Gradle plugin (added by the plugin manifest). The build fails without the JSON.
class PushPlugin(private val application: android.app.Application) : MobilerPlugin {
    init {
        PushOpens.install(application)
    }

    override suspend fun handle(op: String, input: String): PluginResponse = when (op) {
        "register" -> register()
        else -> PluginResponse(false, "unknown op '$op'")
    }

    // Streaming entrypoint (cx.subscribe): emit each push payload until the collecting Job is
    // cancelled (cx.unsubscribe → awaitClose detaches the sink).
    override fun subscribe(op: String, input: String): Flow<PluginResponse> = callbackFlow {
        val sink: (String) -> Unit = { trySend(PluginResponse(true, it)) }
        PushBus.attach(sink)
        awaitClose { PushBus.detach(sink) }
    }

    private suspend fun register(): PluginResponse {
        // Fire the POST_NOTIFICATIONS prompt (best-effort) from the foreground Activity on API 33+.
        if (Build.VERSION.SDK_INT >= 33) {
            MobilerActivity.current?.get()?.let { activity ->
                androidx.core.app.ActivityCompat.requestPermissions(
                    activity, arrayOf(android.Manifest.permission.POST_NOTIFICATIONS), 0
                )
            }
        }
        val deferred = CompletableDeferred<PluginResponse>()
        FirebaseMessaging.getInstance().token.addOnCompleteListener { task ->
            if (task.isSuccessful) {
                val token = task.result.orEmpty()
                deferred.complete(PluginResponse(true, """{"token":"$token","platform":"fcm"}"""))
            } else {
                deferred.complete(PluginResponse(false, task.exception?.message ?: "token unavailable"))
            }
        }
        return deferred.await()
    }
}

// In-process bus between FirebaseMessagingService / PushOpens (which may run before any UI exists) and
// the cx.subscribe stream. Buffered events (taps, token rotations) wait for the core to attach a sink
// and flush on attach; "received" is delivered live or dropped.
object PushBus {
    // Taps are user actions, so a handful is plenty; the bound only guards against a runaway backlog.
    private const val MAX_BUFFERED = 32

    private var sink: ((String) -> Unit)? = null
    private val buffer = ArrayDeque<String>()

    @Synchronized fun attach(s: (String) -> Unit) {
        sink = s
        buffer.forEach { s(it) }
        buffer.clear()
    }

    @Synchronized fun detach(s: (String) -> Unit) {
        if (sink === s) sink = null
    }

    /** Buffered until a sink attaches (token rotations). */
    @Synchronized fun emit(payload: String) {
        val s = sink
        if (s != null) {
            s(payload)
        } else {
            if (buffer.size >= MAX_BUFFERED) buffer.removeFirst()
            buffer.addLast(payload)
        }
    }

    /** A push that arrived in the foreground — delivered live only. */
    @Synchronized fun emitReceived(payloadJson: String) {
        sink?.invoke(tag(payloadJson, "received"))
    }

    /** The user tapped a notification — buffered until a sink attaches. */
    fun emitOpened(payloadJson: String) = emit(tag(payloadJson, "opened"))

    private fun tag(payloadJson: String, kind: String): String {
        val obj = runCatching { org.json.JSONObject(payloadJson) }.getOrElse { org.json.JSONObject() }
        return obj.put("mobiler_push", kind).toString()
    }
}

// Turns a notification tap into an "opened" event. A tap brings the launcher activity forward with the
// payload in its intent — our own data-message notifications carry EXTRA_PAYLOAD; FCM-posted
// notification messages carry their data keys as extras next to `google.message_id`. The intent is
// inspected when the activity resumes (MainActivity's onNewIntent calls setIntent, so a tap on an
// already-running app is seen too) and the marker extras are removed so it fires exactly once.
object PushOpens {
    const val EXTRA_PAYLOAD = "mobiler_push_payload"
    private const val FCM_MESSAGE_ID = "google.message_id"

    private var installed = false

    @Synchronized fun install(application: android.app.Application) {
        if (installed) return
        installed = true
        application.registerActivityLifecycleCallbacks(object : android.app.Application.ActivityLifecycleCallbacks {
            override fun onActivityResumed(activity: Activity) = consume(activity.intent)
            override fun onActivityCreated(activity: Activity, savedInstanceState: Bundle?) {}
            override fun onActivityStarted(activity: Activity) {}
            override fun onActivityPaused(activity: Activity) {}
            override fun onActivityStopped(activity: Activity) {}
            override fun onActivitySaveInstanceState(activity: Activity, outState: Bundle) {}
            override fun onActivityDestroyed(activity: Activity) {}
        })
        // The core (and so this plugin) may be built after the first onResume already ran.
        MobilerActivity.current?.get()?.let { consume(it.intent) }
    }

    private fun consume(intent: Intent?) {
        val extras = intent?.extras ?: return
        // Relaunched from Recents: the original tap intent is replayed, but the user didn't tap anything.
        if ((intent.flags and Intent.FLAG_ACTIVITY_LAUNCHED_FROM_HISTORY) != 0) {
            intent.removeExtra(EXTRA_PAYLOAD)
            intent.removeExtra(FCM_MESSAGE_ID)
            return
        }
        val ours = extras.getString(EXTRA_PAYLOAD)
        if (ours != null) {
            intent.removeExtra(EXTRA_PAYLOAD)
            PushBus.emitOpened(ours)
            return
        }
        if (extras.containsKey(FCM_MESSAGE_ID)) {
            val payload = org.json.JSONObject()
            for (key in extras.keySet()) {
                if (key.startsWith("google.") || key.startsWith("gcm.") || key == "from" || key == "collapse_key") continue
                (extras.get(key) as? String)?.let { payload.put(key, it) }
            }
            intent.removeExtra(FCM_MESSAGE_ID)
            PushBus.emitOpened(payload.toString())
        }
    }
}
