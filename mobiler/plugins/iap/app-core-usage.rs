// Rust app-side usage of the iap plugin (drop into shared/src/app.rs).
// The transactions stream is the single source of truth: subscribe at startup, grant on stream
// events, and `finish` only after granting. POST the signed `payload` to your backend to validate.

use mobiler_core::*;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone)]
pub enum Msg {
    EnableStore,
    Products(PluginResponse),
    Buy(String),
    Started(PluginResponse),
    Txn(PluginResponse),
    Restore,
    Validated(PluginResponse),
    Finished(PluginResponse),
}

#[derive(Default, Serialize, Deserialize)]
pub struct Model {
    pub products_json: String,
    pub owned: Vec<String>,
}

impl MyApp {
    fn handle(&self, msg: Msg, model: &mut Model, cx: &mut Cx<Msg>) {
        match msg {
            // Subscribe FIRST (so out-of-app / Ask-to-Buy / renewal transactions aren't missed), then
            // load product metadata for your store UI.
            Msg::EnableStore => {
                cx.subscribe("iap", "iap", "transactions", "", Msg::Txn);
                cx.plugin("iap", "products", r#"["pro_month","coins_100"]"#, Msg::Products);
            }
            Msg::Products(r) => {
                if r.ok {
                    model.products_json = r.output; // JSON array: [{id,title,price,type,…}]
                }
            }
            // Launches the native purchase sheet; the result arrives on the transactions stream.
            Msg::Buy(id) => cx.plugin("iap", "purchase", &id, Msg::Started),
            Msg::Started(_) => {} // thin ack: launched / pending / cancelled
            // The authoritative transaction. Validate server-side, grant, then finish.
            Msg::Txn(r) => {
                if r.ok {
                    // 1) parse r.output → {productId, transactionId, state, payload, …}
                    // 2) POST `payload` (the signed receipt) to your backend for validation:
                    cx.post("https://api.example.com/iap/validate", r.output.clone(), Msg::Validated);
                }
            }
            Msg::Validated(r) => {
                if r.ok {
                    // backend confirmed → grant entitlement, then finish the transaction:
                    // (use the transactionId from the txn; prefix "consume:" + purchaseToken for
                    //  Android consumables)
                    let txn_id = r.output.clone();
                    cx.plugin("iap", "finish", &txn_id, Msg::Finished);
                }
            }
            Msg::Finished(_) => {}
            Msg::Restore => cx.plugin("iap", "restore", "", Msg::Started),
        }
    }
}
