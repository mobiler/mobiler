package dev.mobiler.abi

import android.app.Application
import android.content.Context
import android.util.Log
import android.widget.Toast
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.lifecycle.AndroidViewModel
import dev.mobiler.abi.shared.CoreFfi
import dev.mobiler.abi.shared.types.Action
import dev.mobiler.abi.shared.types.Effect
import dev.mobiler.abi.shared.types.Widget

/**
 * A native capability plugin. The fixed `Effect.Plugin{plugin, op, input}`
 * envelope is dispatched by name to one of these — so adding a plugin never
 * touches the wire ABI or the generated bindings, only this registry.
 */
interface MobilerPlugin {
    fun handle(op: String, input: String)
}

/** Official, bundled plugin — shipped in the generic shell (free tier). */
class ToastPlugin(private val context: Context) : MobilerPlugin {
    override fun handle(op: String, input: String) {
        Log.i("Mobiler", "toast plugin: op=$op input=$input")
        Toast.makeText(context, input, Toast.LENGTH_SHORT).show()
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
    )

    var view: Widget by mutableStateOf(Widget.bincodeDeserialize(core.view()))
        private set

    fun update(action: Action) {
        val effects = core.update(action.bincodeSerialize())
        val requests = dev.mobiler.abi.shared.types.Requests.bincodeDeserialize(effects).value
        for (request in requests) {
            when (val effect = request.effect) {
                is Effect.Render -> view = Widget.bincodeDeserialize(core.view())
                is Effect.Plugin -> dispatch(effect.value)
            }
        }
    }

    private fun dispatch(op: dev.mobiler.abi.shared.types.PluginOperation) {
        val plugin = plugins[op.plugin]
        if (plugin != null) {
            plugin.handle(op.op, op.input)
        } else {
            // Graceful: an app using this plugin needs a custom build that
            // registers it. This is exactly the free-shell vs. custom-build line.
            Log.w("Mobiler", "plugin '${op.plugin}' not available in this build")
        }
    }
}
