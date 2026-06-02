package {{PACKAGE}}

import {{PACKAGE_SHARED_TYPES}}.PluginResponse

import android.app.Application
import android.content.Intent
import android.provider.CalendarContract
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import org.json.JSONObject

/** Free bundled plugin: add a calendar event (no permission). op "add", input JSON
 *  {title, start, end} (start/end = epoch millis). Opens the calendar app's event editor
 *  pre-filled via ACTION_INSERT; the user saves there (the intent doesn't report the outcome,
 *  so this returns "opened"). */
class CalendarPlugin(private val application: Application) : MobilerPlugin {
    override suspend fun handle(op: String, input: String): PluginResponse {
        if (op != "add") return PluginResponse(false, "unknown op '$op'")
        val activity = MobilerActivity.current?.get() ?: return PluginResponse(false, "no foreground activity")
        val obj = JSONObject(input)
        val intent = Intent(Intent.ACTION_INSERT)
            .setData(CalendarContract.Events.CONTENT_URI)
            .putExtra(CalendarContract.Events.TITLE, obj.optString("title"))
            .putExtra(CalendarContract.Events.DESCRIPTION, obj.optString("notes"))
        if (obj.has("start")) intent.putExtra(CalendarContract.EXTRA_EVENT_BEGIN_TIME, obj.optLong("start"))
        if (obj.has("end")) intent.putExtra(CalendarContract.EXTRA_EVENT_END_TIME, obj.optLong("end"))
        return withContext(Dispatchers.Main) {
            try {
                activity.startActivity(intent)
                PluginResponse(true, "opened")
            } catch (e: Exception) {
                PluginResponse(false, e.message ?: "no calendar app")
            }
        }
    }
}
