package dev.mobiler.barbershop

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.result.contract.ActivityResultContracts
import java.io.File

/** Static relay between FilesPlugin (Application context only) and this transient export Activity
 *  (which owns the ActivityResult). */
object FileExportRelay {
    var source: File? = null
    var suggestedName: String = "export"
    var onResult: ((Boolean) -> Unit)? = null
}

/** Transparent helper Activity: runs the system "create document" (SAF) save picker so the user
 *  chooses where to save, then copies the source file's bytes into the chosen location. Shipped by
 *  the files plugin so it needs no edits to the app's MainActivity. */
class FileExportActivity : ComponentActivity() {
    private val create = registerForActivityResult(
        ActivityResultContracts.CreateDocument("application/octet-stream"),
    ) { uri ->
        var ok = false
        val src = FileExportRelay.source
        if (uri != null && src != null) {
            ok = runCatching {
                contentResolver.openOutputStream(uri)?.use { out -> src.inputStream().use { it.copyTo(out) } }
                true
            }.getOrDefault(false)
        }
        FileExportRelay.onResult?.invoke(ok)
        FileExportRelay.onResult = null
        FileExportRelay.source = null
        finish()
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        create.launch(FileExportRelay.suggestedName)
    }
}
