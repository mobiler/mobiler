package {{PACKAGE}}

import {{PACKAGE_SHARED_TYPES}}.PluginResponse

import android.os.Build
import com.google.firebase.messaging.FirebaseMessaging
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.channels.awaitClose
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.callbackFlow

// Remote push (free, bundled). Firebase Cloud Messaging — the only way to get a device token on
// stock Android. Two surfaces:
//   cx.plugin("push", "register", "", |r| Msg::PushToken(r))          // → {"token":"…","platform":"fcm"}
//   cx.subscribe("push", "push", "events", "", |r| Msg::PushEvent(r)) // received payloads + token refreshes
//
// `register` requests POST_NOTIFICATIONS (API 33+) and resolves with the FCM token. The events
// stream attaches to `PushBus`, which `PushMessagingService` feeds from onMessageReceived /
// onNewToken — and which buffers payloads that arrive before the core subscribes (e.g. a message
// that woke a dead process), flushing them on attach.
//
// Needs google-services.json in Android/app/ (carries your Firebase keys; can't be bundled) + the
// google-services Gradle plugin (added by the plugin manifest). The build fails without the JSON.
class PushPlugin(private val application: android.app.Application) : MobilerPlugin {
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

// In-process bus between FirebaseMessagingService (which may run before any UI exists) and the
// cx.subscribe stream. Buffers payloads that arrive before the core attaches a sink, then flushes
// them on attach — so a notification that launched a dead process still reaches the app.
object PushBus {
    private var sink: ((String) -> Unit)? = null
    private val buffer = mutableListOf<String>()

    @Synchronized fun attach(s: (String) -> Unit) {
        sink = s
        buffer.forEach { s(it) }
        buffer.clear()
    }

    @Synchronized fun detach(s: (String) -> Unit) {
        if (sink === s) sink = null
    }

    @Synchronized fun emit(payload: String) {
        val s = sink
        if (s != null) s(payload) else buffer.add(payload)
    }
}
