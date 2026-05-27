package dev.mobiler.coffee

import android.app.Application
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.util.Log
import android.widget.Toast
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.lifecycle.AndroidViewModel
import dev.mobiler.coffee.shared.CoreFfi
import dev.mobiler.coffee.shared.types.Action
import dev.mobiler.coffee.shared.types.Effect
import dev.mobiler.coffee.shared.types.PluginResponse
import dev.mobiler.coffee.shared.types.Requests
import dev.mobiler.coffee.shared.types.Widget

/**
 * A native capability plugin. The opaque `{plugin, op, input}` envelope is
 * dispatched by name to one of these — adding a plugin never touches the wire
 * ABI or the generated bindings, only this registry. Returns a [PluginResponse]
 * (ignored for fire-and-forget calls, sent back to the core for request/response).
 */
interface MobilerPlugin {
    fun handle(op: String, input: String): PluginResponse
}

/** Official, bundled plugin (free tier): fire-and-forget toast. */
class ToastPlugin(private val context: Context) : MobilerPlugin {
    override fun handle(op: String, input: String): PluginResponse {
        Toast.makeText(context, input, Toast.LENGTH_SHORT).show()
        return PluginResponse(true, "")
    }
}

/** Official, bundled plugin: request/response device info. */
class DevicePlugin : MobilerPlugin {
    override fun handle(op: String, input: String): PluginResponse = when (op) {
        "model" -> PluginResponse(true, "${Build.MANUFACTURER} ${Build.MODEL}")
        else -> PluginResponse(false, "unknown op '$op'")
    }
}

/** Official, bundled plugin: copy text to the system clipboard. */
class ClipboardPlugin(private val context: Context) : MobilerPlugin {
    override fun handle(op: String, input: String): PluginResponse {
        val cm = context.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
        cm.setPrimaryClip(ClipData.newPlainText("text", input))
        return PluginResponse(true, "")
    }
}

/** Official, bundled plugin: open the system share sheet with `input` as text. */
class SharePlugin(private val context: Context) : MobilerPlugin {
    override fun handle(op: String, input: String): PluginResponse {
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
    override fun handle(op: String, input: String): PluginResponse {
        val view = Intent(Intent.ACTION_VIEW, Uri.parse(input)).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        context.startActivity(view)
        return PluginResponse(true, "")
    }
}

/** Official, bundled plugin: persist a state blob (paired with cx.save in Rust). */
class StoragePlugin(private val context: Context) : MobilerPlugin {
    private val prefs get() = context.getSharedPreferences("mobiler", Context.MODE_PRIVATE)
    override fun handle(op: String, input: String): PluginResponse = when (op) {
        "save" -> { prefs.edit().putString("state", input).apply(); PluginResponse(true, "") }
        "load" -> PluginResponse(true, prefs.getString("state", "") ?: "")
        else -> PluginResponse(false, "unknown op '$op'")
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
        "clipboard" to ClipboardPlugin(application),
        "share" to SharePlugin(application),
        "browser" to BrowserPlugin(application),
    )

    var view: Widget by mutableStateOf(Widget.bincodeDeserialize(core.view()))
        private set

    init {
        // Hand any persisted state back to the core before the first frame.
        val saved = application.getSharedPreferences("mobiler", Context.MODE_PRIVATE).getString("state", "") ?: ""
        if (saved.isNotEmpty()) update(Action.Restore(saved))
    }

    fun update(action: Action) {
        process(core.update(action.bincodeSerialize()))
    }

    private fun process(effectBytes: ByteArray) {
        val requests = Requests.bincodeDeserialize(effectBytes).value
        for (request in requests) {
            when (val effect = request.effect) {
                is Effect.Render -> view = Widget.bincodeDeserialize(core.view())
                // Fire-and-forget: dispatch, ignore the result, don't resolve.
                is Effect.PluginNotify -> dispatch(effect.value.plugin, effect.value.op, effect.value.input)
                // Request/response: dispatch, resolve the core with the response,
                // then process the effects that resolution produces.
                is Effect.Plugin -> {
                    val resp = dispatch(effect.value.plugin, effect.value.op, effect.value.input)
                    process(core.resolve(request.id, resp.bincodeSerialize()))
                }
            }
        }
    }

    private fun dispatch(plugin: String, op: String, input: String): PluginResponse {
        val p = plugins[plugin]
        if (p == null) {
            // An app using this plugin needs a custom build that registers it.
            Log.w("Mobiler", "plugin '$plugin' not available in this build")
            return PluginResponse(false, "plugin '$plugin' not available in this build")
        }
        return p.handle(op, input)
    }
}
