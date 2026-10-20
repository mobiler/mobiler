package rs.mobiler.saldo

import rs.mobiler.saldo.shared.types.PluginResponse

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.withContext
import org.json.JSONObject

/**
 * Native single-choice picker — request/response. Input {"title", "options":[…], "selected":Int}.
 * Shows an AlertDialog single-choice list (the iOS wheel sheet's Android counterpart). Resolves
 * ok=true with the chosen index as a string, or ok=false on cancel.
 */
class PickerPlugin : MobilerPlugin {
    override suspend fun handle(op: String, input: String): PluginResponse {
        if (op != "choose") return PluginResponse(false, "unknown op '$op'")
        val obj = runCatching { JSONObject(input) }.getOrNull() ?: return PluginResponse(false, "bad input")
        val arr = obj.optJSONArray("options") ?: return PluginResponse(false, "no options")
        val options = Array(arr.length()) { arr.optString(it) }
        if (options.isEmpty()) return PluginResponse(false, "no options")
        val title = obj.optString("title")
        val selected = obj.optInt("selected", -1)
        val activity = MobilerActivity.current?.get() ?: return PluginResponse(false, "no activity")
        // A finishing or destroyed activity can't host a dialog (show() would throw).
        if (activity.isFinishing || activity.isDestroyed) return PluginResponse(false, "no activity")
        return withContext(Dispatchers.Main) {
            suspendCancellableCoroutine { cont ->
                var resumed = false
                fun done(r: PluginResponse) {
                    if (!resumed) { resumed = true; cont.resumeWith(Result.success(r)) }
                }
                val dlg = android.app.AlertDialog.Builder(activity)
                    .setTitle(title)
                    .setSingleChoiceItems(options, selected) { d, which ->
                        done(PluginResponse(true, which.toString())); d.dismiss()
                    }
                    .setOnCancelListener { done(PluginResponse(false, "cancel")) }
                    .create()
                // Any other dismissal still answers; after a pick or a cancel this is a no-op. (An activity
                // recreated around an open dialog leaks it without a dismissal; the shell's configChanges
                // keep that rare.)
                dlg.setOnDismissListener { done(PluginResponse(false, "cancel")) }
                cont.invokeOnCancellation { dlg.dismiss() }
                if (runCatching { dlg.show() }.isFailure) done(PluginResponse(false, "could not show"))
            }
        }
    }
}
