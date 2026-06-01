use mobiler_core::{
    BoxAlign, ButtonStyle, CardStyle, Corner, Cx, Density, FontFamily, ImageRatio, ImageShape,
    InputValue, MobilerApp, MobilerShell, PluginResponse, Rgb, Theme, Widget, button, card,
    card_button, chip, column, image, row, scaffold, slider, stack, stepper, text, title,
    with_theme,
};
use serde::{Deserialize, Serialize};

const HERO: &str = "https://images.unsplash.com/photo-1509042239860-f550ce710b93?w=1200&q=80";

/// Coffee demo, ported onto MobilerApp (was: per-app-typegen `demos/coffee`).
#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum Msg {
    SelectCategory(String),
    OpenProduct(u32),
    CloseProduct,
    IncQty,
    DecQty,
    // Capability demos (exercise the free built-in plugins).
    Share,
    CopyName,
    OpenRecipe,
    ToastHi,
    Tap,
    WhatDevice,
    GotDevice(String),
    AskConfirm,
    Confirmed(bool),
    PickPhoto,
    CapturePhoto,
    GotPhoto(String),
    ScanCode,
    GotScan(String),
    // Auth demo: store a secret, biometric-unlock, read it back.
    SecureDemo,
    StoredSecret(bool),
    Authed(PluginResponse),
    RevealedSecret(PluginResponse),
    // WebSocket demo: connect to an echo server, send, receive the echo.
    WsEcho,
    WsOpen(PluginResponse),
    WsSent(PluginResponse),
    WsFrame(PluginResponse),
    // Notifications demo: ask permission, schedule a reminder ~10s out.
    RemindMe,
    NotifAllowed(PluginResponse),
    NotifScheduled(PluginResponse),
}

#[derive(Clone)]
struct Product {
    name: &'static str,
    price: &'static str,
    rating: &'static str,
    category: &'static str,
    image: &'static str,
    description: &'static str,
}

pub struct Model {
    products: Vec<Product>,
    categories: Vec<&'static str>,
    selected_category: String,
    open_product: Option<usize>,
    sweetness: i32,
    quantity: i32,
    device_info: String,           // filled by the device capability (request/response demo)
    picked_photo: Option<String>,  // a local image URI from the photo-picker capability
    scanned: Option<String>,       // last barcode/QR scanned via the scanner plugin
    secret_status: Option<String>, // biometric + securestore demo status line
    ws_status: Option<String>,     // websocket echo demo status line
    notif_status: Option<String>,  // notifications demo status line
}

impl Default for Model {
    fn default() -> Self {
        Self {
            products: vec![
                Product { name: "Caffè Mocha", price: "$4.53", rating: "4.8", category: "Latte", image: "https://loremflickr.com/400/400/coffee?lock=1", description: "Espresso with steamed milk and a touch of chocolate." },
                Product { name: "Flat White", price: "$3.53", rating: "4.6", category: "Latte", image: "https://loremflickr.com/400/400/coffee?lock=2", description: "Velvety microfoam over a double shot of espresso." },
                Product { name: "Espresso", price: "$2.20", rating: "4.9", category: "Espresso", image: "https://loremflickr.com/400/400/espresso?lock=3", description: "A concentrated, full-bodied single origin shot." },
                Product { name: "Cappuccino", price: "$3.80", rating: "4.5", category: "Espresso", image: "https://loremflickr.com/400/400/cappuccino?lock=4", description: "Equal parts espresso, steamed milk and airy foam." },
                Product { name: "Cold Brew", price: "$4.10", rating: "4.7", category: "Cold", image: "https://loremflickr.com/400/400/coldbrew?lock=5", description: "Steeped 18 hours for a smooth, low-acidity cup." },
                Product { name: "Iced Latte", price: "$4.20", rating: "4.4", category: "Cold", image: "https://loremflickr.com/400/400/icedcoffee?lock=6", description: "Chilled espresso and milk over ice." },
            ],
            categories: vec!["All", "Latte", "Espresso", "Cold"],
            selected_category: "All".to_string(),
            open_product: None,
            sweetness: 50,
            quantity: 1,
            device_info: String::new(),
            picked_photo: None,
            scanned: None,
            secret_status: None,
            ws_status: None,
            notif_status: None,
        }
    }
}

impl Model {
    fn visible(&self) -> Vec<(usize, &Product)> {
        self.products
            .iter()
            .enumerate()
            .filter(|(_, p)| self.selected_category == "All" || p.category == self.selected_category.as_str())
            .collect()
    }
}

#[derive(Default)]
pub struct Coffee;

impl MobilerApp for Coffee {
    type Event = Msg;
    type Model = Model;

    fn update(&self, event: Msg, model: &mut Model, cx: &mut Cx<Msg>) {
        match event {
            Msg::SelectCategory(c) => model.selected_category = c,
            Msg::OpenProduct(i) => {
                model.open_product = Some(i as usize);
                model.sweetness = 50;
                model.quantity = 1;
            }
            Msg::CloseProduct => model.open_product = None,
            Msg::IncQty => model.quantity += 1,
            Msg::DecQty => model.quantity = (model.quantity - 1).max(1),
            // Free built-in capabilities, exercised against the open product.
            Msg::Share => {
                if let Some(p) = model.open_product.and_then(|i| model.products.get(i)) {
                    cx.share(format!("{} — {} (★ {}) from the Mobiler coffee demo", p.name, p.price, p.rating));
                }
            }
            Msg::CopyName => {
                if let Some(p) = model.open_product.and_then(|i| model.products.get(i)) {
                    cx.copy(p.name);
                }
            }
            Msg::OpenRecipe => cx.open_url("https://en.wikipedia.org/wiki/Coffee"),
            Msg::ToastHi => cx.toast("Brewing… ☕ (toast from the Rust core)"),
            Msg::Tap => cx.haptic("medium"),
            Msg::WhatDevice => cx.device_model(|r| Msg::GotDevice(r.output)),
            Msg::GotDevice(info) => model.device_info = info,
            Msg::AskConfirm => cx.confirm("Add to cart?", "Add this coffee to your order?", |r| Msg::Confirmed(r.ok)),
            Msg::Confirmed(ok) => cx.toast(if ok { "Added to cart ✓" } else { "Cancelled" }),
            Msg::PickPhoto => cx.pick_photo(|r| Msg::GotPhoto(if r.ok { r.output } else { String::new() })),
            Msg::CapturePhoto => cx.capture_photo(|r| Msg::GotPhoto(if r.ok { r.output } else { String::new() })),
            Msg::GotPhoto(uri) => {
                if !uri.is_empty() {
                    model.picked_photo = Some(uri);
                }
            }
            // Scanner plugin (free, bundled). Returns "<format>:<value>" (e.g. "qr:…",
            // "ean13:…"); on cancel / no camera / denied it returns ok:false (output = reason).
            Msg::ScanCode => cx.plugin("scanner", "scan", "", |r| {
                Msg::GotScan(if r.ok { r.output } else { format!("(no scan: {})", r.output) })
            }),
            Msg::GotScan(result) => model.scanned = Some(result),
            // Auth demo: store a secret in the keychain/keystore, then require a biometric
            // unlock before reading it back — the canonical biometric + securestore pairing.
            Msg::SecureDemo => {
                model.secret_status = Some("Storing secret…".into());
                cx.plugin("securestore", "set", r#"{"key":"demo","value":"espresso-42"}"#, |r| Msg::StoredSecret(r.ok));
            }
            Msg::StoredSecret(ok) => {
                if ok {
                    model.secret_status = Some("Stored. Authenticate to reveal it.".into());
                    cx.plugin("biometric", "authenticate", "Reveal the secret", Msg::Authed);
                } else {
                    model.secret_status = Some("Couldn't store the secret.".into());
                }
            }
            Msg::Authed(resp) => {
                if resp.ok {
                    cx.plugin("securestore", "get", r#"{"key":"demo"}"#, Msg::RevealedSecret);
                } else {
                    model.secret_status = Some(format!("Auth failed: {}", resp.output));
                }
            }
            Msg::RevealedSecret(resp) => {
                model.secret_status = Some(if resp.ok {
                    format!("Unlocked secret: {}", resp.output)
                } else {
                    format!("Read failed: {}", resp.output)
                });
            }
            // WebSocket echo: connect → on open, send "hello from mobiler" → recv the echo.
            Msg::WsEcho => {
                model.ws_status = Some("Connecting…".into());
                cx.plugin("websocket", "connect", "wss://echo.websocket.org", Msg::WsOpen);
            }
            Msg::WsOpen(resp) => {
                if resp.ok {
                    model.ws_status = Some("Connected — sending…".into());
                    cx.plugin("websocket", "recv", "", Msg::WsFrame);
                    cx.plugin("websocket", "send", "hello from mobiler", Msg::WsSent);
                } else {
                    model.ws_status = Some(format!("WS connect failed: {}", resp.output));
                }
            }
            Msg::WsSent(_) => {}
            Msg::WsFrame(resp) => {
                model.ws_status = Some(if resp.ok {
                    format!("Echo: {}", resp.output)
                } else {
                    "WS closed".into()
                });
                // One round-trip is enough for the demo; close after the first echo.
                if resp.ok {
                    cx.plugin("websocket", "close", "", Msg::WsSent);
                }
            }
            // Notifications: ask permission, then schedule a reminder ~10s out so a tester
            // can background the app and watch it fire.
            Msg::RemindMe => {
                model.notif_status = Some("Requesting permission…".into());
                cx.plugin("notifications", "requestPermission", "", Msg::NotifAllowed);
            }
            Msg::NotifAllowed(resp) => {
                if resp.ok {
                    model.notif_status = Some("Scheduling a reminder in ~10s — background the app".into());
                    cx.plugin(
                        "notifications",
                        "schedule",
                        r#"{"id":1,"title":"Coffee reminder","body":"Your espresso awaits ☕","after_seconds":10}"#,
                        Msg::NotifScheduled,
                    );
                } else {
                    model.notif_status = Some(format!("Notifications not allowed: {}", resp.output));
                }
            }
            Msg::NotifScheduled(resp) => {
                model.notif_status = Some(if resp.ok {
                    "Reminder set — background the app to see it fire".into()
                } else {
                    format!("Schedule failed: {}", resp.output)
                });
            }
        }
    }

    fn input(&self, id: &str, value: InputValue, model: &mut Model, _cx: &mut Cx<Msg>) {
        if id == "sweetness" {
            if let InputValue::Int(v) = value {
                model.sweetness = v as i32;
            }
        }
    }

    fn view(&self, model: &Model) -> Widget {
        // Dogfood the theming engine: a terracotta brand (large corners, rounded font,
        // comfortable density). Wrapping in a Scaffold is what lets the shells apply it —
        // theme is carried on the Scaffold, the visual twin of dark_mode.
        let theme = Theme {
            seed: Rgb::new(0xC8, 0x5A, 0x3C),
            accent: None,
            corner: Corner::Large,
            density: Density::Comfortable,
            font: FontFamily::Rounded,
        };
        // Both screens are themed Scaffolds (title bar + body). Detail keeps its in-body
        // "← Back" button (guarded by the CoffeeUITests regression), so no top-bar back.
        let root = match model.open_product.and_then(|i| model.products.get(i)) {
            Some(product) => scaffold(product.name, false, vec![], detail(product, model)),
            None => scaffold("Coffee", false, vec![], storefront(model)),
        };
        with_theme(root, theme)
    }
}

fn storefront(model: &Model) -> Widget {
    let hero = stack(
        BoxAlign::BottomStart,
        true,
        vec![
            image(HERO, ImageShape::Rounded, ImageRatio::Wide),
            column(vec![title("Fall in Love with Coffee"), button("Get Started", ButtonStyle::Filled, Msg::SelectCategory("All".to_string()))]),
        ],
    );
    let chips = row(
        model.categories.iter().map(|c| {
            chip((*c).to_string(), model.selected_category.as_str() == *c, Msg::SelectCategory((*c).to_string()))
        }).collect(),
    );
    let products = mobiler_core::grid(model.visible().iter().map(|(i, p)| product_card(*i, p)).collect());
    column(vec![hero, chips, products])
}

fn product_card(index: usize, p: &Product) -> Widget {
    card_button(
        column(vec![
            image(p.image, ImageShape::Rounded, ImageRatio::Square),
            text(p.name),
            row(vec![text(p.price), text(format!("★ {}", p.rating))]),
        ]),
        CardStyle::Filled,
        Msg::OpenProduct(index as u32),
    )
}

fn detail(p: &Product, model: &Model) -> Widget {
    let mut items = vec![
        button("← Back", ButtonStyle::Text, Msg::CloseProduct),
        image(p.image, ImageShape::Rounded, ImageRatio::Wide),
        title(p.name),
        text(format!("★ {}    {}", p.rating, p.price)),
        // Free built-in capabilities — tap to try the shipped plugins.
        row(vec![
            button("Share", ButtonStyle::Outlined, Msg::Share),
            button("Copy name", ButtonStyle::Outlined, Msg::CopyName),
            button("Recipe ↗", ButtonStyle::Outlined, Msg::OpenRecipe),
        ]),
        row(vec![
            button("Toast", ButtonStyle::Outlined, Msg::ToastHi),
            button("Haptic", ButtonStyle::Outlined, Msg::Tap),
            button("Device", ButtonStyle::Outlined, Msg::WhatDevice),
        ]),
        row(vec![
            button("Pick a photo", ButtonStyle::Outlined, Msg::PickPhoto),
            button("Take a photo", ButtonStyle::Outlined, Msg::CapturePhoto),
        ]),
        row(vec![
            button("Scan a code", ButtonStyle::Filled, Msg::ScanCode),
            button("Lock test", ButtonStyle::Outlined, Msg::SecureDemo),
            button("WS echo", ButtonStyle::Outlined, Msg::WsEcho),
            button("Remind me", ButtonStyle::Outlined, Msg::RemindMe),
        ]),
    ];
    if !model.device_info.is_empty() {
        items.push(mobiler_core::caption(format!("This device: {}", model.device_info)));
    }
    // The scanner plugin returns "<format>:<value>" — show it so a tester can read the result.
    if let Some(code) = &model.scanned {
        items.push(mobiler_core::caption(format!("Scanned: {}", code)));
    }
    // biometric + securestore demo status.
    if let Some(s) = &model.secret_status {
        items.push(mobiler_core::caption(s.clone()));
    }
    // websocket echo demo status.
    if let Some(s) = &model.ws_status {
        items.push(mobiler_core::caption(s.clone()));
    }
    // notifications demo status.
    if let Some(s) = &model.notif_status {
        items.push(mobiler_core::caption(s.clone()));
    }
    // The photo capability returns a local image URI — fed straight to the image widget.
    if let Some(uri) = &model.picked_photo {
        items.push(image(uri.as_str(), ImageShape::Rounded, ImageRatio::Wide));
    }
    items.extend([
        text(p.description),
        mobiler_core::caption(format!("Sweetness: {}%", model.sweetness)),
        slider("sweetness", model.sweetness, 100),
        row(vec![text("Quantity"), stepper(model.quantity, Msg::DecQty, Msg::IncQty)]),
        button(format!("Add {} to cart · {}", model.quantity, p.price), ButtonStyle::Filled, Msg::AskConfirm),
        card(text("Tip: tap a product on the storefront to open this screen."), CardStyle::Outlined),
    ]);
    column(items)
}

/// The Crux app the FFI + codegen target.
pub type App = MobilerShell<Coffee>;

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn filters_by_category() {
        let app = Coffee;
        let mut model = Model::default();
        app.update(Msg::SelectCategory("Latte".to_string()), &mut model, &mut Cx::default());
        let visible = model.visible();
        assert!(!visible.is_empty());
        assert!(visible.iter().all(|(_, p)| p.category == "Latte"));
    }

    #[test]
    fn open_close_and_qty() {
        let app = Coffee;
        let mut model = Model::default();
        app.update(Msg::OpenProduct(2), &mut model, &mut Cx::default());
        assert_eq!(model.open_product, Some(2));
        app.update(Msg::IncQty, &mut model, &mut Cx::default());
        app.update(Msg::IncQty, &mut model, &mut Cx::default());
        assert_eq!(model.quantity, 3);
        app.input("sweetness", InputValue::Int(80), &mut model, &mut Cx::default());
        assert_eq!(model.sweetness, 80);
        app.update(Msg::CloseProduct, &mut model, &mut Cx::default());
        assert!(model.open_product.is_none());
    }

    #[test]
    fn photo_result_sets_image_and_ignores_cancel() {
        // Both the picker (PickPhoto) and the camera (CapturePhoto) deliver their result
        // via GotPhoto, so this covers the result handling for both capabilities.
        let app = Coffee;
        let mut model = Model::default();
        // Happy path: a URI is stored and shown.
        app.update(Msg::GotPhoto("content://media/42".into()), &mut model, &mut Cx::default());
        assert_eq!(model.picked_photo.as_deref(), Some("content://media/42"));
        // Sad path: cancel / failure delivers an empty string → keep the prior photo.
        app.update(Msg::GotPhoto(String::new()), &mut model, &mut Cx::default());
        assert_eq!(model.picked_photo.as_deref(), Some("content://media/42"));
    }
}
