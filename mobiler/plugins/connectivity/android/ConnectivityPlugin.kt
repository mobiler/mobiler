package {{PACKAGE}}

import {{PACKAGE_SHARED_TYPES}}.PluginResponse

import android.app.Application
import android.content.Context
import android.net.ConnectivityManager
import android.net.NetworkCapabilities

/** Free bundled plugin: current network connectivity. op "status" → "offline" or
 *  "online:<wifi|cellular|other>". Needs ACCESS_NETWORK_STATE (install-time perm). */
class ConnectivityPlugin(private val application: Application) : MobilerPlugin {
    override suspend fun handle(op: String, input: String): PluginResponse {
        if (op != "status") return PluginResponse(false, "unknown op '$op'")
        val cm = application.getSystemService(Context.CONNECTIVITY_SERVICE) as? ConnectivityManager
            ?: return PluginResponse(false, "connectivity service unavailable")
        val caps = cm.activeNetwork?.let { cm.getNetworkCapabilities(it) }
        val out = when {
            caps == null || !caps.hasCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET) -> "offline"
            caps.hasTransport(NetworkCapabilities.TRANSPORT_WIFI) -> "online:wifi"
            caps.hasTransport(NetworkCapabilities.TRANSPORT_CELLULAR) -> "online:cellular"
            else -> "online:other"
        }
        return PluginResponse(true, out)
    }
}
