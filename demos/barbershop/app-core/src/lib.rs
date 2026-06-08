//! Fade House — a barbershop/grooming booking app. A Mobiler showcase for the broader UI
//! vocabulary: an icon bottom-tab bar, a floating action button, the expanded icon set, and
//! the theming engine (a warm brass brand on a dark shell). Logic + UI in Rust; rendered by
//! the generic shells on web (here) and native.

use mobiler_core::{
    BoxAlign, ButtonStyle, Caption, CardStyle, ChartLegendItem, ChartRefLine, ChartRegion,
    ChartSeries, ChartTick, Corner, Cx, Density, FontFamily, Icon, ImageRatio,
    ImageShape, InputValue, MobilerApp, MobilerShell, PluginResponse, Rgb, Spacing, Theme, Tone, Widget, avatar_status,
    badge, button, calendar, caption, card, card_button, chip, column, divider, donut_chart,
    email_field, emphasis,
    gauge_chart, grid, icon_button, image, lazy_list, multiline_field, phone_field, progress, rating,
    rating_input, region_chart, rings_chart,
    pdf_view, row, scaffold, scroller, search_field, secure_field, segment, segmented, skeleton,
    spacer, stack, video_player, video_playlist, web_view,
    stacked_bar_chart, subtitle, swipe_action, tab_icon, text, text_field, title, with_captions, with_error,
    with_fab, with_muted, with_pip, with_poster, with_rate, with_refresh, with_seek_index, with_sheet, with_start_at, with_theme,
};
use mobiler_core::format::{self, Currency, Locale};
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
    /// Show a transient toast (feedback for the fire-and-forget capability demos above).
    Notify(String),

    // --- Bookings tab: new widgets (Chart, Calendar, SwipeAction, pull-to-refresh) ---
    /// Pull-to-refresh on the Bookings tab (`Scaffold.on_refresh`) — reload availability.
    RefreshBookings,
    BookingsRefreshed,
    /// Tap a day in the inline `Calendar`.
    PickDay(u8),
    /// Swipe a booking row and tap "Cancel" (`SwipeAction`).
    CancelBooking(u32),

    // --- Profile "Notes & devices": sqlite / speech / bluetooth ---
    /// Persist the note to on-device SQLite (`sqlite` plugin).
    SaveNote,
    /// Load the saved note back from SQLite.
    LoadNote,
    NoteLoaded(String),
    /// Dictate the note via speech-to-text (`speech` plugin).
    DictateNote,
    Dictated(String),
    /// Scan for nearby Bluetooth LE devices (`bluetooth` plugin).
    ScanBt,
    BtScanned(String),
    /// Deep-link to the app's iOS Settings page (to re-enable a denied permission).
    OpenAppSettings,

    // --- Profile "Sign in": OAuth login flow (`oauth` plugin) ---
    /// Start an OAuth login in the system auth browser (`oauth` plugin).
    OAuthLogin,
    /// OAuth result: (ok, redirect-URL-or-error).
    OAuthDone(bool, String),

    /// The device's preferred locale tag (built-in `device` "locale" capability).
    GotDeviceLocale(String),
    /// Toggle the "Live" ticker subscription on/off (the streaming primitive demo).
    ToggleLive,
    /// A streamed tick from the `ticker` subscription (the counter value as a string).
    Tick(String),
    /// Connect/disconnect the "Echo WS" row (the `websocket` plugin over the streaming primitive).
    ToggleWs,
    /// A frame streamed from the echo WebSocket (`ok` false = closed).
    WsFrame(PluginResponse),
    /// The `LazyList` Feed scrolled near the end — append the next page.
    FeedLoadMore,
    /// The Feed was pulled-to-refresh — reset to page 1.
    FeedRefresh,

    /// --- Profile "Push" card: remote push (`push` plugin) ---
    /// Register for a device token + subscribe to inbound push events.
    EnablePush,
    /// The device token (or error) returned by `push` register.
    PushRegistered(PluginResponse),
    /// An inbound push payload (received/tapped) or a `{"type":"token_refresh",…}` event.
    PushEvent(PluginResponse),

    /// --- Profile "Store" card: in-app purchase (`iap` plugin) ---
    /// Buy a product (launches the native purchase sheet).
    BuyProduct(String),
    /// Restore prior purchases.
    RestorePurchases,
    /// Product metadata JSON from `iap` products.
    StoreProducts(PluginResponse),
    /// Thin ack from a purchase/restore launch.
    StoreStarted(PluginResponse),
    /// A transaction from the `iap` transactions stream (purchase / restore / renewal).
    StoreTxn(PluginResponse),

    /// --- Profile "Intro video" card: the controllable `Widget::Video` player ---
    VideoPlay,
    VideoPause,
    VideoRestart,
    /// The clip finished (`on_ended`).
    VideoEnded,
    /// v2 controls: cycle speed (1×→1.5×→2×), mute, captions, Picture-in-Picture.
    VideoCycleRate,
    VideoToggleMute,
    VideoToggleCaptions,
    VideoTogglePip,
    /// --- Profile "Playlist" card: an auto-advancing `video_playlist` ---
    PlaylistPlay,
    PlaylistJump(i64),
    PlaylistEnded,
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
    /// Bookings-tab pull-to-refresh in flight (`Scaffold.refreshing`).
    refreshing: bool,
    /// Sample upcoming bookings (swipe a row to cancel).
    bookings: Vec<String>,
    /// Day tapped in the inline calendar.
    picked_day: Option<u8>,
    /// Note text (edited in Profile; saved/loaded via SQLite, dictated via speech).
    note: String,
    /// Last note loaded back from SQLite.
    saved_note: String,
    /// Bluetooth scan status / result line.
    bt_status: String,
    /// Bluetooth permission was denied — offer an "Open Settings" affordance.
    bt_denied: bool,
    /// "Sign in" showcase form — exercises the form-field kinds + inline validation.
    email: String,
    phone: String,
    password: String,
    bio: String,
    /// Status line for the OAuth sign-in demo.
    oauth_status: String,
    /// Device locale tag detected at startup (e.g. "de-CH").
    device_locale: String,
    /// "Live" streaming demo (cx.subscribe to the built-in `ticker`): whether the
    /// subscription is active, how many events have streamed in, and the last value.
    live_on: bool,
    live_count: u32,
    live_last: String,
    /// "Echo WS" row of the Live card — a real WebSocket (the `websocket` plugin) over the same
    /// streaming primitive: subscribed state + the last frame received from the echo server.
    ws_on: bool,
    ws_last: String,
    /// "Push" card — remote push (`push` plugin): the device token (after register) + the last
    /// inbound payload. Needs `mobiler plugin add push` + your Firebase/APNs config to actually
    /// deliver; without it, register reports "unavailable" (graceful degrade).
    push_token: String,
    push_last: String,
    /// "Store" card — in-app purchase (`iap` plugin): the loaded products JSON + the last transaction.
    /// Needs `mobiler plugin add iap` + store products (or an iOS `.storekit` file) to actually transact.
    store_products: String,
    store_last: String,
    /// "Intro video" card — the controllable `Widget::Video`: app-driven play/pause + restart, and the
    /// current position (ms) the shell reports ~1/sec via `input`.
    video_playing: bool,
    video_seek_ms: i64,
    video_pos_ms: i64,
    video_ended: bool,
    video_rate: f32,
    video_muted: bool,
    video_captions: bool,
    video_pip: bool,
    video_duration_ms: i64,
    video_state: i64,
    playlist_playing: bool,
    playlist_seek_index: i64,
    playlist_index: i64,
    /// "Feed" card — a long paged list demoing `LazyList` (pull-to-refresh + load-more). The app
    /// owns the items; load-more appends a page (up to 60), refresh resets to page 1.
    feed: Vec<String>,
    feed_refreshing: bool,
}

/// One page (10 items) of synthetic feed rows starting at item `start` (1-based).
fn feed_page(start: usize) -> Vec<String> {
    (start..start + 10).map(|i| format!("Booking #{i}")).collect()
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
            refreshing: false,
            bookings: vec![
                "Skin Fade · Fri 10:00".to_string(),
                "Beard Trim · Sat 14:30".to_string(),
                "Hot Towel Shave · Mon 09:15".to_string(),
            ],
            picked_day: None,
            note: String::new(),
            saved_note: String::new(),
            bt_status: String::new(),
            bt_denied: false,
            email: String::new(),
            phone: String::new(),
            password: String::new(),
            bio: String::new(),
            oauth_status: String::new(),
            device_locale: String::new(),
            live_on: false,
            live_count: 0,
            live_last: String::new(),
            ws_on: false,
            ws_last: String::new(),
            push_token: String::new(),
            push_last: String::new(),
            store_products: String::new(),
            store_last: String::new(),
            video_playing: false,
            video_seek_ms: -1,
            video_pos_ms: 0,
            video_ended: false,
            video_rate: 1.0,
            video_muted: false,
            video_captions: false,
            video_pip: false,
            video_duration_ms: 0,
            video_state: 0,
            playlist_playing: false,
            playlist_seek_index: -1,
            playlist_index: 0,
            feed: feed_page(1),
            feed_refreshing: false,
        }
    }
}

/// Parse a "4.8"-style rating into tenths (48) for the `rating` widget.
fn tenths(s: &str) -> u32 {
    (s.parse::<f32>().unwrap_or(0.0) * 10.0).round() as u32
}

/// Pull a query parameter out of a redirect URL (tiny, dependency-free) — used to read the
/// OAuth `code` from the redirect the `oauth` plugin returns.
fn query_param(url: &str, key: &str) -> Option<String> {
    let query = url.split_once('?').map(|(_, q)| q)?;
    query.split('&').find_map(|pair| {
        let (k, v) = pair.split_once('=')?;
        (k == key).then(|| v.to_string())
    })
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
                    Msg::Notify(if r.ok {
                        "Opening your mail app…".into()
                    } else {
                        "No mail app set up to handle email on this device".into()
                    })
                });
            }
            Msg::CallShop => {
                let input = serde_json::json!({ "number": "+15551234567" }).to_string();
                cx.plugin("composer", "call", input, |r| {
                    Msg::Notify(if r.ok { "Opening the dialer…".into() } else { "Can't place a call on this device".into() })
                });
            }
            Msg::SpeakBooking => {
                let text = match (model.pending_date.as_deref(), model.pending_time.as_deref()) {
                    (Some(d), Some(t)) => format!("Your next visit is on {d} at {t}."),
                    _ => "You have no upcoming bookings. Tap the calendar button to book a cut.".to_string(),
                };
                cx.plugin("tts", "speak", text, |r| {
                    Msg::Notify(if r.ok { "Speaking your booking aloud 🔊".into() } else { "Couldn't speak right now".into() })
                });
            }
            // The StoreKit / Play review prompt is system rate-limited and is suppressed in
            // TestFlight / dev builds — it only appears in store-distributed builds.
            Msg::RequestReview => cx.plugin("review", "request", "", |r| {
                Msg::Notify(if r.ok {
                    "Review requested (the prompt only shows in App Store builds)".into()
                } else {
                    "Review unavailable".into()
                })
            }),
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
            Msg::Notify(s) => cx.toast(s),

            // --- Bookings tab: Chart / Calendar / SwipeAction / pull-to-refresh ---
            Msg::RefreshBookings => {
                model.refreshing = true;
                // No real backend — simulate a reload that clears the flag (a real app would
                // resolve `BookingsRefreshed` from an http/plugin response).
                cx.plugin("connectivity", "status", "", |_| Msg::BookingsRefreshed);
            }
            Msg::BookingsRefreshed => {
                model.refreshing = false;
                cx.toast("Bookings up to date ✓");
            }
            Msg::PickDay(d) => {
                model.picked_day = Some(d);
                cx.toast(format!("Selected day {d}"));
            }
            Msg::CancelBooking(i) => {
                let i = i as usize;
                if i < model.bookings.len() {
                    let b = model.bookings.remove(i);
                    cx.toast(format!("Cancelled {b}"));
                }
            }

            // --- Profile "Notes & devices": sqlite / speech / bluetooth ---
            Msg::SaveNote => {
                let input = serde_json::json!({
                    "sql": "INSERT OR REPLACE INTO note(id, body) VALUES (1, ?)",
                    "args": [model.note],
                })
                .to_string();
                cx.plugin("sqlite", "exec", input, |r| {
                    Msg::Notify(if r.ok { "Note saved to SQLite ✓".into() } else { format!("Save failed: {}", r.output) })
                });
            }
            Msg::LoadNote => cx.plugin("sqlite", "query", "SELECT body FROM note WHERE id = 1", |r| {
                Msg::NoteLoaded(if r.ok { r.output } else { String::new() })
            }),
            Msg::NoteLoaded(json) => {
                // The sqlite plugin returns rows as a JSON array of {column: value} objects.
                let body = serde_json::from_str::<serde_json::Value>(&json)
                    .ok()
                    .and_then(|v| v.get(0).and_then(|r| r.get("body")).and_then(|b| b.as_str().map(String::from)));
                if let Some(b) = body {
                    model.saved_note = b.clone();
                    // Pre-fill the editable field too (so a restart shows the persisted note).
                    if model.note.is_empty() {
                        model.note = b;
                    }
                } else {
                    model.saved_note = "(no saved note yet)".to_string();
                }
            }
            Msg::DictateNote => cx.plugin("speech", "listen", "", |r| {
                Msg::Dictated(if r.ok { r.output } else { String::new() })
            }),
            Msg::Dictated(text) => {
                if text.is_empty() {
                    cx.toast("Didn't catch that");
                } else {
                    model.note = text;
                    cx.toast("Dictated ✓ — tap Save");
                }
            }
            Msg::ScanBt => {
                model.bt_status = "Scanning…".into();
                model.bt_denied = false;
                // Pass the plugin's output through on failure too, so the real reason shows.
                cx.plugin("bluetooth", "scan", "", |r| Msg::BtScanned(r.output));
            }
            Msg::BtScanned(out) => {
                if let Ok(serde_json::Value::Array(devs)) = serde_json::from_str::<serde_json::Value>(&out) {
                    model.bt_denied = false;
                    model.bt_status = format!("Found {} nearby device(s)", devs.len());
                } else {
                    // A failure message from the plugin (denied / off / unavailable).
                    model.bt_denied = out.contains("denied");
                    model.bt_status = match out.as_str() {
                        "denied" => "Bluetooth denied — re-enable it in Settings, then scan again.".into(),
                        "bluetooth off" => "Bluetooth is off — turn it on, then scan again.".into(),
                        "" => "Bluetooth unavailable (use a real device).".into(),
                        other => other.to_string(),
                    };
                }
            }
            // iOS won't re-show the permission prompt once denied — deep-link to the app's Settings.
            Msg::OpenAppSettings => cx.notify("browser", "open", "app-settings:"),

            // OAuth sign-in demo. A real flow points `url` at the provider's authorize endpoint;
            // here we use an httpbin 302 → our custom scheme, so the full redirect round-trip
            // (open browser → redirect → capture → return) runs end-to-end without an IdP. The
            // app's id (= redirect scheme on both platforms) is dev.mobiler.barbershop.
            Msg::OAuthLogin => {
                model.oauth_status = "Opening sign-in…".into();
                let url = "https://httpbin.org/redirect-to?url=dev.mobiler.barbershop%3A%2F%2Foauth%3Fcode%3Ddemo123&status_code=302";
                let input = format!(r#"{{"url":"{url}","scheme":"dev.mobiler.barbershop"}}"#);
                cx.plugin("oauth", "login", input, |r| Msg::OAuthDone(r.ok, r.output));
            }
            Msg::GotDeviceLocale(tag) => model.device_locale = tag,
            Msg::ToggleLive => {
                model.live_on = !model.live_on;
                if model.live_on {
                    model.live_count = 0;
                    // Subscribe to the built-in `ticker` stream: one event/second, each
                    // re-entering update as Msg::Tick — the streaming primitive in action.
                    cx.subscribe("ticker", "ticker", "start", "1000", |r| Msg::Tick(r.output));
                } else {
                    cx.unsubscribe("ticker");
                }
            }
            Msg::Tick(value) => {
                model.live_count += 1;
                model.live_last = value;
            }
            Msg::ToggleWs => {
                model.ws_on = !model.ws_on;
                if model.ws_on {
                    model.ws_last = "connecting…".to_string();
                    // Subscribe to a real echo WebSocket via the `websocket` plugin (streaming):
                    // the server greets on connect, then echoes — each frame re-enters as WsFrame.
                    cx.subscribe("ws", "websocket", "stream", "wss://echo.websocket.org", Msg::WsFrame);
                } else {
                    cx.unsubscribe("ws");
                }
            }
            Msg::WsFrame(resp) => {
                if resp.ok {
                    model.ws_last = resp.output;
                } else {
                    model.ws_on = false;
                    // Surface the real close reason (the shell sends the error text) so a failure
                    // is diagnosable on-device, not just a generic "disconnected".
                    model.ws_last = if resp.output.is_empty() || resp.output == "closed" {
                        "disconnected".to_string()
                    } else {
                        format!("closed: {}", resp.output)
                    };
                }
            }
            Msg::FeedLoadMore => {
                // Append the next page until we hit the cap (6 pages = 60 items), then `has_more`
                // goes false and the shell stops firing.
                if model.feed.len() < 60 {
                    model.feed.extend(feed_page(model.feed.len() + 1));
                }
            }
            Msg::FeedRefresh => {
                model.feed = feed_page(1);
                model.feed.insert(0, "↻ refreshed".to_string());
                model.feed_refreshing = false;
            }
            Msg::EnablePush => {
                // Ask the OS for a device token AND subscribe to inbound pushes (a real app
                // subscribes at startup so a tap that launched the app isn't missed). Both no-op
                // gracefully until `mobiler plugin add push` + Firebase/APNs config are in place.
                model.push_token = "registering…".to_string();
                cx.plugin("push", "register", "", Msg::PushRegistered);
                cx.subscribe("push", "push", "events", "", Msg::PushEvent);
            }
            Msg::PushRegistered(resp) => {
                model.push_token = if resp.ok {
                    resp.output // {"token":"…","platform":"apns"|"fcm"} — POST to your backend
                } else {
                    format!("unavailable: {} (run `mobiler plugin add push`)", resp.output)
                };
            }
            Msg::PushEvent(resp) => {
                if resp.ok {
                    model.push_last = resp.output;
                }
            }
            Msg::BuyProduct(id) => cx.plugin("iap", "purchase", &id, Msg::StoreStarted),
            Msg::RestorePurchases => cx.plugin("iap", "restore", "", Msg::StoreStarted),
            Msg::StoreProducts(resp) => {
                if resp.ok {
                    model.store_products = resp.output; // JSON array: [{id,title,price,type},…]
                }
            }
            Msg::StoreStarted(_) => {} // thin ack — the real transaction arrives on StoreTxn
            Msg::StoreTxn(resp) => {
                if resp.ok {
                    // A real app POSTs resp.output's signed `payload` to its backend, then grants +
                    // finishes. Here we just surface the transaction.
                    model.store_last = resp.output;
                }
            }
            // Drive the controllable Widget::Video (play/pause via the app-owned `playing` field;
            // seek by setting `video_seek_ms`; position arrives in `fn input`).
            Msg::VideoPlay => { model.video_playing = true; model.video_ended = false; }
            Msg::VideoPause => model.video_playing = false,
            Msg::VideoRestart => { model.video_seek_ms = 0; model.video_playing = true; model.video_ended = false; }
            Msg::VideoEnded => { model.video_playing = false; model.video_ended = true; }
            Msg::VideoCycleRate => {
                model.video_rate = if model.video_rate < 1.25 { 1.5 } else if model.video_rate < 1.75 { 2.0 } else { 1.0 };
            }
            Msg::VideoToggleMute => model.video_muted = !model.video_muted,
            Msg::VideoToggleCaptions => model.video_captions = !model.video_captions,
            Msg::VideoTogglePip => model.video_pip = !model.video_pip,
            Msg::PlaylistPlay => model.playlist_playing = true,
            Msg::PlaylistJump(i) => { model.playlist_seek_index = i; model.playlist_playing = true; }
            Msg::PlaylistEnded => model.playlist_playing = false,
            Msg::OAuthDone(ok, output) => {
                model.oauth_status = if ok {
                    match query_param(&output, "code") {
                        Some(code) => format!("Signed in ✓ — got auth code: {code}"),
                        None => format!("Redirected: {output}"),
                    }
                } else {
                    format!("Sign-in cancelled/failed: {output}")
                };
            }
        }
    }

    /// Ensure the note table exists, then load any saved note so it shows on launch/restart
    /// (the data persists in SQLite; the model doesn't, so we re-read it at startup).
    fn init(&self, _model: &mut Model, cx: &mut Cx<Msg>) {
        cx.plugin(
            "sqlite",
            "exec",
            "CREATE TABLE IF NOT EXISTS note(id INTEGER PRIMARY KEY, body TEXT)",
            |_| Msg::LoadNote,
        );
        // Detect the device's preferred locale (built-in `device` capability) so the
        // formatting card can show it — works on iOS, Android, and web.
        cx.device_locale(|r| Msg::GotDeviceLocale(r.output));
        // In-app purchase (`iap` plugin): subscribe to the transactions stream at startup (the single
        // source of truth) + load product metadata for the Store card. No-ops gracefully until
        // `mobiler plugin add iap`. iOS sim-tests against demos/barbershop/iOS/Products.storekit.
        cx.subscribe("iap", "iap", "transactions", "", Msg::StoreTxn);
        cx.plugin("iap", "products", r#"["com.fadehouse.tip","com.fadehouse.pro","com.fadehouse.premium"]"#, Msg::StoreProducts);
    }

    fn input(&self, id: &str, value: InputValue, model: &mut Model, _cx: &mut Cx<Msg>) {
        match value {
            InputValue::Text(t) => match id {
                "search" => model.search = t,
                "note" => model.note = t,
                "email" => model.email = t,
                "phone" => model.phone = t,
                "password" => model.password = t,
                "bio" => model.bio = t,
                _ => {}
            },
            // The Video widget reports its current position (ms) ~1/sec via the input mechanism, plus
            // transport state on suffixed ids ("{id}.duration" / ".state" / ".buffered" / ".index").
            InputValue::Int(ms) if id == "intro" => model.video_pos_ms = ms,
            InputValue::Int(d) if id == "intro.duration" => model.video_duration_ms = d,
            InputValue::Int(s) if id == "intro.state" => model.video_state = s,
            InputValue::Int(i) if id == "playlist.index" => model.playlist_index = i,
            _ => {}
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
            Tab::Bookings => ("Bookings", bookings_screen(model)),
            Tab::Profile => ("Profile", profile_screen(model)),
        };
        // Themed Scaffold + icon tab bar + a "book now" floating action button.
        let mut root = with_fab(scaffold(title_text, true, tabs, body), Icon::Calendar, Msg::Book);
        // The Bookings tab is pull-to-refresh (the app owns `refreshing`).
        if model.tab == Tab::Bookings {
            root = with_refresh(root, model.refreshing, Msg::RefreshBookings);
        }
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

fn bookings_screen(model: &Model) -> Widget {
    // Visits-per-day, stacked by barber (Mon–Sun) — axis + legend on.
    let week = vec!["M".into(), "T".into(), "W".into(), "T".into(), "F".into(), "S".into(), "S".into()];
    let visits = stacked_bar_chart(
        vec![
            ChartSeries::new("Alex", vec![1.0, 2.0, 1.0, 2.0, 2.0, 3.0, 1.0]),
            ChartSeries::new("Sam", vec![1.0, 1.0, 0.0, 2.0, 1.0, 2.0, 1.0]).with_color(Rgb::new(0x3F, 0xA7, 0xD6)),
        ],
        week,
    );
    // Service mix this week (Donut) — each series is one slice.
    let mix = donut_chart(vec![
        ChartSeries::new("Cut", vec![18.0]),
        ChartSeries::new("Beard", vec![9.0]),
        ChartSeries::new("Shave", vec![5.0]),
        ChartSeries::new("Color", vec![3.0]),
    ]);
    // Today's progress toward the daily target (radial Gauge).
    let today = gauge_chart(ChartSeries::new("Bookings", vec![7.0]).with_goal(10.0));
    // Weekly goals as concentric progress Rings (fitness-style).
    let goals = rings_chart(vec![
        ChartSeries::new("Revenue", vec![1280.0]).with_goal(1500.0),
        ChartSeries::new("Bookings", vec![34.0]).with_goal(40.0),
        ChartSeries::new("New clients", vec![6.0]).with_goal(8.0),
    ]);
    // Inline month Calendar — tap a day to pick it (June 2026).
    let month = calendar(2026, 6, model.picked_day, Msg::PickDay);
    // Upcoming bookings — swipe a row to reveal "Cancel" (SwipeAction).
    let rows: Vec<Widget> = if model.bookings.is_empty() {
        vec![caption("No upcoming bookings — pull to refresh or book a cut.")]
    } else {
        model
            .bookings
            .iter()
            .enumerate()
            .map(|(i, b)| {
                swipe_action(
                    card(emphasis(b.clone()), CardStyle::Filled),
                    vec![("Cancel", Tone::Danger, Msg::CancelBooking(i as u32))],
                )
            })
            .collect()
    };
    column(vec![
        caption("Pull down to refresh availability."),
        subtitle("This week"),
        card(visits, CardStyle::Outlined),
        row(vec![
            card(column(vec![caption("Service mix"), mix]), CardStyle::Outlined),
            card(column(vec![caption("Today's target"), today]), CardStyle::Outlined),
        ]),
        subtitle("Weekly goals"),
        card(goals, CardStyle::Outlined),
        subtitle("Pick a date"),
        card(month, CardStyle::Outlined),
        subtitle("Upcoming"),
        column(rows),
        button("Book now", ButtonStyle::Filled, Msg::Book),
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

/// Bluetooth scan controls — adds an "Open Settings" affordance when the permission was denied
/// (iOS won't re-prompt once denied; the user must re-enable it in Settings).
fn bt_section(model: &Model) -> Widget {
    let mut items = vec![
        button("Scan Bluetooth devices", ButtonStyle::Outlined, Msg::ScanBt),
        caption(if model.bt_status.is_empty() { "Find nearby BLE devices (real device only).".to_string() } else { model.bt_status.clone() }),
    ];
    if model.bt_denied {
        items.push(button("Open Settings", ButtonStyle::Text, Msg::OpenAppSettings));
    }
    column(items)
}

/// A `RegionChart` showcase — a variable-width "coverage-gap" chart (the Swiss insurance /
/// pension style): colored value bands across an irregular timeline, a solid target line + a
/// dashed ceiling, a right-side bracket, and a legend. Illustrative data.
/// Locale-aware formatting showcase: one amount + today's date rendered across locales using
/// `mobiler_core::format` (pure Rust, synchronous — runs in the core, not via a platform formatter).
/// The "Live" streaming card — a button that subscribes to / unsubscribes from the
/// built-in `ticker` stream (`cx.subscribe`/`unsubscribe`), and a status line that
/// updates every second as ticks stream in. Proves the streaming primitive: a native
/// source pushing N events over time into `update`.
fn live_card(model: &Model) -> Widget {
    let (on, count, last) = (model.live_on, model.live_count, model.live_last.as_str());
    let status = if on {
        caption(format!("● live — {count} tick(s) received (last: {last})"))
    } else if count > 0 {
        caption(format!("Stopped after {count} tick(s)."))
    } else {
        caption("Tap Start to stream live updates pushed from a native source.")
    };
    // Second row: a real WebSocket over the same primitive (the `websocket` plugin).
    let ws_status = if model.ws_on {
        caption(format!("● connected — last frame: {}", model.ws_last))
    } else if !model.ws_last.is_empty() {
        caption(format!("WS: {}", model.ws_last))
    } else {
        caption("Or stream from a real echo WebSocket (the `websocket` plugin).")
    };
    card(
        column(vec![
            emphasis("Live"),
            caption("A push stream from the device (cx.subscribe → the built-in `ticker`)."),
            status,
            button(if on { "Stop" } else { "Start" }, ButtonStyle::Filled, Msg::ToggleLive),
            divider(),
            ws_status,
            button(if model.ws_on { "Disconnect" } else { "Echo WS" }, ButtonStyle::Outlined, Msg::ToggleWs),
        ]),
        CardStyle::Outlined,
    )
}

/// The "Push" card — remote push (`push` plugin): Register fetches the device token (APNs/FCM) and
/// subscribes to inbound notifications; the status line shows the token + the last payload. Real
/// delivery needs `mobiler plugin add push` + your Firebase/APNs config — without it, register
/// degrades to "unavailable" (the call returns ok:false from the default plugin dispatch).
fn push_card(model: &Model) -> Widget {
    let token_line = if model.push_token.is_empty() {
        caption("Tap Register to get this device's push token (POST it to your backend).")
    } else {
        caption(format!("Token: {}", model.push_token))
    };
    let event_line = if model.push_last.is_empty() {
        caption("Inbound notifications appear here (foreground-received or tapped).")
    } else {
        caption(format!("Last push: {}", model.push_last))
    };
    card(
        column(vec![
            emphasis("Push"),
            caption("Remote push (cx.plugin \"register\" + cx.subscribe \"events\" — APNs / FCM)."),
            token_line,
            event_line,
            button("Register for push", ButtonStyle::Filled, Msg::EnablePush),
        ]),
        CardStyle::Outlined,
    )
}

/// The "Store" card — in-app purchase (`iap` plugin): Buy buttons launch the native purchase sheet,
/// and the transactions stream surfaces the result. iOS is testable on the simulator via
/// `demos/barbershop/iOS/Products.storekit`; Android needs a Play Console test track. Degrades
/// gracefully (purchase returns ok:false from the default plugin dispatch) until `mobiler plugin add iap`.
fn store_card(model: &Model) -> Widget {
    let status = if model.store_last.is_empty() {
        caption("Tap a product to buy. iOS: tested on-sim via a local .storekit file (no real money).")
    } else {
        caption(format!("Last transaction: {}", model.store_last))
    };
    card(
        column(vec![
            emphasis("Store"),
            caption("In-app purchase (cx.plugin \"purchase\" + the transactions stream — StoreKit 2 / Play Billing)."),
            status,
            row(vec![
                button("Tip $1", ButtonStyle::Outlined, Msg::BuyProduct("com.fadehouse.tip".to_string())),
                button("Pro", ButtonStyle::Filled, Msg::BuyProduct("com.fadehouse.pro".to_string())),
            ]),
            row(vec![
                button("Premium (sub)", ButtonStyle::Outlined, Msg::BuyProduct("com.fadehouse.premium".to_string())),
                button("Restore", ButtonStyle::Text, Msg::RestorePurchases),
            ]),
        ]),
        CardStyle::Outlined,
    )
}

/// The "Intro video" card — the controllable `Widget::Video`: app-driven Play/Pause/Restart, the
/// shell-reported position (~1/sec via `input`), and the `on_ended` event. A public MP4 so it plays
/// on all three shells (an HLS `.m3u8` would play on iOS/Android + Safari only in v1).
fn video_card(model: &Model) -> Widget {
    let state_label = match model.video_state {
        1 => "buffering",
        3 => "playing",
        4 => "ended",
        2 => "paused",
        _ => "idle",
    };
    let status = caption(format!(
        "{} • {}s / {}s",
        state_label,
        model.video_pos_ms / 1000,
        model.video_duration_ms.max(0) / 1000
    ));
    // Build the player, then layer on the v2 cosmetics (poster, resume offset, rate, mute, captions,
    // PiP) — each modifier is a no-op-safe match-and-rebind on Widget::Video.
    let mut player = video_player(
        "intro",
        // Public H.264 MP4 with range support (plays in AVPlayer / ExoPlayer / <video>).
        // NB: the old Google `gtv-videos-bucket` sample URLs now return HTTP 403.
        "https://media.w3.org/2010/05/sintel/trailer.mp4",
        model.video_playing,
        model.video_seek_ms,
        Msg::VideoEnded,
    );
    player = with_poster(player, "https://picsum.photos/seed/fadehouse/640/360");
    player = with_start_at(player, 8000); // resume 0:08 in on first load
    player = with_rate(player, model.video_rate);
    if model.video_muted {
        player = with_muted(player);
    }
    if model.video_pip {
        player = with_pip(player);
    }
    if model.video_captions {
        // A self-contained WebVTT via a data: URL (same-origin → renders on web with no CORS).
        player = with_captions(
            player,
            vec![Caption {
                url: "data:text/vtt,WEBVTT%0A%0A00:00:00.000 --> 00:00:30.000%0AFade House — intro".to_string(),
                label: "English".to_string(),
                language: "en".to_string(),
                default_on: true,
            }],
        );
    }
    card(
        column(vec![
            emphasis("Intro video"),
            caption("A controllable native player (Widget::Video — AVPlayer / Media3 ExoPlayer / <video>)."),
            player,
            status,
            row(vec![
                button("Play", ButtonStyle::Filled, Msg::VideoPlay),
                button("Pause", ButtonStyle::Outlined, Msg::VideoPause),
                button("Restart", ButtonStyle::Text, Msg::VideoRestart),
            ]),
            row(vec![
                button(&format!("{:.1}×", model.video_rate), ButtonStyle::Text, Msg::VideoCycleRate),
                button(if model.video_muted { "Unmute" } else { "Mute" }, ButtonStyle::Text, Msg::VideoToggleMute),
                button(if model.video_captions { "CC ✓" } else { "CC" }, ButtonStyle::Text, Msg::VideoToggleCaptions),
                button(if model.video_pip { "PiP ✓" } else { "PiP" }, ButtonStyle::Text, Msg::VideoTogglePip),
            ]),
        ]),
        CardStyle::Outlined,
    )
}

/// The "Playlist" card — a `video_playlist` over two clips that auto-advances gaplessly. The shell
/// reports the current track via `input("playlist.index", …)`; the buttons force-jump via `seek_index`.
fn playlist_card(model: &Model) -> Widget {
    let player = with_seek_index(
        video_playlist(
            "playlist",
            vec![
                "https://media.w3.org/2010/05/video/movie_300.mp4".to_string(),
                "https://media.w3.org/2010/05/sintel/trailer.mp4".to_string(),
            ],
            0,
            model.playlist_playing,
            Msg::PlaylistEnded,
        ),
        model.playlist_seek_index,
    );
    card(
        column(vec![
            emphasis("Playlist"),
            caption(format!("Two clips, auto-advancing • now playing clip {}", model.playlist_index + 1)),
            player,
            row(vec![
                button("Play", ButtonStyle::Filled, Msg::PlaylistPlay),
                button("Clip 1", ButtonStyle::Outlined, Msg::PlaylistJump(0)),
                button("Clip 2", ButtonStyle::Outlined, Msg::PlaylistJump(1)),
            ]),
        ]),
        CardStyle::Outlined,
    )
}

/// The "Web" card — the general `Widget::WebView` (WKWebView / Android WebView / `<iframe>`). Here
/// it embeds a public page; the same widget hosts a player embed (e.g. a Bunny.net embed URL) on all
/// platforms. NOT the default video player — that's the Intro video card above (`Widget::Video`).
fn web_card() -> Widget {
    // The WebView hosts a Bunny.net Stream player embed (Bunny's own public demo from their Player.js
    // blog). In a real app build the embed URL with `mobiler_core::bunny::embed_url(library_id,
    // video_id)`, or `embed_url_signed(..)` from your BACKEND (never ship the token key). Bunny's
    // player embeds cleanly in a bare WebView (unlike YouTube, which needs an http origin) and brings
    // its own captions/quality/thumbnails on every platform.
    card(
        column(vec![
            emphasis("Embedded web"),
            caption("A native web view (Widget::WebView) — WKWebView / Android WebView / <iframe>. Here: a Bunny.net Stream player embed (the hosted-player use case)."),
            web_view("https://iframe.mediadelivery.net/embed/197133/dc48a09e-d9bb-420a-83d7-72dc2304c034?autoplay=false"),
        ]),
        CardStyle::Outlined,
    )
}

/// The "Feed" card — a `LazyList` of synthetic bookings: pull-to-refresh at the top, load-more
/// when you scroll near the end (stops at 60). The list owns a bounded scroll region.
fn feed_card(model: &Model) -> Widget {
    let items: Vec<Widget> = model
        .feed
        .iter()
        .map(|row| card(text(row.clone()), CardStyle::Filled))
        .collect();
    let list = with_refresh(
        lazy_list(items, false, model.feed.len() < 60, Msg::FeedLoadMore),
        model.feed_refreshing,
        Msg::FeedRefresh,
    );
    card(
        column(vec![
            emphasis("Feed"),
            caption("A long paged list — pull to refresh, scroll to load more (`LazyList`)."),
            list,
        ]),
        CardStyle::Outlined,
    )
}

fn format_card(device_locale: &str) -> Widget {
    let amount = 1234.5;
    let row_for = |label: &str, value: String| {
        row(vec![
            caption(label.to_string()),
            spacer(Spacing::Sm),
            text(value),
        ])
    };
    // The device's preferred locale (detected at startup) mapped to a formatting Locale.
    let detected = if device_locale.is_empty() {
        caption("Detecting your device locale…")
    } else {
        match Locale::from_tag(device_locale) {
            Some(loc) => row_for(
                "Your device",
                format!("{device_locale} → {}", format::format_currency(amount, Currency::Chf, loc)),
            ),
            None => caption(format!("Your device: {device_locale} (unsupported — using default)")),
        }
    };
    card(
        column(vec![
            emphasis("Locale formatting"),
            caption("The same CHF 1234.5 / date, formatted per locale:"),
            row_for("de-CH", format::format_currency(amount, Currency::Chf, Locale::DeCh)),
            row_for("de-DE", format::format_currency(amount, Currency::Eur, Locale::DeDe)),
            row_for("fr-FR", format::format_currency(amount, Currency::Eur, Locale::FrFr)),
            row_for("en-US", format::format_currency(amount, Currency::Usd, Locale::EnUs)),
            row_for("sr-Latn", format::format_currency(amount, Currency::Rsd, Locale::SrLatn)),
            row_for("sr-Cyrl", format::format_currency(amount, Currency::Rsd, Locale::SrCyrl)),
            divider(),
            row_for("de-CH", format::format_date_long(2026, 6, 4, Locale::DeCh)),
            row_for("sr-Cyrl", format::format_date_long(2026, 6, 4, Locale::SrCyrl)),
            divider(),
            detected,
        ]),
        CardStyle::Outlined,
    )
}

fn coverage_chart() -> Widget {
    // Brand-ish palette for the bands.
    let teal = Rgb::new(0x8E, 0xC6, 0xBA);
    let teal_l = Rgb::new(0xCF, 0xE8, 0xE1);
    let teal_l2 = Rgb::new(0xB4, 0xDB, 0xD1);
    let blue = Rgb::new(0x9E, 0xC5, 0xF0);
    let green = Rgb::new(0x5E, 0x8C, 0x52);
    let gap = Rgb::new(0x5A, 0x7D, 0x9A);
    // x timeline (illustrative positions): 0 → 3 Mt. → 21 Mt. → Children 18/25 → 65 J.
    let (t3, t21, tc, t65) = (12.0, 25.0, 62.0, 100.0);
    let regions = vec![
        ChartRegion::new(0.0, t3, 0.0, 80_000.0, "80% CHF 80'000").vertical().with_color(Rgb::new(0xFF, 0xFF, 0xFF)),
        ChartRegion::new(t3, t21, 0.0, 80_000.0, "80% CHF 80'000").vertical().with_color(teal),
        ChartRegion::new(t21, t65, 0.0, 13_058.0, "CHF 13'058").with_color(teal_l),
        ChartRegion::new(t21, t65, 13_058.0, 34_906.0, "CHF 21'848").with_color(teal_l2),
        ChartRegion::new(t21, tc, 34_906.0, 47_002.0, "2 × 6'048 = 12'096").with_color(blue),
        ChartRegion::new(t21, tc, 47_002.0, 55_742.0, "2 × 4'369 = 8'739").with_color(green),
        ChartRegion::new(t21, tc, 55_742.0, 80_000.0, "CHF 24'258").with_color(gap),
        ChartRegion::new(tc, t65, 34_906.0, 80_000.0, "CHF 45'093").with_color(gap),
    ];
    let chart = region_chart(
        regions,
        vec![ChartTick::new(t3, "3 Mt."), ChartTick::new(t21, "21 Mt."), ChartTick::new(tc, "Children 18/25"), ChartTick::new(t65, "65 J.")],
        100.0,
        90_000.0,
        vec![ChartRefLine::target(80_000.0, "CHF 80'000"), ChartRefLine::max(90_000.0, "CHF 90'000")],
        vec![
            ChartLegendItem::new("Daily sickness", teal),
            ChartLegendItem::new("IV pension", teal_l),
            ChartLegendItem::new("PK IV pension", teal_l2),
            ChartLegendItem::new("Children", blue),
            ChartLegendItem::new("PK children", green),
            ChartLegendItem::new("Gap", gap),
        ],
    );
    chart
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
        // Notes & devices — on-device SQLite (sqlite), dictation (speech), BLE scan (bluetooth).
        card(
            column(vec![
                emphasis("Notes & devices"),
                text_field("note", "Type or dictate a note…", model.note.as_str()),
                row(vec![
                    button("Save", ButtonStyle::Outlined, Msg::SaveNote),
                    button("Load", ButtonStyle::Text, Msg::LoadNote),
                    button("Dictate", ButtonStyle::Text, Msg::DictateNote),
                ]),
                caption(if model.saved_note.is_empty() { "Saved note appears here.".to_string() } else { format!("Saved: {}", model.saved_note) }),
                divider(),
                bt_section(model),
            ]),
            CardStyle::Outlined,
        ),
        // Form-field showcase — kinds (email/phone/secure/multiline) + inline validation.
        card(
            column(vec![
                emphasis("Sign in"),
                caption("Form fields: email, phone, password, and a multi-line note — each with the right keyboard, masking, and inline validation."),
                {
                    let f = email_field("email", "you@example.com", model.email.as_str());
                    if !model.email.is_empty() && !model.email.contains('@') {
                        with_error(f, "Enter a valid email address")
                    } else { f }
                },
                phone_field("phone", "+41 79 123 45 67", model.phone.as_str()),
                {
                    let f = secure_field("password", "Password", model.password.as_str());
                    if !model.password.is_empty() && model.password.len() < 8 {
                        with_error(f, "At least 8 characters")
                    } else { f }
                },
                multiline_field("bio", "Anything your stylist should know…", model.bio.as_str()),
                divider(),
                // OAuth login via the system auth browser (`oauth` plugin).
                button("Sign in with OAuth", ButtonStyle::Filled, Msg::OAuthLogin),
                caption(if model.oauth_status.is_empty() {
                    "Opens the system auth browser and captures the redirect (demo flow).".to_string()
                } else {
                    model.oauth_status.clone()
                }),
            ]),
            CardStyle::Outlined,
        ),
        // RegionChart showcase — a variable-width "coverage-gap" chart.
        card(
            column(vec![
                emphasis("Coverage"),
                caption("Income protection across your career (a RegionChart)."),
                coverage_chart(),
            ]),
            CardStyle::Outlined,
        ),
        // Locale-aware formatting showcase — the same amount/date rendered per locale
        // (mobiler_core::format; pure Rust, synchronous, runs in the core).
        format_card(&model.device_locale),
        // In-app PDF viewer (Widget::PdfView) — display a backend-generated report.
        card(
            column(vec![
                emphasis("Report"),
                caption("A backend-generated PDF, displayed in-app (Widget::PdfView)."),
                pdf_view("https://www.w3.org/WAI/ER/tests/xhtml/testfiles/resources/pdf/dummy.pdf"),
            ]),
            CardStyle::Outlined,
        ),
        // Live streaming demo (cx.subscribe → built-in `ticker`): a native source
        // pushes N events over time into update(), each re-rendering this card.
        live_card(model),
        push_card(model),
        store_card(model),
        video_card(model),
        playlist_card(model),
        web_card(),
        feed_card(model),
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
    fn cancel_booking_removes_the_row() {
        let (app, mut model) = app();
        let mut cx = Cx::<Msg>::default();
        let before = model.bookings.len();
        app.update(Msg::CancelBooking(0), &mut model, &mut cx);
        assert_eq!(model.bookings.len(), before - 1);
        // Out-of-range index is a no-op (doesn't panic).
        app.update(Msg::CancelBooking(99), &mut model, &mut cx);
        assert_eq!(model.bookings.len(), before - 1);
    }

    #[test]
    fn refresh_sets_then_clears_the_flag() {
        let (app, mut model) = app();
        let mut cx = Cx::<Msg>::default();
        app.update(Msg::RefreshBookings, &mut model, &mut cx);
        assert!(model.refreshing, "pull sets the spinner");
        app.update(Msg::BookingsRefreshed, &mut model, &mut cx);
        assert!(!model.refreshing, "the reload response clears it");
    }

    #[test]
    fn pick_day_and_dictate_update_the_model() {
        let (app, mut model) = app();
        let mut cx = Cx::<Msg>::default();
        app.update(Msg::PickDay(12), &mut model, &mut cx);
        assert_eq!(model.picked_day, Some(12));
        app.update(Msg::Dictated("call me tomorrow".into()), &mut model, &mut cx);
        assert_eq!(model.note, "call me tomorrow");
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
