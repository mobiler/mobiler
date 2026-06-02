package {{PACKAGE}}

import {{PACKAGE_SHARED_TYPES}}.PluginResponse

import android.app.Application
import android.content.Intent
import android.net.Uri
import androidx.core.content.FileProvider
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.io.File

/** Free bundled plugin: share a file/image via the system share sheet (no permission). op "file",
 *  input = a URI — typically a content:// from filepicker/photo/camera (used as-is); a file:// /
 *  path is wrapped via FileProvider (requires the path to be declared in file_paths.xml). */
class SharefilePlugin(private val application: Application) : MobilerPlugin {
    override suspend fun handle(op: String, input: String): PluginResponse {
        if (op != "file") return PluginResponse(false, "unknown op '$op'")
        if (input.isEmpty()) return PluginResponse(false, "no file uri")
        val activity = MobilerActivity.current?.get() ?: return PluginResponse(false, "no foreground activity")
        return withContext(Dispatchers.Main) {
            try {
                val uri: Uri = if (input.startsWith("content://")) {
                    Uri.parse(input)
                } else {
                    val f = File(Uri.parse(input).path ?: input)
                    FileProvider.getUriForFile(application, "${application.packageName}.fileprovider", f)
                }
                val send = Intent(Intent.ACTION_SEND).apply {
                    type = application.contentResolver.getType(uri) ?: "*/*"
                    putExtra(Intent.EXTRA_STREAM, uri)
                    addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
                }
                activity.startActivity(
                    Intent.createChooser(send, "Share").addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION),
                )
                PluginResponse(true, "shared")
            } catch (e: Exception) {
                PluginResponse(false, e.message ?: "share failed")
            }
        }
    }
}
