package dev.mobiler.barbershop

import dev.mobiler.barbershop.shared.types.PluginResponse

import android.app.Application
import android.content.Intent
import android.net.Uri
import android.util.Base64
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.withContext
import org.json.JSONArray
import org.json.JSONObject
import java.io.File
import java.net.URL

/** Free bundled plugin: app-sandbox file I/O + download + export (see mobiler-plugin.toml for the op
 *  JSON). Paths are relative to the app's private filesDir (sandbox); `read`/`export` also accept a
 *  file:// URI. No permission needed (export uses the system save picker). */
class FilesPlugin(private val application: Application) : MobilerPlugin {
    // Resolve a sandbox-relative path (or an absolute file://-/ path) to a File; reject `..` escapes.
    private fun resolve(path: String): File? {
        if (path.startsWith("file://")) return Uri.parse(path).path?.let { File(it) }
        if (path.startsWith("/")) return File(path)
        val root = application.filesDir
        val f = File(root, path)
        return if (f.canonicalPath.startsWith(root.canonicalPath)) f else null
    }

    override suspend fun handle(op: String, input: String): PluginResponse = withContext(Dispatchers.IO) {
        val obj = runCatching { JSONObject(input) }.getOrNull() ?: JSONObject()
        try {
            when (op) {
                "read" -> {
                    val f = resolve(obj.optString("path")) ?: return@withContext PluginResponse(false, "bad path")
                    if (!f.exists()) return@withContext PluginResponse(false, "not found")
                    if (obj.optBoolean("base64")) PluginResponse(true, Base64.encodeToString(f.readBytes(), Base64.NO_WRAP))
                    else PluginResponse(true, f.readText())
                }
                "write", "append" -> {
                    val f = resolve(obj.optString("path")) ?: return@withContext PluginResponse(false, "bad path")
                    f.parentFile?.mkdirs()
                    val content = obj.optString("content")
                    val bytes = if (obj.optBoolean("base64")) Base64.decode(content, Base64.DEFAULT) else content.toByteArray()
                    if (op == "append") f.appendBytes(bytes) else f.writeBytes(bytes)
                    PluginResponse(true, Uri.fromFile(f).toString())
                }
                "delete" -> {
                    val f = resolve(obj.optString("path")) ?: return@withContext PluginResponse(false, "bad path")
                    f.delete()
                    PluginResponse(true, "")
                }
                "list" -> {
                    val dir = resolve(obj.optString("dir")) ?: application.filesDir
                    val arr = JSONArray()
                    dir.listFiles()?.forEach { e ->
                        arr.put(JSONObject().put("name", e.name).put("size", e.length()).put("dir", e.isDirectory))
                    }
                    PluginResponse(true, arr.toString())
                }
                "download" -> {
                    val f = resolve(obj.optString("path")) ?: return@withContext PluginResponse(false, "bad path")
                    f.parentFile?.mkdirs()
                    URL(obj.optString("url")).openStream().use { ins -> f.outputStream().use { out -> ins.copyTo(out) } }
                    PluginResponse(true, Uri.fromFile(f).toString())
                }
                "export" -> {
                    val f = resolve(obj.optString("path")) ?: return@withContext PluginResponse(false, "bad path")
                    if (!f.exists()) return@withContext PluginResponse(false, "not found")
                    val activity = MobilerActivity.current?.get() ?: return@withContext PluginResponse(false, "no foreground activity")
                    FileExportRelay.source = f
                    FileExportRelay.suggestedName = obj.optString("name").ifEmpty { f.name }
                    val ok = suspendCancellableCoroutine<Boolean> { cont ->
                        FileExportRelay.onResult = { done -> if (cont.isActive) cont.resumeWith(Result.success(done)) }
                        activity.startActivity(Intent(activity, FileExportActivity::class.java))
                    }
                    if (ok) PluginResponse(true, "exported") else PluginResponse(false, "cancelled")
                }
                else -> PluginResponse(false, "unknown op '$op'")
            }
        } catch (e: Exception) {
            PluginResponse(false, e.message ?: "files error")
        }
    }
}
