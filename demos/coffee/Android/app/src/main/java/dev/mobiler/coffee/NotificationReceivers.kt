package dev.mobiler.coffee

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
import java.time.ZoneId
import org.json.JSONArray

// The notifications plugin's alarm side: arming / cancelling the per-reminder alarm, posting a
// reminder, the firing receiver and the re-arm receiver. Both receivers are statically declared
// (plugin manifest), so they run with no live app process.

/** The markers every alarm and tap intent carries (spec §4.3). */
object LocalNotif {
    const val CHANNEL_ID = "mobiler_reminders"
    /** Every tray entry this plugin posts uses this tag, so management ops touch only ours. */
    const val TAG = "mobiler.local"
    /** The category on every launch intent this plugin's notifications open (see reminderLaunchIntent). */
    const val CATEGORY = "mobiler.local"
    const val EXTRA_ID = "mobiler_local_id"
    const val EXTRA_ACTION = "mobiler_local_action"
    const val EXTRA_PAYLOAD = "mobiler_local_payload"
    /** Set on every alarm the registry plugin arms; an alarm without it is a pre-registry (legacy) one. */
    const val EXTRA_V2 = "mobiler_v2"
    /** A snooze button's intent carries the whole posted reminder (its registry entry is gone once it fired). */
    const val EXTRA_REMINDER = "mobiler_local_reminder"
    const val EXTRA_SNOOZE_MINUTES = "mobiler_local_snooze_minutes"
    /** NotificationActionReceiver intent actions: the delete intent, and snooze button `i` (prefix + i). */
    const val ACTION_DISMISS = "mobiler.local.dismiss"
    const val ACTION_SNOOZE_PREFIX = "mobiler.local.snooze."
}

internal fun ensureReminderChannel(context: Context) {
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
        val mgr = context.getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
        if (mgr.getNotificationChannel(LocalNotif.CHANNEL_ID) == null) {
            mgr.createNotificationChannel(
                NotificationChannel(LocalNotif.CHANNEL_ID, "Reminders", NotificationManager.IMPORTANCE_HIGH)
            )
        }
    }
}

// The alarm intent for reminder `id`. Intent extras don't take part in PendingIntent identity, so
// (request code `id`, NotificationReceiver) is the same PendingIntent a legacy alarm used: arming or
// cancelling an id also replaces / cancels a pre-registry alarm of that id.
private fun alarmIntent(ctx: Context, id: Int) = Intent(ctx, NotificationReceiver::class.java).apply {
    putExtra(LocalNotif.EXTRA_V2, true)
    putExtra(LocalNotif.EXTRA_ID, id)
    putExtra(LocalNotif.EXTRA_ACTION, "")
}

private fun alarmPendingIntent(ctx: Context, id: Int): PendingIntent = PendingIntent.getBroadcast(
    ctx, id, alarmIntent(ctx, id), PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
)

/** Whether reminder `id`'s alarm PendingIntent still exists: a reboot or a force-stop clears it,
 *  cancelAlarm cancels it. */
private fun alarmAlive(ctx: Context, id: Int): Boolean = PendingIntent.getBroadcast(
    ctx, id, alarmIntent(ctx, id), PendingIntent.FLAG_NO_CREATE or PendingIntent.FLAG_IMMUTABLE
) != null

/**
 * The launcher intent every tap on a reminder opens (its body and its `open` buttons).
 * The `mobiler.local` category takes part in PendingIntent identity (filterEquals) but not in resolving
 * this explicit intent, so reminder N's tap PendingIntent never collides with another plugin's (e.g.
 * push notification N), whose extras would otherwise overwrite ours. Not intent data: MainActivity reads
 * data as a deep link.
 */
fun reminderLaunchIntent(ctx: Context): Intent? =
    ctx.packageManager.getLaunchIntentForPackage(ctx.packageName)?.addCategory(LocalNotif.CATEGORY)

/** Sets (or replaces) the inexact alarm for `r` at `r.fireAtMs`. The registry entry is written first. */
fun armAlarm(ctx: Context, r: Reminder) {
    val alarm = ctx.getSystemService(Context.ALARM_SERVICE) as AlarmManager
    alarm.setAndAllowWhileIdle(AlarmManager.RTC_WAKEUP, r.fireAtMs, alarmPendingIntent(ctx, r.id))
}

fun cancelAlarm(ctx: Context, id: Int) {
    val pi = alarmPendingIntent(ctx, id)
    (ctx.getSystemService(Context.ALARM_SERVICE) as AlarmManager).cancel(pi)
    pi.cancel()
}

/**
 * Posts `r` to the tray now:
 * - a body tap opens the app with the `mobiler_local_*` extras (→ `opened`, see LocalOpens);
 * - an `open` button does the same with `mobiler_local_action=<action id>`;
 * - a `snooze` button and the delete intent (swipe away) go to NotificationActionReceiver.
 *
 * Every PendingIntent of reminder `id` uses request code `id`; they are told apart by an intent field
 * that takes part in PendingIntent identity (filterEquals), never by extras: the body intent has
 * category `mobiler.local`, open button `i` adds category `mobiler.local.a<i>`, snooze button `i` and
 * the delete intent have their own intent action. So each (id, action index) has its own PendingIntent.
 */
fun postReminder(ctx: Context, r: Reminder) {
    ensureReminderChannel(ctx)
    val flags = PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
    val builder = NotificationCompat.Builder(ctx, LocalNotif.CHANNEL_ID)
        .setSmallIcon(android.R.drawable.ic_dialog_info)
        .setContentTitle(r.title.ifEmpty { "Reminder" })
        .setContentText(r.body)
        .setStyle(NotificationCompat.BigTextStyle().bigText(r.body))
        .setAutoCancel(true)
        .setPriority(NotificationCompat.PRIORITY_HIGH)
    openIntent(ctx, r, "", null)?.let { builder.setContentIntent(PendingIntent.getActivity(ctx, r.id, it, flags)) }
    // Swipe away / "clear all" → `dismissed`. A programmatic cancel (snooze, an open button, the
    // management ops) sends no delete intent.
    val dismiss = Intent(ctx, NotificationActionReceiver::class.java).setAction(LocalNotif.ACTION_DISMISS)
        .putExtra(LocalNotif.EXTRA_ID, r.id)
    if (r.payload != null) dismiss.putExtra(LocalNotif.EXTRA_PAYLOAD, r.payload)
    builder.setDeleteIntent(PendingIntent.getBroadcast(ctx, r.id, dismiss, flags))

    val actions = r.actionsJson?.let { runCatching { JSONArray(it) }.getOrNull() } ?: JSONArray()
    for (i in 0 until actions.length()) {
        val a = actions.optJSONObject(i) ?: continue
        val aid = a.optString("id")
        val label = a.optString("label")
        val pi = when (a.optString("kind")) {
            "open" -> openIntent(ctx, r, aid, i)?.let { PendingIntent.getActivity(ctx, r.id, it, flags) }
            "snooze" -> {
                val snooze = Intent(ctx, NotificationActionReceiver::class.java)
                    .setAction(LocalNotif.ACTION_SNOOZE_PREFIX + i)
                    .putExtra(LocalNotif.EXTRA_ID, r.id)
                    .putExtra(LocalNotif.EXTRA_ACTION, aid)
                    .putExtra(LocalNotif.EXTRA_SNOOZE_MINUTES, a.optInt("snooze_minutes", 10))
                    .putExtra(LocalNotif.EXTRA_REMINDER, r.toJson())
                PendingIntent.getBroadcast(ctx, r.id, snooze, flags)
            }
            else -> null
        } ?: continue
        builder.addAction(0, label, pi)
    }
    // No-op (SecurityException swallowed) when notifications aren't permitted.
    runCatching { NotificationManagerCompat.from(ctx).notify(LocalNotif.TAG, r.id, builder.build()) }
}

/** The launch intent for a body tap (`actionId` "", `index` null) or open button `index`. */
private fun openIntent(ctx: Context, r: Reminder, actionId: String, index: Int?): Intent? {
    val launch = reminderLaunchIntent(ctx) ?: return null
    if (index != null) launch.addCategory(LocalNotif.CATEGORY + ".a" + index)
    launch.putExtra(LocalNotif.EXTRA_ID, r.id)
    launch.putExtra(LocalNotif.EXTRA_ACTION, actionId)
    if (r.payload != null) launch.putExtra(LocalNotif.EXTRA_PAYLOAD, r.payload)
    return launch
}

/** Removes this plugin's tray entries: all of them (`ids == null`) or the given ids. */
fun removeDeliveredReminders(ctx: Context, ids: Collection<Int>?) {
    val nm = NotificationManagerCompat.from(ctx)
    val mgr = ctx.getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
    for (sbn in mgr.activeNotifications) {
        val ours = sbn.tag == LocalNotif.TAG ||
            // A reminder posted by the pre-registry plugin: untagged, on our channel.
            (sbn.tag == null && Build.VERSION.SDK_INT >= Build.VERSION_CODES.O &&
                sbn.notification.channelId == LocalNotif.CHANNEL_ID)
        if (ours && (ids == null || sbn.id in ids)) nm.cancel(sbn.tag, sbn.id)
    }
}

/**
 * Re-arms every registry entry (boot, app update, time / zone change, plugin init): wall-clock ones
 * are re-computed in the current zone, future ones and unmoved live overdue ones armed, other overdue
 * ones posted now when late within `late_minutes`, older ones dropped (see rearmDecision). A posted or
 * dropped entry's alarm is cancelled, alive or not.
 */
fun rearmAll(ctx: Context, nowMs: Long) {
    val zone = ZoneId.systemDefault()
    synchronized(ReminderRegistry) {
        for (r in ReminderRegistry.all(ctx)) {
            val (decision, updated) = rearmDecision(r, nowMs, zone, alarmAlive(ctx, r.id))
            when (decision) {
                RearmDecision.ARM -> {
                    if (updated != r) ReminderRegistry.put(ctx, updated)
                    armAlarm(ctx, updated)
                }
                RearmDecision.POST -> {
                    ReminderRegistry.remove(ctx, r.id)
                    cancelAlarm(ctx, r.id)
                    postReminder(ctx, updated)
                }
                RearmDecision.DROP -> {
                    ReminderRegistry.remove(ctx, r.id)
                    cancelAlarm(ctx, r.id)
                }
            }
        }
    }
}

// Fired by the alarm. A registry alarm (mobiler_v2) posts its registry entry and removes it; no entry
// means it was cancelled, so nothing is posted. `late_minutes` never applies here: an alarm Doze
// delayed is on time by definition. A legacy alarm (set before the registry existed) still posts from
// its own title/body extras, so reminders pending across the update aren't lost.
class NotificationReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        if (!intent.getBooleanExtra(LocalNotif.EXTRA_V2, false)) {
            postLegacy(context, intent)
            return
        }
        val id = intent.getIntExtra(LocalNotif.EXTRA_ID, -1)
        val now = System.currentTimeMillis()
        val r = synchronized(ReminderRegistry) {
            val entry = ReminderRegistry.get(context, id)
            // Not yet due: a replacement scheduled while this alarm was in flight; its own alarm posts it.
            if (entry == null || !receiverShouldPost(entry, now)) null
            else entry.also { ReminderRegistry.remove(context, id) }
        } ?: return
        postReminder(context, r)
    }

    private fun postLegacy(context: Context, intent: Intent) {
        ensureReminderChannel(context)
        val id = intent.getIntExtra("id", 1)
        val notif = NotificationCompat.Builder(context, LocalNotif.CHANNEL_ID)
            .setSmallIcon(android.R.drawable.ic_dialog_info)
            .setContentTitle(intent.getStringExtra("title") ?: "Reminder")
            .setContentText(intent.getStringExtra("body") ?: "")
            .setAutoCancel(true)
            .setPriority(NotificationCompat.PRIORITY_HIGH)
            .build()
        runCatching { NotificationManagerCompat.from(context).notify(id, notif) } // no-op if not permitted
    }
}

// Alarms don't survive a reboot, and a wall-clock reminder must move with a time-zone change: the
// system broadcasts below re-arm the registry. Exported only for those system broadcasts.
class NotificationRearmReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        when (intent.action) {
            Intent.ACTION_BOOT_COMPLETED,
            Intent.ACTION_MY_PACKAGE_REPLACED,
            Intent.ACTION_TIMEZONE_CHANGED,
            Intent.ACTION_TIME_CHANGED -> rearmAll(context, System.currentTimeMillis())
        }
    }
}

// A reminder's snooze button and its delete intent (swiped away). Statically declared and not exported:
// only this app's own PendingIntents reach it, and it runs with no live app process. Native only: the
// core never runs while the app is closed (ADR-0036); the core learns of both through the events stream.
class NotificationActionReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        val id = intent.getIntExtra(LocalNotif.EXTRA_ID, -1)
        if (id < 1) return
        val action = intent.action ?: return
        when {
            action == LocalNotif.ACTION_DISMISS ->
                ReminderEvents.emit(context, "dismissed", id, null, intent.getStringExtra(LocalNotif.EXTRA_PAYLOAD))
            action.startsWith(LocalNotif.ACTION_SNOOZE_PREFIX) -> snooze(context, intent, id)
        }
    }

    // The firing receiver removed the entry when it posted, so the snooze writes a new one from the
    // reminder the button carries: snoozed, due in `snooze_minutes`, no `wall` (a zone change never moves
    // it). A later cancel / cancelAll / re-schedule of the id removes it like any entry.
    private fun snooze(context: Context, intent: Intent, id: Int) {
        val carried = intent.getStringExtra(LocalNotif.EXTRA_REMINDER)?.let { Reminder.fromJson(it) } ?: return
        if (carried.id != id) return
        val actionId = intent.getStringExtra(LocalNotif.EXTRA_ACTION)
        val minutes = intent.getIntExtra(LocalNotif.EXTRA_SNOOZE_MINUTES, 10)
        synchronized(ReminderRegistry) {
            val snoozed = snoozeEntry(ReminderRegistry.get(context, id), carried, System.currentTimeMillis(), minutes)
            if (snoozed != null) {
                ReminderRegistry.put(context, snoozed)
                armAlarm(context, snoozed)
            }
            // Snoozing cancels the tray entry; a programmatic cancel sends no delete intent (no `dismissed`).
            runCatching { NotificationManagerCompat.from(context).cancel(LocalNotif.TAG, id) }
            if (snoozed != null) ReminderEvents.emit(context, "snoozed", id, actionId, carried.payload)
        }
    }
}
