package {{PACKAGE}}

import {{PACKAGE_SHARED_TYPES}}.PluginResponse

import android.Manifest
import android.annotation.SuppressLint
import android.app.Application
import android.content.pm.PackageManager
import androidx.core.app.ActivityCompat
import androidx.core.content.ContextCompat
import com.google.android.gms.location.LocationServices
import com.google.android.gms.location.Priority
import kotlinx.coroutines.suspendCancellableCoroutine

/** Free bundled plugin: current device location via Play Services FusedLocationProvider (higher
 *  accuracy + lower power than the framework LocationManager). op "get" → "lat,lng", ok=false on
 *  denial/error. Opt-in alternative to the default `geolocation` plugin — registers under the same cx
 *  name "geolocation". Needs ACCESS_FINE/COARSE_LOCATION; if not granted yet it fires the system prompt
 *  (best-effort) and returns "permission requested — try again"; the next call returns the location. */
class GeolocationPlugin(private val application: Application) : MobilerPlugin {
    private val fused by lazy { LocationServices.getFusedLocationProviderClient(application) }

    @SuppressLint("MissingPermission")
    override suspend fun handle(op: String, input: String): PluginResponse {
        if (op != "get") return PluginResponse(false, "unknown op '$op'")
        val fine = Manifest.permission.ACCESS_FINE_LOCATION
        val coarse = Manifest.permission.ACCESS_COARSE_LOCATION
        val granted = ContextCompat.checkSelfPermission(application, fine) == PackageManager.PERMISSION_GRANTED ||
            ContextCompat.checkSelfPermission(application, coarse) == PackageManager.PERMISSION_GRANTED
        if (!granted) {
            MobilerActivity.current?.get()?.let { act ->
                ActivityCompat.requestPermissions(act, arrayOf(fine, coarse), 0)
            }
            return PluginResponse(false, "permission requested — try again")
        }
        return suspendCancellableCoroutine { cont ->
            var resumed = false
            fun done(r: PluginResponse) { if (!resumed) { resumed = true; cont.resumeWith(Result.success(r)) } }
            fun coords(lat: Double, lng: Double) = PluginResponse(true, "$lat,$lng")
            fused.getCurrentLocation(Priority.PRIORITY_HIGH_ACCURACY, null)
                .addOnSuccessListener { loc ->
                    if (loc != null) {
                        done(coords(loc.latitude, loc.longitude))
                    } else {
                        // No fresh fix → fall back to the last cached location.
                        fused.lastLocation
                            .addOnSuccessListener { last ->
                                done(if (last != null) coords(last.latitude, last.longitude) else PluginResponse(false, "no location"))
                            }
                            .addOnFailureListener { e -> done(PluginResponse(false, e.message ?: "location unavailable")) }
                    }
                }
                .addOnFailureListener { e -> done(PluginResponse(false, e.message ?: "location unavailable")) }
        }
    }
}
