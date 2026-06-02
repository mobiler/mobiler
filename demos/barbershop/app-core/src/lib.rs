//! Fade House — a barbershop/grooming booking app. A Mobiler showcase for the broader UI
//! vocabulary: an icon bottom-tab bar, a floating action button, the expanded icon set, and
//! the theming engine (a warm brass brand on a dark shell). Logic + UI in Rust; rendered by
//! the generic shells on web (here) and native.

use mobiler_core::{
    BoxAlign, ButtonStyle, CardStyle, Corner, Cx, Density, FontFamily, Icon, ImageRatio,
    ImageShape, InputValue, MobilerApp, MobilerShell, Rgb, Spacing, Theme, Tone, Widget, avatar_status,
    badge, button, caption, card, card_button, chip, column, divider, emphasis, grid, icon_button,
    image, progress, rating, rating_input, row, scaffold, scroller, search_field, segment, segmented,
    skeleton, spacer, stack, subtitle, tab_icon, text, title, with_fab, with_sheet, with_theme,
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
    /// The date picked for a booking (empty string = the user cancelled the picker).
    DatePicked(String),
    /// The time picked for a booking (empty string = the user cancelled the picker).
    TimePicked(String),
    /// The result of the final confirm dialog (`true` = confirmed).
    BookingDone(bool),

    // --- Native capability demos (free bundled plugins via `mobiler plugin add`) ---
    /// Pick a client for the booking from the system contact picker (`contacts` plugin).
    PickClient,
    GotClient(String),
    /// Add the booking to the device calendar via the system editor (`calendar` plugin).
    AddToCalendar,
    CalendarDone(String),
    /// Find the nearest shop using the device location (`geolocation` plugin).
    FindNearest,
    GotLocation(String),
    /// Check network connectivity (`connectivity` plugin).
    CheckSignal,
    GotSignal(String),
    /// Read the accelerometer (`sensors` plugin).
    ReadMotion,
    GotMotion(String),
    /// Record 3s from the mic then play it back (`audio` plugin).
    RecordAudio,
    Recorded(String),
    PlayAudio,
    Played(String),

    // --- "Get in touch" + share/voice/review (composer / tts / review / sharefile / video) ---
    /// Email the shop via the system mail composer (`composer` plugin).
    EmailShop,
    /// Call the shop via the system dialer (`composer` plugin).
    CallShop,
    /// Speak the next-booking summary aloud (`tts` plugin).
    SpeakBooking,
    /// Ask for an App Store / Play review (`review` plugin).
    RequestReview,
    /// Record a short video with the camera (`video` plugin).
    RecordVideo,
    VideoRecorded(String),
    /// Share the last recorded clip via the system share sheet (`sharefile` plugin).
    ShareClip,
    Shared(String),
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
    /// Date chosen during the FAB "book a cut" flow (`cx.pick_date` → `cx.pick_time`).
    pending_date: Option<String>,
    /// Time chosen during the FAB "book a cut" flow.
    pending_time: Option<String>,
    /// Client picked from contacts ("name|phone"), shown in the booking sheet.
    client: String,
    /// Nearest-shop location ("lat,lng") from geolocation.
    location: String,
    /// Network status from the connectivity plugin.
    signal: String,
    /// Last result line for the Profile "Device & capabilities" panel (sensors/audio).
    device: String,
    /// URI of the last recorded clip (enables Play).
    last_audio: Option<String>,
    /// URI of the last recorded video (enables Share).
    last_video: Option<String>,
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
            pending_date: None,
            pending_time: None,
            client: String::new(),
            location: String::new(),
            signal: String::new(),
            device: String::new(),
            last_audio: None,
            last_video: None,
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
            // The FAB "book a cut" flow: native date picker → native time picker →
            // confirm dialog. Each step is a request/response capability; the response
            // comes back as the next Msg, and an empty string means the user cancelled.
            Msg::Book => {
                model.pending_date = None;
                model.pending_time = None;
                cx.pick_date(|r| Msg::DatePicked(if r.ok { r.output } else { String::new() }));
            }
            Msg::DatePicked(date) => {
                if date.is_empty() {
                    return; // cancelled the date picker
                }
                model.pending_date = Some(date);
                cx.pick_time(|r| Msg::TimePicked(if r.ok { r.output } else { String::new() }));
            }
            Msg::TimePicked(time) => {
                if time.is_empty() {
                    model.pending_date = None; // cancelled the time picker
                    return;
                }
                model.pending_time = Some(time.clone());
                let date = model.pending_date.clone().unwrap_or_default();
                cx.confirm("Confirm booking", format!("Book your visit for {date} at {time}?"), |r| Msg::BookingDone(r.ok));
            }
            Msg::BookingDone(ok) => {
                if ok {
                    let date = model.pending_date.clone().unwrap_or_default();
                    let time = model.pending_time.clone().unwrap_or_default();
                    cx.toast(format!("Booked for {date} at {time} ✓"));
                }
                model.pending_date = None;
                model.pending_time = None;
            }

            // --- Native capability demos ---
            Msg::PickClient => cx.plugin("contacts", "pick", "", |r| {
                Msg::GotClient(if r.ok { r.output } else { String::new() })
            }),
            Msg::GotClient(c) => {
                if !c.is_empty() {
                    model.client = c;
                }
            }
            Msg::AddToCalendar => {
                let title = model.open_service.and_then(|i| model.services.get(i)).map_or_else(
                    || "Fade House appointment".to_string(),
                    |s| format!("Fade House — {}", s.name),
                );
                let notes = if model.client.is_empty() {
                    "Booked via Fade House".to_string()
                } else {
                    format!("Client: {}", model.client)
                };
                let input = serde_json::json!({ "title": title, "notes": notes }).to_string();
                cx.plugin("calendar", "add", input, |r| Msg::CalendarDone(r.output));
            }
            Msg::CalendarDone(s) => cx.toast(match s.as_str() {
                "saved" | "opened" => "Added to your calendar ✓".to_string(),
                _ => "Calendar not updated".to_string(),
            }),
            Msg::FindNearest => cx.plugin("geolocation", "get", "", |r| {
                Msg::GotLocation(if r.ok { r.output } else { String::new() })
            }),
            Msg::GotLocation(loc) => {
                if loc.is_empty() {
                    cx.toast("Location unavailable — grant permission and retry");
                } else {
                    model.location = loc;
                    cx.toast("Found shops near you ✓");
                }
            }
            Msg::CheckSignal => cx.plugin("connectivity", "status", "", |r| Msg::GotSignal(r.output)),
            Msg::GotSignal(s) => model.signal = s,
            Msg::ReadMotion => cx.plugin("sensors", "read", "accelerometer", |r| {
                Msg::GotMotion(if r.ok { r.output } else { String::new() })
            }),
            Msg::GotMotion(m) => {
                model.device = if m.is_empty() { "accelerometer: unavailable".into() } else { format!("accelerometer: {m}") };
            }
            Msg::RecordAudio => {
                model.device = "Recording 3s…".into();
                cx.plugin("audio", "record", "3", |r| {
                    Msg::Recorded(if r.ok { r.output } else { String::new() })
                });
            }
            Msg::Recorded(uri) => {
                if uri.is_empty() {
                    model.device = "Record failed (grant mic permission, then retry)".into();
                } else {
                    model.last_audio = Some(uri);
                    model.device = "Recorded 3s ✓ — tap Play".into();
                }
            }
            Msg::PlayAudio => {
                if let Some(uri) = model.last_audio.clone() {
                    cx.plugin("audio", "play", uri, |r| Msg::Played(r.output));
                } else {
                    cx.toast("Record something first");
                }
            }
            Msg::Played(s) => model.device = format!("Playback: {s}"),

            // --- Get in touch / voice / review / share (composer, tts, review, sharefile, video) ---
            Msg::EmailShop => {
                let input = serde_json::json!({
                    "to": "hello@fadehouse.example",
                    "subject": "Booking enquiry",
                    "body": "Hi Fade House,\n\nI'd like to book an appointment.",
                })
                .to_string();
                cx.plugin("composer", "email", input, |r| {
                    Msg::Played(if r.ok { "Email composer opened".into() } else { "No mail app available".into() })
                });
            }
            Msg::CallShop => {
                let input = serde_json::json!({ "number": "+15551234567" }).to_string();
                cx.plugin("composer", "call", input, |r| {
                    Msg::Played(if r.ok { "Dialer opened".into() } else { "Can't place call".into() })
                });
            }
            Msg::SpeakBooking => {
                let text = match (model.pending_date.as_deref(), model.pending_time.as_deref()) {
                    (Some(d), Some(t)) => format!("Your next visit is on {d} at {t}."),
                    _ => "You have no upcoming bookings. Tap the calendar button to book a cut.".to_string(),
                };
                cx.plugin("tts", "speak", text, |_| Msg::Played("Spoke your booking".into()));
            }
            Msg::RequestReview => cx.plugin("review", "request", "", |_| Msg::Played("Thanks for rating us!".into())),
            Msg::RecordVideo => {
                model.device = "Opening camera…".into();
                cx.plugin("video", "record", "", |r| {
                    Msg::VideoRecorded(if r.ok { r.output } else { String::new() })
                });
            }
            Msg::VideoRecorded(uri) => {
                if uri.is_empty() {
                    model.device = "No video recorded".into();
                } else {
                    model.last_video = Some(uri);
                    model.device = "Recorded a clip ✓ — tap Share clip".into();
                }
            }
            Msg::ShareClip => {
                if let Some(uri) = model.last_video.clone().or_else(|| model.last_audio.clone()) {
                    cx.plugin("sharefile", "file", uri, |r| Msg::Shared(r.output));
                } else {
                    cx.toast("Record a video or audio clip first");
                }
            }
            Msg::Shared(s) => cx.toast(match s.as_str() {
                "shared" => "Shared ✓".to_string(),
                _ => "Share cancelled".to_string(),
            }),
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
            accent: Some(Rgb::new(0xE0, 0x6A, 0x2C)), // warm orange — for the brand gradient
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
            Tab::Profile => ("Profile", profile_screen(model)),
        };
        // Themed Scaffold + icon tab bar + a "book now" floating action button.
        let mut root = with_fab(scaffold(title_text, true, tabs, body), Icon::Calendar, Msg::Book);
        // Tapping a service opens a booking bottom sheet (Sheet).
        if let Some(s) = model.open_service.and_then(|i| model.services.get(i)) {
            root = with_sheet(root, format!("Book {}", s.name), booking_sheet(s, model.user_rating, &model.client), Msg::CloseSheet);
        }
        with_theme(root, theme)
    }
}

fn booking_sheet(s: &Service, user_rating: u32, client: &str) -> Widget {
    // Client line: pick from the device contacts (contacts plugin).
    let client_line = if client.is_empty() {
        button("Pick client from contacts", ButtonStyle::Outlined, Msg::PickClient)
    } else {
        row(vec![emphasis(format!("Client: {client}")), button("Change", ButtonStyle::Text, Msg::PickClient)])
    };
    column(vec![
        row(vec![
            image(s.image, ImageShape::Rounded, ImageRatio::Square),
            column(vec![title(s.name), text(s.price), rating(tenths(s.rating), 5)]),
        ]),
        spacer(Spacing::Sm),
        client_line,
        spacer(Spacing::Sm),
        emphasis("Rate your last visit"),
        // Tappable star rating (Rating with on_rate) — one event per star.
        rating_input(user_rating, 5, vec![Msg::Rate(1), Msg::Rate(2), Msg::Rate(3), Msg::Rate(4), Msg::Rate(5)]),
        spacer(Spacing::Sm),
        row(vec![
            button("Confirm booking", ButtonStyle::Filled, Msg::ConfirmBooking),
            // Add the appointment to the device calendar (calendar plugin).
            button("Add to calendar", ButtonStyle::Outlined, Msg::AddToCalendar),
        ]),
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
    // Native: nearest-shop (geolocation) + connection status (connectivity).
    let nearby = if model.location.is_empty() && model.signal.is_empty() {
        caption("Find shops near you, or check your connection.")
    } else {
        let mut parts = Vec::new();
        if !model.location.is_empty() {
            parts.push(format!("📍 {}", model.location));
        }
        if !model.signal.is_empty() {
            parts.push(format!("signal: {}", model.signal));
        }
        caption(parts.join("   ·   "))
    };
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
        // Brand-gradient promo banner (CardStyle::Brand — fills seed → accent).
        card_button(
            column(vec![title("20% off your first cut"), caption("New here? Book today and save.")]),
            CardStyle::Brand,
            Msg::Book,
        ),
        row(vec![
            button("Find nearest", ButtonStyle::Outlined, Msg::FindNearest),
            button("Check signal", ButtonStyle::Text, Msg::CheckSignal),
        ]),
        nearby,
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

/// Profile completeness 0.0–1.0 — grows as the visitor exercises the app's capabilities.
/// Drives the determinate `Progress` bar on the Profile screen.
fn completeness(model: &Model) -> f32 {
    let mut v = 0.25_f32;
    if !model.client.is_empty() {
        v += 0.25;
    }
    if !model.location.is_empty() {
        v += 0.25;
    }
    if !model.device.is_empty() {
        v += 0.25;
    }
    v.min(1.0)
}

fn profile_screen(model: &Model) -> Widget {
    let pct = (completeness(model) * 100.0).round() as u32;
    column(vec![
        subtitle("Marcus Reed"),
        caption("marcus@example.com"),
        // Determinate Progress bar — "profile completeness" grows as capabilities are tried.
        caption(format!("Profile {pct}% complete")),
        progress(Some(completeness(model))),
        divider(),
        row(vec![icon_button(Icon::Person, Msg::SelectTab(Tab::Profile)), text("Account")]),
        row(vec![icon_button(Icon::Bell, Msg::Notifications), text("Notifications")]),
        row(vec![icon_button(Icon::Heart, Msg::SelectTab(Tab::Profile)), text("Favorites")]),
        row(vec![icon_button(Icon::Settings, Msg::SelectTab(Tab::Profile)), text("Settings")]),
        spacer(Spacing::Md),
        // Device capability demos (free bundled plugins: sensors + audio).
        card(
            column(vec![
                emphasis("Device & capabilities"),
                row(vec![
                    button("Read accelerometer", ButtonStyle::Outlined, Msg::ReadMotion),
                ]),
                row(vec![
                    button("Record 3s", ButtonStyle::Outlined, Msg::RecordAudio),
                    button("Play", ButtonStyle::Text, Msg::PlayAudio),
                ]),
                caption(if model.device.is_empty() { "Tap a capability to try it on device.".to_string() } else { model.device.clone() }),
            ]),
            CardStyle::Outlined,
        ),
        // Get in touch + voice + share (free bundled plugins: composer / tts / review / video / sharefile).
        card(
            column(vec![
                emphasis("Get in touch"),
                row(vec![
                    button("Email the shop", ButtonStyle::Outlined, Msg::EmailShop),
                    button("Call", ButtonStyle::Text, Msg::CallShop),
                ]),
                row(vec![
                    button("Read my booking aloud", ButtonStyle::Outlined, Msg::SpeakBooking),
                ]),
                row(vec![
                    button("Record a clip", ButtonStyle::Outlined, Msg::RecordVideo),
                    button("Share clip", ButtonStyle::Text, Msg::ShareClip),
                ]),
                button("Rate Fade House", ButtonStyle::Text, Msg::RequestReview),
            ]),
            CardStyle::Outlined,
        ),
        // Skeleton placeholders — the shimmer shown while content streams in.
        card(
            column(vec![
                emphasis("Loyalty"),
                caption("Syncing your points…"),
                skeleton(),
                skeleton(),
            ]),
            CardStyle::Outlined,
        ),
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
    fn book_flow_holds_then_clears_pending_datetime() {
        let (app, mut model) = app();
        let mut cx = Cx::<Msg>::default();
        app.update(Msg::Book, &mut model, &mut cx);
        app.update(Msg::DatePicked("2026-06-10".into()), &mut model, &mut cx);
        assert_eq!(model.pending_date.as_deref(), Some("2026-06-10"));
        app.update(Msg::TimePicked("14:30".into()), &mut model, &mut cx);
        assert_eq!(model.pending_time.as_deref(), Some("14:30"));
        // Confirming clears the pending slots (the toast fires via cx).
        app.update(Msg::BookingDone(true), &mut model, &mut cx);
        assert_eq!(model.pending_date, None);
        assert_eq!(model.pending_time, None);
    }

    #[test]
    fn book_flow_cancelling_a_picker_aborts_cleanly() {
        let (app, mut model) = app();
        let mut cx = Cx::<Msg>::default();
        // Cancel the date picker (empty string) — nothing is held.
        app.update(Msg::DatePicked(String::new()), &mut model, &mut cx);
        assert_eq!(model.pending_date, None);
        // Cancel the time picker after a date — the date is dropped too.
        app.update(Msg::DatePicked("2026-06-10".into()), &mut model, &mut cx);
        app.update(Msg::TimePicked(String::new()), &mut model, &mut cx);
        assert_eq!(model.pending_date, None);
        assert_eq!(model.pending_time, None);
    }

    #[test]
    fn completeness_grows_as_capabilities_are_exercised() {
        let model = Model::default();
        assert!((completeness(&model) - 0.25).abs() < f32::EPSILON, "base profile is 25%");
        let full = Model {
            client: "Sam|+1555".into(),
            location: "37.77,-122.41".into(),
            device: "accelerometer: 0,0,9.8".into(),
            ..Model::default()
        };
        assert!((completeness(&full) - 1.0).abs() < f32::EPSILON, "all three tried → 100%");
    }

    #[test]
    fn record_video_then_share_tracks_last_clip() {
        let (app, mut model) = app();
        let mut cx = Cx::<Msg>::default();
        app.update(Msg::VideoRecorded("file:///tmp/clip.mov".into()), &mut model, &mut cx);
        assert_eq!(model.last_video.as_deref(), Some("file:///tmp/clip.mov"));
        // Cancelling a later recording leaves the previous clip in place.
        app.update(Msg::VideoRecorded(String::new()), &mut model, &mut cx);
        assert_eq!(model.last_video.as_deref(), Some("file:///tmp/clip.mov"));
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
