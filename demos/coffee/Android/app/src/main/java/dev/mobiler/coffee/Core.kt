package dev.mobiler.coffee

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.lifecycle.ViewModel
import dev.mobiler.coffee.shared.CoreFfi
import dev.mobiler.coffee.shared.types.Effect
import dev.mobiler.coffee.shared.types.Event
import dev.mobiler.coffee.shared.types.Request
import dev.mobiler.coffee.shared.types.Requests
import dev.mobiler.coffee.shared.types.Widget

class Core : ViewModel() {
    private val core: CoreFfi = CoreFfi()

    var view: Widget by mutableStateOf(Widget.bincodeDeserialize(core.view()))
        private set

    fun update(event: Event) {
        val effects = core.update(event.bincodeSerialize())
        val requests = Requests.bincodeDeserialize(effects).value
        for (request in requests) {
            processEffect(request)
        }
    }

    private fun processEffect(request: Request) {
        when (request.effect) {
            is Effect.Render -> {
                view = Widget.bincodeDeserialize(core.view())
            }
        }
    }
}
