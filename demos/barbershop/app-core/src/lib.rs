//! Fade House — a barbershop/grooming booking app. A Mobiler showcase for the broader UI
//! vocabulary: an icon bottom-tab bar, a floating action button, the expanded icon set, and
//! the theming engine (a warm brass brand on a dark shell). Logic + UI in Rust; rendered by
//! the generic shells on web (here) and native.

use mobiler_core::{
    BoxAlign, ButtonStyle, CardStyle, Corner, Cx, Density, FontFamily, Icon, ImageRatio,
    ImageShape, InputValue, MobilerApp, MobilerShell, Rgb, Spacing, Theme, Tone, Widget, avatar_status,
    badge, button, caption, card, card_button, chip, column, divider, emphasis, grid, icon_button,
    image, rating, rating_input, row, scaffold, scroller, search_field, segment, segmented, spacer,
    stack, subtitle, tab_icon, text, title, with_fab, with_sheet, with_theme,
};
use serde::{Deserialize, Serialize};

const HERO: &str = "https://images.unsplash.com/photo-1503951914875-452162b0f3f1?w=1200&q=80";

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tab {
    Home,
    Services,
    Bookings,
    Profile,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Audience {
    Men,
    Women,
    Kids,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum Msg {
    SelectTab(Tab),
    SelectCategory(String),
    SelectAudience(Audience),
    OpenService(u32),
    CloseSheet,
    Rate(u8),
    ConfirmBooking,
    Notifications,
    Book,
}

#[derive(Clone)]
struct Service {
    name: &'static str,
    price: &'static str,
    rating: &'static str,
    category: &'static str,
    image: &'static str,
}

#[derive(Clone)]
struct Barber {
    name: &'static str,
    specialty: &'static str,
    rating: &'static str,
    image: &'static str,
}

pub struct Model {
    tab: Tab,
    category: String,
    audience: Audience,
    search: String,
    services: Vec<Service>,
    barbers: Vec<Barber>,
    /// Index of the service whose booking sheet is open (`None` = closed).
    open_service: Option<usize>,
    /// Stars the user tapped in the booking sheet, in tenths (0 = unrated).
    user_rating: u32,
}

impl Default for Model {
    fn default() -> Self {
        Self {
            tab: Tab::Home,
            category: "All".to_string(),
            audience: Audience::Men,
            search: String::new(),
            services: vec![
                Service { name: "Classic Cut", price: "$28", rating: "4.9", category: "Hair", image: "https://loremflickr.com/400/400/haircut?lock=1" },
                Service { name: "Skin Fade", price: "$32", rating: "4.8", category: "Hair", image: "https://loremflickr.com/400/400/barber?lock=2" },
                Service { name: "Beard Trim", price: "$18", rating: "4.7", category: "Beard", image: "https://loremflickr.com/400/400/beard?lock=3" },
                Service { name: "Hot Towel Shave", price: "$24", rating: "4.9", category: "Beard", image: "https://loremflickr.com/400/400/shave?lock=4" },
                Service { name: "Cut + Beard", price: "$42", rating: "5.0", category: "Combo", image: "https://loremflickr.com/400/400/grooming?lock=5" },
                Service { name: "Kids Cut", price: "$20", rating: "4.6", category: "Hair", image: "https://loremflickr.com/400/400/kidshaircut?lock=6" },
            ],
            barbers: vec![
                Barber { name: "Marco", specialty: "Fades & tapers", rating: "4.9", image: "https://loremflickr.com/200/200/barber,man?lock=11" },
                Barber { name: "Dev", specialty: "Beard sculpting", rating: "4.8", image: "https://loremflickr.com/200/200/man,beard?lock=12" },
                Barber { name: "Iris", specialty: "Classic cuts", rating: "5.0", image: "https://loremflickr.com/200/200/hairstylist?lock=13" },
                Barber { name: "Theo", specialty: "Hot shaves", rating: "4.7", image: "https://loremflickr.com/200/200/barbershop?lock=14" },
            ],
            open_service: None,
            user_rating: 0,
        }
    }
}

/// Parse a "4.8"-style rating into tenths (48) for the `rating` widget.
fn tenths(s: &str) -> u32 {
    (s.parse::<f32>().unwrap_or(0.0) * 10.0).round() as u32
}

#[derive(Default)]
pub struct FadeHouse;

impl MobilerApp for FadeHouse {
    type Event = Msg;
    type Model = Model;

    fn update(&self, msg: Msg, model: &mut Model, cx: &mut Cx<Msg>) {
        match msg {
            Msg::SelectTab(t) => model.tab = t,
            Msg::SelectCategory(c) => model.category = c,
            Msg::SelectAudience(a) => model.audience = a,
            Msg::OpenService(i) => {
                model.open_service = Some(i as usize);
                model.user_rating = 0;
            }
            Msg::CloseSheet => model.open_service = None,
            Msg::Rate(stars) => model.user_rating = u32::from(stars) * 10,
            Msg::ConfirmBooking => {
                let name = model.open_service.and_then(|i| model.services.get(i)).map(|s| s.name).unwrap_or("");
                cx.toast(format!("Booked “{name}” ✓"));
                model.open_service = None;
            }
            Msg::Notifications => cx.toast("No new notifications"),
            Msg::Book => cx.toast("Pick a date & time — coming next"),
        }
    }

    fn input(&self, id: &str, value: InputValue, model: &mut Model, _cx: &mut Cx<Msg>) {
        if id == "search" {
            if let InputValue::Text(t) = value {
                model.search = t;
            }
        }
    }

    fn view(&self, model: &Model) -> Widget {
        // Brand: warm brass on a dark shell — the classic barbershop look.
        let theme = Theme {
            seed: Rgb::new(0xC8, 0x8A, 0x3C),
            corner: Corner::Medium,
            density: Density::Comfortable,
            font: FontFamily::System,
        };
        let tabs = vec![
            tab_icon("Home", Icon::Home, model.tab == Tab::Home, Msg::SelectTab(Tab::Home)),
            tab_icon("Services", Icon::Scissors, model.tab == Tab::Services, Msg::SelectTab(Tab::Services)),
            tab_icon("Bookings", Icon::Calendar, model.tab == Tab::Bookings, Msg::SelectTab(Tab::Bookings)),
            tab_icon("Profile", Icon::Person, model.tab == Tab::Profile, Msg::SelectTab(Tab::Profile)),
        ];
        let (title_text, body) = match model.tab {
            Tab::Home => ("Fade House", home(model)),
            Tab::Services => ("Services", services_screen(model)),
            Tab::Bookings => ("Bookings", bookings_screen()),
            Tab::Profile => ("Profile", profile_screen()),
        };
        // Themed Scaffold + icon tab bar + a "book now" floating action button.
        let mut root = with_fab(scaffold(title_text, true, tabs, body), Icon::Calendar, Msg::Book);
        // Tapping a service opens a booking bottom sheet (Sheet).
        if let Some(s) = model.open_service.and_then(|i| model.services.get(i)) {
            root = with_sheet(root, format!("Book {}", s.name), booking_sheet(s, model.user_rating), Msg::CloseSheet);
        }
        with_theme(root, theme)
    }
}

fn booking_sheet(s: &Service, user_rating: u32) -> Widget {
    column(vec![
        row(vec![
            image(s.image, ImageShape::Rounded, ImageRatio::Square),
            column(vec![title(s.name), text(s.price), rating(tenths(s.rating), 5)]),
        ]),
        spacer(Spacing::Sm),
        emphasis("Rate your last visit"),
        // Tappable star rating (Rating with on_rate) — one event per star.
        rating_input(user_rating, 5, vec![Msg::Rate(1), Msg::Rate(2), Msg::Rate(3), Msg::Rate(4), Msg::Rate(5)]),
        spacer(Spacing::Sm),
        button("Confirm booking", ButtonStyle::Filled, Msg::ConfirmBooking),
    ])
}

fn barbers_scroller(model: &Model) -> Widget {
    // "Our barbers" — a horizontally-scrolling rail (Scroller) of Avatars.
    scroller(
        model
            .barbers
            .iter()
            .map(|b| {
                column(vec![
                    avatar_status(b.image, Tone::Success),
                    emphasis(b.name),
                    caption(b.specialty),
                    rating(tenths(b.rating), 5),
                ])
            })
            .collect(),
    )
}

fn category_carousel(model: &Model) -> Widget {
    // A horizontally-scrolling chip rail (Scroller) — more categories than fit on one row.
    let categories = ["All", "Hair", "Beard", "Combo", "Shave", "Kids", "Color"];
    scroller(
        categories
            .iter()
            .map(|c| chip((*c).to_string(), model.category.as_str() == *c, Msg::SelectCategory((*c).to_string())))
            .collect(),
    )
}

fn audience_segmented(model: &Model) -> Widget {
    // A single-choice segmented control (Segmented).
    segmented(vec![
        segment("Men", model.audience == Audience::Men, Msg::SelectAudience(Audience::Men)),
        segment("Women", model.audience == Audience::Women, Msg::SelectAudience(Audience::Women)),
        segment("Kids", model.audience == Audience::Kids, Msg::SelectAudience(Audience::Kids)),
    ])
}

fn home(model: &Model) -> Widget {
    let hero = stack(
        BoxAlign::BottomStart,
        true,
        vec![
            image(HERO, ImageShape::Rounded, ImageRatio::Wide),
            column(vec![
                title("Look sharp."),
                caption("Top barbers near you — book in seconds."),
                button("Book a cut", ButtonStyle::Filled, Msg::Book),
            ]),
        ],
    );
    column(vec![
        row(vec![
            column(vec![caption("Welcome back"), emphasis("Marcus")]),
            spacer(Spacing::Md),
            icon_button(Icon::Bell, Msg::Notifications),
        ]),
        // Search bar (SearchField) — emits Input { id: "search", … }.
        search_field("search", "Search services…", model.search.as_str()),
        hero,
        audience_segmented(model),
        category_carousel(model),
        subtitle("Our barbers"),
        barbers_scroller(model),
        subtitle("Popular services"),
        services_grid(model),
    ])
}

fn services_screen(model: &Model) -> Widget {
    column(vec![
        search_field("search", "Search services…", model.search.as_str()),
        audience_segmented(model),
        category_carousel(model),
        spacer(Spacing::Sm),
        services_grid(model),
    ])
}

fn services_grid(model: &Model) -> Widget {
    let cat = model.category.as_str();
    let q = model.search.to_lowercase();
    let cards: Vec<Widget> = model
        .services
        .iter()
        .enumerate()
        // Kids audience narrows to kids services; Men/Women show the full menu.
        .filter(|(_, s)| model.audience != Audience::Kids || s.name.contains("Kids"))
        .filter(|(_, s)| cat == "All" || s.category == cat)
        .filter(|(_, s)| q.is_empty() || s.name.to_lowercase().contains(&q))
        .map(|(i, s)| service_card(i as u32, s))
        .collect();
    if cards.is_empty() {
        return card(caption("No services match your search."), CardStyle::Outlined);
    }
    grid(cards)
}

fn service_card(index: u32, s: &Service) -> Widget {
    card_button(
        column(vec![
            image(s.image, ImageShape::Rounded, ImageRatio::Square),
            emphasis(s.name),
            row(vec![text(s.price), rating(tenths(s.rating), 5)]),
            badge(s.category, Tone::Info),
        ]),
        CardStyle::Filled,
        Msg::OpenService(index),
    )
}

fn bookings_screen() -> Widget {
    column(vec![
        subtitle("Upcoming"),
        card(
            column(vec![
                emphasis("No upcoming bookings"),
                caption("Tap the calendar button to book your next visit."),
                button("Book now", ButtonStyle::Filled, Msg::Book),
            ]),
            CardStyle::Outlined,
        ),
    ])
}

fn profile_screen() -> Widget {
    column(vec![
        subtitle("Marcus Reed"),
        caption("marcus@example.com"),
        divider(),
        row(vec![icon_button(Icon::Person, Msg::SelectTab(Tab::Profile)), text("Account")]),
        row(vec![icon_button(Icon::Bell, Msg::Notifications), text("Notifications")]),
        row(vec![icon_button(Icon::Heart, Msg::SelectTab(Tab::Profile)), text("Favorites")]),
        row(vec![icon_button(Icon::Settings, Msg::SelectTab(Tab::Profile)), text("Settings")]),
    ])
}

pub type App = MobilerShell<FadeHouse>;

#[cfg(test)]
mod test {
    use super::*;

    fn app() -> (FadeHouse, Model) {
        (FadeHouse, Model::default())
    }

    #[test]
    fn starts_on_home_with_all_services() {
        let (_, model) = app();
        assert_eq!(model.tab, Tab::Home);
        assert_eq!(model.services.len(), 6);
    }

    #[test]
    fn category_filter_narrows_the_grid() {
        let (app, mut model) = app();
        let mut cx = Cx::<Msg>::default();
        app.update(Msg::SelectCategory("Beard".into()), &mut model, &mut cx);
        let beard = model.services.iter().filter(|s| s.category == "Beard").count();
        assert_eq!(beard, 2);
        assert_eq!(model.category, "Beard");
    }

    #[test]
    fn select_tab_switches_screen() {
        let (app, mut model) = app();
        let mut cx = Cx::<Msg>::default();
        app.update(Msg::SelectTab(Tab::Services), &mut model, &mut cx);
        assert_eq!(model.tab, Tab::Services);
    }

    #[test]
    fn open_service_opens_sheet_rate_then_confirm_closes() {
        let (app, mut model) = app();
        let mut cx = Cx::<Msg>::default();
        app.update(Msg::OpenService(0), &mut model, &mut cx);
        assert_eq!(model.open_service, Some(0));
        app.update(Msg::Rate(4), &mut model, &mut cx);
        assert_eq!(model.user_rating, 40);
        app.update(Msg::ConfirmBooking, &mut model, &mut cx);
        assert_eq!(model.open_service, None, "confirming closes the sheet");
    }

    #[test]
    fn search_input_updates_query_and_audience_switches() {
        let (app, mut model) = app();
        let mut cx = Cx::<Msg>::default();
        app.input("search", InputValue::Text("beard".into()), &mut model, &mut cx);
        assert_eq!(model.search, "beard");
        app.update(Msg::SelectAudience(Audience::Kids), &mut model, &mut cx);
        assert_eq!(model.audience, Audience::Kids);
    }
}
