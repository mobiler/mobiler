use mobiler_core::{
    BoxAlign, ButtonStyle, CardStyle, Cx, ImageRatio, ImageShape, InputValue, MobilerApp,
    MobilerShell, Widget, button, card, card_button, chip, column, image, row, slider, stack,
    stepper, text, title,
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
    GotPhoto(String),
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
            Msg::GotPhoto(uri) => {
                if !uri.is_empty() {
                    model.picked_photo = Some(uri);
                }
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
        match model.open_product.and_then(|i| model.products.get(i)) {
            Some(product) => detail(product, model),
            None => storefront(model),
        }
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
        button("Pick a photo", ButtonStyle::Outlined, Msg::PickPhoto),
    ];
    if !model.device_info.is_empty() {
        items.push(mobiler_core::caption(format!("This device: {}", model.device_info)));
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
}
