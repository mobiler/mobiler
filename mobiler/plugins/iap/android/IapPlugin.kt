package {{PACKAGE}}

import {{PACKAGE_SHARED_TYPES}}.PluginResponse

import com.android.billingclient.api.AcknowledgePurchaseParams
import com.android.billingclient.api.BillingClient
import com.android.billingclient.api.BillingClient.ProductType
import com.android.billingclient.api.BillingClientStateListener
import com.android.billingclient.api.BillingFlowParams
import com.android.billingclient.api.BillingResult
import com.android.billingclient.api.ConsumeParams
import com.android.billingclient.api.PendingPurchasesParams
import com.android.billingclient.api.ProductDetails
import com.android.billingclient.api.Purchase
import com.android.billingclient.api.PurchasesUpdatedListener
import com.android.billingclient.api.QueryProductDetailsParams
import com.android.billingclient.api.QueryPurchasesParams
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.channels.awaitClose
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.callbackFlow
import org.json.JSONArray
import org.json.JSONObject

// In-app purchase (free, bundled; Android EXPERIMENTAL — needs a Play Console test track). Google Play
// Billing 7. Two surfaces, both riding existing primitives:
//   cx.plugin("iap","products"/"purchase"/"restore"/"finish", …)  // one-shots
//   cx.subscribe("iap","iap","transactions","", …)                // ONE event per transaction
//
// The transactions STREAM is the single source of truth: `launchBillingFlow` results come back on the
// BillingClient's PurchasesUpdatedListener (not as a return value), which feeds the singleton `IapBus`
// → the stream. IapBus buffers events that arrive before the core subscribes. Call `finish` only after
// granting content (Play auto-refunds unacknowledged purchases in 3 days): default acknowledges; an
// input prefixed `consume:` consumes (consumables, re-buyable).
class IapPlugin(private val application: android.app.Application) : MobilerPlugin {
    private val purchasesListener = PurchasesUpdatedListener { result, purchases ->
        if (result.responseCode == BillingClient.BillingResponseCode.OK && purchases != null) {
            for (p in purchases) IapBus.emit(purchaseJson(p, isRestore = false))
        } else if (result.responseCode == BillingClient.BillingResponseCode.USER_CANCELED) {
            IapBus.emit(JSONObject().put("state", "cancelled").put("platform", "play").toString())
        }
    }

    private val client: BillingClient = BillingClient.newBuilder(application)
        .setListener(purchasesListener)
        .enablePendingPurchases(PendingPurchasesParams.newBuilder().enableOneTimeProducts().build())
        .build()

    // ProductDetails + (for subs) the first offer token, cached by product id for launchBillingFlow.
    private val details = mutableMapOf<String, ProductDetails>()
    private val offerToken = mutableMapOf<String, String>()

    override suspend fun handle(op: String, input: String): PluginResponse = when (op) {
        "products" -> products(input)
        "purchase" -> purchase(input)
        "restore" -> restore()
        "finish" -> finish(input)
        else -> PluginResponse(false, "unknown op '$op'")
    }

    override fun subscribe(op: String, input: String): Flow<PluginResponse> = callbackFlow {
        val sink: (String) -> Unit = { trySend(PluginResponse(true, it)) }
        IapBus.attach(sink)
        awaitClose { IapBus.detach(sink) }
    }

    // BillingClient connects asynchronously; every op awaits a live connection first.
    private suspend fun ensureConnected(): Boolean {
        if (client.isReady) return true
        val ready = CompletableDeferred<Boolean>()
        client.startConnection(object : BillingClientStateListener {
            override fun onBillingSetupFinished(result: BillingResult) {
                if (!ready.isCompleted) ready.complete(result.responseCode == BillingClient.BillingResponseCode.OK)
            }
            override fun onBillingServiceDisconnected() {
                if (!ready.isCompleted) ready.complete(false) // a later op retries the connection
            }
        })
        return ready.await()
    }

    private suspend fun products(input: String): PluginResponse {
        if (!ensureConnected()) return PluginResponse(false, "billing unavailable")
        val ids = runCatching { JSONArray(input) }.getOrNull()
            ?: return PluginResponse(false, "expected a JSON array of product ids")
        val idList = (0 until ids.length()).map { ids.getString(it) }
        val all = queryDetails(idList, ProductType.INAPP) + queryDetails(idList, ProductType.SUBS)
        val arr = JSONArray()
        for (pd in all) {
            details[pd.productId] = pd
            val obj = JSONObject()
                .put("id", pd.productId)
                .put("title", pd.name)
                .put("description", pd.description)
            if (pd.productType == ProductType.SUBS) {
                // Basic sub: take the first offer's first pricing phase for display + stash its token.
                val offer = pd.subscriptionOfferDetails?.firstOrNull()
                val phase = offer?.pricingPhases?.pricingPhaseList?.firstOrNull()
                offer?.offerToken?.let { offerToken[pd.productId] = it }
                obj.put("type", "subscription")
                    .put("price", phase?.formattedPrice ?: "")
                    .put("priceMicros", phase?.priceAmountMicros ?: 0L)
                    .put("currency", phase?.priceCurrencyCode ?: "")
            } else {
                val one = pd.oneTimePurchaseOfferDetails
                obj.put("type", "inapp")
                    .put("price", one?.formattedPrice ?: "")
                    .put("priceMicros", one?.priceAmountMicros ?: 0L)
                    .put("currency", one?.priceCurrencyCode ?: "")
            }
            arr.put(obj)
        }
        return PluginResponse(true, arr.toString())
    }

    private suspend fun queryDetails(ids: List<String>, type: String): List<ProductDetails> {
        val products = ids.map {
            QueryProductDetailsParams.Product.newBuilder().setProductId(it).setProductType(type).build()
        }
        val params = QueryProductDetailsParams.newBuilder().setProductList(products).build()
        val deferred = CompletableDeferred<List<ProductDetails>>()
        client.queryProductDetailsAsync(params) { _, list -> deferred.complete(list) }
        return runCatching { deferred.await() }.getOrDefault(emptyList())
    }

    private fun purchase(productId: String): PluginResponse {
        val pd = details[productId] ?: return PluginResponse(false, "call products first for '$productId'")
        val activity = MobilerActivity.current?.get() ?: return PluginResponse(false, "no foreground activity")
        val paramsBuilder = BillingFlowParams.ProductDetailsParams.newBuilder().setProductDetails(pd)
        offerToken[productId]?.let { paramsBuilder.setOfferToken(it) }
        val flow = BillingFlowParams.newBuilder()
            .setProductDetailsParamsList(listOf(paramsBuilder.build()))
            .build()
        val result = client.launchBillingFlow(activity, flow)
        // The actual purchase lands on the PurchasesUpdatedListener → the stream.
        return if (result.responseCode == BillingClient.BillingResponseCode.OK) {
            PluginResponse(true, "{\"status\":\"launched\"}")
        } else {
            PluginResponse(false, "launch failed: ${result.debugMessage}")
        }
    }

    private suspend fun restore(): PluginResponse {
        if (!ensureConnected()) return PluginResponse(false, "billing unavailable")
        for (type in listOf(ProductType.INAPP, ProductType.SUBS)) {
            val params = QueryPurchasesParams.newBuilder().setProductType(type).build()
            val deferred = CompletableDeferred<List<Purchase>>()
            client.queryPurchasesAsync(params) { _, purchases -> deferred.complete(purchases) }
            for (p in runCatching { deferred.await() }.getOrDefault(emptyList())) {
                IapBus.emit(purchaseJson(p, isRestore = true))
            }
        }
        return PluginResponse(true, "{\"status\":\"ok\"}")
    }

    private suspend fun finish(input: String): PluginResponse {
        if (!ensureConnected()) return PluginResponse(false, "billing unavailable")
        val consume = input.startsWith("consume:")
        val token = input.removePrefix("consume:")
        val deferred = CompletableDeferred<Boolean>()
        if (consume) {
            val params = ConsumeParams.newBuilder().setPurchaseToken(token).build()
            client.consumeAsync(params) { result, _ -> deferred.complete(result.responseCode == BillingClient.BillingResponseCode.OK) }
        } else {
            val params = AcknowledgePurchaseParams.newBuilder().setPurchaseToken(token).build()
            client.acknowledgePurchase(params) { result -> deferred.complete(result.responseCode == BillingClient.BillingResponseCode.OK) }
        }
        val ok = runCatching { deferred.await() }.getOrDefault(false)
        return PluginResponse(ok, if (ok) "{\"status\":\"ok\"}" else "finish failed")
    }

    private fun purchaseJson(p: Purchase, isRestore: Boolean): String {
        val state = when (p.purchaseState) {
            Purchase.PurchaseState.PURCHASED -> if (isRestore) "restored" else "purchased"
            Purchase.PurchaseState.PENDING -> "pending"
            else -> "unspecified"
        }
        return JSONObject()
            .put("productId", p.products.firstOrNull() ?: "")
            .put("transactionId", p.orderId ?: "")
            .put("state", state)
            .put("platform", "play")
            .put("acknowledged", p.isAcknowledged)
            .put("payload", JSONObject().put("purchaseToken", p.purchaseToken).put("signature", p.signature).put("originalJson", p.originalJson))
            .put("isRestore", isRestore)
            .toString()
    }
}

// In-process bus between the BillingClient's PurchasesUpdatedListener and the cx.subscribe stream.
// Buffers events that arrive before the core attaches a sink, then flushes on attach (mirrors PushBus).
object IapBus {
    private var sink: ((String) -> Unit)? = null
    private val buffer = mutableListOf<String>()

    @Synchronized fun attach(s: (String) -> Unit) {
        sink = s
        buffer.forEach { s(it) }
        buffer.clear()
    }

    @Synchronized fun detach(s: (String) -> Unit) {
        if (sink === s) sink = null
    }

    @Synchronized fun emit(payload: String) {
        val s = sink
        if (s != null) s(payload) else buffer.add(payload)
    }
}
