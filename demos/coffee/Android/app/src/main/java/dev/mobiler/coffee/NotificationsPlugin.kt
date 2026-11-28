package dev.mobiler.coffee

import dev.mobiler.coffee.shared.types.PluginResponse

import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.provider.Settings
import androidx.activity.ComponentActivity
import androidx.activity.result.contract.ActivityResultContracts
import androidx.core.app.ActivityCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.content.ContextCompat
import java.time.ZoneId
import kotlin.coroutines.resume
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.channels.awaitClose
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.buffer
import kotlinx.coroutines.flow.callbackFlow
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withTimeoutOrNull
import org.json.JSONArray
import org.json.JSONException
import org.json.JSONObject

// Local scheduled notifications. Fires even when the app is closed via AlarmManager → a
// manifest-declared BroadcastReceiver → the system notification tray. Ops (input is JSON; a failure
// is ok:false "<code>" or "<code>: <detail>", codes invalid_input | past | unsupported | failed):
//   requestPermission: ""     → ok = allowed, "granted" | "denied" (Android 13+: after the user answers)
//   permissionStatus: ""      → ok:true, "granted" | "denied" | "notDetermined" (never prompts)
//   openSettings: ""          → ok:true when the app's notification settings page opened
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
//
// Stream (cx.subscribe("notifications", "events", "")): {"type":"opened"|"snoozed"|"dismissed",…}, see
// ReminderEvents.kt. Snooze and dismiss are handled natively (NotificationActionReceiver), taps by
// LocalOpens; every event is buffered (persisted) until the core subscribes.
class NotificationsPlugin(private val application: android.app.Application) : MobilerPlugin {
    init {
        // Alarms are gone after a force-stop (and a reboot the app missed): re-arm on every launch, on the
        // lane (disk + binder work, off the main thread). Queued before any op can be, so the app's first
        // calls (a re-plan) run after it; rearmAll holds the registry lock against the receivers.
        ReminderLane.post { runCatching { rearmAll(application, System.currentTimeMillis()) } }
        LocalOpens.install(application)
    }

    // The ops that touch the registry, the alarms or the tray run on ReminderLane, one at a time in the
    // order the shell started them. The permission and settings ops need the activity and touch none of
    // that, so they stay on the main dispatcher where the shell calls them.
    override suspend fun handle(op: String, input: String): PluginResponse = when (op) {
        "requestPermission" -> requestPermission()
        "permissionStatus" -> PluginResponse(true, permissionStatus())
        "openSettings" -> openSettings()
        "schedule" -> ReminderLane.run { schedule(input) }
        "cancel" -> ReminderLane.run { cancel(input) }
        "cancelAll" -> ReminderLane.run { cancelAll() }
        "listPending" -> ReminderLane.run { listPending() }
        "removeDelivered" -> ReminderLane.run { removeDelivered(input) }
        else -> PluginResponse(false, "unsupported: unknown op '$op'")
    }

    // The events stream: flushes the persisted buffer in order, then delivers live, until unsubscribed. The
    // channel is unbounded so a 100-event flush never fails a send; a send that fails (the stream closed)
    // throws inside the sink, which keeps that event buffered for the next subscriber.
    override fun subscribe(op: String, input: String): Flow<PluginResponse> = callbackFlow {
        if (op != "events") {
            send(PluginResponse(false, "unsupported: unknown stream '$op'"))
            close()
            awaitClose {}
            return@callbackFlow
        }
        val id = ReminderEvents.attach(application) { trySend(PluginResponse(true, it)).getOrThrow() }
        awaitClose { ReminderEvents.detach(id) }
    }.buffer(Channel.UNLIMITED)

    private fun enabled() = NotificationManagerCompat.from(application).areNotificationsEnabled()

    private fun prefs() = application.getSharedPreferences("mobiler_notifications", android.content.Context.MODE_PRIVATE)

    // "granted" | "denied" | "notDetermined". Before Android 13 there is no runtime prompt: the app is
    // allowed unless the user turned its notifications off. On 13+ a not-granted permission is "denied"
    // once this plugin asked (the persisted "asked" flag), or once the user denied it from any prompt
    // (the system then wants a rationale); otherwise the prompt was never answered: "notDetermined".
    private fun permissionStatus(): String {
        if (enabled()) return "granted"
        if (Build.VERSION.SDK_INT < 33) return "denied"
        if (prefs().getBoolean(KEY_ASKED, false)) return "denied"
        val activity = MobilerActivity.current?.get()
        if (activity != null && ActivityCompat.shouldShowRequestPermissionRationale(activity, POST)) return "denied"
        return "notDetermined"
    }

    // Android 13+: shows the system prompt and answers once the user does (≤ 60 s; the current status
    // after that). The system itself answers at once, with no dialog, when the permission is already
    // granted or permanently denied. Runs on the main dispatcher and suspends, never blocks; the one-shot
    // callback is unregistered on every path. Calls are serialised (one registry key).
    private suspend fun requestPermission(): PluginResponse = permissionLock.withLock {
        if (Build.VERSION.SDK_INT >= 33 && !enabled() &&
            ContextCompat.checkSelfPermission(application, POST) != PackageManager.PERMISSION_GRANTED
        ) {
            val activity = MobilerActivity.current?.get() as? ComponentActivity
            if (activity != null) {
                prefs().edit().putBoolean(KEY_ASKED, true).commit()
                withTimeoutOrNull(PERMISSION_TIMEOUT_MS) { askPermission(activity) }
            }
        }
        val ok = enabled()
        PluginResponse(ok, if (ok) "granted" else "denied")
    }

    private suspend fun askPermission(activity: ComponentActivity): Boolean =
        suspendCancellableCoroutine { cont ->
            var launcher: androidx.activity.result.ActivityResultLauncher<String>? = null
            val registered = activity.activityResultRegistry.register(
                PERMISSION_KEY, ActivityResultContracts.RequestPermission()
            ) { granted ->
                launcher?.unregister()
                if (cont.isActive) cont.resume(granted)
            }
            launcher = registered
            // A timeout (or the core going away) cancels the wait: drop the callback with it.
            cont.invokeOnCancellation { registered.unregister() }
            if (!cont.isActive) {
                // An answer to an earlier, timed-out prompt was still pending for the key and was handed
                // to this callback at once: answer with it (the caller re-reads the real status).
                registered.unregister()
                return@suspendCancellableCoroutine
            }
            try {
                registered.launch(POST)
            } catch (e: Exception) {
                registered.unregister()
                if (cont.isActive) cont.resume(false)
            }
        }

    // The app's notification settings page (Android 8+), else the app details page.
    private fun openSettings(): PluginResponse {
        val ctx = MobilerActivity.current?.get() ?: application
        val notif = Intent(Settings.ACTION_APP_NOTIFICATION_SETTINGS)
            .putExtra(Settings.EXTRA_APP_PACKAGE, application.packageName)
        val details = Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, Uri.fromParts("package", application.packageName, null))
        for (intent in listOf(notif, details)) {
            if (ctx === application) intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
            try {
                ctx.startActivity(intent)
                return PluginResponse(true, "")
            } catch (e: Exception) {
                // e.g. ActivityNotFoundException on a stripped-down build: try the next page.
            }
        }
        return PluginResponse(false, "failed: no settings page could be opened")
    }

    // Every registry op runs to completion on ReminderLane without suspending, and the lane runs them in
    // the order the core issued them (a cancelAll then 40 schedules leaves exactly those 40). The
    // registry lock also orders them against the receivers.
    private fun schedule(input: String): PluginResponse {
        val r = parseSchedule(input, System.currentTimeMillis(), ZoneId.systemDefault())
            .getOrElse { return PluginResponse(false, it.message ?: "invalid_input") }
        ensureReminderChannel(application)
        synchronized(ReminderRegistry) {
            val previous = ReminderRegistry.get(application, r.id)
            try {
                // Written before the alarm; a re-schedule of an id replaces it (and any snooze of it).
                ReminderRegistry.put(application, r)
                armAlarm(application, r)
            } catch (e: Exception) {
                // Nothing was scheduled: put back the reminder this one would have replaced.
                if (previous != null) ReminderRegistry.put(application, previous)
                else ReminderRegistry.remove(application, r.id)
                return PluginResponse(false, "failed: ${e.message ?: e.javaClass.simpleName}")
            }
        }
        return PluginResponse(true, "")
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
        // Only this plugin's tray entry: the tagged one, or a pre-registry (untagged) one on our channel.
        removeDeliveredReminders(application, listOf(id))
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
        // Under the registry lock: the receivers' re-arm and snooze run on their own threads.
        val all = synchronized(ReminderRegistry) { ReminderRegistry.all(application) }
        all.sortedWith(compareBy({ it.fireAtMs }, { it.id })).forEach {
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

    private companion object {
        const val POST = android.Manifest.permission.POST_NOTIFICATIONS
        const val KEY_ASKED = "asked"
        const val PERMISSION_KEY = "mobiler_notif_perm"
        const val PERMISSION_TIMEOUT_MS = 60_000L
        val permissionLock = Mutex()
    }

    private fun validId(v: Any?): Int? {
        val n = v as? Number ?: return null
        val d = n.toDouble()
        if (d != Math.floor(d) || d < 1.0 || d > Int.MAX_VALUE.toDouble()) return null
        return n.toInt()
    }
}
