package {{PACKAGE}}

import android.app.Activity
import android.app.AlertDialog
import android.app.Application
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
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.withContext
import java.lang.ref.WeakReference
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import org.json.JSONObject
import {{PACKAGE_SHARED}}.CoreFfi
import {{PACKAGE_SHARED_TYPES}}.Action
import {{PACKAGE_SHARED_TYPES}}.Effect
import {{PACKAGE_SHARED_TYPES}}.PluginResponse
import {{PACKAGE_SHARED_TYPES}}.Requests
import {{PACKAGE_SHARED_TYPES}}.Widget

/**
 * A native capability plugin. The opaque `{plugin, op, input}` envelope is
 * dispatched by name to one of these — adding a plugin never touches the wire
 * ABI or the generated bindings, only this registry. `handle` is `suspend`, so
 * capabilities can do real async work (network, disk) off the main thread;
 * fire-and-forget calls ignore the result, request/response calls send it back.
 */
interface MobilerPlugin {
    suspend fun handle(op: String, input: String): PluginResponse
}

/** Official, bundled plugin (free tier): fire-and-forget toast. */
class ToastPlugin(private val context: Context) : MobilerPlugin {
    override suspend fun handle(op: String, input: String): PluginResponse {
        Toast.makeText(context, input, Toast.LENGTH_SHORT).show()
        return PluginResponse(true, "")
    }
}

/** Official, bundled plugin: request/response device info. */
class DevicePlugin : MobilerPlugin {
    override suspend fun handle(op: String, input: String): PluginResponse = when (op) {
        "model" -> PluginResponse(true, "${Build.MANUFACTURER} ${Build.MODEL}")
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
 * Official, bundled plugin: HTTP (paired with cx.http/get/post/patch/delete in
 * Rust). `op` is the method; `input` is `{"url": ..., "body": ...}`. Runs on the
 * IO dispatcher; returns the response body with `ok` = success (2xx).
 */
class HttpPlugin : MobilerPlugin {
    private val client = OkHttpClient()
    override suspend fun handle(op: String, input: String): PluginResponse = withContext(Dispatchers.IO) {
        try {
            val obj = JSONObject(input)
            val url = obj.getString("url")
            val bodyStr = if (obj.has("body") && !obj.isNull("body")) obj.getString("body") else null
            val reqBody = bodyStr?.toRequestBody("application/json".toMediaType())
            val request = Request.Builder().url(url).method(op, reqBody).build()
            client.newCall(request).execute().use { resp ->
                PluginResponse(resp.isSuccessful, resp.body?.string() ?: "")
            }
        } catch (e: Exception) {
            PluginResponse(false, e.message ?: "http error")
        }
    }
}

// Bridge between the (generic) shell and the Rust core. Speaks ONLY the fixed
// Mobiler ABI: sends an `Action`, receives a `Widget` tree + capability effects.
class Core(application: Application) : AndroidViewModel(application) {
    private val core: CoreFfi = CoreFfi()

    // The shell's plugin registry. A custom/cloud build registers more here
    // (e.g. premium plugins); the generic shell ships only the official ones.
    private val plugins: Map<String, MobilerPlugin> = mapOf(
        "toast" to ToastPlugin(application),
        "device" to DevicePlugin(),
        "storage" to StoragePlugin(application),
        "http" to HttpPlugin(),
        "clipboard" to ClipboardPlugin(application),
        "share" to SharePlugin(application),
        "browser" to BrowserPlugin(application),
        "haptics" to HapticsPlugin(application),
        "dialog" to DialogPlugin(),
        "photo" to PhotoPlugin(),
        "camera" to CameraPlugin(),
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
                is Effect.PluginNotify -> dispatch(effect.value.plugin, effect.value.op, effect.value.input)
                // Request/response: dispatch (awaiting any async work), resolve the
                // core with the response, then process the effects that produces.
                is Effect.Plugin -> {
                    val resp = dispatch(effect.value.plugin, effect.value.op, effect.value.input)
                    process(core.resolve(request.id, resp.bincodeSerialize()))
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
}
