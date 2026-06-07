import Foundation
import SharedTypes
import StoreKit

// In-app purchase (free, bundled). StoreKit 2 via the system framework — no package, no entitlements.
//   cx.plugin("iap","products", <json id array>, |r| Msg::Products(r))
//   cx.plugin("iap","purchase", <product id>, |r| Msg::Started(r))   // launches the sheet
//   cx.subscribe("iap","iap","transactions","", |r| Msg::Txn(r))     // ONE event per transaction
//   cx.plugin("iap","restore","", |r| Msg::Started(r))
//   cx.plugin("iap","finish", <transactionId>, |r| Msg::Done(r))     // call AFTER granting content
//
// The transactions STREAM is the single source of truth: `purchase`/`restore` return a thin ack and
// the authoritative transaction arrives on the stream — from purchase(), Transaction.updates (out-of-app
// purchases / Ask-to-Buy approvals / subscription renewals), or currentEntitlements (restore). The
// `IapStore` singleton holds the emit sink + a pre-subscribe buffer (so a renewal that lands before the
// core subscribes isn't lost) and parks unfinished transactions so `finish` can call the real object.
enum IapPlugin {
    static func handle(op: String, input: String) async -> PluginResponse {
        switch op {
        case "products": return await IapStore.shared.products(input)
        case "purchase": return await IapStore.shared.purchase(productId: input)
        case "restore": return await IapStore.shared.restore()
        case "finish": return await IapStore.shared.finish(transactionId: input)
        default: return PluginResponse(ok: false, output: "unknown op '\(op)'")
        }
    }

    static func subscribe(op: String, input: String, emit: @escaping @Sendable (PluginResponse) -> Void) async {
        let sink: @Sendable (String) -> Void = { emit(PluginResponse(ok: true, output: $0)) }
        await IapStore.shared.attach(sink)
        await withTaskCancellationHandler {
            while !Task.isCancelled {
                try? await Task.sleep(nanoseconds: 1_000_000_000)
            }
        } onCancel: {
            Task { @MainActor in IapStore.shared.detach() }
        }
    }
}

@MainActor
final class IapStore {
    static let shared = IapStore()

    private var sink: (@Sendable (String) -> Void)?
    private var buffer: [String] = []
    private var products: [String: Product] = [:]          // cached for purchase()
    private var unfinished: [String: Transaction] = [:]     // parked by id, so finish() has the real object
    private var emitted: Set<UInt64> = []                   // dedupe purchase() vs the Transaction.updates echo
    private var updatesTask: Task<Void, Never>?             // app-lifetime listener (out-of-app / renewals)

    // --- stream wiring (called by the plugin's subscribe) ---

    func attach(_ sink: @escaping @Sendable (String) -> Void) {
        self.sink = sink
        for payload in buffer { sink(payload) }
        buffer.removeAll()
        startUpdatesListener()  // idempotent
    }

    func detach() { sink = nil }  // keep the updates listener running; its emits buffer until re-attach

    private func startUpdatesListener() {
        guard updatesTask == nil else { return }
        updatesTask = Task.detached {
            for await result in Transaction.updates {
                await IapStore.shared.handleUpdate(result)
            }
        }
    }

    private func handleUpdate(_ result: VerificationResult<Transaction>) {
        let (txn, verified) = Self.unwrap(result)
        let state = txn.revocationDate != nil ? "revoked" : "purchased"
        park(txn)
        emitTransaction(txn, jws: result.jwsRepresentation, verified: verified, state: state, isRestore: false)
    }

    // --- ops ---

    func products(_ idsJSON: String) async -> PluginResponse {
        let ids = (try? JSONSerialization.jsonObject(with: Data(idsJSON.utf8))) as? [String] ?? []
        guard !ids.isEmpty else { return PluginResponse(ok: false, output: "expected a JSON array of product ids") }
        do {
            let fetched = try await Product.products(for: ids)
            for p in fetched { products[p.id] = p }
            let arr: [[String: Any]] = fetched.map { p in
                [
                    "id": p.id,
                    "title": p.displayName,
                    "description": p.description,
                    "price": p.displayPrice,
                    "priceMicros": NSDecimalNumber(decimal: p.price * 1_000_000).int64Value,
                    "currency": p.priceFormatStyle.currencyCode,
                    "type": Self.typeString(p.type),
                ]
            }
            return PluginResponse(ok: true, output: Self.json(arr))
        } catch {
            return PluginResponse(ok: false, output: error.localizedDescription)
        }
    }

    func purchase(productId: String) async -> PluginResponse {
        do {
            let product: Product
            if let cached = products[productId] {
                product = cached
            } else if let fetched = try await Product.products(for: [productId]).first {
                products[productId] = fetched
                product = fetched
            } else {
                return PluginResponse(ok: false, output: "unknown product '\(productId)'")
            }
            let result = try await product.purchase()
            switch result {
            case .success(let verification):
                let (txn, verified) = Self.unwrap(verification)
                park(txn)
                emitTransaction(txn, jws: verification.jwsRepresentation, verified: verified, state: "purchased", isRestore: false)
                return PluginResponse(ok: true, output: "{\"status\":\"launched\"}")
            case .userCancelled:
                return PluginResponse(ok: false, output: "cancelled")
            case .pending:
                // Ask-to-Buy / SCA: the result arrives later via Transaction.updates → the stream.
                return PluginResponse(ok: true, output: "{\"status\":\"pending\"}")
            @unknown default:
                return PluginResponse(ok: true, output: "{\"status\":\"launched\"}")
            }
        } catch {
            return PluginResponse(ok: false, output: error.localizedDescription)
        }
    }

    func restore() async -> PluginResponse {
        // Force-refresh entitlements, then emit each one to the stream as a restore.
        try? await AppStore.sync()
        for await result in Transaction.currentEntitlements {
            let (txn, verified) = Self.unwrap(result)
            park(txn)
            emitTransaction(txn, jws: result.jwsRepresentation, verified: verified, state: "restored", isRestore: true)
        }
        return PluginResponse(ok: true, output: "{\"status\":\"ok\"}")
    }

    func finish(transactionId: String) async -> PluginResponse {
        guard let txn = unfinished[transactionId] else {
            return PluginResponse(ok: false, output: "unknown transaction '\(transactionId)'")
        }
        await txn.finish()
        unfinished[transactionId] = nil
        return PluginResponse(ok: true, output: "{\"status\":\"ok\"}")
    }

    // --- helpers ---

    private func park(_ txn: Transaction) { unfinished[String(txn.id)] = txn }

    private func emitTransaction(_ txn: Transaction, jws: String, verified: Bool, state: String, isRestore: Bool) {
        if emitted.contains(txn.id) { return }  // suppress the purchase()-then-updates duplicate
        emitted.insert(txn.id)
        let dict: [String: Any] = [
            "productId": txn.productID,
            "transactionId": String(txn.id),
            "originalId": String(txn.originalID),
            "state": state,
            "platform": "storekit",
            "verified": verified,
            "payload": jws,
            "isRestore": isRestore,
        ]
        emit(Self.json(dict))
    }

    private func emit(_ payload: String) {
        if let sink { sink(payload) } else { buffer.append(payload) }
    }

    private static func unwrap(_ result: VerificationResult<Transaction>) -> (Transaction, Bool) {
        switch result {
        case .verified(let t): return (t, true)
        case .unverified(let t, _): return (t, false)
        }
    }

    private static func typeString(_ type: Product.ProductType) -> String {
        switch type {
        case .consumable: return "consumable"
        case .nonConsumable: return "non_consumable"
        case .autoRenewable: return "subscription"
        case .nonRenewable: return "non_renewing_subscription"
        default: return "unknown"
        }
    }

    private static func json(_ obj: Any) -> String {
        if let data = try? JSONSerialization.data(withJSONObject: obj),
           let s = String(data: data, encoding: .utf8) {
            return s
        }
        return "{}"
    }
}
