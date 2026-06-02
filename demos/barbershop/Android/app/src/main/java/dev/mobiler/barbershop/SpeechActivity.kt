package dev.mobiler.barbershop

import android.content.Intent
import android.os.Bundle
import android.speech.RecognizerIntent
import androidx.activity.ComponentActivity
import androidx.activity.result.contract.ActivityResultContracts

/** Relay between SpeechPlugin (app context only) and this transient Activity. */
object SpeechRelay {
    var onResult: ((String?) -> Unit)? = null
}

/** Transparent helper Activity: runs the system speech-recognition dialog (ACTION_RECOGNIZE_SPEECH —
 *  no RECORD_AUDIO permission needed), relays the top transcription, and finishes. */
class SpeechActivity : ComponentActivity() {
    private val recognize = registerForActivityResult(ActivityResultContracts.StartActivityForResult()) { result ->
        val text = if (result.resultCode == RESULT_OK) {
            result.data?.getStringArrayListExtra(RecognizerIntent.EXTRA_RESULTS)?.firstOrNull()
        } else {
            null
        }
        SpeechRelay.onResult?.invoke(text)
        SpeechRelay.onResult = null
        finish()
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val intent = Intent(RecognizerIntent.ACTION_RECOGNIZE_SPEECH).apply {
            putExtra(RecognizerIntent.EXTRA_LANGUAGE_MODEL, RecognizerIntent.LANGUAGE_MODEL_FREE_FORM)
            putExtra(RecognizerIntent.EXTRA_PROMPT, "Speak now")
        }
        try {
            recognize.launch(intent)
        } catch (e: Exception) {
            SpeechRelay.onResult?.invoke(null)
            SpeechRelay.onResult = null
            finish()
        }
    }
}
