package {{PACKAGE}}

import android.app.Application
import android.content.Context
import android.os.Build
import android.util.Log
import android.widget.Toast
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.lifecycle.AndroidViewModel
import {{PACKAGE_SHARED}}.CoreFfi
import {{PACKAGE_SHARED_TYPES}}.Action
import {{PACKAGE_SHARED_TYPES}}.Effect
import {{PACKAGE_SHARED_TYPES}}.PluginResponse
import {{PACKAGE_SHARED_TYPES}}.Requests
import {{PACKAGE_SHARED_TYPES}}.Widget

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
