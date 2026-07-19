package dev.mobiler.barbershop

import android.app.Activity
import android.app.AlertDialog
import android.app.Application
import android.app.DatePickerDialog
import android.app.TimePickerDialog
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.os.VibrationEffect
import android.os.Vibrator
import android.os.VibratorManager
import android.util.Log
import android.widget.Toast
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.flow.collect
import kotlinx.coroutines.channels.awaitClose
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.withContext
import java.io.IOException
import java.lang.ref.WeakReference
import java.util.Calendar
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import org.json.JSONObject
import dev.mobiler.barbershop.shared.CoreFfi
import dev.mobiler.barbershop.shared.types.Action
import dev.mobiler.barbershop.shared.types.Effect
import dev.mobiler.barbershop.shared.types.HttpHeader
import dev.mobiler.barbershop.shared.types.HttpOutcome
import dev.mobiler.barbershop.shared.types.PluginResponse
import dev.mobiler.barbershop.shared.types.Requests
import dev.mobiler.barbershop.shared.types.Widget

// Keeps every non-HTTP plugin compiling unchanged now that PluginResponse.output is
// bytes. Kotlin allows a top-level function named like the type, so existing
// `PluginResponse(true, "text")` call sites resolve here.
fun PluginResponse(ok: Boolean, output: String): PluginResponse =
    PluginResponse(ok, output.toByteArray(Charsets.UTF_8).toUByteList())

/**
 * A native capability plugin. The opaque `{plugin, op, input}` envelope is
 * dispatched by name to one of these — adding a plugin never touches the wire
 * ABI or the generated bindings, only this registry. `handle` is `suspend`, so
 * capabilities can do real async work (network, disk) off the main thread;
 * fire-and-forget calls ignore the result, request/response calls send it back.
 */
interface MobilerPlugin {
    suspend fun handle(op: String, input: String): PluginResponse

    /** Streaming subscription (cx.subscribe): emit a PluginResponse per event until
     *  the collecting coroutine is cancelled (cx.unsubscribe). Default: no stream —
     *  only streaming-capable plugins override this. */
    fun subscribe(op: String, input: String): kotlinx.coroutines.flow.Flow<PluginResponse> =
        kotlinx.coroutines.flow.emptyFlow()
}

/** Official, bundled plugin (free tier): fire-and-forget toast. */
class ToastPlugin(private val context: Context) : MobilerPlugin {
    override suspend fun handle(op: String, input: String): PluginResponse {
        Toast.makeText(context, input, Toast.LENGTH_SHORT).show()
        return PluginResponse(true, "")
    }
}

/** Built-in `ticker` stream: emits an incrementing counter every `input` ms until the
 *  collecting coroutine is cancelled (cx.unsubscribe). The deterministic demonstrator
 *  for the streaming primitive (cx.subscribe). */
class TickerPlugin : MobilerPlugin {
    override suspend fun handle(op: String, input: String): PluginResponse =
        PluginResponse(false, "ticker is a streaming capability — use cx.subscribe")
    override fun subscribe(op: String, input: String): kotlinx.coroutines.flow.Flow<PluginResponse> {
        val ms = input.toLongOrNull() ?: 1000L
        return kotlinx.coroutines.flow.flow {
            var count = 0
            while (true) {
                kotlinx.coroutines.delay(ms)
                count += 1
                emit(PluginResponse(true, count.toString()))
            }
        }
    }
}

/** In-process bus from MainActivity (deep-link intents + fg/bg lifecycle) to the built-in `system`
 *  cx.subscribe stream. Deep links arriving before the core subscribes buffer and flush on attach
 *  (launch-from-dead); lifecycle emits live, with the current state sent on attach. */
object SystemBus {
    private var sink: ((String) -> Unit)? = null
    private val buffer = mutableListOf<String>()
    private var lastState = "active"

    @Synchronized fun attach(s: (String) -> Unit) {
        sink = s
        buffer.forEach { s(it) }
        buffer.clear()
        s("""{"type":"lifecycle","state":"$lastState"}""")
    }
    @Synchronized fun detach(s: (String) -> Unit) { if (sink === s) sink = null }

    @Synchronized fun emitDeepLink(url: String) {
        val payload = """{"type":"deeplink","url":${org.json.JSONObject.quote(url)}}"""
        val s = sink
        if (s != null) s(payload) else buffer.add(payload)
    }
    @Synchronized fun emitLifecycle(state: String) {
        lastState = state
        sink?.invoke("""{"type":"lifecycle","state":"$state"}""")  // live only — not buffered
    }
}

/** Built-in `system` stream: deep-link URLs + app lifecycle (foreground/background), fed by
 *  MainActivity via SystemBus. Mirrors the push plugin's PushBus/callbackFlow shape. */
class SystemPlugin : MobilerPlugin {
    override suspend fun handle(op: String, input: String): PluginResponse =
        PluginResponse(false, "system is a streaming capability — use cx.subscribe")
    override fun subscribe(op: String, input: String): kotlinx.coroutines.flow.Flow<PluginResponse> =
        kotlinx.coroutines.flow.callbackFlow {
            val sink: (String) -> Unit = { trySend(PluginResponse(true, it)) }
            SystemBus.attach(sink)
            awaitClose { SystemBus.detach(sink) }
        }
}

/** Official, bundled plugin: request/response device info. */
class DevicePlugin : MobilerPlugin {
    override suspend fun handle(op: String, input: String): PluginResponse = when (op) {
        "model" -> PluginResponse(true, "${Build.MANUFACTURER} ${Build.MODEL}")
        "locale" -> PluginResponse(true, java.util.Locale.getDefault().toLanguageTag())
        else -> PluginResponse(false, "unknown op '$op'")
    }
}

/** Official, bundled plugin: copy text to the system clipboard. */
class ClipboardPlugin(private val context: Context) : MobilerPlugin {
    override suspend fun handle(op: String, input: String): PluginResponse {
        val cm = context.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
        cm.setPrimaryClip(ClipData.newPlainText("text", input))
        return PluginResponse(true, "")
    }
}

/** Official, bundled plugin: open the system share sheet with `input` as text. */
class SharePlugin(private val context: Context) : MobilerPlugin {
    override suspend fun handle(op: String, input: String): PluginResponse {
        val send = Intent(Intent.ACTION_SEND).apply {
            type = "text/plain"
            putExtra(Intent.EXTRA_TEXT, input)
        }
        // Launched from a non-Activity (Application) context, so NEW_TASK is required.
        context.startActivity(Intent.createChooser(send, null).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
        return PluginResponse(true, "")
    }
}

/** Official, bundled plugin: open a URL in the browser / default handler. */
class BrowserPlugin(private val context: Context) : MobilerPlugin {
    override suspend fun handle(op: String, input: String): PluginResponse {
        val view = Intent(Intent.ACTION_VIEW, Uri.parse(input)).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        context.startActivity(view)
        return PluginResponse(true, "")
    }
}

/** Official, bundled plugin: a haptic tap. `op` is the style; needs VIBRATE (manifest). */
class HapticsPlugin(private val context: Context) : MobilerPlugin {
    override suspend fun handle(op: String, input: String): PluginResponse {
        val vibrator = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            (context.getSystemService(Context.VIBRATOR_MANAGER_SERVICE) as VibratorManager).defaultVibrator
        } else {
            @Suppress("DEPRECATION") context.getSystemService(Context.VIBRATOR_SERVICE) as Vibrator
        }
        val ms = when (op) { "light" -> 10L; "heavy" -> 40L; else -> 20L }
        vibrator.vibrate(VibrationEffect.createOneShot(ms, VibrationEffect.DEFAULT_AMPLITUDE))
        return PluginResponse(true, "")
    }
}

/**
 * Holds the current Activity (weakly) so capabilities that need a window — a dialog
 * has no Application-context window — can reach it. MainActivity updates it in
 * onResume/onPause.
 */
object MobilerActivity {
    var current: WeakReference<Activity>? = null
}

/** Official, bundled plugin: confirm dialog (request/response). Input is JSON
 *  {title, message}; resolves ok=true when confirmed. Suspends until the user taps. */
class DialogPlugin : MobilerPlugin {
    override suspend fun handle(op: String, input: String): PluginResponse {
        if (op != "confirm") return PluginResponse(false, "unknown op '$op'")
        val activity = MobilerActivity.current?.get() ?: return PluginResponse(false, "no activity")
        val obj = JSONObject(input)
        val title = obj.optString("title")
        val message = obj.optString("message")
        return withContext(Dispatchers.Main) {
            suspendCancellableCoroutine { cont ->
                val dialog = AlertDialog.Builder(activity)
                    .setTitle(title.ifEmpty { null })
                    .setMessage(message)
                    .setPositiveButton("OK") { _, _ -> cont.resumeWith(Result.success(PluginResponse(true, "ok"))) }
                    .setNegativeButton("Cancel") { _, _ -> cont.resumeWith(Result.success(PluginResponse(false, "cancel"))) }
                    .setOnCancelListener { cont.resumeWith(Result.success(PluginResponse(false, "cancel"))) }
                    .create()
                cont.invokeOnCancellation { dialog.dismiss() }
                dialog.show()
            }
        }
    }
}

/** Official, bundled plugin: native date / time pickers (request/response). op is
 *  "date" (→ ISO "YYYY-MM-DD") or "time" (→ 24-hour "HH:MM"); ok=false on cancel.
 *  Suspends until the user picks or dismisses. */
class DateTimePlugin : MobilerPlugin {
    override suspend fun handle(op: String, input: String): PluginResponse {
        val activity = MobilerActivity.current?.get() ?: return PluginResponse(false, "no activity")
        val now = Calendar.getInstance()
        return withContext(Dispatchers.Main) {
            suspendCancellableCoroutine { cont ->
                var resumed = false
                fun done(r: PluginResponse) {
                    if (!resumed) { resumed = true; cont.resumeWith(Result.success(r)) }
                }
                when (op) {
                    "date" -> {
                        val dlg = DatePickerDialog(
                            activity,
                            { _, y, m, d -> done(PluginResponse(true, "%04d-%02d-%02d".format(y, m + 1, d))) },
                            now.get(Calendar.YEAR), now.get(Calendar.MONTH), now.get(Calendar.DAY_OF_MONTH),
                        )
                        dlg.setOnCancelListener { done(PluginResponse(false, "cancel")) }
                        cont.invokeOnCancellation { dlg.dismiss() }
                        dlg.show()
                    }
                    "time" -> {
                        val dlg = TimePickerDialog(
                            activity,
                            { _, h, min -> done(PluginResponse(true, "%02d:%02d".format(h, min))) },
                            now.get(Calendar.HOUR_OF_DAY), now.get(Calendar.MINUTE), true,
                        )
                        dlg.setOnCancelListener { done(PluginResponse(false, "cancel")) }
                        cont.invokeOnCancellation { dlg.dismiss() }
                        dlg.show()
                    }
                    else -> done(PluginResponse(false, "unknown op '$op'"))
                }
            }
        }
    }
}

/**
 * Set by MainActivity: launches the system photo picker and calls back with the
 * picked image URI (null if cancelled). The ActivityResult launcher must be
 * registered on the Activity, so it can't live in the (Application-context) plugin.
 */
object PhotoPicker {
    var launch: ((onResult: (String?) -> Unit) -> Unit)? = null
}

/** Official, bundled plugin: pick an image via the system photo picker (no permission). */
class PhotoPlugin : MobilerPlugin {
    override suspend fun handle(op: String, input: String): PluginResponse {
        if (op != "pick") return PluginResponse(false, "unknown op '$op'")
        val launch = PhotoPicker.launch ?: return PluginResponse(false, "photo picker unavailable")
        val uri = suspendCancellableCoroutine<String?> { cont ->
            launch { result -> cont.resumeWith(Result.success(result)) }
        }
        return if (uri != null) PluginResponse(true, uri) else PluginResponse(false, "cancelled")
    }
}

/**
 * Set by MainActivity: launches the system camera and calls back with the captured
 * image URI (null if cancelled). Like [PhotoPicker], the ActivityResult launcher must
 * be registered on the Activity, so it can't live in the (Application-context) plugin.
 */
object CameraCapture {
    var launch: ((onResult: (String?) -> Unit) -> Unit)? = null
}

/** Official, bundled plugin: capture a photo with the system camera (cx.capture_photo).
 *  Intent-based (the system camera app handles capture), so no CAMERA permission. */
class CameraPlugin : MobilerPlugin {
    override suspend fun handle(op: String, input: String): PluginResponse {
        if (op != "capture") return PluginResponse(false, "unknown op '$op'")
        val launch = CameraCapture.launch ?: return PluginResponse(false, "camera unavailable")
        val uri = suspendCancellableCoroutine<String?> { cont ->
            launch { result -> cont.resumeWith(Result.success(result)) }
        }
        return if (uri != null) PluginResponse(true, uri) else PluginResponse(false, "cancelled")
    }
}

/** Official, bundled plugin: persist a state blob (paired with cx.save in Rust). */
class StoragePlugin(private val context: Context) : MobilerPlugin {
    private val prefs get() = context.getSharedPreferences("mobiler", Context.MODE_PRIVATE)
    override suspend fun handle(op: String, input: String): PluginResponse = when (op) {
        "save" -> { prefs.edit().putString("state", input).apply(); PluginResponse(true, "") }
        "load" -> PluginResponse(true, prefs.getString("state", "") ?: "")
        else -> PluginResponse(false, "unknown op '$op'")
    }
}

/**
 * HTTP capability (paired with `cx.request`/`get`/`post`/`put`/... in Rust). `op` is
 * the method; `input` is `{"url":..., "headers":[{"name":...,"value":...}], "body":...}`.
 * Runs on the IO dispatcher; returns a bincode `HttpOutcome` with `ok` = 2xx.
 */
class HttpPlugin : MobilerPlugin {
    private val client = OkHttpClient()

    override suspend fun handle(op: String, input: String): PluginResponse = withContext(Dispatchers.IO) {
        try {
            val obj = JSONObject(input)
            val url = obj.getString("url")
            val bodyStr = if (obj.has("body") && !obj.isNull("body")) obj.getString("body") else null

            var callerSetContentType = false
            val builder = Request.Builder().url(url)
            if (obj.has("headers")) {
                val headers = obj.getJSONArray("headers")
                for (i in 0 until headers.length()) {
                    val h = headers.getJSONObject(i)
                    val name = h.getString("name")
                    builder.addHeader(name, h.getString("value"))
                    if (name.lowercase() == "content-type") callerSetContentType = true
                }
            }

            // OkHttp REQUIRES a body for these verbs — `.method("PUT", null)` throws.
            val needsBody = op in setOf("POST", "PUT", "PATCH")
            val mediaType = if (callerSetContentType) null else "application/json".toMediaType()
            val reqBody = when {
                bodyStr != null -> bodyStr.toRequestBody(mediaType)
                // A bodyless PUT/POST/PATCH must send no Content-Type at all — matching
                // iOS's addValue and web's fetch, which both omit it for an empty body.
                needsBody -> "".toRequestBody(null)
                else -> null
            }

            val request = builder.method(op, reqBody).build()
            client.newCall(request).execute().use { resp ->
                val headers = resp.headers.map { (name, value) -> HttpHeader(name, value) }
                val bytes = resp.body?.bytes() ?: ByteArray(0)
                val outcome = HttpOutcome.Response(resp.code.toUShort(), headers, bytes.toUByteList())
                PluginResponse(resp.isSuccessful, outcome.bincodeSerialize().toUByteList())
            }
        } catch (e: IOException) {
            // IOException means no response was obtained.
            transportError(e.message ?: "network error")
        } catch (e: Exception) {
            transportError(e.message ?: "http error")
        }
    }

    private fun transportError(message: String): PluginResponse =
        PluginResponse(false, HttpOutcome.TransportError(message).bincodeSerialize().toUByteList())
}

/**
 * The generated types use List<UByte>, not List<Byte> — ByteArray.toList() gives
 * the wrong element type and will not compile.
 */
private fun ByteArray.toUByteList(): List<UByte> = this.map { it.toUByte() }

// Bridge between the (generic) shell and the Rust core. Speaks ONLY the fixed
// Mobiler ABI: sends an `Action`, receives a `Widget` tree + capability effects.
class Core(application: Application) : AndroidViewModel(application) {
    private val core: CoreFfi = CoreFfi()

    // Live streaming subscriptions (cx.subscribe), keyed by subscription key, so
    // cx.unsubscribe(key) can cancel the matching collecting coroutine.
    private val streamJobs = mutableMapOf<String, kotlinx.coroutines.Job>()

    // The shell's plugin registry. A custom/cloud build registers more here
    // (e.g. premium plugins); the generic shell ships only the official ones.
    private val plugins: Map<String, MobilerPlugin> = mapOf(
        "toast" to ToastPlugin(application),
        "device" to DevicePlugin(),
        "ticker" to TickerPlugin(),
        "system" to SystemPlugin(),
        "storage" to StoragePlugin(application),
        "http" to HttpPlugin(),
        "clipboard" to ClipboardPlugin(application),
        "share" to SharePlugin(application),
        "browser" to BrowserPlugin(application),
        "haptics" to HapticsPlugin(application),
        "dialog" to DialogPlugin(),
        "datetime" to DateTimePlugin(),
        "photo" to PhotoPlugin(),
        "camera" to CameraPlugin(),
        "connectivity" to ConnectivityPlugin(application),
        "geolocation" to GeolocationPlugin(application),
        "contacts" to ContactsPlugin(application),
        "calendar" to CalendarPlugin(application),
        "audio" to AudioPlugin(application),
        "sensors" to SensorsPlugin(application),
        "composer" to ComposerPlugin(application),
        "tts" to TtsPlugin(application),
        "review" to ReviewPlugin(application),
        "sharefile" to SharefilePlugin(application),
        "video" to VideoPlugin(application),
        "sqlite" to SqlitePlugin(application),
        "speech" to SpeechPlugin(application),
        "bluetooth" to BluetoothPlugin(application),
        "oauth" to OAuthPlugin(application),
        "websocket" to WebSocketPlugin(application),
        "files" to FilesPlugin(application),
        "geofence" to GeofencePlugin(application),
        "background-fetch" to BackgroundFetchPlugin(application),
        // mobiler:plugins — `mobiler plugin add` inserts plugin registrations above this line
    )

    var view: Widget by mutableStateOf(Widget.bincodeDeserialize(core.view()))
        private set

    init {
        // Hand any persisted state back to the core before the first frame.
        val saved = application.getSharedPreferences("mobiler", Context.MODE_PRIVATE).getString("state", "") ?: ""
        if (saved.isNotEmpty()) update(Action.Restore(saved))
        // Always fire Start (after any Restore) so the app can load initial data.
        update(Action.Start)
    }

    fun update(action: Action) {
        // Effects resolve on a coroutine so async capabilities (http) don't block
        // the main thread. In-flight request/response calls are tracked by the core
        // via request ids, so interleaved updates are fine.
        viewModelScope.launch { process(core.update(action.bincodeSerialize())) }
    }

    private suspend fun process(effectBytes: ByteArray) {
        val requests = Requests.bincodeDeserialize(effectBytes).value
        for (request in requests) {
            when (val effect = request.effect) {
                is Effect.Render -> view = Widget.bincodeDeserialize(core.view())
                // Fire-and-forget: dispatch, ignore the result, don't resolve.
                // `stream`/`unsubscribe` cancels a live subscription; else dispatch.
                is Effect.PluginNotify -> {
                    val n = effect.value
                    if (n.plugin == "stream" && n.op == "unsubscribe") streamJobs.remove(n.input)?.cancel()
                    else dispatch(n.plugin, n.op, n.input)
                }
                // Request/response: dispatch (awaiting any async work), resolve the
                // core with the response, then process the effects that produces.
                is Effect.Plugin -> {
                    val resp = dispatch(effect.value.plugin, effect.value.op, effect.value.input)
                    process(core.resolve(request.id, resp.bincodeSerialize()))
                }
                // Streaming subscription: a native source (Flow) resolves the SAME
                // request id once per event until the Job is cancelled (unsubscribe).
                is Effect.PluginStream -> {
                    val call = effect.value
                    val id = request.id
                    streamJobs[call.key] = viewModelScope.launch {
                        dispatchStream(call.plugin, call.op, call.input).collect { resp ->
                            process(core.resolve(id, resp.bincodeSerialize()))
                        }
                    }
                }
            }
        }
    }

    private suspend fun dispatch(plugin: String, op: String, input: String): PluginResponse {
        val p = plugins[plugin]
        if (p == null) {
            // An app using this plugin needs a custom build that registers it.
            Log.w("Mobiler", "plugin '$plugin' not available in this build")
            return PluginResponse(false, "plugin '$plugin' not available in this build")
        }
        return p.handle(op, input)
    }

    /** Streaming dispatch (cx.subscribe): the named plugin's event Flow, or an empty
     *  flow if it isn't registered / isn't streaming-capable. */
    private fun dispatchStream(plugin: String, op: String, input: String): kotlinx.coroutines.flow.Flow<PluginResponse> {
        val p = plugins[plugin] ?: return kotlinx.coroutines.flow.emptyFlow()
        return p.subscribe(op, input)
    }
}
