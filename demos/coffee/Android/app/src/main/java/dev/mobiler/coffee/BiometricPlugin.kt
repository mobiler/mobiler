package dev.mobiler.coffee

import dev.mobiler.coffee.shared.types.PluginResponse

import androidx.biometric.BiometricManager
import androidx.biometric.BiometricPrompt
import androidx.core.content.ContextCompat
import androidx.fragment.app.FragmentActivity
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlin.coroutines.resume

// Biometric authentication (free, bundled). Presents the system Face/fingerprint sheet via
// androidx.biometric — which requires a FragmentActivity host (the Mobiler shell's MainActivity
// is one). `input` is the prompt title (optional). Returns ok:true on success; ok:false with a
// reason on cancel / failure / no enrolled biometric / no hardware.
//
// Gate a sensitive action in your Rust core by checking the result before proceeding; pair with
// the `securestore` plugin to protect stored secrets.
class BiometricPlugin(private val application: android.app.Application) : MobilerPlugin {
    override suspend fun handle(op: String, input: String): PluginResponse {
        if (op != "authenticate") return PluginResponse(false, "unknown op '$op'")
        val activity = MobilerActivity.current?.get() as? FragmentActivity
            ?: return PluginResponse(false, "no FragmentActivity")

        // Surface a clear reason when auth can't even be attempted.
        val canAuth = BiometricManager.from(application).canAuthenticate(
            BiometricManager.Authenticators.BIOMETRIC_STRONG or BiometricManager.Authenticators.DEVICE_CREDENTIAL
        )
        if (canAuth != BiometricManager.BIOMETRIC_SUCCESS) {
            return PluginResponse(false, "biometric unavailable ($canAuth)")
        }

        val title = input.ifEmpty { "Authenticate" }
        return suspendCancellableCoroutine { cont ->
            val executor = ContextCompat.getMainExecutor(application)
            val prompt = BiometricPrompt(activity, executor, object : BiometricPrompt.AuthenticationCallback() {
                override fun onAuthenticationSucceeded(result: BiometricPrompt.AuthenticationResult) {
                    if (cont.isActive) cont.resume(PluginResponse(true, "ok"))
                }
                override fun onAuthenticationError(code: Int, msg: CharSequence) {
                    if (cont.isActive) cont.resume(PluginResponse(false, msg.toString()))
                }
                // onAuthenticationFailed (a single bad finger) is transient — the prompt stays up,
                // so we don't resume here; resume only on terminal success/error.
            })
            val info = BiometricPrompt.PromptInfo.Builder()
                .setTitle(title)
                .setAllowedAuthenticators(
                    BiometricManager.Authenticators.BIOMETRIC_STRONG or BiometricManager.Authenticators.DEVICE_CREDENTIAL
                )
                .build()
            // BiometricPrompt must be driven on the main thread.
            activity.runOnUiThread { prompt.authenticate(info) }
        }
    }
}
