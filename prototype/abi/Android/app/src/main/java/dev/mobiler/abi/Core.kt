package dev.mobiler.abi

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.lifecycle.ViewModel
import dev.mobiler.abi.shared.CoreFfi
import dev.mobiler.abi.shared.types.Action
import dev.mobiler.abi.shared.types.Effect
import dev.mobiler.abi.shared.types.Request
import dev.mobiler.abi.shared.types.Requests
import dev.mobiler.abi.shared.types.Widget

// Bridge between the prebuilt shell and the Rust core. Speaks ONLY the fixed
// Mobiler ABI: sends an `Action`, receives a `Widget` tree. Nothing here is
// app-specific.
class Core : ViewModel() {
    private val core: CoreFfi = CoreFfi()

    var view: Widget by mutableStateOf(Widget.bincodeDeserialize(core.view()))
        private set

    fun update(action: Action) {
        val effects = core.update(action.bincodeSerialize())
        val requests = Requests.bincodeDeserialize(effects).value
        for (request in requests) {
            when (request.effect) {
                is Effect.Render -> view = Widget.bincodeDeserialize(core.view())
            }
        }
    }
}
