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
}

pub struct Model {
    products: Vec<Product>,
    categories: Vec<&'static str>,
    selected_category: String,
}

impl Default for Model {
    fn default() -> Self {
        Self {
            products: vec![
                Product { name: "Caffè Mocha", price: "$4.53", rating: "4.8", category: "Latte", image: "https://loremflickr.com/400/400/coffee?lock=1" },
                Product { name: "Flat White", price: "$3.53", rating: "4.6", category: "Latte", image: "https://loremflickr.com/400/400/coffee?lock=2" },
                Product { name: "Espresso", price: "$2.20", rating: "4.9", category: "Espresso", image: "https://loremflickr.com/400/400/espresso?lock=3" },
                Product { name: "Cappuccino", price: "$3.80", rating: "4.5", category: "Espresso", image: "https://loremflickr.com/400/400/cappuccino?lock=4" },
                Product { name: "Cold Brew", price: "$4.10", rating: "4.7", category: "Cold", image: "https://loremflickr.com/400/400/coldbrew?lock=5" },
                Product { name: "Iced Latte", price: "$4.20", rating: "4.4", category: "Cold", image: "https://loremflickr.com/400/400/icedcoffee?lock=6" },
            ],
            categories: vec!["All", "Latte", "Espresso", "Cold"],
            selected_category: "All".to_string(),
        }
    }
}

impl Model {
    /// Products matching the selected category ("All" shows everything).
    fn visible_products(&self) -> Vec<&Product> {
        self.products
            .iter()
            .filter(|p| self.selected_category == "All" || p.category == self.selected_category.as_str())
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
    Card { child: Box<Widget> },
    /// Selectable category pill.
    Chip { label: String, selected: bool, on_press: Event },
    /// Fixed 2-column grid; children flow left-to-right, top-to-bottom.
    Grid { children: Vec<Widget> },
    /// Remote image loaded by URL. `shape` controls corners, `ratio` the aspect.
    Image { source: String, shape: ImageShape, ratio: ImageRatio },
    /// Z-stack: children layered back-to-front, positioned by `align`. With
    /// `scrim`, the first child is treated as a background image, darkened for
    /// legibility, and the rest render on top with light content.
    Box {
        children: Vec<Widget>,
        align: BoxAlign,
        scrim: bool,
    },
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
        }
        render()
    }

    fn view(&self, model: &Model) -> ViewModel {
        let hero = Widget::Box {
            align: BoxAlign::BottomStart,
            scrim: true,
            children: vec![
                Widget::Image {
                    source: HERO_IMAGE.to_string(),
                    shape: ImageShape::Rounded,
                    ratio: ImageRatio::Wide,
                },
                Widget::Column {
                    children: vec![
                        Widget::Text {
                            content: "Fall in Love with Coffee".to_string(),
                        },
                        Widget::Button {
                            label: "Get Started".to_string(),
                            on_press: Event::SelectCategory("All".to_string()),
                        },
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
            children: model.visible_products().iter().map(|p| product_card(p)).collect(),
        };

        Widget::Column {
            children: vec![hero, chips, grid],
        }
    }
}

fn product_card(p: &Product) -> Widget {
    Widget::Card {
        child: Box::new(Widget::Column {
            children: vec![
                Widget::Image {
                    source: p.image.to_string(),
                    shape: ImageShape::Rounded,
                    ratio: ImageRatio::Square,
                },
                Widget::Text {
                    content: p.name.to_string(),
                },
                Widget::Row {
                    children: vec![
                        Widget::Text {
                            content: p.price.to_string(),
                        },
                        Widget::Text {
                            content: format!("★ {}", p.rating),
                        },
                    ],
                },
            ],
        }),
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn defaults_to_all_products() {
        let model = Model::default();
        assert_eq!(model.selected_category, "All");
        assert_eq!(model.visible_products().len(), model.products.len());
    }

    #[test]
    fn selecting_category_filters_products() {
        let app = CoffeeApp;
        let mut model = Model::default();
        app.update(Event::SelectCategory("Latte".to_string()), &mut model)
            .expect_only_render();
        let visible = model.visible_products();
        assert!(!visible.is_empty());
        assert!(visible.iter().all(|p| p.category == "Latte"));
    }
}
