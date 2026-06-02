package dev.mobiler.barbershop

import dev.mobiler.barbershop.shared.types.PluginResponse

import android.Manifest
import android.app.Application
import android.content.pm.PackageManager
import android.media.MediaPlayer
import android.media.MediaRecorder
import android.net.Uri
import android.os.Build
import androidx.core.app.ActivityCompat
import androidx.core.content.ContextCompat
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.withContext
import java.io.File

/** Free bundled plugin: audio record + play.
 *  - op "record", input = seconds (default 3) → records from the mic to an .m4a, returns its
 *    file URI. Needs RECORD_AUDIO (runtime): if not granted, fires the prompt best-effort and
 *    returns "permission requested — try again" (the notifications/geolocation pattern).
 *  - op "play", input = a URI → plays to completion, returns "done". No permission. */
class AudioPlugin(private val application: Application) : MobilerPlugin {
    override suspend fun handle(op: String, input: String): PluginResponse = when (op) {
        "record" -> record(input)
        "play" -> play(input)
        else -> PluginResponse(false, "unknown op '$op'")
    }

    private suspend fun record(input: String): PluginResponse {
        val perm = Manifest.permission.RECORD_AUDIO
        if (ContextCompat.checkSelfPermission(application, perm) != PackageManager.PERMISSION_GRANTED) {
            MobilerActivity.current?.get()?.let { ActivityCompat.requestPermissions(it, arrayOf(perm), 0) }
            return PluginResponse(false, "permission requested — try again")
        }
        val seconds = (input.toLongOrNull() ?: 3L).coerceIn(1, 60)
        val out = File(application.cacheDir, "rec_${System.currentTimeMillis()}.m4a")
        return withContext(Dispatchers.Main) {
            @Suppress("DEPRECATION")
            val recorder = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) MediaRecorder(application) else MediaRecorder()
            try {
                recorder.setAudioSource(MediaRecorder.AudioSource.MIC)
                recorder.setOutputFormat(MediaRecorder.OutputFormat.MPEG_4)
                recorder.setAudioEncoder(MediaRecorder.AudioEncoder.AAC)
                recorder.setOutputFile(out.absolutePath)
                recorder.prepare()
                recorder.start()
                delay(seconds * 1000)
                recorder.stop()
                recorder.release()
                PluginResponse(true, Uri.fromFile(out).toString())
            } catch (e: Exception) {
                runCatching { recorder.release() }
                PluginResponse(false, e.message ?: "record failed")
            }
        }
    }

    private suspend fun play(input: String): PluginResponse {
        if (input.isEmpty()) return PluginResponse(false, "no uri to play")
        return suspendCancellableCoroutine { cont ->
            var done = false
            fun finish(r: PluginResponse) { if (!done) { done = true; cont.resumeWith(Result.success(r)) } }
            try {
                val mp = MediaPlayer()
                mp.setDataSource(application, Uri.parse(input))
                mp.setOnCompletionListener { it.release(); finish(PluginResponse(true, "done")) }
                mp.setOnErrorListener { p, _, _ -> p.release(); finish(PluginResponse(false, "play error")); true }
                mp.prepare()
                mp.start()
                cont.invokeOnCancellation { runCatching { mp.release() } }
            } catch (e: Exception) {
                finish(PluginResponse(false, e.message ?: "play failed"))
            }
        }
    }
}
