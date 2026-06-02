package dev.mobiler.barbershop

import dev.mobiler.barbershop.shared.types.PluginResponse

import android.app.Application
import android.content.Intent
import kotlinx.coroutines.suspendCancellableCoroutine

/** Free bundled plugin: speech-to-text dictation (no permission). op "listen" → the recognized
 *  text, ok=false on cancel. Launches the transient SpeechActivity (system recognizer dialog). */
class SpeechPlugin(private val application: Application) : MobilerPlugin {
    override suspend fun handle(op: String, input: String): PluginResponse {
        if (op != "listen") return PluginResponse(false, "unknown op '$op'")
        val activity = MobilerActivity.current?.get() ?: return PluginResponse(false, "no foreground activity")
        val text = suspendCancellableCoroutine<String?> { cont ->
            SpeechRelay.onResult = { result ->
                if (cont.isActive) cont.resumeWith(Result.success(result))
            }
            activity.startActivity(Intent(activity, SpeechActivity::class.java))
        }
        return if (text != null) PluginResponse(true, text) else PluginResponse(false, "cancelled")
    }
}
