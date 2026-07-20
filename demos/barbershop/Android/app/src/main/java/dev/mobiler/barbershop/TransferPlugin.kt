package dev.mobiler.barbershop

import dev.mobiler.barbershop.shared.types.HttpHeader
import dev.mobiler.barbershop.shared.types.HttpOutcome
import dev.mobiler.barbershop.shared.types.PluginResponse
import dev.mobiler.barbershop.shared.types.TransferEvent

import android.app.Application
import android.net.Uri
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.channels.awaitClose
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.callbackFlow
import kotlinx.coroutines.launch
import okhttp3.Call
import okhttp3.Callback
import okhttp3.MediaType
import okhttp3.MediaType.Companion.toMediaTypeOrNull
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody
import okhttp3.Response
import okio.Buffer
import okio.BufferedSink
import okio.buffer
import okio.sink
import org.json.JSONObject
import java.io.File
import java.io.IOException
import java.io.InputStream

/**
 * Streaming file upload/download (free, bundled). See `mobiler-plugin.toml` + the iOS twin
 * (`ios/TransferPlugin.swift`) for the manifest. Rides the streaming primitive:
 *   cx.upload(url, source).start("up", Msg::Xfer)   // PUT (default)/POST source → url
 *   cx.download(url, dest).start("dl", Msg::Xfer)    // GET url → dest
 *   cx.unsubscribe(key)                              // cancels
 * Envelope JSON (see mobiler-core::transfer::TransferReq): {"url","source"|"dest","method"?,
 * "headers":[{"name","value"}]}. `op` is "upload" or "download" (the plugin op, not the HTTP verb).
 *
 * CANCEL SEMANTICS (must match iOS — see TransferPlugin.swift): `cx.unsubscribe` cancels the
 * collecting coroutine. Whether that lands before the transfer starts or mid-flight, we emit
 * NOTHING for it — the app already unsubscribed and isn't listening for a terminal event (this
 * mirrors the web shell's silent pre-send cancel; see mobiler-web's `TransferHandle::drop`). A
 * `cancelled` flag (set in `awaitClose`, which runs before `call.cancel()` reaches the network
 * layer) distinguishes that deliberate teardown from a genuine transport failure, which DOES emit
 * `Done{TransportError}`.
 */
class TransferPlugin(private val application: Application) : MobilerPlugin {
    private val client = OkHttpClient()

    // Transfer only makes sense as a stream (progress + terminal event) — `handle` exists only to
    // satisfy the MobilerPlugin interface, same as the built-in ticker/system streams.
    override suspend fun handle(op: String, input: String): PluginResponse =
        PluginResponse(false, "transfer is a streaming capability — use cx.subscribe")

    override fun subscribe(op: String, input: String): Flow<PluginResponse> = callbackFlow {
        val obj = runCatching { JSONObject(input) }.getOrNull()
        if (obj == null) {
            trySend(eventResponse(TransferEvent.Done(HttpOutcome.TransportError("malformed transfer envelope"), null)))
            close()
            return@callbackFlow
        }
        val url = obj.optString("url")
        val builder = Request.Builder().url(url)
        // Content-Type is threaded through as `appContentType` instead of added as a plain header
        // here: OkHttp's BridgeInterceptor writes the upload RequestBody's `contentType()` as the
        // Content-Type header, REPLACING anything set via `addHeader` — so an app-supplied value
        // added here would be silently overwritten by streamingUploadBody's fixed
        // "application/octet-stream". Filtering it out and letting `contentType()` be the single
        // source of truth (app value if present, else octet-stream) keeps this in sync with iOS
        // (`request.addValue`) and web (`xhr.set_request_header`), which both honor it.
        var appContentType: String? = null
        obj.optJSONArray("headers")?.let { hs ->
            for (i in 0 until hs.length()) {
                val h = hs.getJSONObject(i)
                val name = h.optString("name")
                if (op == "upload" && name.equals("Content-Type", ignoreCase = true)) {
                    appContentType = h.optString("value")
                } else {
                    builder.addHeader(name, h.optString("value"))
                }
            }
        }

        // Set by `awaitClose` (see below) the instant unsubscribe fires, BEFORE `call.cancel()`
        // reaches OkHttp — so any completion/failure callback that races with it always observes
        // `cancelled == true` first and stays silent.
        var cancelled = false
        var call: Call? = null
        var lastEmit = 0L

        // ~10/sec progress throttle, purely on elapsed time (same rule as the web shell). The
        // terminal Done is never throttled — it's sent unconditionally by its own call site.
        fun maybeProgress(transferred: Long, total: Long?) {
            val now = System.currentTimeMillis()
            if (now - lastEmit < 100) return
            lastEmit = now
            trySend(eventResponse(TransferEvent.Progress(transferred.toULong(), total?.toULong())))
        }

        fun finish(ev: TransferEvent.Done) {
            if (!cancelled) trySend(eventResponse(ev))
            close()
        }

        if (op == "upload") {
            val method = obj.optString("method").ifEmpty { "PUT" }
            val source = obj.optString("source")
            val body = streamingUploadBody(application, source, appContentType, ::maybeProgress)
            if (body == null) {
                finish(TransferEvent.Done(HttpOutcome.TransportError("cannot open upload source '$source'"), null))
                return@callbackFlow
            }
            val request = builder.method(method, body).build()
            val c = client.newCall(request)
            call = c
            c.enqueue(object : Callback {
                override fun onFailure(call: Call, e: IOException) {
                    finish(TransferEvent.Done(HttpOutcome.TransportError(e.message ?: "upload failed"), null))
                }
                override fun onResponse(call: Call, response: Response) {
                    response.use { r ->
                        val headers = r.headers.map { (n, v) -> HttpHeader(n, v) }
                        finish(TransferEvent.Done(HttpOutcome.Response(r.code.toUShort(), headers, emptyList()), null))
                    }
                }
            })
        } else {
            val destPath = obj.optString("dest")
            val dest = resolveSandboxPath(application, destPath)
            if (dest == null) {
                finish(TransferEvent.Done(HttpOutcome.TransportError("bad dest '$destPath'"), null))
                return@callbackFlow
            }
            val request = builder.build() // no explicit method → OkHttp defaults to GET
            val c = client.newCall(request)
            call = c
            // Launched as a CHILD coroutine (not run inline here) so this producer body can reach
            // `awaitClose` immediately and register the cancellation hook concurrently with the
            // blocking read below — required for `call.cancel()` to actually interrupt a stalled
            // `source.read()` (a plain blocking Okio call that does not itself check cancellation).
            launch(Dispatchers.IO) {
                try {
                    c.execute().use { response ->
                        val respBody = response.body
                        val total = respBody?.contentLength()?.takeIf { it >= 0 }
                        dest.parentFile?.mkdirs()
                        var got = 0L
                        if (respBody != null) {
                            respBody.source().use { src ->
                                dest.sink().buffer().use { sink ->
                                    val buf = Buffer()
                                    while (true) {
                                        // Cooperative cancellation for the common case (steady
                                        // stream, frequent small reads); `call.cancel()` (fired
                                        // from `awaitClose` on unsubscribe) is the fallback that
                                        // unblocks a `read()` stalled waiting on the socket.
                                        ensureActive()
                                        val n = src.read(buf, 64L * 1024)
                                        if (n == -1L) break
                                        sink.write(buf, n)
                                        got += n
                                        maybeProgress(got, total)
                                    }
                                }
                            }
                        }
                        val headers = response.headers.map { (n, v) -> HttpHeader(n, v) }
                        finish(TransferEvent.Done(HttpOutcome.Response(response.code.toUShort(), headers, emptyList()), Uri.fromFile(dest).toString()))
                    }
                } catch (e: CancellationException) {
                    dest.delete()
                    throw e // never swallow — let structured concurrency see the cancellation
                } catch (e: IOException) {
                    dest.delete()
                    finish(TransferEvent.Done(HttpOutcome.TransportError(e.message ?: "download failed"), null))
                }
            }
        }

        awaitClose {
            cancelled = true
            call?.cancel()
        }
    }
}

/**
 * Builds a streaming, re-openable upload `RequestBody` for `source`: a `content://` URI (via the
 * ContentResolver — length usually unknown) or a sandbox-relative/absolute/`file://` path (length
 * known upfront). Returns null if the source cannot be opened at all (probed once, eagerly, so a
 * bad path fails fast instead of only inside `writeTo`). `onProgress` is called after every chunk
 * write with (bytesSentSoFar, totalOrNull).
 */
private fun streamingUploadBody(
    application: Application,
    source: String,
    contentType: String?,
    onProgress: (sent: Long, total: Long?) -> Unit,
): RequestBody? {
    val isContentUri = source.startsWith("content://")
    val sourceFile: File? = if (isContentUri) null else resolveSandboxPath(application, source)
    if (!isContentUri && sourceFile == null) return null

    fun open(): InputStream? =
        if (isContentUri) application.contentResolver.openInputStream(Uri.parse(source))
        else sourceFile!!.takeIf { it.exists() }?.inputStream()

    // Eager probe: fail fast on a missing/unreadable source rather than only inside writeTo
    // (which OkHttp calls later, off this thread, where a null-body error is harder to report).
    open()?.close() ?: return null

    val total: Long? = if (isContentUri) null else sourceFile!!.length().takeIf { it > 0 }
    // App-supplied Content-Type wins (needed e.g. for a presigned S3/GCS PUT signed against a
    // specific content type); otherwise default to octet-stream. This is the single place the
    // upload's Content-Type header is decided — see the caller in `subscribe` above.
    val mediaType: MediaType? = (contentType ?: "application/octet-stream").toMediaTypeOrNull()

    return object : RequestBody() {
        override fun contentType(): MediaType? = mediaType
        override fun contentLength(): Long = total ?: -1L
        override fun writeTo(sink: BufferedSink) {
            val input = open() ?: throw IOException("cannot open source '$source'")
            input.use { ins ->
                var sent = 0L
                val chunk = ByteArray(64 * 1024)
                while (true) {
                    val n = ins.read(chunk)
                    if (n == -1) break
                    sink.write(chunk, 0, n)
                    sent += n
                    onProgress(sent, total)
                }
            }
        }
    }
}

/** Resolve a sandbox-relative path (or an absolute/`file://` path) to a File; reject `..` escapes
 *  outside the app's private filesDir. Mirrors `files` plugin's `resolve`. */
private fun resolveSandboxPath(application: Application, path: String): File? {
    if (path.startsWith("file://")) return Uri.parse(path).path?.let { File(it) }
    if (path.startsWith("/")) return File(path)
    val root = application.filesDir
    val f = File(root, path)
    return if (f.canonicalPath.startsWith(root.canonicalPath)) f else null
}

/** ok = true for a Progress tick, or a Done whose outcome is a 2xx Response (matches HttpPlugin's
 *  `ok` = 2xx convention; a non-2xx response and a TransportError are both `ok = false`). */
private fun eventResponse(ev: TransferEvent): PluginResponse {
    val ok = when (ev) {
        is TransferEvent.Progress -> true
        is TransferEvent.Done -> {
            val outcome = ev.outcome
            outcome is HttpOutcome.Response && outcome.status.toInt() in 200..299
        }
    }
    return PluginResponse(ok, ev.bincodeSerialize().toUByteList())
}

/** The generated types use List<UByte>, not List<Byte> — ByteArray.toList() gives the wrong
 *  element type and will not compile. */
private fun ByteArray.toUByteList(): List<UByte> = this.map { it.toUByte() }
