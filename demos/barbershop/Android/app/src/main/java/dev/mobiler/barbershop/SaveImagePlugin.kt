package dev.mobiler.barbershop

import dev.mobiler.barbershop.shared.types.PluginResponse

import android.Manifest
import android.app.Application
import android.content.ContentValues
import android.content.Context
import android.content.pm.PackageManager
import android.graphics.BitmapFactory
import android.media.MediaScannerConnection
import android.net.Uri
import android.os.Build
import android.os.Environment
import android.provider.MediaStore
import androidx.core.content.ContextCompat
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import okhttp3.OkHttpClient
import okhttp3.Request
import org.json.JSONObject
import java.io.File
import java.io.IOException
import java.io.InputStream
import java.util.concurrent.TimeUnit

/** Free bundled plugin: save an image to the gallery. Op "save", input {source, name?, headers?}: an https URL
 *  (fetched here, up to 50 MB), a file:// URI, an absolute path, or a path relative to the app's files dir.
 *  The bytes must be a PNG, JPEG, GIF or WebP and are saved unchanged under Pictures/<app name>/.
 *  Android 10+ needs no permission; Android 8–9 asks for storage access and waits for the answer.
 *  Replies: ok "saved" once MediaStore has the file, else ok:false "<code>" or "<code>: <detail>" (ADR-0068). */
class SaveImagePlugin(private val application: Application) : MobilerPlugin {
    override suspend fun handle(op: String, input: String): PluginResponse {
        if (op != "save") return PluginResponse(false, "unavailable: unknown op '$op'")
        return try {
            val ask = Source.parse(input)
            val image = withContext(Dispatchers.IO) { Source.resolve(application, ask) }
            if (Build.VERSION.SDK_INT < 29 && !storageGranted()) {
                val activity = MobilerActivity.current?.get() ?: return PluginResponse(false, "no_activity")
                if (!SaveImagePermissionActivity.request(activity)) return PluginResponse(false, "denied")
            }
            try {
                withContext(Dispatchers.IO) { save(image) }
            } finally {
                if (image.fetched) image.file.delete()
            }
            PluginResponse(true, "saved")
        } catch (e: Refused) {
            PluginResponse(false, e.reply)
        } catch (e: Exception) {
            PluginResponse(false, "failed: ${e.message ?: e.javaClass.simpleName}")
        }
    }

    private fun storageGranted() = ContextCompat.checkSelfPermission(
        application, Manifest.permission.WRITE_EXTERNAL_STORAGE,
    ) == PackageManager.PERMISSION_GRANTED

    /** The folder under Pictures/: the app's name, without characters a path can't hold. */
    private fun folder(): String =
        application.applicationInfo.loadLabel(application.packageManager).toString()
            .replace(Regex("[/\\\\:*?\"<>|\\p{Cntrl}]"), "").trim().ifEmpty { "Images" }

    private fun save(image: Image) {
        if (Build.VERSION.SDK_INT >= 29) {
            val resolver = application.contentResolver
            val values = ContentValues().apply {
                put(MediaStore.Images.Media.DISPLAY_NAME, image.name)
                put(MediaStore.Images.Media.MIME_TYPE, image.mime)
                put(MediaStore.Images.Media.RELATIVE_PATH, "${Environment.DIRECTORY_PICTURES}/${folder()}")
                put(MediaStore.Images.Media.IS_PENDING, 1)
            }
            val uri = resolver.insert(MediaStore.Images.Media.getContentUri(MediaStore.VOLUME_EXTERNAL_PRIMARY), values)
                ?: throw Refused("failed", "MediaStore refused the insert")
            try {
                val out = resolver.openOutputStream(uri) ?: throw Refused("failed", "no output stream")
                out.use { o -> image.file.inputStream().use { it.copyTo(o) } }
                values.clear()
                values.put(MediaStore.Images.Media.IS_PENDING, 0)
                if (resolver.update(uri, values, null, null) != 1) throw Refused("failed", "MediaStore did not publish the image")
            } catch (e: Exception) {
                resolver.delete(uri, null, null)
                throw e
            }
        } else {
            @Suppress("DEPRECATION")
            val dir = File(Environment.getExternalStoragePublicDirectory(Environment.DIRECTORY_PICTURES), folder())
            if (!dir.isDirectory && !dir.mkdirs()) throw Refused("failed", "can't create ${dir.path}")
            val out = Source.uniqueIn(dir, image.name)
            try {
                image.file.inputStream().use { input -> out.outputStream().use { input.copyTo(it) } }
            } catch (e: IOException) {
                out.delete()
                throw Refused("failed", e.message)
            }
            MediaScannerConnection.scanFile(application, arrayOf(out.path), arrayOf(image.mime), null)
        }
    }

    /** A refusal with its reply code (ADR-0068). */
    private class Refused(code: String, detail: String? = null) : Exception(code) {
        val reply: String = if (detail.isNullOrEmpty()) code else "$code: $detail"
    }

    private class Ask(val source: String, val name: String?, val headers: Map<String, String>)

    /** A checked image file: fetched into cacheDir/share/, or the app's own file. */
    private class Image(val file: File, val name: String, val mime: String, val fetched: Boolean)

    /** Source rules, the same as the sharefile plugin's (each plugin is copied into an app on its own, so neither
     *  can depend on the other). Keep the two in step. */
    private object Source {
        const val MAX_BYTES = 50L * 1024 * 1024
        private const val KEEP_MS = 24L * 60 * 60 * 1000
        private val http by lazy {
            OkHttpClient.Builder().connectTimeout(20, TimeUnit.SECONDS).readTimeout(60, TimeUnit.SECONDS).build()
        }

        fun parse(input: String): Ask {
            val trimmed = input.trim()
            if (!trimmed.startsWith("{")) throw Refused("bad_input", "expected {\"source\": …}")
            val o = try { JSONObject(trimmed) } catch (e: Exception) { throw Refused("bad_input") }
            fun str(k: String) = if (o.has(k) && !o.isNull(k)) o.optString(k) else null
            val headers = mutableMapOf<String, String>()
            o.optJSONObject("headers")?.let { h -> h.keys().forEach { k -> (h.opt(k) as? String)?.let { headers[k] = it } } }
            val ask = Ask((str("source") ?: "").trim(), str("name"), headers)
            val lower = ask.source.lowercase()
            when {
                ask.source.isEmpty() -> throw Refused("bad_input")
                lower.startsWith("http://") -> throw Refused("insecure_source")
                lower.startsWith("https://") || lower.startsWith("file://") -> {}
                Regex("^[a-z][a-z0-9+.-]*:").containsMatchIn(lower) -> throw Refused("unsupported_source")
            }
            return ask
        }

        /** Fetch (https) or find (local) the image and check that its bytes are a raster image. */
        fun resolve(app: Context, ask: Ask): Image {
            val src = ask.source
            val fetched = src.lowercase().startsWith("https://")
            val file = if (fetched) fetch(app, ask) else localFile(app, src).also {
                if (!it.isFile) throw Refused("not_found")
            }
            try {
                val head = file.inputStream().use { s -> val b = ByteArray(16); b.copyOf(s.read(b).coerceAtLeast(0)) }
                val mime = sniffImage(head) ?: throw Refused("not_an_image")
                val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
                BitmapFactory.decodeFile(file.path, bounds)
                if (bounds.outWidth <= 0 || bounds.outHeight <= 0) throw Refused("not_an_image")
                return Image(file, fileName(src, ask.name, mime), mime, fetched)
            } catch (e: Exception) {
                if (fetched) file.delete()
                throw e
            }
        }

        private fun fetch(app: Context, ask: Ask): File {
            val dir = File(app.cacheDir, "share").apply { mkdirs() }
            prune(dir)
            val out = File.createTempFile("img-", ".part", dir)
            try {
                val req = Request.Builder().url(ask.source).apply { ask.headers.forEach { (k, v) -> header(k, v) } }.build()
                http.newCall(req).execute().use { resp ->
                    if (!resp.isSuccessful) throw Refused("fetch_failed", "HTTP ${resp.code}")
                    val body = resp.body ?: throw Refused("fetch_failed", "no body")
                    if (body.contentLength() > MAX_BYTES) throw Refused("too_large")
                    copyCapped(body.byteStream(), out)
                }
                return out
            } catch (e: Refused) {
                out.delete()
                throw e
            } catch (e: IOException) {
                out.delete()
                throw Refused("fetch_failed", e.message)
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

        /** Delete fetched files older than a day. */
        private fun prune(dir: File) {
            val cutoff = System.currentTimeMillis() - KEEP_MS
            dir.listFiles()?.forEach { if (it.lastModified() < cutoff) it.delete() }
        }

        fun uniqueIn(dir: File, name: String): File {
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

        /** The gallery name: `name`, else the source's last segment; folder parts, control and bidi-override
         *  characters dropped; the image type's extension added when missing or different. */
        fun fileName(source: String, name: String?, mime: String): String {
            val path = source.substringBefore('?').substringBefore('#')
            val picked = name?.takeIf { it.isNotBlank() } ?: path
            var base = picked.substringAfterLast('/').substringAfterLast('\\')
                .filter { c -> !Character.isISOControl(c) && c !in '‪'..'‮' && c !in '⁦'..'⁩' && c != '‎' && c != '‏' }
                .trim()
            if (base.isEmpty() || base == "." || base == "..") base = "image"
            val allowed = when (mime) {
                "image/png" -> listOf("png")
                "image/jpeg" -> listOf("jpg", "jpeg")
                "image/webp" -> listOf("webp")
                else -> listOf("gif")
            }
            val ext = if (base.contains('.')) base.substringAfterLast('.').lowercase() else null
            return if (ext in allowed) base else "$base.${allowed.first()}"
        }
    }
}
