package {{PACKAGE}}

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.lifecycle.ViewModel
import {{PACKAGE_SHARED}}.CoreFfi
import {{PACKAGE_SHARED_TYPES}}.Effect
import {{PACKAGE_SHARED_TYPES}}.Event
import {{PACKAGE_SHARED_TYPES}}.Request
import {{PACKAGE_SHARED_TYPES}}.Requests
import {{PACKAGE_SHARED_TYPES}}.Widget

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
