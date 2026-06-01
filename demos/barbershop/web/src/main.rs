// The web client: the generic Mobiler web shell renders the shared barbershop core — the
// same `barbershop-core` a mobile build would use. Build/serve with Trunk (`trunk serve`).
fn main() {
    mobiler_web::run::<barbershop_core::App>();
}
