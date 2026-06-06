package {{PACKAGE}}

import android.app.NotificationChannel
import android.app.NotificationManager
import android.content.Context
import android.os.Build
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import com.google.firebase.messaging.FirebaseMessagingService
import com.google.firebase.messaging.RemoteMessage
import org.json.JSONObject

// FCM entry point — runs in the app process even when no UI is alive (FCM starts it). Statically
// declared in the manifest by the plugin (a <service> with the MESSAGING_EVENT intent-filter).
//   onNewToken       → push a {"type":"token_refresh","token":…} event so the app re-POSTs to its backend.
//   onMessageReceived→ build a tray notification (data messages don't auto-display) + emit the payload.
// Both go through PushBus, which buffers until the core subscribes. Send DATA messages from your
// backend so the app controls display + the event stream fires consistently (foreground + background).
private const val CHANNEL_ID = "mobiler_push"

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

        // Post a tray notification so the user sees something even for a data-only message.
        ensureChannel(this)
        val title = message.notification?.title ?: message.data["title"] ?: "Notification"
        val body = message.notification?.body ?: message.data["body"] ?: ""
        val notif = NotificationCompat.Builder(this, CHANNEL_ID)
            .setSmallIcon(android.R.drawable.ic_dialog_info)
            .setContentTitle(title)
            .setContentText(body)
            .setAutoCancel(true)
            .setPriority(NotificationCompat.PRIORITY_HIGH)
            .build()
        runCatching { NotificationManagerCompat.from(this).notify(System.currentTimeMillis().toInt(), notif) }

        PushBus.emit(json)
    }
}

private fun ensureChannel(context: Context) {
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
        val mgr = context.getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
        if (mgr.getNotificationChannel(CHANNEL_ID) == null) {
            mgr.createNotificationChannel(
                NotificationChannel(CHANNEL_ID, "Push", NotificationManager.IMPORTANCE_HIGH)
            )
        }
    }
}
