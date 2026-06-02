package {{PACKAGE}}

import {{PACKAGE_SHARED_TYPES}}.PluginResponse

import android.Manifest
import android.annotation.SuppressLint
import android.app.Application
import android.bluetooth.BluetoothAdapter
import android.bluetooth.BluetoothGatt
import android.bluetooth.BluetoothGattCallback
import android.bluetooth.BluetoothGattCharacteristic
import android.bluetooth.BluetoothManager
import android.bluetooth.BluetoothProfile
import android.bluetooth.le.ScanCallback
import android.bluetooth.le.ScanResult
import android.content.Context
import android.content.pm.PackageManager
import android.os.Build
import androidx.core.app.ActivityCompat
import androidx.core.content.ContextCompat
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.withContext
import org.json.JSONArray
import org.json.JSONObject
import java.util.UUID

/** Free bundled plugin: Bluetooth Low Energy. ops:
 *   - "scan"    : scan ~4s → JSON [{id,name,rssi}] of nearby BLE devices.
 *   - "connect" : input = device id (MAC) → connect GATT + discover services → "connected".
 *   - "read"    : input = {"device":id,"service":uuid,"characteristic":uuid} → value (UTF-8 or hex).
 *  Best-effort runtime perms (like the geolocation/notifications plugins): returns
 *  "permission requested — try again" until granted, then works on the next call. CANNOT be tested
 *  on an emulator (no BLE radio) — needs a real device near a peripheral. */
@SuppressLint("MissingPermission")
class BluetoothPlugin(private val application: Application) : MobilerPlugin {
    // One persistent connection per device — a GATT has a single callback for its lifetime, so the
    // connection holds the in-flight continuations for connect + read.
    private val conns = HashMap<String, Conn>()

    private fun adapter(): BluetoothAdapter? =
        (application.getSystemService(Context.BLUETOOTH_SERVICE) as? BluetoothManager)?.adapter

    private fun ensurePerms(): Boolean {
        val perms = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            arrayOf(Manifest.permission.BLUETOOTH_SCAN, Manifest.permission.BLUETOOTH_CONNECT)
        } else {
            arrayOf(Manifest.permission.ACCESS_FINE_LOCATION)
        }
        val granted = perms.all { ContextCompat.checkSelfPermission(application, it) == PackageManager.PERMISSION_GRANTED }
        if (!granted) {
            MobilerActivity.current?.get()?.let { ActivityCompat.requestPermissions(it, perms, 0) }
        }
        return granted
    }

    override suspend fun handle(op: String, input: String): PluginResponse {
        val adapter = adapter() ?: return PluginResponse(false, "bluetooth unavailable")
        if (!adapter.isEnabled) return PluginResponse(false, "bluetooth off")
        if (!ensurePerms()) return PluginResponse(false, "permission requested — try again")
        return when (op) {
            "scan" -> scan(adapter)
            "connect" -> connect(adapter, input.trim())
            "read" -> read(input)
            else -> PluginResponse(false, "unknown op '$op'")
        }
    }

    private suspend fun scan(adapter: BluetoothAdapter): PluginResponse {
        val scanner = adapter.bluetoothLeScanner ?: return PluginResponse(false, "scanner unavailable")
        val found = LinkedHashMap<String, ScanResult>()
        val cb = object : ScanCallback() {
            override fun onScanResult(callbackType: Int, result: ScanResult) {
                found[result.device.address] = result
            }
        }
        return withContext(Dispatchers.Main) {
            scanner.startScan(cb)
            delay(4000)
            scanner.stopScan(cb)
            val arr = JSONArray()
            found.values.forEach { r ->
                arr.put(
                    JSONObject().apply {
                        put("id", r.device.address)
                        put("name", r.device.name ?: r.scanRecord?.deviceName ?: "")
                        put("rssi", r.rssi)
                    },
                )
            }
            PluginResponse(true, arr.toString())
        }
    }

    private suspend fun connect(adapter: BluetoothAdapter, address: String): PluginResponse {
        val device = try {
            adapter.getRemoteDevice(address)
        } catch (e: Exception) {
            return PluginResponse(false, "bad device id")
        }
        return withContext(Dispatchers.Main) {
            suspendCancellableCoroutine { cont ->
                var resumed = false
                fun done(r: PluginResponse) { if (!resumed) { resumed = true; cont.resumeWith(Result.success(r)) } }
                val conn = Conn { done(it) }
                conns[address] = conn
                conn.gatt = device.connectGatt(application, false, conn.callback)
            }
        }
    }

    private suspend fun read(input: String): PluginResponse {
        val obj = try { JSONObject(input) } catch (e: Exception) { return PluginResponse(false, "bad input") }
        val address = obj.optString("device")
        val conn = conns[address] ?: return PluginResponse(false, "not connected — call connect first")
        val gatt = conn.gatt ?: return PluginResponse(false, "not connected")
        val service = gatt.getService(uuid(obj.optString("service")) ?: return PluginResponse(false, "bad service uuid"))
            ?: return PluginResponse(false, "service not found")
        val chr = service.getCharacteristic(uuid(obj.optString("characteristic")) ?: return PluginResponse(false, "bad characteristic uuid"))
            ?: return PluginResponse(false, "characteristic not found")
        return withContext(Dispatchers.Main) {
            suspendCancellableCoroutine { cont ->
                var resumed = false
                fun done(r: PluginResponse) { if (!resumed) { resumed = true; cont.resumeWith(Result.success(r)) } }
                conn.pendingRead = { done(it) }
                if (!gatt.readCharacteristic(chr)) {
                    conn.pendingRead = null
                    done(PluginResponse(false, "read failed to start"))
                }
            }
        }
    }

    private fun uuid(s: String): UUID? = try { UUID.fromString(s) } catch (e: Exception) { null }

    /** Holds one device's GATT + the in-flight connect/read continuations (a GATT has one callback). */
    private class Conn(private val onConnect: (PluginResponse) -> Unit) {
        var gatt: BluetoothGatt? = null
        var pendingRead: ((PluginResponse) -> Unit)? = null
        private var connectResolved = false

        val callback = object : BluetoothGattCallback() {
            override fun onConnectionStateChange(g: BluetoothGatt, status: Int, newState: Int) {
                gatt = g
                when (newState) {
                    BluetoothProfile.STATE_CONNECTED -> g.discoverServices()
                    BluetoothProfile.STATE_DISCONNECTED ->
                        if (!connectResolved) { connectResolved = true; onConnect(PluginResponse(false, "disconnected")) }
                }
            }

            override fun onServicesDiscovered(g: BluetoothGatt, status: Int) {
                if (!connectResolved) {
                    connectResolved = true
                    onConnect(
                        if (status == BluetoothGatt.GATT_SUCCESS) PluginResponse(true, "connected")
                        else PluginResponse(false, "service discovery failed"),
                    )
                }
            }

            @Deprecated("Pre-API-33 signature; both are overridden so either platform routes here.")
            @Suppress("DEPRECATION")
            override fun onCharacteristicRead(g: BluetoothGatt, c: BluetoothGattCharacteristic, status: Int) {
                deliverRead(status, c.value)
            }

            override fun onCharacteristicRead(g: BluetoothGatt, c: BluetoothGattCharacteristic, value: ByteArray, status: Int) {
                deliverRead(status, value)
            }

            private fun deliverRead(status: Int, value: ByteArray?) {
                val cb = pendingRead ?: return
                pendingRead = null
                cb(decode(status, value))
            }
        }

        private fun decode(status: Int, value: ByteArray?): PluginResponse {
            if (status != BluetoothGatt.GATT_SUCCESS || value == null) return PluginResponse(false, "read failed")
            val text = value.toString(Charsets.UTF_8)
            val printable = text.isNotEmpty() && text.all { it.code in 9..126 || it.code > 160 }
            return PluginResponse(true, if (printable) text else value.joinToString("") { "%02x".format(it) })
        }
    }
}
