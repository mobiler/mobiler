package dev.mobiler.barbershop

import dev.mobiler.barbershop.shared.types.PluginResponse

import android.app.Application
import android.content.Intent
import kotlinx.coroutines.suspendCancellableCoroutine

/** Free bundled plugin: pick a contact via the system picker (no permission).
 *  op "pick" → "name|phone" on success, ok=false on cancel. Launches the transient
 *  ContactsPickerActivity (shipped with this plugin) and awaits its relayed result. */
class ContactsPlugin(private val application: Application) : MobilerPlugin {
    override suspend fun handle(op: String, input: String): PluginResponse {
        if (op != "pick") return PluginResponse(false, "unknown op '$op'")
        val activity = MobilerActivity.current?.get() ?: return PluginResponse(false, "no foreground activity")
        val res = suspendCancellableCoroutine<String?> { cont ->
            ContactsRelay.onResult = { result ->
                if (cont.isActive) cont.resumeWith(Result.success(result))
            }
            activity.startActivity(Intent(activity, ContactsPickerActivity::class.java))
        }
        return if (res != null) PluginResponse(true, res) else PluginResponse(false, "cancelled")
    }
}
