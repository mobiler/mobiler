package {{PACKAGE}}

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.result.contract.ActivityResultContracts

/** Static relay between FilePickerPlugin (Application context only) and this transient
 *  Activity (which owns the ActivityResult). */
object FilePickerRelay {
    var onResult: ((String?) -> Unit)? = null
}

/** Transparent helper Activity that runs the system document picker (SAF) and relays the
 *  picked URI back to FilePickerPlugin, then finishes. Shipped by the filepicker plugin so
 *  it needs no edits to the app's MainActivity. The launcher is registered as a field
 *  initializer (required: before the Activity is STARTED), then fired in onCreate. */
class FilePickerActivity : ComponentActivity() {
    private val pick = registerForActivityResult(ActivityResultContracts.GetContent()) { uri ->
        FilePickerRelay.onResult?.invoke(uri?.toString())
        FilePickerRelay.onResult = null
        finish()
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        pick.launch("*/*")
    }
}
