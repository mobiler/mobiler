---
title: mobiler.toml options
description: Fonts, the launch screen, deep links, local network on iOS and the display name, set once and applied to every shell by the CLI.
---

Some things an app needs live in the native projects: font files, the launch screen, URL schemes, iOS
network rules. You describe them once in `mobiler.toml` at the app root, and `mobiler build`, `dev` and
`watch` write them into the shells before building. Commit the files they write along with
`mobiler.toml`, so a plain Gradle, Xcode or CI build sees them too.

A problem in `mobiler.toml` never fails a build: it prints a warning and leaves the last synced files as
they were. Each section also has its own command (`mobiler fonts sync`, `splash sync`, `deep-links sync`,
`local-network sync`) that applies it on demand and prints what it wrote; `deep-links sync` and
`local-network sync` exit non-zero when they can't apply the setting, so CI can check them.

## Fonts

```toml
[fonts]
display = { family = "Space Grotesk", files = ["assets/fonts/SpaceGrotesk-Regular.ttf", "assets/fonts/SpaceGrotesk-SemiBold.ttf"] }
body    = { family = "Roboto", files = ["assets/fonts/Roboto-Regular.ttf", "assets/fonts/Roboto-Medium.ttf"] }
```

`display` is used for titles, `body` for everything else; set `font: FontFamily::Custom` in your
[theme](/guides/theming/). The files (TrueType or OpenType) are copied into Android `res/font/`, the iOS
app (with `UIAppFonts`) and, if the app has `web/index.html`, `web/fonts/`. Text still scales with the
phone's font-size setting.

## Launch screen

```toml
[splash]
background      = "#FFFBFE"                      # light
background_dark = "#1C1B1F"                      # optional
logo            = "assets/splash/logo.png"       # optional; PNG with a transparent background
logo_dark       = "assets/splash/logo-dark.png"  # optional
logo_size       = 120                            # optional; 24 to 288, default 120
```

The colours follow the phone's light or dark setting. The CLI resizes the logo for each platform: Android's
launch window and its 12+ system splash, iOS's launch screen, and the web page before the app's first
render. Give a logo at least three times `logo_size` in pixels.

## Deep links

A new app registers a URL scheme (its bundle id, for example `com.example.myapp://…`), and links arrive in
your core on the built-in `system` stream when you subscribe to it. An app that doesn't use deep links can
turn the registration off:

```toml
[deep_links]
enabled = false
```

## Local network on iOS

iOS blocks plain `http://`. An app that talks to a dev server on your computer or a device on your network
can allow local addresses (`localhost`, `.local` names) without opening plain HTTP to the internet:

```toml
[ios]
local_network = true
```

A LAN IP address on a real device also needs iOS's Local Network permission. Android's debug build already
allows the emulator's host address, `10.0.2.2`.

## Display name

The name users see under the icon and in system dialogs is set by the CLI rather than `mobiler.toml`:

```sh
mobiler display-name "My App"   # set it
mobiler display-name            # print it
```

It writes Android's `app_name` and iOS's `CFBundleDisplayName`, and leaves project identifiers alone.
`mobiler upgrade` keeps it.

The [CLI reference](/reference/cli/) links to the full details of each option.
