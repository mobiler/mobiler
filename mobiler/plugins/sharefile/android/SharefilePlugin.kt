package {{PACKAGE}}

import {{PACKAGE_SHARED_TYPES}}.PluginResponse

import android.app.Application
import android.content.ClipData
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.webkit.MimeTypeMap
import androidx.core.content.FileProvider
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import okhttp3.OkHttpClient
import okhttp3.Request
import org.json.JSONObject
import java.io.File
import java.io.IOException
import java.io.InputStream
import java.util.concurrent.TimeUnit

/** Free bundled plugin: share a file or image through the system share sheet (no permission). Op "file".
 *  Input is a bare handle (a content:// URI, a file:// URI or an absolute path), or a JSON object
 *  {source, mime?, text?, name?, headers?} whose source may also be an https URL (fetched here) or a path
 *  relative to the app's files dir (the `files` plugin's paths). Every file is staged in cacheDir/share/,
 *  which the template's FileProvider covers. Replies: ok "shared" once the chooser is up (Android can't
 *  see the choice), else ok:false "<code>" or "<code>: <detail>" (ADR-0068). */
class SharefilePlugin(private val application: Application) : MobilerPlugin {
    override suspend fun handle(op: String, input: String): PluginResponse {
        if (op != "file") return PluginResponse(false, "unavailable: unknown op '$op'")
        val staged = try {
            val ask = Source.parse(input)
            ask to withContext(Dispatchers.IO) { Source.stage(application, ask) }
        } catch (e: Refused) {
            return PluginResponse(false, e.reply)
        } catch (e: Exception) {
            return PluginResponse(false, "failed: ${e.message ?: e.javaClass.simpleName}")
        }
        val (ask, file) = staged
        val activity = MobilerActivity.current?.get() ?: return PluginResponse(false, "no_activity")
        return withContext(Dispatchers.Main) {
            try {
                val uri: Uri = file.contentUri
                    ?: provide(file.file!!)
                    // Outside the provider's folders (e.g. a `files` download): share a copy from cacheDir/share/.
                    ?: withContext(Dispatchers.IO) { Source.copyToShare(application, file.file!!, file.name) }?.let(::provide)
                    // An app whose template predates the share/ path in file_paths.xml.
                    ?: return@withContext PluginResponse(false, "provider_missing: run mobiler upgrade")
                val send = Intent(Intent.ACTION_SEND).apply {
                    type = file.mime
                    putExtra(Intent.EXTRA_STREAM, uri)
                    // ClipData carries the read grant to the target and gives the chooser its preview.
                    clipData = ClipData.newRawUri(file.name, uri)
                    ask.text?.takeIf { it.isNotEmpty() }?.let { putExtra(Intent.EXTRA_TEXT, it) }
                    addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
                }
                activity.startActivity(
                    Intent.createChooser(send, null).addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION),
                )
                PluginResponse(true, "shared")
            } catch (e: Exception) {
                PluginResponse(false, "failed: ${e.message ?: e.javaClass.simpleName}")
            }
        }
    }

    /** The FileProvider URI for a file, or null when no provider path covers it. */
    private fun provide(f: File): Uri? = try {
        FileProvider.getUriForFile(application, "${application.packageName}.fileprovider", f)
    } catch (e: IllegalArgumentException) {
        null
    }

    /** A refusal with its reply code (ADR-0068). */
    private class Refused(code: String, detail: String? = null) : Exception(code) {
        val reply: String = if (detail.isNullOrEmpty()) code else "$code: $detail"
    }

    /** A share request: a bare handle or {source, mime, text, name, headers}. */
    private class Ask(val source: String, val mime: String?, val text: String?, val name: String?, val headers: Map<String, String>)

    /** A file ready to hand on: a local file (fetched into cacheDir/share/, or the app's own), or a content:// URI. */
    private class Staged(val file: File?, val contentUri: Uri?, val name: String, val mime: String)

    /** Source rules. The saveimage plugin carries the same rules in its own file: plugins are copied into an
     *  app one by one, so neither can depend on the other. Keep the two in step. */
    private object Source {
        const val MAX_BYTES = 50L * 1024 * 1024
        private const val KEEP_MS = 24L * 60 * 60 * 1000
        private val http by lazy {
            OkHttpClient.Builder().connectTimeout(20, TimeUnit.SECONDS).readTimeout(60, TimeUnit.SECONDS).build()
        }

        fun parse(input: String): Ask {
            val trimmed = input.trim()
            val ask = if (trimmed.startsWith("{")) {
                val o = try { JSONObject(trimmed) } catch (e: Exception) { throw Refused("bad_input") }
                fun str(k: String) = if (o.has(k) && !o.isNull(k)) o.optString(k) else null
                val headers = mutableMapOf<String, String>()
                o.optJSONObject("headers")?.let { h -> h.keys().forEach { k -> (h.opt(k) as? String)?.let { headers[k] = it } } }
                Ask((str("source") ?: "").trim(), str("mime"), str("text"), str("name"), headers)
            } else {
                Ask(trimmed, null, null, null, emptyMap())
            }
            val lower = ask.source.lowercase()
            when {
                ask.source.isEmpty() -> throw Refused("bad_input")
                lower.startsWith("http://") -> throw Refused("insecure_source")
                lower.startsWith("https://") || lower.startsWith("content://") || lower.startsWith("file://") -> {}
                Regex("^[a-z][a-z0-9+.-]*:").containsMatchIn(lower) -> throw Refused("unsupported_source")
            }
            return ask
        }

        /** Resolve the source: an https source is fetched into cacheDir/share/ (capped at 50 MB); a local file is used
         *  where it is; a content:// URI is handed on as is. */
        fun stage(app: Context, ask: Ask): Staged {
            val dir = File(app.cacheDir, "share").apply { mkdirs() }
            prune(dir)
            val src = ask.source
            val lower = src.lowercase()
            if (lower.startsWith("content://")) {
                val uri = Uri.parse(src)
                val mime = ask.mime?.let(::bareMime)?.takeIf { it.isNotEmpty() }
                    ?: app.contentResolver.getType(uri) ?: "application/octet-stream"
                return Staged(null, uri, fileName(uri.lastPathSegment ?: "", ask.name, mime), mime)
            }
            val tmp = File.createTempFile("in-", ".part", dir)
            try {
                var declared = ""
                if (lower.startsWith("https://")) {
                    val req = Request.Builder().url(src).apply { ask.headers.forEach { (k, v) -> header(k, v) } }.build()
                    http.newCall(req).execute().use { resp ->
                        if (!resp.isSuccessful) throw Refused("fetch_failed", "HTTP ${resp.code}")
                        val body = resp.body ?: throw Refused("fetch_failed", "no body")
                        if (body.contentLength() > MAX_BYTES) throw Refused("too_large")
                        declared = bareMime(body.contentType()?.toString() ?: "")
                        copyCapped(body.byteStream(), tmp)
                    }
                } else {
                    val file = localFile(app, src)
                    if (!file.isFile) throw Refused("not_found")
                    val head = file.inputStream().use { s -> val b = ByteArray(16); b.copyOf(s.read(b).coerceAtLeast(0)) }
                    val mime = sniffImage(head)
                        ?: ask.mime?.let(::bareMime)?.takeIf { it.isNotEmpty() }
                        ?: MimeTypeMap.getSingleton().getMimeTypeFromExtension(file.extension.lowercase())
                        ?: "application/octet-stream"
                    return Staged(file, null, fileName(file.name, ask.name, mime), mime)
                }
                val head = tmp.inputStream().use { s -> val b = ByteArray(16); b.copyOf(s.read(b).coerceAtLeast(0)) }
                val mime = sniffImage(head)
                    ?: ask.mime?.let(::bareMime)?.takeIf { it.isNotEmpty() }
                    ?: declared.takeIf { it.isNotEmpty() }
                    ?: MimeTypeMap.getSingleton().getMimeTypeFromExtension(src.substringBefore('?').substringAfterLast('.', "").lowercase())
                    ?: "application/octet-stream"
                val out = uniqueIn(dir, fileName(src, ask.name, mime))
                if (!tmp.renameTo(out)) throw Refused("failed", "could not stage the file")
                return Staged(out, null, out.name, mime)
            } catch (e: IOException) {
                throw Refused("fetch_failed", e.message)
            } finally {
                tmp.delete()
            }
        }

        /** Copy a local file into cacheDir/share/ under `name` (a share of a file the provider doesn't cover). */
        fun copyToShare(app: Context, from: File, name: String): File? {
            val dir = File(app.cacheDir, "share").apply { mkdirs() }
            val out = uniqueIn(dir, name)
            return try {
                from.inputStream().use { input -> out.outputStream().use { input.copyTo(it) } }
                out
            } catch (e: IOException) {
                out.delete()
                null
            }
        }

        /** file://, an absolute path, or a path relative to the app's files dir (never climbing out of it). */
        private fun localFile(app: Context, src: String): File {
            if (src.lowercase().startsWith("file://")) return File(Uri.parse(src).path ?: throw Refused("bad_input"))
            if (src.startsWith("/")) return File(src)
            val base = app.filesDir.canonicalFile
            val f = File(base, src).canonicalFile
            if (!f.path.startsWith(base.path + File.separator)) throw Refused("bad_input", "path leaves the files dir")
            return f
        }

        private fun copyCapped(input: InputStream, to: File) {
            to.outputStream().use { out ->
                val buf = ByteArray(64 * 1024)
                var total = 0L
                while (true) {
                    val n = input.read(buf)
                    if (n < 0) break
                    total += n
                    if (total > MAX_BYTES) throw Refused("too_large")
                    out.write(buf, 0, n)
                }
            }
        }

        /** Delete staged files older than a day (a receiving app may still be reading a recent one). */
        private fun prune(dir: File) {
            val cutoff = System.currentTimeMillis() - KEEP_MS
            dir.listFiles()?.forEach { if (it.lastModified() < cutoff) it.delete() }
        }

        private fun uniqueIn(dir: File, name: String): File {
            val stem = name.substringBeforeLast('.', name)
            val ext = if (name.contains('.')) "." + name.substringAfterLast('.') else ""
            var f = File(dir, name)
            var i = 1
            while (f.exists()) f = File(dir, "$stem-${i++}$ext")
            return f
        }

        /** The raster image type a file's first bytes show (PNG, JPEG, GIF, WebP), else null. */
        fun sniffImage(b: ByteArray): String? = when {
            b.size >= 8 && b[0] == 0x89.toByte() && b[1] == 'P'.code.toByte() && b[2] == 'N'.code.toByte() && b[3] == 'G'.code.toByte() -> "image/png"
            b.size >= 3 && b[0] == 0xFF.toByte() && b[1] == 0xD8.toByte() && b[2] == 0xFF.toByte() -> "image/jpeg"
            b.size >= 6 && String(b, 0, 6, Charsets.US_ASCII).let { it == "GIF87a" || it == "GIF89a" } -> "image/gif"
            b.size >= 12 && String(b, 0, 4, Charsets.US_ASCII) == "RIFF" && String(b, 8, 4, Charsets.US_ASCII) == "WEBP" -> "image/webp"
            else -> null
        }

        fun bareMime(m: String): String = m.substringBefore(';').trim().lowercase()

        /** The name a receiver sees: `name`, else the source's last segment; folder parts, control and
         *  bidi-override characters dropped; for a known type its extension is added when missing or different. */
        fun fileName(source: String, name: String?, mime: String): String {
            val path = source.substringBefore('?').substringBefore('#')
            val picked = name?.takeIf { it.isNotBlank() } ?: path
            var base = picked.substringAfterLast('/').substringAfterLast('\\')
                .filter { c -> !Character.isISOControl(c) && c !in '‪'..'‮' && c !in '⁦'..'⁩' && c != '‎' && c != '‏' }
                .trim()
            if (base.isEmpty() || base == "." || base == "..") base = "shared"
            val allowed = when (bareMime(mime)) {
                "image/png" -> listOf("png")
                "image/jpeg" -> listOf("jpg", "jpeg")
                "image/webp" -> listOf("webp")
                "image/gif" -> listOf("gif")
                "application/pdf" -> listOf("pdf")
                else -> return base
            }
            val ext = if (base.contains('.')) base.substringAfterLast('.').lowercase() else null
            return if (ext in allowed) base else "$base.${allowed.first()}"
        }
    }
}
