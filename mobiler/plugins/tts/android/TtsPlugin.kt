package {{PACKAGE}}

import {{PACKAGE_SHARED_TYPES}}.PluginResponse

import android.app.Application
import android.speech.tts.TextToSpeech
import android.speech.tts.UtteranceProgressListener
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.withContext
import org.json.JSONObject

/** Free bundled plugin: text-to-speech (no permission). op "speak", input = the text (or
 *  {"text": …}). Speaks it and resolves "done" when finished. */
class TtsPlugin(private val application: Application) : MobilerPlugin {
    override suspend fun handle(op: String, input: String): PluginResponse {
        if (op != "speak") return PluginResponse(false, "unknown op '$op'")
        val text = if (input.startsWith("{")) JSONObject(input).optString("text") else input
        if (text.isEmpty()) return PluginResponse(false, "no text")
        return withContext(Dispatchers.Main) {
            suspendCancellableCoroutine { cont ->
                var resumed = false
                fun done(r: PluginResponse) { if (!resumed) { resumed = true; cont.resumeWith(Result.success(r)) } }
                var tts: TextToSpeech? = null
                tts = TextToSpeech(application) { status ->
                    if (status != TextToSpeech.SUCCESS) {
                        done(PluginResponse(false, "tts init failed"))
                        return@TextToSpeech
                    }
                    tts?.setOnUtteranceProgressListener(object : UtteranceProgressListener() {
                        override fun onStart(utteranceId: String?) {}
                        override fun onDone(utteranceId: String?) {
                            tts?.shutdown()
                            done(PluginResponse(true, "done"))
                        }
                        @Deprecated("required override")
                        override fun onError(utteranceId: String?) {
                            tts?.shutdown()
                            done(PluginResponse(false, "tts error"))
                        }
                    })
                    tts?.speak(text, TextToSpeech.QUEUE_FLUSH, null, "mobiler-tts")
                }
                cont.invokeOnCancellation { tts?.shutdown() }
            }
        }
    }
}
