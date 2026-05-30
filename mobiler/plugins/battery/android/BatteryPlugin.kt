package {{PACKAGE}}

import android.app.Application
import android.content.Context
import android.os.BatteryManager

// A MobilerPlugin (interface declared in Core.kt, same package) that reports the battery
// level as a percentage string ("0".."100"). Installed by `mobiler plugin add battery`.
class BatteryPlugin(private val application: Application) : MobilerPlugin {
    override suspend fun handle(op: String, input: String): PluginResponse {
        if (op != "level") return PluginResponse(false, "unknown op '$op'")
        val bm = application.getSystemService(Context.BATTERY_SERVICE) as? BatteryManager
            ?: return PluginResponse(false, "battery service unavailable")
        val pct = bm.getIntProperty(BatteryManager.BATTERY_PROPERTY_CAPACITY)
        return PluginResponse(true, pct.toString())
    }
}
