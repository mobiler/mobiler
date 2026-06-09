package {{PACKAGE}}

import android.app.NotificationChannel
import android.app.NotificationManager
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.os.Build
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import com.google.android.gms.location.Geofence
import com.google.android.gms.location.GeofencingEvent
import com.google.android.gms.location.LocationResult

// Receives geofence transitions + fused-location updates even when the app process is dead (started
// by the OS to deliver the PendingIntent). For a region cross: posts a local notification + emits a
// {"type":"geofence",...} event through GeofenceBus. For a location update: emits {"type":"location",...}.
// Both buffer in GeofenceBus until the core subscribes. Statically declared in the manifest by the plugin.
private const val CHANNEL_ID = "mobiler_geofence"

class GeofenceBroadcastReceiver : BroadcastReceiver() {
    companion object {
        const val ACTION_GEOFENCE = "{{PACKAGE}}.GEOFENCE"
        const val ACTION_LOCATION = "{{PACKAGE}}.GEOFENCE_LOCATION"
    }

    override fun onReceive(context: Context, intent: Intent) {
        when (intent.action) {
            ACTION_GEOFENCE -> handleGeofence(context, intent)
            ACTION_LOCATION -> handleLocation(context, intent)
        }
    }

    private fun handleGeofence(context: Context, intent: Intent) {
        val event = GeofencingEvent.fromIntent(intent) ?: return
        if (event.hasError()) return
        val transition = event.geofenceTransition
        val name = when (transition) {
            Geofence.GEOFENCE_TRANSITION_ENTER -> "enter"
            Geofence.GEOFENCE_TRANSITION_EXIT -> "exit"
            else -> return
        }
        for (geofence in event.triggeringGeofences ?: emptyList()) {
            val id = geofence.requestId
            GeofenceBus.emit(context, """{"type":"geofence","id":${jsonStr(id)},"event":"$name"}""")
            GeofenceBus.notif(context, id)?.let { (title, body) -> postNotification(context, id, title, body) }
        }
    }

    private fun handleLocation(context: Context, intent: Intent) {
        val result = LocationResult.extractResult(intent) ?: return
        val loc = result.lastLocation ?: return
        GeofenceBus.emit(context, """{"type":"location","lat":${loc.latitude},"lng":${loc.longitude}}""")
    }

    private fun postNotification(context: Context, id: String, title: String, body: String) {
        ensureChannel(context)
        val notif = NotificationCompat.Builder(context, CHANNEL_ID)
            .setSmallIcon(android.R.drawable.ic_dialog_map)
            .setContentTitle(title)
            .setContentText(body)
            .setAutoCancel(true)
            .setPriority(NotificationCompat.PRIORITY_HIGH)
            .build()
        runCatching { NotificationManagerCompat.from(context).notify(id.hashCode(), notif) } // no-op if not permitted
    }

    // JSON-encode a string value (quotes + escapes) via org.json.
    private fun jsonStr(s: String): String = org.json.JSONObject.quote(s)
}

private fun ensureChannel(context: Context) {
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
        val mgr = context.getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
        if (mgr.getNotificationChannel(CHANNEL_ID) == null) {
            mgr.createNotificationChannel(
                NotificationChannel(CHANNEL_ID, "Places", NotificationManager.IMPORTANCE_HIGH)
            )
        }
    }
}
