package dev.mobiler.barbershop

import dev.mobiler.barbershop.shared.types.PluginResponse

import android.os.Build
import androidx.core.app.NotificationManagerCompat
import java.time.ZoneId
import org.json.JSONArray
import org.json.JSONException
import org.json.JSONObject

// Local scheduled notifications. Fires even when the app is closed via AlarmManager → a
// manifest-declared BroadcastReceiver → the system notification tray. Ops (input is JSON; a failure
// is ok:false "<code>" or "<code>: <detail>", codes invalid_input | past | unsupported | failed):
//   requestPermission: ""                                   → ok = notifications allowed
//   schedule: {"id":7,"title":"…","body":"…", "at":"YYYY-MM-DD HH:MM" | "after_seconds":N,
//              "payload"?:"…","actions"?:[…],"late_minutes"?:1440}   → ok:true ""
//   cancel:   {"id":7}                                      → ok:true (pending, registry and tray)
//   cancelAll: ""                                           → ok:true (every reminder of this plugin)
//   listPending: ""     → [{"id":7,"fire_at":"YYYY-MM-DD HH:MM:SS","snoozed":false}] by fire_at
//   removeDelivered: {"ids":[1,2]} or "" for all           → ok:true (this plugin's tray entries)
//
// Every reminder is kept in a persisted registry (ReminderRegistry.kt), so it survives a reboot, an
// app update and a time or time-zone change (NotificationRearmReceiver), and plugin init re-arms it
// after a force-stop. The timing is inexact (setAndAllowWhileIdle), which needs no exact-alarm
// permission; fine for reminders.
class NotificationsPlugin(private val application: android.app.Application) : MobilerPlugin {
    init {
        // Alarms are gone after a force-stop (and a reboot the app missed): re-arm on every launch.
        runCatching { rearmAll(application, System.currentTimeMillis()) }
    }

    override suspend fun handle(op: String, input: String): PluginResponse = when (op) {
        "requestPermission" -> requestPermission()
        "schedule" -> schedule(input)
        "cancel" -> cancel(input)
        "cancelAll" -> cancelAll()
        "listPending" -> listPending()
        "removeDelivered" -> removeDelivered(input)
        else -> PluginResponse(false, "unsupported: unknown op '$op'")
    }

    private fun requestPermission(): PluginResponse {
        if (Build.VERSION.SDK_INT >= 33) {
            // Fire the system prompt (best-effort) from the foreground Activity; the result lands
            // in the OS, and the app re-checks via areNotificationsEnabled() on the next call.
            val activity = MobilerActivity.current?.get()
            if (activity != null) {
                androidx.core.app.ActivityCompat.requestPermissions(
                    activity, arrayOf(android.Manifest.permission.POST_NOTIFICATIONS), 0
                )
            }
        }
        val enabled = NotificationManagerCompat.from(application).areNotificationsEnabled()
        return PluginResponse(enabled, if (enabled) "granted" else "requested")
    }

    // Every op runs to completion on the caller's thread without suspending, so calls keep the order
    // the core issued them in (a cancelAll then 40 schedules leaves exactly those 40). The registry
    // lock also orders them against the receivers.
    private fun schedule(input: String): PluginResponse {
        val r = parseSchedule(input, System.currentTimeMillis(), ZoneId.systemDefault())
            .getOrElse { return PluginResponse(false, it.message ?: "invalid_input") }
        ensureReminderChannel(application)
        return try {
            synchronized(ReminderRegistry) {
                // Written before the alarm; a re-schedule of an id replaces it (and any snooze of it).
                ReminderRegistry.put(application, r)
                armAlarm(application, r)
            }
            PluginResponse(true, "")
        } catch (e: Exception) {
            ReminderRegistry.remove(application, r.id)
            PluginResponse(false, "failed: ${e.message ?: e.javaClass.simpleName}")
        }
    }

    private fun cancel(input: String): PluginResponse {
        // A missing id (or empty input) keeps the old default of 1.
        val id = if (input.isBlank()) 1 else {
            val o = try { JSONObject(input) } catch (e: JSONException) {
                return PluginResponse(false, "invalid_input: not a JSON object")
            }
            if (o.isNull("id")) 1 else validId(o.get("id"))
                ?: return PluginResponse(false, "invalid_input: `id` must be an integer in 1..=2147483647")
        }
        synchronized(ReminderRegistry) {
            ReminderRegistry.remove(application, id)
            cancelAlarm(application, id)
        }
        val nm = NotificationManagerCompat.from(application)
        nm.cancel(LocalNotif.TAG, id)
        nm.cancel(id) // a reminder posted by the pre-registry plugin (untagged), as before
        return PluginResponse(true, "")
    }

    private fun cancelAll(): PluginResponse {
        synchronized(ReminderRegistry) {
            for (r in ReminderRegistry.all(application)) cancelAlarm(application, r.id)
            ReminderRegistry.clear(application)
        }
        removeDeliveredReminders(application, null)
        return PluginResponse(true, "")
    }

    private fun listPending(): PluginResponse {
        val zone = ZoneId.systemDefault()
        val out = JSONArray()
        ReminderRegistry.all(application).sortedWith(compareBy({ it.fireAtMs }, { it.id })).forEach {
            out.put(
                JSONObject()
                    .put("id", it.id)
                    .put("fire_at", reminderLocalTime(it.fireAtMs, zone))
                    .put("snoozed", it.snoozed)
            )
        }
        return PluginResponse(true, out.toString())
    }

    private fun removeDelivered(input: String): PluginResponse {
        if (input.isBlank()) {
            removeDeliveredReminders(application, null)
            return PluginResponse(true, "")
        }
        val arr = try { JSONObject(input).getJSONArray("ids") } catch (e: JSONException) {
            return PluginResponse(false, "invalid_input: expected {\"ids\":[…]} or \"\"")
        }
        val ids = HashSet<Int>()
        for (i in 0 until arr.length()) {
            ids.add(validId(arr.get(i)) ?: return PluginResponse(false, "invalid_input: ids must be integers in 1..=2147483647"))
        }
        removeDeliveredReminders(application, ids)
        return PluginResponse(true, "")
    }

    private fun validId(v: Any?): Int? {
        val n = v as? Number ?: return null
        val d = n.toDouble()
        if (d != Math.floor(d) || d < 1.0 || d > Int.MAX_VALUE.toDouble()) return null
        return n.toInt()
    }
}
