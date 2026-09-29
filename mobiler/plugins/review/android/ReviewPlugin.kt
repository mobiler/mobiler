package {{PACKAGE}}

import {{PACKAGE_SHARED_TYPES}}.PluginResponse

import android.app.Application
import com.google.android.play.core.review.ReviewManagerFactory
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.withContext

/** Free bundled plugin: request the Play in-app review flow (no permission). op "request" →
 *  ok "requested" once the flow ran (Play never says whether the card showed), or ok=false
 *  "unavailable" when Play can't start it (e.g. no or outdated Play Store) — ADR-0013. */
class ReviewPlugin(private val application: Application) : MobilerPlugin {
    override suspend fun handle(op: String, input: String): PluginResponse {
        if (op != "request") return PluginResponse(false, "unknown op '$op'")
        val activity = MobilerActivity.current?.get() ?: return PluginResponse(false, "no foreground activity")
        return withContext(Dispatchers.Main) {
            suspendCancellableCoroutine { cont ->
                var resumed = false
                fun done(r: PluginResponse) { if (!resumed) { resumed = true; cont.resumeWith(Result.success(r)) } }
                val manager = ReviewManagerFactory.create(activity)
                manager.requestReviewFlow().addOnCompleteListener { task ->
                    if (task.isSuccessful) {
                        manager.launchReviewFlow(activity, task.result)
                            .addOnCompleteListener { launch ->
                                done(if (launch.isSuccessful) PluginResponse(true, "requested")
                                     else PluginResponse(false, "unavailable"))
                            }
                    } else {
                        done(PluginResponse(false, "unavailable"))
                    }
                }
            }
        }
    }
}
