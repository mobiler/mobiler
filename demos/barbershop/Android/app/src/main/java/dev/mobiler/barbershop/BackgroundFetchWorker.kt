package dev.mobiler.barbershop

import android.app.NotificationChannel
import android.app.NotificationManager
import android.content.Context
import android.os.Build
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.work.Worker
import androidx.work.WorkerParameters
import org.json.JSONObject

// Runs on WorkManager's schedule — even with no UI alive (WorkManager starts the process). Posts a
// local notification (if configured) + emits a {"type":"fetch","id":..} event through
// BackgroundFetchBus, which buffers it until the core subscribes.
private const val CHANNEL_ID = "mobiler_bgfetch"

class BackgroundFetchWorker(
    private val context: Context,
    params: WorkerParameters,
) : Worker(context, params) {
    override fun doWork(): Result {
        val id = inputData.getString("id") ?: "refresh"
        val title = inputData.getString("title").orEmpty()
        val body = inputData.getString("body").orEmpty()

        if (title.isNotEmpty()) {
            ensureChannel(context)
            val notif = NotificationCompat.Builder(context, CHANNEL_ID)
                .setSmallIcon(android.R.drawable.ic_popup_sync)
                .setContentTitle(title)
                .setContentText(body)
                .setAutoCancel(true)
                .setPriority(NotificationCompat.PRIORITY_DEFAULT)
                .build()
            runCatching { NotificationManagerCompat.from(context).notify(id.hashCode(), notif) }
        }

        BackgroundFetchBus.emit(context, """{"type":"fetch","id":${JSONObject.quote(id)}}""")
        return Result.success()
    }
}

private fun ensureChannel(context: Context) {
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
        val mgr = context.getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
        if (mgr.getNotificationChannel(CHANNEL_ID) == null) {
            mgr.createNotificationChannel(
                NotificationChannel(CHANNEL_ID, "Background updates", NotificationManager.IMPORTANCE_DEFAULT)
            )
        }
    }
}
