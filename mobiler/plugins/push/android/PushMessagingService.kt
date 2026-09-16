package {{PACKAGE}}

import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.os.Build
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import com.google.firebase.messaging.FirebaseMessagingService
import com.google.firebase.messaging.RemoteMessage
import org.json.JSONObject
import java.util.concurrent.atomic.AtomicInteger

// FCM entry point — runs in the app process even when no UI is alive (FCM starts it). Statically
// declared in the manifest by the plugin (a <service> with the MESSAGING_EVENT intent-filter).
//   onNewToken       → push a {"type":"token_refresh","token":…} event so the app re-POSTs to its backend.
//   onMessageReceived→ build a tray notification (data messages don't auto-display) whose tap opens the
//                      app carrying the payload (→ an "opened" event), and — only if the app is in the
//                      foreground — emit the payload as "received" right away.
// Send DATA messages from your backend so the app controls display. (Notification messages sent while
// the app is backgrounded are posted by FCM itself and never reach this service; PushPlugin still
// recognises their tap as "opened".)
private const val CHANNEL_ID = "mobiler_push"

// Unique per notification: both the notification id and the PendingIntent request code. A shared
// request code + FLAG_UPDATE_CURRENT would make every notification's tap deliver the LATEST payload.
private val nextId = AtomicInteger((System.currentTimeMillis() and 0xFFFFFF).toInt())

class PushMessagingService : FirebaseMessagingService() {
    override fun onNewToken(token: String) {
        PushBus.emit("""{"type":"token_refresh","token":"$token"}""")
    }

    override fun onMessageReceived(message: RemoteMessage) {
        // Merge the data map (+ any notification title/body) into one JSON payload for the app.
        val payload = JSONObject()
        for ((k, v) in message.data) payload.put(k, v)
        message.notification?.let { n ->
            n.title?.let { payload.put("title", it) }
            n.body?.let { payload.put("body", it) }
        }
        val json = payload.toString()
        val id = nextId.incrementAndGet()

        // Post a tray notification so the user sees something even for a data-only message. Its tap
        // brings the app forward with the payload attached; PushPlugin turns that into "opened".
        ensureChannel(this)
        val title = message.notification?.title ?: message.data["title"] ?: "Notification"
        val body = message.notification?.body ?: message.data["body"] ?: ""
        val builder = NotificationCompat.Builder(this, CHANNEL_ID)
            .setSmallIcon(smallIcon(this))
            .setContentTitle(title)
            .setContentText(body)
            .setAutoCancel(true)
            .setPriority(NotificationCompat.PRIORITY_HIGH)
        packageManager.getLaunchIntentForPackage(packageName)?.let { launch ->
            launch.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP or Intent.FLAG_ACTIVITY_CLEAR_TOP)
            launch.putExtra(PushOpens.EXTRA_PAYLOAD, json)
            builder.setContentIntent(
                PendingIntent.getActivity(this, id, launch, PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
            )
        }
        runCatching { NotificationManagerCompat.from(this).notify(id, builder.build()) }

        // "received" is live-only and foreground-only (matching iOS willPresent): a push that arrives
        // while the app is backgrounded or dead is represented by the tray notification alone.
        if (MobilerActivity.current?.get() != null) PushBus.emitReceived(json)
    }
}

// Tray icon: the app's `mobiler_push_icon` drawable (a white-on-transparent status-bar icon) when it
// exists, else a system fallback.
private fun smallIcon(context: Context): Int =
    context.resources.getIdentifier("mobiler_push_icon", "drawable", context.packageName)
        .takeIf { it != 0 } ?: android.R.drawable.ic_dialog_info

private fun ensureChannel(context: Context) {
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
        val mgr = context.getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
        // Channel name shown in system Settings → Notifications: the app's `mobiler_push_channel_name`
        // string resource when it exists, else "Notifications". Re-creating an existing channel only
        // updates its name, so a renamed resource takes effect on the next push.
        val nameRes = context.resources.getIdentifier("mobiler_push_channel_name", "string", context.packageName)
        val name = if (nameRes != 0) context.getString(nameRes) else "Notifications"
        mgr.createNotificationChannel(NotificationChannel(CHANNEL_ID, name, NotificationManager.IMPORTANCE_HIGH))
    }
}
