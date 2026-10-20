---
title: Theming
description: Give an app its own brand with a Theme value, from one seed colour to a full light and dark palette.
---

A theme is data in your core: a `Theme` value on the scaffold. Every shell applies it, so one value
re-brands the app on Android, iOS and the web. Anything you leave unset keeps the platform's stock look.

## From one colour

```rust
fn brand() -> Theme {
    Theme {
        seed: Rgb::new(0x0E, 0x9F, 0x8E),   // the brand colour; the shells derive the rest
        corner: Corner::Large,              // None, Small, Medium, Large
        density: Density::Comfortable,      // Compact, Comfortable, Large (bigger touch targets)
        font: FontFamily::System,           // System, Rounded, Serif, Monospace, or Custom
        ..Default::default()
    }
}

fn view(&self, model: &Model) -> Widget {
    with_theme(scaffold("Home", false, tabs(model), body(model)), brand())
}
```

Always end a `Theme { … }` literal with `..Default::default()`: new fields arrive in new releases, and the
default keeps your code compiling.

## A full design system

For an exact look, set a `palette`: colour roles for light and dark mode (page, cards, bars, text,
outlines, primary, status colours, selection, errors). Every role is optional.

```rust
let roles = |page: Rgb| ColorRoles {
    background: Some(page),
    primary: Some(teal),
    on_primary: Some(white),
    ..Default::default()
};
Theme {
    seed: teal,
    palette: Some(Palette { light: roles(mint), dark: roles(ink) }),
    ..Default::default()
}
```

- **Light and dark.** `with_appearance(widget, Appearance::System)` follows the phone's setting live;
  `Light` and `Dark` fix it.
- **Your own fonts.** List font files in `mobiler.toml` `[fonts]` and set `font: FontFamily::Custom`. The
  CLI copies them into every shell. A `type_scale` sets size and weight per text style.
- **The launch screen** follows light and dark too: see [`[splash]`](/guides/configuration/#launch-screen).
- **Shell wording.** `with_labels(widget, ShellLabels { … })` sets the text the shells draw themselves
  (back buttons, "Load more", dialog defaults) in your app's language.

## Platform-only options

A few options exist for one platform and are ignored by the others. `with_ios_glass_tabs(scaffold)`
draws iOS's floating Liquid Glass tab bar and a glass action button; Android and the web keep their
standard bars.

## See it

- [Saldo](/tutorial/) uses a full palette (Aqua Mint) with glass tabs on iOS.
- The [demos](/examples/) re-brand the same stock shells: barbershop in brass on dark, coffee in
  terracotta, todo in indigo.
