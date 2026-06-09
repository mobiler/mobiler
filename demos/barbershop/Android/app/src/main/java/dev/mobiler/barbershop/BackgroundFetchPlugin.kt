package dev.mobiler.barbershop

import dev.mobiler.barbershop.shared.types.PluginResponse

import android.app.Application
import android.content.Context
import androidx.work.Data
import androidx.work.ExistingPeriodicWorkPolicy
import androidx.work.PeriodicWorkRequestBuilder
import androidx.work.WorkManager
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.channels.awaitClose
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.callbackFlow
import org.json.JSONArray
import org.json.JSONObject

// background-fetch (free, bundled, EXPERIMENTAL). WorkManager wakes the app on its own schedule to run
// BackgroundFetchWorker, which posts a local notification AND feeds BackgroundFetchBus (persisted
// buffer) → the cx.subscribe stream on next foreground. The Rust core never runs in the background.
//   cx.plugin("background-fetch","schedule",{"id":"refresh","min_interval_seconds":3600,"notify_title":..,"notify_body":..})
//   cx.subscribe(key,"background-fetch","events","",on) → {"type":"fetch","id":"refresh"}
//
// WorkManager enforces a 15-minute minimum interval; `min_interval_seconds` is clamped up to that floor.
class BackgroundFetchPlugin(private val application: Application) : MobilerPlugin {
    override suspend fun handle(op: String, input: String): PluginResponse = when (op) {
        "schedule" -> schedule(input)
        "cancel" -> cancel(input)
        else -> PluginResponse(false, "unknown op '$op'")
    }

    // Streaming entrypoint (cx.subscribe): emit each fetch event until the collecting Job is cancelled.
    override fun subscribe(op: String, input: String): Flow<PluginResponse> = callbackFlow {
        val sink: (String) -> Unit = { trySend(PluginResponse(true, it)) }
        BackgroundFetchBus.attach(application, sink)
        awaitClose { BackgroundFetchBus.detach(sink) }
    }

    private fun schedule(input: String): PluginResponse {
        val obj = runCatching { JSONObject(input) }.getOrNull()
            ?: return PluginResponse(false, "invalid input JSON")
        val id = obj.optString("id", "refresh")
        // WorkManager's minimum periodic interval is 15 min; treat min_interval_seconds as a floor.
        val seconds = obj.optLong("min_interval_seconds", 3600).coerceAtLeast(15 * 60)
        val data = Data.Builder()
            .putString("id", id)
            .putString("title", obj.optString("notify_title"))
            .putString("body", obj.optString("notify_body"))
            .build()
        val request = PeriodicWorkRequestBuilder<BackgroundFetchWorker>(seconds, TimeUnit.SECONDS)
            .setInputData(data)
            .build()
        WorkManager.getInstance(application)
            .enqueueUniquePeriodicWork(id, ExistingPeriodicWorkPolicy.UPDATE, request)
        return PluginResponse(true, "")
    }

    private fun cancel(input: String): PluginResponse {
        val id = runCatching { JSONObject(input).optString("id", "refresh") }.getOrDefault("refresh")
        WorkManager.getInstance(application).cancelUniqueWork(id)
        return PluginResponse(true, "")
    }
}

// In-process bus between BackgroundFetchWorker (which may run with no UI alive) and the cx.subscribe
// stream. Persists pending events in SharedPreferences so a wake that ran in a since-killed process is
// still delivered on the next launch (flushed on attach). Mirrors PushBus / GeofenceBus.
object BackgroundFetchBus {
    private const val PREFS = "mobiler.bgfetch"
    private const val BUFFER = "buffer"
    private var sink: ((String) -> Unit)? = null

    private fun prefs(ctx: Context) = ctx.getSharedPreferences(PREFS, Context.MODE_PRIVATE)

    @Synchronized fun attach(ctx: Context, s: (String) -> Unit) {
        sink = s
        val arr = JSONArray(prefs(ctx).getString(BUFFER, "[]"))
        for (i in 0 until arr.length()) s(arr.getString(i))
        prefs(ctx).edit().remove(BUFFER).apply()
    }

    @Synchronized fun detach(s: (String) -> Unit) {
        if (sink === s) sink = null
    }

    @Synchronized fun emit(ctx: Context, payload: String) {
        val s = sink
        if (s != null) {
            s(payload)
        } else {
            val arr = JSONArray(prefs(ctx).getString(BUFFER, "[]"))
            arr.put(payload)
            prefs(ctx).edit().putString(BUFFER, arr.toString()).apply()
        }
    }
}
