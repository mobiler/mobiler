use crux_core::{
    App, Command,
    macros::effect,
    render::{RenderOperation, render},
};
use facet::Facet;
use serde::{Deserialize, Serialize};

/// Coffee hero photo (Unsplash CDN, no API key needed).
const HERO_IMAGE: &str =
    "https://images.unsplash.com/photo-1509042239860-f550ce710b93?w=1200&q=80";

#[derive(Facet, Serialize, Deserialize, Clone, Debug)]
#[repr(C)]
pub enum Event {
    /// Tapping a category chip (or "Get Started").
    SelectCategory(String),
    /// Open a product's detail screen (index into the product list).
    OpenProduct(u32),
    /// Back from detail to the storefront.
    CloseProduct,
    /// A slider moved; `id` identifies which one.
    SliderChanged { id: String, value: i32 },
    IncQty,
    DecQty,
}

#[effect(facet_typegen)]
#[derive(Debug)]
pub enum Effect {
    Render(RenderOperation),
}

/// A coffee product. Lives only in the Rust model; `view` turns it into Widgets.
#[derive(Clone, Debug)]
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
    /// Some(index) shows that product's detail screen; None shows the storefront.
    open_product: Option<usize>,
    /// Detail-screen controls.
    sweetness: i32,
    quantity: i32,
}

impl Default for Model {
    fn default() -> Self {
        Self {
            products: vec![
                Product { name: "Caffè Mocha", price: "$4.53", rating: "4.8", category: "Latte", image: "https://loremflickr.com/400/400/coffee?lock=1", description: "Espresso with steamed milk and a touch of chocolate." },
                Product { name: "Flat White", price: "$3.53", rating: "4.6", category: "Latte", image: "https://loremflickr.com/400/400/coffee?lock=2", description: "Velvety microfoam over a double shot of espresso." },
                Product { name: "Espresso", price: "$2.20", rating: "4.9", category: "Espresso", image: "https://loremflickr.com/400/400/espresso?lock=3", description: "A concentrated, full-bodied single origin shot." },
                Product { name: "Cappuccino", price: "$3.80", rating: "4.5", category: "Espresso", image: "https://loremflickr.com/400/400/cappuccino?lock=4", description: "Equal parts espresso, steamed milk and airy foam." },
                Product { name: "Cold Brew", price: "$4.10", rating: "4.7", category: "Cold", image: "https://loremflickr.com/400/400/coldbrew?lock=5", description: "Steeped for 18 hours for a smooth, low-acidity cup." },
                Product { name: "Iced Latte", price: "$4.20", rating: "4.4", category: "Cold", image: "https://loremflickr.com/400/400/icedcoffee?lock=6", description: "Chilled espresso and milk over ice." },
            ],
            categories: vec!["All", "Latte", "Espresso", "Cold"],
            selected_category: "All".to_string(),
            open_product: None,
            sweetness: 50,
            quantity: 1,
        }
    }
}

impl Model {
    /// (index, product) pairs matching the selected category ("All" = everything).
    fn visible_products(&self) -> Vec<(usize, &Product)> {
        self.products
            .iter()
            .enumerate()
            .filter(|(_, p)| self.selected_category == "All" || p.category == self.selected_category.as_str())
            .collect()
    }
}

/// How an image's corners are treated. Concrete radii decided in the render layer.
#[derive(Facet, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
pub enum ImageShape {
    Square,
    Rounded,
    Circle,
}

/// Aspect-ratio token for images (concrete ratio decided in the render layer).
#[derive(Facet, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
pub enum ImageRatio {
    Wide,
    Square,
    Tall,
}

/// Where a `Box`'s overlaid content sits within the stack.
#[derive(Facet, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
pub enum BoxAlign {
    TopStart,
    TopEnd,
    Center,
    BottomStart,
    BottomCenter,
    BottomEnd,
}

#[derive(Facet, Serialize, Deserialize, Clone, Debug)]
#[repr(C)]
pub enum Widget {
    Text { content: String },
    Button { label: String, on_press: Event },
    Row { children: Vec<Widget> },
    Column { children: Vec<Widget> },
    /// Card; tappable when `on_press` is set.
    Card { child: Box<Widget>, on_press: Option<Event> },
    Chip { label: String, selected: bool, on_press: Event },
    Grid { children: Vec<Widget> },
    Image { source: String, shape: ImageShape, ratio: ImageRatio },
    Box { children: Vec<Widget>, align: BoxAlign, scrim: bool },
    /// Continuous 0..=`max` slider; emits `SliderChanged { id, value }`.
    Slider { id: String, value: i32, max: i32 },
    /// Numeric stepper with −/+ controls carrying their own events.
    Stepper { value: i32, on_decrement: Event, on_increment: Event },
}

pub type ViewModel = Widget;

#[derive(Default)]
pub struct CoffeeApp;

impl App for CoffeeApp {
    type Event = Event;
    type Model = Model;
    type ViewModel = ViewModel;
    type Effect = Effect;

    fn update(&self, event: Event, model: &mut Model) -> Command<Effect, Event> {
        match event {
            Event::SelectCategory(category) => model.selected_category = category,
            Event::OpenProduct(index) => {
                model.open_product = Some(index as usize);
                model.quantity = 1;
                model.sweetness = 50;
            }
            Event::CloseProduct => model.open_product = None,
            Event::SliderChanged { id, value } => {
                if id == "sweetness" {
                    model.sweetness = value.clamp(0, 100);
                }
            }
            Event::IncQty => model.quantity += 1,
            Event::DecQty => model.quantity = (model.quantity - 1).max(1),
        }
        render()
    }

    fn view(&self, model: &Model) -> ViewModel {
        match model.open_product.and_then(|i| model.products.get(i)) {
            Some(product) => detail_view(product, model),
            None => storefront_view(model),
        }
    }
}

fn storefront_view(model: &Model) -> Widget {
    let hero = Widget::Box {
        align: BoxAlign::BottomStart,
        scrim: true,
        children: vec![
            Widget::Image { source: HERO_IMAGE.to_string(), shape: ImageShape::Rounded, ratio: ImageRatio::Wide },
            Widget::Column {
                children: vec![
                    Widget::Text { content: "Fall in Love with Coffee".to_string() },
                    Widget::Button { label: "Get Started".to_string(), on_press: Event::SelectCategory("All".to_string()) },
                ],
            },
        ],
    };

    let chips = Widget::Row {
        children: model
            .categories
            .iter()
            .map(|c| Widget::Chip {
                label: (*c).to_string(),
                selected: model.selected_category.as_str() == *c,
                on_press: Event::SelectCategory((*c).to_string()),
            })
            .collect(),
    };

    let grid = Widget::Grid {
        children: model.visible_products().iter().map(|(i, p)| product_card(*i, p)).collect(),
    };

    Widget::Column { children: vec![hero, chips, grid] }
}

fn product_card(index: usize, p: &Product) -> Widget {
    Widget::Card {
        on_press: Some(Event::OpenProduct(index as u32)),
        child: Box::new(Widget::Column {
            children: vec![
                Widget::Image { source: p.image.to_string(), shape: ImageShape::Rounded, ratio: ImageRatio::Square },
                Widget::Text { content: p.name.to_string() },
                Widget::Row {
                    children: vec![
                        Widget::Text { content: p.price.to_string() },
                        Widget::Text { content: format!("★ {}", p.rating) },
                    ],
                },
            ],
        }),
    }
}

fn detail_view(p: &Product, model: &Model) -> Widget {
    Widget::Column {
        children: vec![
            Widget::Button { label: "← Back".to_string(), on_press: Event::CloseProduct },
            Widget::Image { source: p.image.to_string(), shape: ImageShape::Rounded, ratio: ImageRatio::Wide },
            Widget::Text { content: p.name.to_string() },
            Widget::Text { content: format!("★ {}    {}", p.rating, p.price) },
            Widget::Text { content: p.description.to_string() },
            Widget::Text { content: format!("Sweetness: {}%", model.sweetness) },
            Widget::Slider { id: "sweetness".to_string(), value: model.sweetness, max: 100 },
            Widget::Row {
                children: vec![
                    Widget::Text { content: "Quantity".to_string() },
                    Widget::Stepper { value: model.quantity, on_decrement: Event::DecQty, on_increment: Event::IncQty },
                ],
            },
            Widget::Button { label: format!("Add {} to cart · {}", model.quantity, p.price), on_press: Event::CloseProduct },
        ],
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn defaults_to_storefront_all_products() {
        let model = Model::default();
        assert!(model.open_product.is_none());
        assert_eq!(model.selected_category, "All");
        assert_eq!(model.visible_products().len(), model.products.len());
    }

    #[test]
    fn selecting_category_filters_products() {
        let app = CoffeeApp;
        let mut model = Model::default();
        app.update(Event::SelectCategory("Latte".to_string()), &mut model).expect_only_render();
        let visible = model.visible_products();
        assert!(!visible.is_empty());
        assert!(visible.iter().all(|(_, p)| p.category == "Latte"));
    }

    #[test]
    fn open_close_and_detail_controls() {
        let app = CoffeeApp;
        let mut model = Model::default();
        app.update(Event::OpenProduct(2), &mut model).expect_only_render();
        assert_eq!(model.open_product, Some(2));
        assert_eq!(model.quantity, 1);

        app.update(Event::IncQty, &mut model);
        app.update(Event::IncQty, &mut model);
        app.update(Event::DecQty, &mut model);
        assert_eq!(model.quantity, 2);

        app.update(Event::SliderChanged { id: "sweetness".to_string(), value: 80 }, &mut model);
        assert_eq!(model.sweetness, 80);

        app.update(Event::CloseProduct, &mut model);
        assert!(model.open_product.is_none());
    }
}
