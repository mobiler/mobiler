package {{PACKAGE}}

import {{PACKAGE_SHARED_TYPES}}.PluginResponse

import android.app.Application
import android.content.Intent
import kotlinx.coroutines.suspendCancellableCoroutine

/** Free bundled plugin: pick a file via the system document picker (no permission — SAF).
 *  op "pick" → a content:// URI on success, ok=false on cancel. Launches a transient
 *  FilePickerActivity (shipped with this plugin) and awaits its relayed result. */
class FilePickerPlugin(private val application: Application) : MobilerPlugin {
    override suspend fun handle(op: String, input: String): PluginResponse {
        if (op != "pick") return PluginResponse(false, "unknown op '$op'")
        val activity = MobilerActivity.current?.get() ?: return PluginResponse(false, "no foreground activity")
        val uri = suspendCancellableCoroutine<String?> { cont ->
            FilePickerRelay.onResult = { result ->
                if (cont.isActive) cont.resumeWith(Result.success(result))
            }
            activity.startActivity(Intent(activity, FilePickerActivity::class.java))
        }
        return if (uri != null) PluginResponse(true, uri) else PluginResponse(false, "cancelled")
    }
}
