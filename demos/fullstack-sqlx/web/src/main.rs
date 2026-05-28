// The web client: the generic Mobiler web shell renders the shared notes core — the
// same `app-core` a mobile build would use. Build/serve with Trunk (`trunk serve`).
fn main() {
    mobiler_web::run::<app_core::App>();
}
