package dev.mobiler.coffee

import dev.mobiler.coffee.shared.types.PluginResponse

import com.google.mlkit.vision.barcode.common.Barcode
import com.google.mlkit.vision.codescanner.GmsBarcodeScannerOptions
import com.google.mlkit.vision.codescanner.GmsBarcodeScanning
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlin.coroutines.resume

// Barcode/QR scanner (free, bundled). Uses ML Kit's GmsBarcodeScanner — a complete
// prebuilt scanner UI from Google Play Services that handles the camera preview AND the
// camera permission itself, so the plugin needs no CameraX/Activity-launcher plumbing and
// no CAMERA permission in the manifest. Single-shot: opens the scanner, returns the first
// code as "<format>:<value>" (e.g. "qr:https://…", "ean13:978…"), then closes.
//
// Needs the foreground Activity (the scanner is launched from an Activity context); the
// generic shell already tracks it via the MobilerActivity WeakReference.
class ScannerPlugin(private val application: android.app.Application) : MobilerPlugin {
    override suspend fun handle(op: String, input: String): PluginResponse {
        if (op != "scan") return PluginResponse(false, "unknown op '$op'")
        val activity = MobilerActivity.current?.get()
            ?: return PluginResponse(false, "no foreground activity")

        val options = GmsBarcodeScannerOptions.Builder()
            .setBarcodeFormats(Barcode.FORMAT_ALL_FORMATS)
            .enableAutoZoom()
            .build()
        val scanner = GmsBarcodeScanning.getClient(activity, options)

        return suspendCancellableCoroutine { cont ->
            scanner.startScan()
                .addOnSuccessListener { barcode ->
                    val value = barcode.rawValue ?: barcode.displayValue
                    if (value != null) {
                        cont.resume(PluginResponse(true, "${formatName(barcode.format)}:$value"))
                    } else {
                        cont.resume(PluginResponse(false, "empty barcode"))
                    }
                }
                .addOnCanceledListener { cont.resume(PluginResponse(false, "cancelled")) }
                .addOnFailureListener { e -> cont.resume(PluginResponse(false, e.message ?: "scan failed")) }
        }
    }

    // A short, app-friendly symbology tag so the Rust core can branch on format.
    private fun formatName(format: Int): String = when (format) {
        Barcode.FORMAT_QR_CODE -> "qr"
        Barcode.FORMAT_EAN_13 -> "ean13"
        Barcode.FORMAT_EAN_8 -> "ean8"
        Barcode.FORMAT_UPC_A -> "upca"
        Barcode.FORMAT_UPC_E -> "upce"
        Barcode.FORMAT_CODE_128 -> "code128"
        Barcode.FORMAT_CODE_39 -> "code39"
        Barcode.FORMAT_CODE_93 -> "code93"
        Barcode.FORMAT_CODABAR -> "codabar"
        Barcode.FORMAT_ITF -> "itf"
        Barcode.FORMAT_DATA_MATRIX -> "datamatrix"
        Barcode.FORMAT_PDF417 -> "pdf417"
        Barcode.FORMAT_AZTEC -> "aztec"
        else -> "other"
    }
}
