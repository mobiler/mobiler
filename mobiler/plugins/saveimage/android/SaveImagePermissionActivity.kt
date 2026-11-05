package {{PACKAGE}}

import android.Manifest
import android.app.Activity
import android.content.Intent
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.result.contract.ActivityResultContracts
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeoutOrNull

/** Transparent helper Activity for the saveimage plugin on Android 8–9 (API 26–28): asks for
 *  WRITE_EXTERNAL_STORAGE and reports the answer, so the plugin can wait for it instead of failing the
 *  first tap. Shipped by the plugin, so the app's MainActivity needs no edit. Not exported. */
class SaveImagePermissionActivity : ComponentActivity() {
    /** The request this window answers: taken when it opens, so a late close of one window never answers the next. */
    private var mine: CompletableDeferred<Answer>? = null

    private val ask = registerForActivityResult(ActivityResultContracts.RequestPermission()) { granted ->
        mine?.complete(if (granted) Answer.GRANTED else Answer.DENIED)
        finish()
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        mine = pending
        // After a configuration change the request is already showing; don't ask twice.
        if (savedInstanceState == null) ask.launch(Manifest.permission.WRITE_EXTERNAL_STORAGE)
    }

    override fun onDestroy() {
        // Closed without an answer (e.g. the task was removed): count it as a refusal. A no-op once answered.
        if (isFinishing) mine?.complete(Answer.DENIED)
        super.onDestroy()
    }

    enum class Answer { GRANTED, DENIED, NO_ACTIVITY }

    companion object {
        private val lock = Mutex()
        @Volatile private var pending: CompletableDeferred<Answer>? = null

        /** Ask from `from` and wait for the user's answer, one request at a time. `granted` is re-checked once the
         *  lock is held, so a save that queued behind an approved one doesn't ask again. */
        suspend fun request(from: Activity, granted: () -> Boolean): Answer = lock.withLock {
            if (granted()) return@withLock Answer.GRANTED
            if (from.isFinishing || from.isDestroyed) return@withLock Answer.NO_ACTIVITY
            val d = CompletableDeferred<Answer>()
            pending = d
            try {
                val started = withContext(Dispatchers.Main) {
                    runCatching { from.startActivity(Intent(from, SaveImagePermissionActivity::class.java)) }.isSuccess
                }
                if (!started) return@withLock Answer.NO_ACTIVITY
                // A window that never answers (it can't be shown) must not hold every later save.
                withTimeoutOrNull(5 * 60 * 1000L) { d.await() } ?: Answer.DENIED
            } finally {
                pending = null
            }
        }
    }
}
