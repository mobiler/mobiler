package {{PACKAGE}}

import {{PACKAGE_SHARED_TYPES}}.PluginResponse

import android.Manifest
import android.annotation.SuppressLint
import android.app.Application
import android.content.Context
import android.content.pm.PackageManager
import android.location.LocationManager
import android.os.Build
import androidx.core.app.ActivityCompat
import androidx.core.content.ContextCompat
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.withContext

/** Free bundled plugin: current device location. op "get" → "lat,lng" (e.g. "37.77,-122.41"),
 *  ok=false on denial/error. Uses the framework LocationManager (no Play Services dep).
 *  Needs ACCESS_FINE/COARSE_LOCATION. If the permission isn't granted yet it fires the system
 *  prompt (best-effort, like the notifications plugin) and returns "permission requested — try
 *  again"; the next call (after the user grants) returns the location. */
class GeolocationPlugin(private val application: Application) : MobilerPlugin {
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
        val lm = application.getSystemService(Context.LOCATION_SERVICE) as? LocationManager
            ?: return PluginResponse(false, "location service unavailable")
        val provider = when {
            lm.isProviderEnabled(LocationManager.GPS_PROVIDER) -> LocationManager.GPS_PROVIDER
            lm.isProviderEnabled(LocationManager.NETWORK_PROVIDER) -> LocationManager.NETWORK_PROVIDER
            else -> return PluginResponse(false, "location disabled")
        }
        return withContext(Dispatchers.Main) {
            suspendCancellableCoroutine { cont ->
                var resumed = false
                fun done(r: PluginResponse) { if (!resumed) { resumed = true; cont.resumeWith(Result.success(r)) } }
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
                    lm.getCurrentLocation(provider, null, application.mainExecutor) { loc ->
                        done(
                            if (loc != null) PluginResponse(true, "${loc.latitude},${loc.longitude}")
                            else PluginResponse(false, "no location"),
                        )
                    }
                } else {
                    val last = lm.getLastKnownLocation(provider)
                    done(
                        if (last != null) PluginResponse(true, "${last.latitude},${last.longitude}")
                        else PluginResponse(false, "no location"),
                    )
                }
            }
        }
    }
}
