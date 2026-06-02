import SharedTypes
import Foundation
import CoreBluetooth

/// Bluetooth Low Energy — matches the Android contract. ops:
///   - "scan"    : scan ~4s → JSON [{id,name,rssi}] of nearby peripherals.
///   - "connect" : input = device id (the CBPeripheral UUID from scan) → discover services → "connected".
///   - "read"    : input = {"device":id,"service":uuid,"characteristic":uuid} → value (UTF-8 or hex).
/// Needs NSBluetoothAlwaysUsageDescription. Cannot be tested on the simulator (no BLE) — needs a
/// real device near a peripheral.
enum BluetoothPlugin {
    static func handle(op: String, input: String) async -> PluginResponse {
        switch op {
        case "scan": return await BleManager.shared.scan()
        case "connect": return await BleManager.shared.connect(input.trimmingCharacters(in: .whitespaces))
        case "read": return await BleManager.shared.read(input)
        default: return PluginResponse(ok: false, output: "unknown op '\(op)'")
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

    override init() {
        super.init()
        central = CBCentralManager(delegate: self, queue: .main)
    }

    func centralManagerDidUpdateState(_ central: CBCentralManager) {}

    private func poweredOn() -> Bool { central.state == .poweredOn }

    // MARK: scan
    func scan() async -> PluginResponse {
        guard poweredOn() else { return PluginResponse(ok: false, output: "bluetooth off") }
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
        guard poweredOn() else { return PluginResponse(ok: false, output: "bluetooth off") }
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

    func peripheral(_ p: CBPeripheral, didUpdateValueFor characteristic: CBCharacteristic, error: Error?) {
        guard let cont = readCont else { return }
        readCont = nil
        if let v = characteristic.value {
            let text = String(data: v, encoding: .utf8)
            let out = (text != nil && !(text!.isEmpty)) ? text! : v.map { String(format: "%02x", $0) }.joined()
            cont.resume(returning: PluginResponse(ok: true, output: out))
        } else {
            cont.resume(returning: PluginResponse(ok: false, output: error?.localizedDescription ?? "no value"))
        }
    }
}
