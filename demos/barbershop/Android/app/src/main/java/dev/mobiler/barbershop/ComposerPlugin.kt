package dev.mobiler.barbershop

import dev.mobiler.barbershop.shared.types.PluginResponse

import android.app.Application
import android.content.Intent
import android.net.Uri
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import org.json.JSONObject

/** Free bundled plugin: open the system composer for email / SMS / a phone call (no permission).
 *  - op "email", input {to?, subject?, body?} → ACTION_SENDTO mailto:
 *  - op "sms",   input {to?, body?}           → ACTION_SENDTO smsto:
 *  - op "call",  input {number}               → ACTION_DIAL tel: (dialer, not auto-call → no perm)
 *  Returns ok=true once the chooser/app opens (the user finishes there). */
class ComposerPlugin(private val application: Application) : MobilerPlugin {
    override suspend fun handle(op: String, input: String): PluginResponse {
        val activity = MobilerActivity.current?.get() ?: return PluginResponse(false, "no foreground activity")
        val obj = if (input.isEmpty()) JSONObject() else JSONObject(input)
        val intent = when (op) {
            "email" -> Intent(Intent.ACTION_SENDTO, Uri.parse("mailto:")).apply {
                obj.optString("to").takeIf { it.isNotEmpty() }?.let { putExtra(Intent.EXTRA_EMAIL, arrayOf(it)) }
                putExtra(Intent.EXTRA_SUBJECT, obj.optString("subject"))
                putExtra(Intent.EXTRA_TEXT, obj.optString("body"))
            }
            "sms" -> Intent(Intent.ACTION_SENDTO, Uri.parse("smsto:" + obj.optString("to"))).apply {
                putExtra("sms_body", obj.optString("body"))
            }
            "call" -> Intent(Intent.ACTION_DIAL, Uri.parse("tel:" + obj.optString("number")))
            else -> return PluginResponse(false, "unknown op '$op'")
        }
        return withContext(Dispatchers.Main) {
            try {
                activity.startActivity(intent)
                PluginResponse(true, "opened")
            } catch (e: Exception) {
                PluginResponse(false, e.message ?: "no app to handle '$op'")
            }
        }
    }
}
