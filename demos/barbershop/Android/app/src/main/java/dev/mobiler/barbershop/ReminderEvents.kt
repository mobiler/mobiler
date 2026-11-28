package dev.mobiler.barbershop

import android.app.Activity
import android.content.Context
import android.content.Intent
import android.os.Bundle
import androidx.core.app.NotificationManagerCompat
import java.time.ZoneId
import org.json.JSONArray
import org.json.JSONException
import org.json.JSONObject

// The notifications plugin's `events` stream (spec §4.5):
//   {"type":"opened"|"action"|"snoozed"|"dismissed","id":7,"action_id"?:"…","payload"?:"…","at":"YYYY-MM-DD HH:MM:SS"}
// Every event is first written to a persisted, ordered buffer (SharedPreferences "mobiler_notifications",
// key "events", at most 100, oldest dropped), then flushed in order into the attached sink; each event is
// removed only after the sink returned, so a crash mid-flush re-delivers rather than loses
// (at-least-once into the shell's stream, as geofence, ADR-0036). EventStore / EventBuffer /
// reminderEventJson are pure, so they run in a plain JVM unit test.

/** The event JSON of spec §4.5. `action_id` and `payload` are omitted when null; `at` is local time. */
fun reminderEventJson(type: String, id: Int, actionId: String?, payload: String?, atMs: Long, zone: ZoneId): String {
    val o = JSONObject().put("type", type).put("id", id)
    if (actionId != null) o.put("action_id", actionId)
    if (payload != null) o.put("payload", payload)
    return o.put("at", reminderLocalTime(atMs, zone)).toString()
}

/** Where the event buffer persists: one JSON array of event strings. */
interface EventStore {
    fun load(): String?
    fun save(value: String)
}

/**
 * The persisted buffer and the one sink. A second attach replaces the first (ADR-0006); a detach by a
 * replaced subscriber is ignored. A sink that throws is detached and the event it failed on stays
 * buffered, for the next attach.
 */
class EventBuffer(private val store: EventStore, private val cap: Int = MAX_EVENTS) {
    private var sink: ((String) -> Unit)? = null
    private var sinkId = 0
    private var lastId = 0

    /** Buffers `event`, then delivers the buffer if a sink is attached. Never throws into the caller. */
    @Synchronized fun emit(event: String) {
        val list = load()
        list.add(event)
        while (list.size > cap) list.removeAt(0)
        save(list)
        try {
            flush()
        } catch (e: Exception) {
            // The sink's failure already detached it; the event stays buffered.
        }
    }

    /** Attaches `s` and flushes the buffer into it in order. A sink failure propagates (and detaches it). */
    @Synchronized fun attach(s: (String) -> Unit): Int {
        sink = s
        sinkId = ++lastId
        flush()
        return sinkId
    }

    @Synchronized fun detach(id: Int) {
        if (id == sinkId) sink = null
    }

    private fun flush() {
        val s = sink ?: return
        val list = load()
        while (list.isNotEmpty()) {
            try {
                s(list[0])
            } catch (e: Exception) {
                sink = null
                throw e
            }
            list.removeAt(0)
            save(list)
        }
    }

    private fun load(): MutableList<String> {
        val raw = store.load() ?: return ArrayList()
        return try {
            val a = JSONArray(raw)
            (0 until a.length()).mapNotNullTo(ArrayList()) { a.optString(it, null) }
        } catch (e: JSONException) {
            ArrayList()
        }
    }

    private fun save(list: List<String>) {
        store.save(JSONArray(list).toString())
    }

    companion object {
        const val MAX_EVENTS = 100
    }
}

/** The buffer in the registry's SharedPreferences file, key "events", written with commit(). */
private class PrefsEventStore(private val ctx: Context) : EventStore {
    private fun prefs() = ctx.getSharedPreferences("mobiler_notifications", Context.MODE_PRIVATE)
    override fun load(): String? = prefs().getString("events", null)
    override fun save(value: String) {
        prefs().edit().putString("events", value).commit()
    }
}

object ReminderEvents {
    private var buffer: EventBuffer? = null

    @Synchronized private fun buffer(ctx: Context): EventBuffer =
        buffer ?: EventBuffer(PrefsEventStore(ctx.applicationContext)).also { buffer = it }

    /** Buffers one event (and delivers it at once when the core is subscribed). */
    fun emit(ctx: Context, type: String, id: Int, actionId: String?, payload: String?) {
        buffer(ctx).emit(
            reminderEventJson(type, id, actionId, payload, System.currentTimeMillis(), ZoneId.systemDefault())
        )
    }

    /** Attaches the stream's sink, flushing the buffer into it in order; returns the id for [detach]. */
    fun attach(ctx: Context, sink: (String) -> Unit): Int = buffer(ctx).attach(sink)

    fun detach(id: Int) {
        synchronized(this) { buffer }?.detach(id)
    }
}

// Turns a tap on a reminder (its body, or an `open` action button) into an `opened` event. The tap brings
// the launcher activity forward with the `mobiler_local_*` extras; they are read when the activity resumes
// (MainActivity's onNewIntent calls setIntent, so a tap on a running app is seen too) and removed, so each
// tap is reported once. The event is buffered until the core subscribes (a cold-start tap). The push
// plugin's PushOpens is the precedent.
object LocalOpens {
    private var installed = false

    @Synchronized fun install(app: android.app.Application) {
        if (installed) return
        installed = true
        app.registerActivityLifecycleCallbacks(object : android.app.Application.ActivityLifecycleCallbacks {
            override fun onActivityResumed(activity: Activity) = consume(app, activity.intent)
            override fun onActivityCreated(activity: Activity, savedInstanceState: Bundle?) {}
            override fun onActivityStarted(activity: Activity) {}
            override fun onActivityPaused(activity: Activity) {}
            override fun onActivityStopped(activity: Activity) {}
            override fun onActivitySaveInstanceState(activity: Activity, outState: Bundle) {}
            override fun onActivityDestroyed(activity: Activity) {}
        })
        // The plugin (built with the core) may be created after the first onResume already ran.
        MobilerActivity.current?.get()?.let { consume(app, it.intent) }
    }

    private fun consume(ctx: Context, intent: Intent?) {
        if (intent == null || !intent.hasExtra(LocalNotif.EXTRA_ID)) return
        val id = intent.getIntExtra(LocalNotif.EXTRA_ID, -1)
        val action = intent.getStringExtra(LocalNotif.EXTRA_ACTION).orEmpty()
        val payload = intent.getStringExtra(LocalNotif.EXTRA_PAYLOAD)
        intent.removeExtra(LocalNotif.EXTRA_ID)
        intent.removeExtra(LocalNotif.EXTRA_ACTION)
        intent.removeExtra(LocalNotif.EXTRA_PAYLOAD)
        // Relaunched from Recents: the original tap intent is replayed, but the user didn't tap anything.
        if ((intent.flags and Intent.FLAG_ACTIVITY_LAUNCHED_FROM_HISTORY) != 0) return
        if (id < 1) return
        if (action.isNotEmpty()) {
            // An action button doesn't auto-cancel its notification (only a body tap does); a programmatic
            // cancel sends no delete intent, so no `dismissed` follows.
            runCatching { NotificationManagerCompat.from(ctx).cancel(LocalNotif.TAG, id) }
        }
        ReminderEvents.emit(ctx, "opened", id, action.ifEmpty { null }, payload)
    }
}
