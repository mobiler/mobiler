package dev.mobiler.coffee

import dev.mobiler.coffee.shared.types.PluginResponse

import android.app.AlarmManager
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.os.Build
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import org.json.JSONObject

// Local scheduled notifications (paid plugin). Fires even when the app is closed via AlarmManager
// → a manifest-declared BroadcastReceiver → the system notification tray. Ops (input is JSON):
//   requestPermission: ""                                       → ok = notifications allowed
//   schedule: {"id":1,"title":"...","body":"...","after_seconds":10} → ok:true (fires later)
//   cancel:   {"id":1}                                          → ok:true
//
// Needs POST_NOTIFICATIONS (API 33+) and a <receiver android:name=".NotificationReceiver"> — both
// added by the plugin manifest (uses-permission + manifest_application). The reminder timing is
// inexact (setAndAllowWhileIdle) to avoid the restricted exact-alarm permission; fine for
// appointment reminders. Use exact alarms only if you need to-the-minute precision.
private const val CHANNEL_ID = "mobiler_reminders"

class NotificationsPlugin(private val application: android.app.Application) : MobilerPlugin {
    override suspend fun handle(op: String, input: String): PluginResponse = when (op) {
        "requestPermission" -> requestPermission()
        "schedule" -> schedule(input)
        "cancel" -> cancel(input)
        else -> PluginResponse(false, "unknown op '$op'")
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

    private fun schedule(input: String): PluginResponse {
        val obj = runCatching { JSONObject(input) }.getOrNull()
            ?: return PluginResponse(false, "invalid input JSON")
        val id = obj.optInt("id", 1)
        val title = obj.optString("title")
        val body = obj.optString("body")
        val after = obj.optLong("after_seconds", 0)

        ensureChannel(application)
        val intent = Intent(application, NotificationReceiver::class.java).apply {
            putExtra("id", id); putExtra("title", title); putExtra("body", body)
        }
        val pi = PendingIntent.getBroadcast(
            application, id, intent,
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
        )
        val alarm = application.getSystemService(Context.ALARM_SERVICE) as AlarmManager
        val at = System.currentTimeMillis() + after * 1000
        alarm.setAndAllowWhileIdle(AlarmManager.RTC_WAKEUP, at, pi)
        return PluginResponse(true, "")
    }

    private fun cancel(input: String): PluginResponse {
        val id = runCatching { JSONObject(input).optInt("id", 1) }.getOrDefault(1)
        val intent = Intent(application, NotificationReceiver::class.java)
        val pi = PendingIntent.getBroadcast(
            application, id, intent,
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
        )
        (application.getSystemService(Context.ALARM_SERVICE) as AlarmManager).cancel(pi)
        NotificationManagerCompat.from(application).cancel(id)
        return PluginResponse(true, "")
    }
}

// Posts the notification when the alarm fires — runs even if the app process is dead, which is
// why it must be a statically-registered <receiver> (added to the manifest by the plugin).
class NotificationReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        ensureChannel(context)
        val id = intent.getIntExtra("id", 1)
        val notif = NotificationCompat.Builder(context, CHANNEL_ID)
            .setSmallIcon(android.R.drawable.ic_dialog_info)
            .setContentTitle(intent.getStringExtra("title") ?: "Reminder")
            .setContentText(intent.getStringExtra("body") ?: "")
            .setAutoCancel(true)
            .setPriority(NotificationCompat.PRIORITY_HIGH)
            .build()
        runCatching { NotificationManagerCompat.from(context).notify(id, notif) } // no-op if not permitted
    }
}

private fun ensureChannel(context: Context) {
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
        val mgr = context.getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
        if (mgr.getNotificationChannel(CHANNEL_ID) == null) {
            mgr.createNotificationChannel(
                NotificationChannel(CHANNEL_ID, "Reminders", NotificationManager.IMPORTANCE_HIGH)
            )
        }
    }
}
