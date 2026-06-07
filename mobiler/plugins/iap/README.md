# iap — in-app purchase / subscriptions, StoreKit 2 / Play Billing (free, bundled)

```bash
mobiler plugin add iap
```

> ⚠️ **Android is experimental — not yet device-tested.** The iOS flow is verifiable on the simulator
> via a local `.storekit` file (below). Android Play Billing has no local sandbox: the purchase loop
> needs a Play Console app + a signed build on a test track + license testers, and hasn't been run on a
> device yet. Both platforms compile + install. (Mobiler itself is experimental.)

In-app purchases via the **native system sheets** — no Widget, no third-party SDK (iOS StoreKit 2 is a
system framework; Android uses the Play Billing library). Five ops + one stream, all on existing
primitives (no lib ABI change):

```rust
// Subscribe FIRST, at startup — the stream is the single source of truth.
cx.subscribe("iap", "iap", "transactions", "", Msg::Txn),
cx.plugin("iap", "products", r#"["pro_month","coins_100"]"#, Msg::Products), // → product metadata JSON
cx.plugin("iap", "purchase", "pro_month", Msg::Started),  // launches the sheet; result on the stream
cx.plugin("iap", "restore",  "",          Msg::Started),  // restores; results on the stream
cx.plugin("iap", "finish",   "<txnId | [consume:]purchaseToken>", Msg::Done), // AFTER granting content
```

- **The transactions stream is the single source of truth.** `purchase`/`restore` return only a thin
  ack (`launched` / `pending` / `cancelled`); the authoritative transaction always arrives on the
  stream — whether from the purchase sheet, an out-of-app purchase, an Ask-to-Buy approval, a
  subscription renewal, or restore. So there's exactly **one grant path** and no double-grant races.
- **Subscribe at startup.** Transactions that complete outside the app (renewals, Ask-to-Buy
  approvals) are buffered by the plugin until the core subscribes — but only if you subscribe early.
- **Each transaction carries the raw signed receipt** in `payload` (iOS JWS, Android
  `{purchaseToken, signature, originalJson}`). **POST it to your backend** to validate with the App
  Store Server API / Google Play Developer API before granting anything revenue-bearing. The plugin
  also does the platform's built-in check (StoreKit `verified`, Play `PURCHASED`) and sets a `verified`
  flag, but the backend is the real authority.
- **`finish` only after you grant content.** iOS replays unfinished transactions on next launch;
  Android auto-refunds unacknowledged purchases after 3 days. On Android, prefix the input with
  `consume:` for **consumables** (re-buyable) — otherwise it acknowledges (non-consumables / subs).
  iOS ignores the prefix (one `finish()` for everything).

## Transaction event shape

```json
{ "productId": "pro_month", "transactionId": "2000000…", "state": "purchased",
  "platform": "storekit", "verified": true, "payload": "<JWS or {purchaseToken,…}>", "isRestore": false }
```
`state` ∈ `purchased | pending | restored | renewed | revoked | cancelled`. `type` from `products` ∈
`consumable | non_consumable | subscription`.

## Setup (per platform)

- **Create your products first** in **App Store Connect** (iOS) and the **Play Console** (Android). The
  IDs you pass to `products` must match exactly.
- **iOS**: enable the **In-App Purchase** capability on your App ID. No entitlement file or Info.plist
  key — StoreKit 2 is a system framework.
- **Android**: add a Play Console app, create the products, upload a **signed** build to a
  closed/internal **test track**, and add **license testers**. There is no local sandbox.

## Test the iOS flow on the simulator (no App Store Connect, no real money)

StoreKit 2 supports a local **StoreKit Configuration file** that makes `products` + `purchase` work on
the simulator. Add a `Products.storekit` (defining your products + prices) to your iOS target and point
the scheme's **run action** at it (`StoreKitConfigurationFileReference`). Then the purchase sheet runs
on the sim against the local config — ideal for development + CI-style validation. The demo
(`demos/barbershop/iOS/Products.storekit`) shows a working example.

## v1 scope

Consumables, non-consumables, and **basic auto-renewing subscriptions** (the first base plan / offer).
Multi-offer eligibility, proration on upgrade/downgrade, and promotional/intro-offer flows are a
future enhancement.
