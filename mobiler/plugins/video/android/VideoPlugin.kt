package {{PACKAGE}}

import {{PACKAGE_SHARED_TYPES}}.PluginResponse

import android.app.Application
import android.content.Intent
import kotlinx.coroutines.suspendCancellableCoroutine

/** Free bundled plugin: record a video with the system camera (no permission). op "record" →
 *  a content:// URI on success, ok=false on cancel. Launches the transient VideoCaptureActivity. */
class VideoPlugin(private val application: Application) : MobilerPlugin {
    override suspend fun handle(op: String, input: String): PluginResponse {
        if (op != "record") return PluginResponse(false, "unknown op '$op'")
        val activity = MobilerActivity.current?.get() ?: return PluginResponse(false, "no foreground activity")
        val uri = suspendCancellableCoroutine<String?> { cont ->
            VideoRelay.onResult = { result ->
                if (cont.isActive) cont.resumeWith(Result.success(result))
            }
            activity.startActivity(Intent(activity, VideoCaptureActivity::class.java))
        }
        return if (uri != null) PluginResponse(true, uri) else PluginResponse(false, "cancelled")
    }
}
