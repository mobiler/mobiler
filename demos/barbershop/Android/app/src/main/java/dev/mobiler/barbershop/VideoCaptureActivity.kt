package dev.mobiler.barbershop

import android.content.Intent
import android.os.Bundle
import android.provider.MediaStore
import androidx.activity.ComponentActivity
import androidx.activity.result.contract.ActivityResultContracts

/** Relay between VideoPlugin (app context only) and this transient Activity. */
object VideoRelay {
    var onResult: ((String?) -> Unit)? = null
}

/** Transparent helper Activity: launches the system camera in video mode (ACTION_VIDEO_CAPTURE),
 *  relays the recorded video's content:// URI, and finishes. Shipped by the plugin — no MainActivity edit. */
class VideoCaptureActivity : ComponentActivity() {
    private val capture = registerForActivityResult(ActivityResultContracts.StartActivityForResult()) { result ->
        val uri = if (result.resultCode == RESULT_OK) result.data?.data else null
        VideoRelay.onResult?.invoke(uri?.toString())
        VideoRelay.onResult = null
        finish()
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        capture.launch(Intent(MediaStore.ACTION_VIDEO_CAPTURE))
    }
}
