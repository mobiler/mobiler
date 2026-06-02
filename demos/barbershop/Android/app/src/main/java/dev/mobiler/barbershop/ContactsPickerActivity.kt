package dev.mobiler.barbershop

import android.content.Intent
import android.os.Bundle
import android.provider.ContactsContract
import androidx.activity.ComponentActivity
import androidx.activity.result.contract.ActivityResultContracts

/** Relay between ContactsPlugin (app context only) and this transient Activity. */
object ContactsRelay {
    var onResult: ((String?) -> Unit)? = null
}

/** Transparent helper Activity: runs the system contact picker (ACTION_PICK on a phone row),
 *  reads the chosen contact's name + number (the picker grants access to just that row, so no
 *  READ_CONTACTS), relays "name|phone", and finishes. Shipped by the plugin — no MainActivity edit. */
class ContactsPickerActivity : ComponentActivity() {
    private val pick = registerForActivityResult(ActivityResultContracts.StartActivityForResult()) { result ->
        var out: String? = null
        val uri = result.data?.data
        if (result.resultCode == RESULT_OK && uri != null) {
            contentResolver.query(
                uri,
                arrayOf(
                    ContactsContract.CommonDataKinds.Phone.DISPLAY_NAME,
                    ContactsContract.CommonDataKinds.Phone.NUMBER,
                ),
                null, null, null,
            )?.use { c ->
                if (c.moveToFirst()) {
                    out = "${c.getString(0) ?: ""}|${c.getString(1) ?: ""}"
                }
            }
        }
        ContactsRelay.onResult?.invoke(out)
        ContactsRelay.onResult = null
        finish()
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        pick.launch(Intent(Intent.ACTION_PICK, ContactsContract.CommonDataKinds.Phone.CONTENT_URI))
    }
}
