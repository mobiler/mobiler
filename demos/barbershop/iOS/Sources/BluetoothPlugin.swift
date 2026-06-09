import SharedTypes
import Foundation
import CoreBluetooth

/// Bluetooth Low Energy — matches the Android contract. ops:
///   - "scan"    : scan ~4s → JSON [{id,name,rssi}] of nearby peripherals.
///   - "connect" : input = device id (the CBPeripheral UUID from scan) → discover services → "connected".
///   - "read"    : input = {"device":id,"service":uuid,"characteristic":uuid} → value (UTF-8 or hex).
///   - "write"   : input = {device,service,characteristic,value,"hex"?,"without_response"?} → ok.
///   - notify    : cx.subscribe(key,"bluetooth","notify",{device,service,characteristic},on) → each
///                 characteristic-change value (UTF-8 or hex) until cx.unsubscribe.
/// Needs NSBluetoothAlwaysUsageDescription. Cannot be tested on the simulator (no BLE) — needs a
/// real device near a peripheral.
enum BluetoothPlugin {
    static func handle(op: String, input: String) async -> PluginResponse {
        switch op {
        case "scan": return await BleManager.shared.scan()
        case "connect": return await BleManager.shared.connect(input.trimmingCharacters(in: .whitespaces))
        case "read": return await BleManager.shared.read(input)
        case "write": return await BleManager.shared.write(input)
        default: return PluginResponse(ok: false, output: "unknown op '\(op)'")
        }
    }

    // Streaming notify (cx.subscribe): enable notifications + emit each change; park until the
    // subscription's Task is cancelled (cx.unsubscribe), then disable + detach.
    static func subscribe(op: String, input: String, emit: @escaping @Sendable (PluginResponse) -> Void) async {
        let sink: @Sendable (String) -> Void = { emit(PluginResponse(ok: true, output: $0)) }
        if let err = await BleManager.shared.startNotify(input, sink) {
            emit(PluginResponse(ok: false, output: err))
            return
        }
        await withTaskCancellationHandler {
            while !Task.isCancelled { try? await Task.sleep(nanoseconds: 1_000_000_000) }
        } onCancel: {
            Task { await BleManager.shared.stopNotify() }
        }
    }
}

// Single central manager for the app's lifetime. All CoreBluetooth callbacks run on the main queue;
// `@unchecked Sendable` because the continuations are resumed there and each is nil-guarded.
final class BleManager: NSObject, CBCentralManagerDelegate, CBPeripheralDelegate, @unchecked Sendable {
    static let shared = BleManager()

    private var central: CBCentralManager!
    private var discovered: [String: CBPeripheral] = [:]
    private var rssis: [String: Int] = [:]
    private var connected: [String: CBPeripheral] = [:]
    private var scanCont: CheckedContinuation<PluginResponse, Never>?
    private var connectCont: CheckedContinuation<PluginResponse, Never>?
    private var readCont: CheckedContinuation<PluginResponse, Never>?
    private var writeCont: CheckedContinuation<PluginResponse, Never>?
    private var notifySink: (@Sendable (String) -> Void)?
    private weak var notifyPeripheral: CBPeripheral?
    private var notifyCharacteristic: CBCharacteristic?

    override init() {
        super.init()
        central = CBCentralManager(delegate: self, queue: .main)
    }

    func centralManagerDidUpdateState(_ central: CBCentralManager) {}

    private func poweredOn() -> Bool { central.state == .poweredOn }

    // The first call happens before CoreBluetooth finishes powering on / the user answers the
    // permission prompt — poll briefly so the first tap works instead of needing a second. But
    // only wait on a transient state (.unknown/.resetting); a terminal state (denied/off/unsupported)
    // returns immediately so the user gets an actionable message, not a 6s hang.
    private func ensurePoweredOn() async -> Bool {
        if central.state == .unknown || central.state == .resetting {
            var waited = 0
            while central.state != .poweredOn && waited < 6000 {
                try? await Task.sleep(nanoseconds: 200_000_000)
                waited += 200
            }
        }
        return poweredOn()
    }

    private func stateMessage() -> String {
        switch central.state {
        case .unauthorized: return "denied"
        case .poweredOff: return "bluetooth off"
        case .unsupported: return "bluetooth unsupported"
        default: return "bluetooth unavailable"
        }
    }

    // MARK: scan
    func scan() async -> PluginResponse {
        guard await ensurePoweredOn() else { return PluginResponse(ok: false, output: stateMessage()) }
        discovered.removeAll(); rssis.removeAll()
        return await withCheckedContinuation { cont in
            scanCont = cont
            central.scanForPeripherals(withServices: nil)
            DispatchQueue.main.asyncAfter(deadline: .now() + 4) { [weak self] in self?.finishScan() }
        }
    }

    private func finishScan() {
        central.stopScan()
        guard let cont = scanCont else { return }
        scanCont = nil
        let arr: [[String: Any]] = discovered.map { id, p in
            ["id": id, "name": p.name ?? "", "rssi": rssis[id] ?? 0]
        }
        let data = (try? JSONSerialization.data(withJSONObject: arr)) ?? Data("[]".utf8)
        cont.resume(returning: PluginResponse(ok: true, output: String(data: data, encoding: .utf8) ?? "[]"))
    }

    func centralManager(_ c: CBCentralManager, didDiscover p: CBPeripheral, advertisementData: [String: Any], rssi RSSI: NSNumber) {
        let id = p.identifier.uuidString
        discovered[id] = p
        rssis[id] = RSSI.intValue
    }

    // MARK: connect
    func connect(_ id: String) async -> PluginResponse {
        guard await ensurePoweredOn() else { return PluginResponse(ok: false, output: stateMessage()) }
        guard let p = discovered[id] else { return PluginResponse(ok: false, output: "unknown device — scan first") }
        return await withCheckedContinuation { cont in
            connectCont = cont
            p.delegate = self
            central.connect(p)
            DispatchQueue.main.asyncAfter(deadline: .now() + 8) { [weak self] in
                guard let self = self, let c = self.connectCont else { return }
                self.connectCont = nil
                c.resume(returning: PluginResponse(ok: false, output: "connect timeout"))
            }
        }
    }

    func centralManager(_ c: CBCentralManager, didConnect p: CBPeripheral) {
        connected[p.identifier.uuidString] = p
        p.discoverServices(nil)
    }

    func centralManager(_ c: CBCentralManager, didFailToConnect p: CBPeripheral, error: Error?) {
        guard let cont = connectCont else { return }
        connectCont = nil
        cont.resume(returning: PluginResponse(ok: false, output: error?.localizedDescription ?? "connect failed"))
    }

    func peripheral(_ p: CBPeripheral, didDiscoverServices error: Error?) {
        for s in p.services ?? [] { p.discoverCharacteristics(nil, for: s) }
        guard let cont = connectCont else { return }
        connectCont = nil
        cont.resume(returning: PluginResponse(ok: true, output: "connected"))
    }

    func peripheral(_ p: CBPeripheral, didDiscoverCharacteristicsFor service: CBService, error: Error?) {}

    // MARK: read
    func read(_ input: String) async -> PluginResponse {
        guard let data = input.data(using: .utf8),
              let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let id = obj["device"] as? String,
              let svc = obj["service"] as? String,
              let chr = obj["characteristic"] as? String,
              let p = connected[id]
        else { return PluginResponse(ok: false, output: "not connected — connect first") }
        guard let service = p.services?.first(where: { $0.uuid.uuidString.caseInsensitiveCompare(svc) == .orderedSame }),
              let characteristic = service.characteristics?.first(where: { $0.uuid.uuidString.caseInsensitiveCompare(chr) == .orderedSame })
        else { return PluginResponse(ok: false, output: "characteristic not found") }
        return await withCheckedContinuation { cont in
            readCont = cont
            p.readValue(for: characteristic)
            DispatchQueue.main.asyncAfter(deadline: .now() + 5) { [weak self] in
                guard let self = self, let c = self.readCont else { return }
                self.readCont = nil
                c.resume(returning: PluginResponse(ok: false, output: "read timeout"))
            }
        }
    }

    // Routes both read responses and notifications (CoreBluetooth uses one callback for both): a pending
    // read resumes its continuation; otherwise the value is a notification → the stream sink.
    func peripheral(_ p: CBPeripheral, didUpdateValueFor characteristic: CBCharacteristic, error: Error?) {
        if let cont = readCont {
            readCont = nil
            if let v = characteristic.value {
                cont.resume(returning: PluginResponse(ok: true, output: decodeValue(v)))
            } else {
                cont.resume(returning: PluginResponse(ok: false, output: error?.localizedDescription ?? "no value"))
            }
            return
        }
        if let sink = notifySink, let v = characteristic.value { sink(decodeValue(v)) }
    }

    // MARK: write
    func write(_ input: String) async -> PluginResponse {
        guard await ensurePoweredOn() else { return PluginResponse(ok: false, output: stateMessage()) }
        guard let (p, characteristic) = lookup(input),
              let data = input.data(using: .utf8),
              let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any]
        else { return PluginResponse(ok: false, output: "not connected / characteristic not found") }
        let value = bytesFrom(obj)
        if (obj["without_response"] as? Bool) ?? false {
            p.writeValue(value, for: characteristic, type: .withoutResponse)
            return PluginResponse(ok: true, output: "")
        }
        return await withCheckedContinuation { cont in
            writeCont = cont
            p.writeValue(value, for: characteristic, type: .withResponse)
            DispatchQueue.main.asyncAfter(deadline: .now() + 5) { [weak self] in
                guard let self = self, let c = self.writeCont else { return }
                self.writeCont = nil
                c.resume(returning: PluginResponse(ok: false, output: "write timeout"))
            }
        }
    }

    func peripheral(_ p: CBPeripheral, didWriteValueFor characteristic: CBCharacteristic, error: Error?) {
        guard let cont = writeCont else { return }
        writeCont = nil
        cont.resume(returning: error == nil ? PluginResponse(ok: true, output: "") : PluginResponse(ok: false, output: error!.localizedDescription))
    }

    // MARK: notify
    func startNotify(_ input: String, _ sink: @escaping @Sendable (String) -> Void) async -> String? {
        guard await ensurePoweredOn() else { return stateMessage() }
        guard let (p, characteristic) = lookup(input) else { return "not connected / characteristic not found" }
        notifySink = sink
        notifyPeripheral = p
        notifyCharacteristic = characteristic
        p.setNotifyValue(true, for: characteristic)
        return nil
    }

    func stopNotify() {
        if let p = notifyPeripheral, let c = notifyCharacteristic { p.setNotifyValue(false, for: c) }
        notifySink = nil
        notifyPeripheral = nil
        notifyCharacteristic = nil
    }

    // MARK: helpers
    private func lookup(_ input: String) -> (CBPeripheral, CBCharacteristic)? {
        guard let data = input.data(using: .utf8),
              let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let id = obj["device"] as? String,
              let svc = obj["service"] as? String,
              let chr = obj["characteristic"] as? String,
              let p = connected[id],
              let service = p.services?.first(where: { $0.uuid.uuidString.caseInsensitiveCompare(svc) == .orderedSame }),
              let characteristic = service.characteristics?.first(where: { $0.uuid.uuidString.caseInsensitiveCompare(chr) == .orderedSame })
        else { return nil }
        return (p, characteristic)
    }

    private func bytesFrom(_ obj: [String: Any]) -> Data {
        let v = (obj["value"] as? String) ?? ""
        guard (obj["hex"] as? Bool) ?? false else { return Data(v.utf8) }
        var bytes = [UInt8]()
        var i = v.startIndex
        while i < v.endIndex, let j = v.index(i, offsetBy: 2, limitedBy: v.endIndex) {
            if let b = UInt8(v[i..<j], radix: 16) { bytes.append(b) }
            i = j
        }
        return Data(bytes)
    }

    private func decodeValue(_ v: Data) -> String {
        let text = String(data: v, encoding: .utf8)
        return (text != nil && !(text!.isEmpty)) ? text! : v.map { String(format: "%02x", $0) }.joined()
    }
}
