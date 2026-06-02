import SharedTypes
import Network

/// Free bundled plugin: current network connectivity. op "status" → "offline" or
/// "online:<wifi|cellular|other>". One-shot NWPathMonitor read; no permission.
enum ConnectivityPlugin {
    static func handle(op: String, input: String) async -> PluginResponse {
        guard op == "status" else { return PluginResponse(ok: false, output: "unknown op '\(op)'") }
        return await withCheckedContinuation { cont in
            let monitor = NWPathMonitor()
            let queue = DispatchQueue(label: "mobiler.connectivity")
            // pathUpdateHandler fires on `queue` (serial), so this guard is single-threaded.
            var finished = false
            monitor.pathUpdateHandler = { path in
                if finished { return }
                finished = true
                let out: String
                if path.status != .satisfied { out = "offline" }
                else if path.usesInterfaceType(.wifi) { out = "online:wifi" }
                else if path.usesInterfaceType(.cellular) { out = "online:cellular" }
                else { out = "online:other" }
                monitor.cancel()
                cont.resume(returning: PluginResponse(ok: true, output: out))
            }
            monitor.start(queue: queue)
        }
    }
}
