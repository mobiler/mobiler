package {{PACKAGE}}

import {{PACKAGE_SHARED_TYPES}}.PluginResponse

import android.app.Application
import android.content.Context
import android.hardware.Sensor
import android.hardware.SensorEvent
import android.hardware.SensorEventListener
import android.hardware.SensorManager
import kotlinx.coroutines.suspendCancellableCoroutine

/** Free bundled plugin: one-shot motion sensor read (no permission). op "read", input
 *  "accelerometer" (default) or "gyroscope" → "x,y,z". Registers a listener, takes the
 *  first sample, unregisters. */
class SensorsPlugin(private val application: Application) : MobilerPlugin {
    override suspend fun handle(op: String, input: String): PluginResponse {
        if (op != "read") return PluginResponse(false, "unknown op '$op'")
        val type = when (input.ifEmpty { "accelerometer" }) {
            "accelerometer" -> Sensor.TYPE_ACCELEROMETER
            "gyroscope" -> Sensor.TYPE_GYROSCOPE
            else -> return PluginResponse(false, "unknown sensor '$input'")
        }
        val sm = application.getSystemService(Context.SENSOR_SERVICE) as? SensorManager
            ?: return PluginResponse(false, "sensor service unavailable")
        val sensor = sm.getDefaultSensor(type) ?: return PluginResponse(false, "sensor unavailable")
        return suspendCancellableCoroutine { cont ->
            var done = false
            val listener = object : SensorEventListener {
                override fun onSensorChanged(e: SensorEvent) {
                    if (done) return
                    done = true
                    val v = e.values
                    sm.unregisterListener(this)
                    cont.resumeWith(Result.success(PluginResponse(true, "${v[0]},${v[1]},${v[2]}")))
                }
                override fun onAccuracyChanged(s: Sensor?, accuracy: Int) {}
            }
            sm.registerListener(listener, sensor, SensorManager.SENSOR_DELAY_NORMAL)
            cont.invokeOnCancellation { sm.unregisterListener(listener) }
        }
    }
}
