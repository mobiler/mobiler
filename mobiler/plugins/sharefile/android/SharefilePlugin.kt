package {{PACKAGE}}

import {{PACKAGE_SHARED_TYPES}}.PluginResponse

import android.app.Application
import android.content.ClipData
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.webkit.MimeTypeMap
import androidx.core.content.FileProvider
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import okhttp3.HttpUrl.Companion.toHttpUrlOrNull
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.Response
import org.json.JSONObject
import java.io.File
import java.io.IOException
import java.io.InputStream
import java.util.concurrent.TimeUnit

/** Free bundled plugin: share a file or image through the system share sheet (no permission). Op "file".
 *  Input is a bare handle (a content:// URI, a file:// URI or an absolute path), or a JSON object
 *  {source, mime?, text?, name?, headers?} whose source may also be an https URL (fetched here) or a path
 *  relative to the app's files dir (the `files` plugin's paths). A local file must be the app's own (its files,
 *  cache or external files folders). Files the FileProvider doesn't cover are copied into cacheDir/share/,
 *  which the template's file_paths.xml declares. Replies: ok "shared" once the chooser is up (Android can't
 *  see the choice), else ok:false "<code>" or "<code>: <detail>" (ADR-0068). */
class SharefilePlugin(private val application: Application) : MobilerPlugin {
    override suspend fun handle(op: String, input: String): PluginResponse {
        if (op != "file") return PluginResponse(false, "unavailable: unknown op '$op'")
        var staged: Staged? = null
        try {
            val ask = Source.parse(input)
            val file = withContext(Dispatchers.IO) { Source.stage(application, ask) }
            staged = file
            val activity = MobilerActivity.current?.get() ?: return PluginResponse(false, "no_activity")
            val uri: Uri = file.contentUri ?: provide(file.file!!) ?: run {
                // Outside the provider's folders (e.g. a `files` download): share a copy from cacheDir/share/,
                // once the provider is known to cover share/ (an app whose template predates it doesn't).
                if (provide(File(Source.shareDir(application), "probe")) == null) {
                    return PluginResponse(false, "provider_missing: run mobiler upgrade")
                }
                val copy = withContext(Dispatchers.IO) { Source.copyToShare(application, file.file!!, file.name) }
                staged = Staged(copy, null, file.name, file.mime, fetched = true)
                provide(copy) ?: return PluginResponse(false, "provider_missing: run mobiler upgrade")
            }
            return withContext(Dispatchers.Main) {
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
                staged = null // handed on: the receiver reads it; the 24-hour prune removes it later
                PluginResponse(true, "shared")
            }
        } catch (e: CancellationException) {
            throw e
        } catch (e: Refused) {
            return PluginResponse(false, e.reply)
        } catch (e: Exception) {
            return PluginResponse(false, "failed: ${e.message ?: e.javaClass.simpleName}")
        } finally {
            // A file this call fetched or copied but never handed on.
            staged?.takeIf { it.fetched }?.file?.delete()
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

    /** A file ready to hand on: a local file (the app's own, or fetched/copied by this call), or a content:// URI. */
    private class Staged(val file: File?, val contentUri: Uri?, val name: String, val mime: String, val fetched: Boolean)

    /** Source rules. The saveimage plugin carries the same rules in its own file: plugins are copied into an
     *  app one by one, so neither can depend on the other. Keep the two in step. */
    private object Source {
        const val MAX_BYTES = 50L * 1024 * 1024
        private const val KEEP_MS = 24L * 60 * 60 * 1000
        private const val MAX_REDIRECTS = 5
        // Redirects are followed by hand (fetch below): never to http, and the caller's headers only to the same host.
        private val http by lazy {
            OkHttpClient.Builder()
                .connectTimeout(20, TimeUnit.SECONDS).readTimeout(60, TimeUnit.SECONDS).callTimeout(180, TimeUnit.SECONDS)
                .followRedirects(false).followSslRedirects(false)
                .build()
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
                ask.source.any { it == '\t' || it == '\n' || it == '\r' } -> throw Refused("unsupported_source")
                lower.startsWith("http://") -> throw Refused("insecure_source")
                lower.startsWith("https://") || lower.startsWith("content://") || lower.startsWith("file://") -> {}
                Regex("^[a-z][a-z0-9+.-]*:").containsMatchIn(lower) -> throw Refused("unsupported_source")
            }
            return ask
        }

        fun shareDir(app: Context): File = File(app.cacheDir, "share").apply { mkdirs() }

        /** Resolve the source: an https source is fetched into cacheDir/share/ (capped at 50 MB); a local file (the
         *  app's own) is used where it is; a content:// URI is handed on as is. */
        fun stage(app: Context, ask: Ask): Staged {
            val dir = shareDir(app)
            prune(dir)
            val src = ask.source
            val lower = src.lowercase()
            if (lower.startsWith("content://")) {
                val uri = Uri.parse(src)
                val mime = ask.mime?.let(::bareMime)?.takeIf { it.isNotEmpty() }
                    ?: app.contentResolver.getType(uri) ?: "application/octet-stream"
                return Staged(null, uri, fileName(uri.lastPathSegment ?: "", ask.name, mime), mime, fetched = false)
            }
            if (!lower.startsWith("https://")) {
                val file = localFile(app, src)
                val head = try { readHead(file) } catch (e: IOException) { throw Refused("failed", "can't read the file") }
                val mime = sniffImage(head)
                    ?: ask.mime?.let(::bareMime)?.takeIf { it.isNotEmpty() }
                    ?: MimeTypeMap.getSingleton().getMimeTypeFromExtension(file.extension.lowercase())
                    ?: "application/octet-stream"
                return Staged(file, null, fileName(file.name, ask.name, mime), mime, fetched = false)
            }
            val tmp = File.createTempFile("in-", ".part", dir)
            try {
                val declared = fetch(ask, tmp)
                val mime = sniffImage(readHead(tmp))
                    ?: ask.mime?.let(::bareMime)?.takeIf { it.isNotEmpty() }
                    ?: declared.takeIf { it.isNotEmpty() }
                    ?: MimeTypeMap.getSingleton().getMimeTypeFromExtension(src.substringBefore('?').substringAfterLast('.', "").lowercase())
                    ?: "application/octet-stream"
                val out = uniqueIn(dir, fileName(src, ask.name, mime))
                if (!tmp.renameTo(out)) throw Refused("failed", "could not stage the file")
                return Staged(out, null, out.name, mime, fetched = true)
            } finally {
                tmp.delete()
            }
        }

        /** GET an https URL into `to`, following up to 5 redirects by hand: an http target is insecure_source, and
         *  the caller's headers go only to the original host. Returns the declared Content-Type. */
        private fun fetch(ask: Ask, to: File): String {
            val origin = ask.source.toHttpUrlOrNull() ?: throw Refused("bad_input")
            var url = origin
            repeat(MAX_REDIRECTS + 1) {
                val req = Request.Builder().url(url).apply {
                    if (url.host == origin.host && url.port == origin.port) ask.headers.forEach { (k, v) -> header(k, v) }
                }.build()
                val resp: Response = try {
                    http.newCall(req).execute()
                } catch (e: IOException) {
                    throw Refused("fetch_failed", e.message)
                }
                resp.use { r ->
                    if (r.isRedirect) {
                        val next = r.header("Location")?.let { url.resolve(it) } ?: throw Refused("fetch_failed", "redirect without a target")
                        if (!next.isHttps) throw Refused("insecure_source", "redirected to http")
                        url = next
                        return@repeat
                    }
                    if (!r.isSuccessful) throw Refused("fetch_failed", "HTTP ${r.code}")
                    val body = r.body ?: throw Refused("fetch_failed", "no body")
                    if (body.contentLength() > MAX_BYTES) throw Refused("too_large")
                    try {
                        copyCapped(body.byteStream(), to)
                    } catch (e: IOException) {
                        throw Refused("fetch_failed", e.message)
                    }
                    return bareMime(body.contentType()?.toString() ?: "")
                }
            }
            throw Refused("fetch_failed", "too many redirects")
        }

        /** The app's own file: file://, an absolute path, or a path relative to the files dir, which must lie in the
         *  app's files, cache or external files/cache folders (never its databases or preferences). */
        private fun localFile(app: Context, src: String): File {
            val f = when {
                src.lowercase().startsWith("file://") -> File(Uri.parse(src).path ?: throw Refused("bad_input"))
                src.startsWith("/") -> File(src)
                else -> File(app.filesDir, src)
            }.canonicalFile
            val roots = listOfNotNull(app.filesDir, app.cacheDir, app.getExternalFilesDir(null), app.externalCacheDir)
                .map { it.canonicalFile.path + File.separator }
            if (roots.none { f.path.startsWith(it) }) throw Refused("unsupported_source", "not one of the app's files")
            if (!f.isFile) throw Refused("not_found")
            return f
        }

        private fun readHead(f: File): ByteArray = f.inputStream().use { s -> val b = ByteArray(16); b.copyOf(s.read(b).coerceAtLeast(0)) }

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

        /** Copy a local file into cacheDir/share/ under `name` (a share of a file the provider doesn't cover). */
        fun copyToShare(app: Context, from: File, name: String): File {
            val out = uniqueIn(shareDir(app), name)
            try {
                from.inputStream().use { input -> out.outputStream().use { input.copyTo(it) } }
            } catch (e: IOException) {
                out.delete()
                throw Refused("failed", "could not copy the file")
            }
            return out
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

        private val PNG = byteArrayOf(0x89.toByte(), 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A)

        /** The raster image type a file's first bytes show (PNG, JPEG, GIF, WebP), else null. */
        fun sniffImage(b: ByteArray): String? = when {
            b.size >= 8 && b.copyOf(8).contentEquals(PNG) -> "image/png"
            b.size >= 3 && b[0] == 0xFF.toByte() && b[1] == 0xD8.toByte() && b[2] == 0xFF.toByte() -> "image/jpeg"
            b.size >= 6 && String(b, 0, 6, Charsets.US_ASCII).let { it == "GIF87a" || it == "GIF89a" } -> "image/gif"
            b.size >= 12 && String(b, 0, 4, Charsets.US_ASCII) == "RIFF" && String(b, 8, 4, Charsets.US_ASCII) == "WEBP" -> "image/webp"
            else -> null
        }

        fun bareMime(m: String): String = m.substringBefore(';').trim().lowercase()

        /** Characters a name loses: controls, and the invisible and bidi-override ones that can disguise an extension. */
        private fun hidden(c: Char): Boolean = Character.isISOControl(c) ||
            c in '\u200B'..'\u200F' || c in '\u202A'..'\u202E' || c in '\u2066'..'\u2069' || c == '\u061C' || c == '\uFEFF'

        /** The name a receiver sees: `name`, else the source's last segment; folder parts and hidden characters
         *  dropped; for a known type its extension is added when missing or different. */
        fun fileName(source: String, name: String?, mime: String): String {
            val path = source.substringBefore('?').substringBefore('#')
            val picked = name?.takeIf { it.isNotBlank() } ?: path
            var base = picked.substringAfterLast('/').substringAfterLast('\\').filterNot(::hidden).trim()
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
