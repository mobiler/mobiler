package {{PACKAGE}}

import {{PACKAGE_SHARED_TYPES}}.PluginResponse

import android.Manifest
import android.annotation.SuppressLint
import android.app.Application
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import androidx.core.app.ActivityCompat
import androidx.core.content.ContextCompat
import com.google.android.gms.location.Geofence
import com.google.android.gms.location.GeofencingClient
import com.google.android.gms.location.GeofencingRequest
import com.google.android.gms.location.LocationRequest
import com.google.android.gms.location.LocationServices
import com.google.android.gms.location.Priority
import kotlinx.coroutines.channels.awaitClose
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.callbackFlow
import org.json.JSONArray
import org.json.JSONObject

// geofence (free, bundled, EXPERIMENTAL). Background location monitoring via Play Services —
// geofence enter/exit + significant-location-change. A transition is delivered to the statically-
// declared GeofenceBroadcastReceiver (runs even when the process is dead), which posts a local
// notification AND feeds GeofenceBus (persisted buffer) → the cx.subscribe stream on next foreground.
//   cx.plugin("geofence","add",{"id":"shop","lat":..,"lng":..,"radius":150,"notify_title":..,"notify_body":..})
//   cx.subscribe(key,"geofence","events","",on) → {"type":"geofence","id":"shop","event":"enter"|"exit"}
//                                               / {"type":"location","lat":..,"lng":..}
//
// Needs Google Play Services + ACCESS_FINE_LOCATION (+ ACCESS_BACKGROUND_LOCATION for triggers while
// closed, a separate settings grant on Android 11+) + POST_NOTIFICATIONS (API 33+).
class GeofencePlugin(private val application: Application) : MobilerPlugin {
    private val geofencing: GeofencingClient by lazy { LocationServices.getGeofencingClient(application) }
    private val fused by lazy { LocationServices.getFusedLocationProviderClient(application) }

    override suspend fun handle(op: String, input: String): PluginResponse = when (op) {
        "requestPermission" -> requestPermission()
        "add" -> add(input)
        "remove" -> remove(input)
        "list" -> list()
        "startSignificantChanges" -> startSignificantChanges()
        "stopSignificantChanges" -> stopSignificantChanges()
        else -> PluginResponse(false, "unknown op '$op'")
    }

    // Streaming entrypoint (cx.subscribe): emit each event until the collecting Job is cancelled.
    override fun subscribe(op: String, input: String): Flow<PluginResponse> = callbackFlow {
        val sink: (String) -> Unit = { trySend(PluginResponse(true, it)) }
        GeofenceBus.attach(application, sink)
        awaitClose { GeofenceBus.detach(sink) }
    }

    private fun requestPermission(): PluginResponse {
        val fine = Manifest.permission.ACCESS_FINE_LOCATION
        if (ContextCompat.checkSelfPermission(application, fine) != PackageManager.PERMISSION_GRANTED) {
            MobilerActivity.current?.get()?.let { act ->
                val perms = mutableListOf(fine, Manifest.permission.ACCESS_COARSE_LOCATION)
                if (Build.VERSION.SDK_INT >= 33) perms.add(Manifest.permission.POST_NOTIFICATIONS)
                ActivityCompat.requestPermissions(act, perms.toTypedArray(), 0)
            }
            return PluginResponse(false, "permission requested — try again")
        }
        // Background location is a separate "Allow all the time" grant on Android 11+ (settings screen).
        if (Build.VERSION.SDK_INT >= 29 &&
            ContextCompat.checkSelfPermission(application, Manifest.permission.ACCESS_BACKGROUND_LOCATION) !=
            PackageManager.PERMISSION_GRANTED
        ) {
            MobilerActivity.current?.get()?.let { act ->
                ActivityCompat.requestPermissions(act, arrayOf(Manifest.permission.ACCESS_BACKGROUND_LOCATION), 0)
            }
            return PluginResponse(false, "enable \"Allow all the time\" location in settings")
        }
        return PluginResponse(true, "granted")
    }

    @SuppressLint("MissingPermission")
    private fun add(input: String): PluginResponse {
        val obj = runCatching { JSONObject(input) }.getOrNull()
            ?: return PluginResponse(false, "invalid input JSON")
        val id = obj.optString("id").ifEmpty { return PluginResponse(false, "expected {id,lat,lng,radius?}") }
        if (!obj.has("lat") || !obj.has("lng")) return PluginResponse(false, "expected {id,lat,lng,radius?}")
        val lat = obj.optDouble("lat")
        val lng = obj.optDouble("lng")
        val radius = obj.optDouble("radius", 100.0).toFloat()
        if (!hasFine()) return PluginResponse(false, "location permission not granted")

        GeofenceBus.putNotif(application, id, obj.optString("notify_title", "Nearby"), obj.optString("notify_body"))

        val geofence = Geofence.Builder()
            .setRequestId(id)
            .setCircularRegion(lat, lng, radius)
            .setExpirationDuration(Geofence.NEVER_EXPIRE)
            .setTransitionTypes(Geofence.GEOFENCE_TRANSITION_ENTER or Geofence.GEOFENCE_TRANSITION_EXIT)
            .build()
        val request = GeofencingRequest.Builder()
            .setInitialTrigger(GeofencingRequest.INITIAL_TRIGGER_ENTER)
            .addGeofence(geofence)
            .build()
        geofencing.addGeofences(request, transitionPendingIntent())
        return PluginResponse(true, "")
    }

    private fun remove(input: String): PluginResponse {
        val id = runCatching { JSONObject(input).optString("id") }.getOrNull()
            ?: return PluginResponse(false, "expected {id}")
        if (id.isEmpty()) return PluginResponse(false, "expected {id}")
        geofencing.removeGeofences(listOf(id))
        GeofenceBus.removeNotif(application, id)
        return PluginResponse(true, "")
    }

    private fun list(): PluginResponse {
        // Play Services doesn't expose the registered set; track our own. Return the ids we stored
        // notification text for (added via `add`, removed via `remove`) as a best-effort view.
        val ids = GeofenceBus.notifIds(application).joinToString(",") { "\"$it\"" }
        return PluginResponse(true, "[$ids]")
    }

    @SuppressLint("MissingPermission")
    private fun startSignificantChanges(): PluginResponse {
        if (!hasFine()) return PluginResponse(false, "location permission not granted")
        // Coarse, low-power periodic updates delivered to the same receiver (≈ significant change).
        val req = LocationRequest.Builder(Priority.PRIORITY_BALANCED_POWER_ACCURACY, 15 * 60 * 1000L)
            .setMinUpdateDistanceMeters(500f)
            .build()
        fused.requestLocationUpdates(req, locationPendingIntent())
        return PluginResponse(true, "")
    }

    private fun stopSignificantChanges(): PluginResponse {
        fused.removeLocationUpdates(locationPendingIntent())
        return PluginResponse(true, "")
    }

    private fun hasFine() =
        ContextCompat.checkSelfPermission(application, Manifest.permission.ACCESS_FINE_LOCATION) ==
            PackageManager.PERMISSION_GRANTED

    private fun transitionPendingIntent(): PendingIntent {
        val intent = Intent(application, GeofenceBroadcastReceiver::class.java)
            .setAction(GeofenceBroadcastReceiver.ACTION_GEOFENCE)
        return PendingIntent.getBroadcast(
            application, 0, intent,
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_MUTABLE
        )
    }

    private fun locationPendingIntent(): PendingIntent {
        val intent = Intent(application, GeofenceBroadcastReceiver::class.java)
            .setAction(GeofenceBroadcastReceiver.ACTION_LOCATION)
        return PendingIntent.getBroadcast(
            application, 1, intent,
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_MUTABLE
        )
    }
}

// In-process bus between GeofenceBroadcastReceiver (which may run before any UI exists) and the
// cx.subscribe stream. Persists pending events in SharedPreferences so an event that fired in a
// since-killed process is still delivered on the next launch (flushed on attach). Mirrors PushBus.
object GeofenceBus {
    private const val PREFS = "mobiler.geofence"
    private const val BUFFER = "buffer"          // JSON array string of pending event JSON
    private const val NOTIF = "notif."           // per-id {title,body} for the trigger notification
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

    fun putNotif(ctx: Context, id: String, title: String, body: String) {
        val json = JSONObject().put("title", title).put("body", body).toString()
        prefs(ctx).edit().putString(NOTIF + id, json).apply()
    }

    fun removeNotif(ctx: Context, id: String) {
        prefs(ctx).edit().remove(NOTIF + id).apply()
    }

    /** "title" to "body" for a fired region, or null if none was stored. */
    fun notif(ctx: Context, id: String): Pair<String, String>? {
        val raw = prefs(ctx).getString(NOTIF + id, null) ?: return null
        val obj = runCatching { JSONObject(raw) }.getOrNull() ?: return null
        return obj.optString("title", "Nearby") to obj.optString("body")
    }

    fun notifIds(ctx: Context): List<String> =
        prefs(ctx).all.keys.filter { it.startsWith(NOTIF) }.map { it.removePrefix(NOTIF) }
}
