package dev.mobiler.barbershop

import dev.mobiler.barbershop.shared.types.PluginResponse

import android.app.Application
import com.google.android.play.core.review.ReviewManagerFactory
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.withContext

/** Free bundled plugin: request the Play in-app review flow (no permission). op "request" →
 *  "requested" (or "unavailable" if not a Play install — the API no-ops gracefully). */
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
                            .addOnCompleteListener { done(PluginResponse(true, "requested")) }
                    } else {
                        done(PluginResponse(true, "unavailable"))
                    }
                }
            }
        }
    }
}
