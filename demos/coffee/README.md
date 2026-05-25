# Coffee — Mobiler demo

A coffee-shop storefront built with [Mobiler](../../). The Rust core returns a
`Widget` tree and the generic Jetpack Compose shell renders it to native
Material 3 — no UI logic in the native layer.

<img src="screenshots/storefront.png" width="300" alt="Coffee storefront — hero, category chips, and a product grid">

## What it shows

- **Image** widgets (the hero and product thumbnails) loaded over the network via Coil
- **Box** overlay — the headline and "Get Started" CTA stacked over the hero image, on a scrim
- A single **terracotta** brand seed color; Material 3 derives the rest of the palette (dark mode included)
- Selectable **Chip** category filters that filter a 2-column **Grid** of product **Card**s (image + name + price + ★ rating)
- Tapping a chip runs the Crux update loop and re-renders the filtered grid

All styling is **token-based in Rust** (`ImageShape`, `ImageRatio`, `BoxAlign`, …); the Compose shell owns the concrete look.

## Run

```bash
cargo install mobiler        # or use the repo build: cargo build -p mobiler
cd demos/coffee
mobiler dev                  # build the core, generate Kotlin types, build + launch on a device/emulator
```
