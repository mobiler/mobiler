package dev.mobiler.barbershop

import android.content.Context
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.Matrix
import android.net.Uri
import android.os.Build
import androidx.exifinterface.media.ExifInterface
import dev.mobiler.barbershop.shared.types.Photo
import dev.mobiler.barbershop.shared.types.PluginResponse
import org.json.JSONObject
import java.io.ByteArrayOutputStream
import java.io.File
import java.util.UUID

/** cx.pick_photo_with / capture_photo_with (ADR-0045): re-encode a picked or captured image to the
 *  asked format and limits, upright and without metadata, into the app's cache. The rules mirror
 *  mobiler_core::photo (needs_reencode, target_size, quality_ladder, output_format). */
internal object PhotoPipeline {
    private class Opts(val format: String, val maxDimension: Int?, val maxBytes: Long?, val quality: Int, val strip: Boolean)

    private fun parse(input: String): Opts {
        val j = runCatching { JSONObject(input) }.getOrElse { JSONObject() }
        return Opts(
            j.optString("format", "original"),
            if (j.has("max_dimension")) j.getInt("max_dimension").takeIf { it > 0 } else null,
            if (j.has("max_bytes")) j.getLong("max_bytes") else null,
            j.optInt("quality", 85).coerceIn(1, 100),
            j.optBoolean("strip_metadata", true),
        )
    }

    private fun fail(code: String) = PluginResponse(false, code)

    private fun ladder(start: Int): List<Int> {
        val out = mutableListOf(start)
        var q = start
        while (q > 45) { q = maxOf(q - 10, 45); out += q }
        return out
    }

    /** mobiler_core::photo::target_size: longest side ≤ max (never upscale), at most MAX_PIXELS. */
    private fun target(w: Int, h: Int, max: Int?): Pair<Int, Int> {
        val longest = maxOf(w, h)
        val dimensionCap = if (max != null && max > 0 && longest > max) max else null
        var scale = if (dimensionCap != null) dimensionCap.toDouble() / longest else 1.0
        val pixels = w.toDouble() * h * scale * scale
        val pixelCapped = pixels > MAX_PIXELS
        if (pixelCapped) scale *= Math.sqrt(MAX_PIXELS / pixels)
        if (scale >= 1.0) return w to h
        var tw = maxOf(1, Math.floor(w * scale).toInt())
        var th = maxOf(1, Math.floor(h * scale).toInt())
        // When max_dimension is what scaled it, the long side is exactly max (floor can land one short).
        if (dimensionCap != null && !pixelCapped) { if (w >= h) tw = dimensionCap else th = dimensionCap }
        return tw to th
    }

    private const val MAX_PIXELS = 16_000_000.0

    private val cleanPngChunks = setOf("IHDR", "PLTE", "IDAT", "IEND", "tRNS", "gAMA", "cHRM", "sRGB", "iCCP", "sBIT", "pHYs", "bKGD", "cICP")

    /** mobiler_core::photo::png_is_clean: only a PNG (by its bytes) whose every chunk is harmless
     *  is proven free of metadata; anything else counts as carrying some. */
    private fun pngIsClean(b: ByteArray): Boolean {
        val magic = byteArrayOf(0x89.toByte(), 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A)
        if (b.size < 8 || !b.copyOfRange(0, 8).contentEquals(magic)) return false
        var i = 8
        while (true) {
            if (b.size - i < 12) return false
            val len = ((b[i].toLong() and 0xFF) shl 24) or ((b[i + 1].toLong() and 0xFF) shl 16) or
                ((b[i + 2].toLong() and 0xFF) shl 8) or (b[i + 3].toLong() and 0xFF)
            val kind = String(b, i + 4, 4, Charsets.ISO_8859_1)
            if (kind !in cleanPngChunks) return false
            val next = i.toLong() + 12 + len
            if (next > b.size) return false
            // Nothing may follow IEND (a cropped screenshot can keep the original there).
            if (kind == "IEND") return next == b.size.toLong()
            i = next.toInt()
        }
    }

    @Suppress("DEPRECATION") // Bitmap.CompressFormat.WEBP below API 30
    fun process(context: Context, source: Uri, input: String): PluginResponse = try {
        processOrThrow(context, source, input)
    } catch (e: OutOfMemoryError) {
        // A photo too big to decode on this device; the 16 MP cap makes this rare.
        fail("too_large")
    }

    @Suppress("DEPRECATION")
    private fun processOrThrow(context: Context, source: Uri, input: String): PluginResponse {
        val o = parse(input)
        val cr = context.contentResolver
        val original = cr.openInputStream(source)?.use { it.readBytes() } ?: return fail("unsupported_image")
        val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
        BitmapFactory.decodeByteArray(original, 0, original.size, bounds)
        if (bounds.outWidth <= 0 || bounds.outHeight <= 0) return fail("unsupported_image") // e.g. HEIC on API 26/27
        // The decoder's own type first: a provider may report none, or the wrong one.
        val mime = bounds.outMimeType ?: cr.getType(source) ?: "image/jpeg"
        val exif = runCatching { ExifInterface(original.inputStream()) }.getOrNull()
        val orientation = exif?.getAttributeInt(ExifInterface.TAG_ORIENTATION, ExifInterface.ORIENTATION_NORMAL) ?: ExifInterface.ORIENTATION_NORMAL
        val swaps = orientation in setOf(ExifInterface.ORIENTATION_ROTATE_90, ExifInterface.ORIENTATION_ROTATE_270, ExifInterface.ORIENTATION_TRANSPOSE, ExifInterface.ORIENTATION_TRANSVERSE)
        val (w, h) = if (swaps) bounds.outHeight to bounds.outWidth else bounds.outWidth to bounds.outHeight
        // Fail closed (ADR-0045): only a PNG proven clean by its chunks counts as metadata-free.
        val hasMetadata = !pngIsClean(original)
        val fits = (o.maxDimension == null || maxOf(w, h) <= o.maxDimension) && (o.maxBytes == null || original.size <= o.maxBytes)
        val dir = File(context.cacheDir, "photos").apply { mkdirs() }
        if (o.format == "original" && fits && !(o.strip && hasMetadata)) {
            val ext = when (mime) { "image/png" -> "png"; "image/webp" -> "webp"; "image/heic", "image/heif" -> "heic"; else -> "jpg" }
            val out = File(dir, "${UUID.randomUUID()}.$ext").apply { writeBytes(original) }
            return ok(out, mime, original.size.toLong(), w, h)
        }
        val png = o.format == "png" || (o.format == "original" && mime == "image/png")
        val webp = o.format == "webp"
        val (tw, th) = target(w, h, o.maxDimension)
        // Decode at a power-of-two sample first (a 12000×9000 photo must not run out of memory).
        var sample = 1
        while (maxOf(bounds.outWidth, bounds.outHeight) / (sample * 2) >= maxOf(tw, th)) sample *= 2
        val decoded = BitmapFactory.decodeByteArray(original, 0, original.size, BitmapFactory.Options().apply { inSampleSize = sample })
            ?: return fail("unsupported_image")
        val matrix = Matrix().apply {
            when (orientation) {
                ExifInterface.ORIENTATION_ROTATE_90 -> postRotate(90f)
                ExifInterface.ORIENTATION_ROTATE_180 -> postRotate(180f)
                ExifInterface.ORIENTATION_ROTATE_270 -> postRotate(270f)
                ExifInterface.ORIENTATION_FLIP_HORIZONTAL -> postScale(-1f, 1f)
                ExifInterface.ORIENTATION_FLIP_VERTICAL -> postScale(1f, -1f)
                ExifInterface.ORIENTATION_TRANSPOSE -> { postRotate(90f); postScale(-1f, 1f) }
                ExifInterface.ORIENTATION_TRANSVERSE -> { postRotate(270f); postScale(-1f, 1f) }
            }
        }
        // One transform: rotate/flip, then scale to the exact target (upright dimensions).
        val rotatedW = if (swaps) decoded.height else decoded.width
        val rotatedH = if (swaps) decoded.width else decoded.height
        matrix.postScale(tw.toFloat() / rotatedW, th.toFloat() / rotatedH)
        val scaled = Bitmap.createBitmap(decoded, 0, 0, decoded.width, decoded.height, matrix, true)
        if (scaled !== decoded) decoded.recycle()
        val (format, outMime, ext) = when {
            png -> Triple(Bitmap.CompressFormat.PNG, "image/png", "png")
            webp && Build.VERSION.SDK_INT >= Build.VERSION_CODES.R -> Triple(Bitmap.CompressFormat.WEBP_LOSSY, "image/webp", "webp")
            webp -> Triple(Bitmap.CompressFormat.WEBP, "image/webp", "webp")
            else -> Triple(Bitmap.CompressFormat.JPEG, "image/jpeg", "jpg")
        }
        try {
            for (q in if (png) listOf(100) else ladder(o.quality)) {
                val bytes = ByteArrayOutputStream().also { scaled.compress(format, q, it) }.toByteArray()
                if (o.maxBytes == null || bytes.size <= o.maxBytes) {
                    val out = File(dir, "${UUID.randomUUID()}.$ext").apply { writeBytes(bytes) }
                    return ok(out, outMime, bytes.size.toLong(), scaled.width, scaled.height)
                }
            }
            return fail("too_large")
        } finally {
            scaled.recycle()
        }
    }

    private fun ok(file: File, mime: String, size: Long, w: Int, h: Int): PluginResponse =
        PluginResponse(true, Photo(Uri.fromFile(file).toString(), mime, size.toULong(), w.toUInt(), h.toUInt()).bincodeSerialize().map { it.toUByte() })
}
