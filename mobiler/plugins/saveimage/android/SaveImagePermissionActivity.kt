package {{PACKAGE}}

import android.Manifest
import android.app.Activity
import android.content.Intent
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.result.contract.ActivityResultContracts
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock

/** Transparent helper Activity for the saveimage plugin on Android 8–9 (API 26–28): asks for
 *  WRITE_EXTERNAL_STORAGE and reports the answer, so the plugin can wait for it instead of failing the
 *  first tap. Shipped by the plugin, so the app's MainActivity needs no edit. */
class SaveImagePermissionActivity : ComponentActivity() {
    private val ask = registerForActivityResult(ActivityResultContracts.RequestPermission()) { granted ->
        answer(granted)
        finish()
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        // After a configuration change the request is already showing; don't ask twice.
        if (savedInstanceState == null) ask.launch(Manifest.permission.WRITE_EXTERNAL_STORAGE)
    }

    override fun onDestroy() {
        // Closed without an answer (e.g. the task was removed): count it as a refusal.
        if (isFinishing) answer(false)
        super.onDestroy()
    }

    companion object {
        private val lock = Mutex()
        @Volatile private var pending: CompletableDeferred<Boolean>? = null

        private fun answer(granted: Boolean) {
            pending?.complete(granted)
        }

        /** Ask from `from` and wait for the user's answer (one request at a time). */
        suspend fun request(from: Activity): Boolean = lock.withLock {
            val d = CompletableDeferred<Boolean>()
            pending = d
            from.runOnUiThread { from.startActivity(Intent(from, SaveImagePermissionActivity::class.java)) }
            try { d.await() } finally { pending = null }
        }
    }
}
