package dev.mobiler.app

import android.app.Application
import android.content.Context
import android.util.Base64
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.lifecycle.AndroidViewModel
import dev.mobiler.shared.CoreFfi
import dev.mobiler.shared.types.Effect
import dev.mobiler.shared.types.Event
import dev.mobiler.shared.types.Request
import dev.mobiler.shared.types.Requests
import dev.mobiler.shared.types.Widget

/// Persistence: the entire Model is bincode-serialized by Rust and stored in
/// SharedPreferences as a base64 string. Loaded on init (before the first view read),
/// saved after every event. ~kilobytes for typical use; fine for SharedPreferences.
private const val PREFS_NAME = "mobiler"
private const val STATE_KEY = "model_state_b64"

class Core(application: Application) : AndroidViewModel(application) {
    private val core: CoreFfi = CoreFfi()
    private val prefs = application.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)

    init {
        // Restore before reading the first view, so we never paint default state then
        // flicker to loaded state.
        val saved = prefs.getString(STATE_KEY, null)
        if (saved != null) {
            try {
                val bytes = Base64.decode(saved, Base64.NO_WRAP)
                // import_state returns false on schema mismatch; we just ignore and
                // fall back to default Model. No crash, no migration logic yet.
                core.importState(bytes)
            } catch (_: IllegalArgumentException) {
                // Not valid base64 (somehow). Ignore.
            }
        }
    }

    var view: Widget by mutableStateOf(Widget.bincodeDeserialize(core.view()))
        private set

    fun update(event: Event) {
        val effects = core.update(event.bincodeSerialize())
        val requests = Requests.bincodeDeserialize(effects).value
        for (request in requests) {
            processEffect(request)
        }
        persist()
    }

    private fun processEffect(request: Request) {
        when (request.effect) {
            is Effect.Render -> {
                view = Widget.bincodeDeserialize(core.view())
            }
        }
    }

    private fun persist() {
        val bytes = core.exportState()
        if (bytes.isEmpty()) return
        val encoded = Base64.encodeToString(bytes, Base64.NO_WRAP)
        prefs.edit().putString(STATE_KEY, encoded).apply()
    }
}
