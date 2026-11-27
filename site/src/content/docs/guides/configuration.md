---
title: mobiler.toml options
description: Fonts, the launch screen, the launcher icon, bundled files, deep links, local network on iOS and the display name, set once and applied to every shell by the CLI.
---

Some things an app needs live in the native projects: font files, the launch screen, the launcher icon, bundled files, URL schemes, iOS
network rules. You describe them once in `mobiler.toml` at the app root, and `mobiler build`, `dev` and
`watch` write them into the shells before building. Commit the files they write along with
`mobiler.toml`, so a plain Gradle, Xcode or CI build sees them too.

A problem in `mobiler.toml` never fails a build: it prints a warning and leaves the last synced files as
they were. Each section also has its own command (`mobiler fonts sync`, `splash sync`, `icon sync`, `assets sync`, `deep-links sync`,
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

## Launcher icon

```toml
[icon]
foreground = "brand/icon-foreground.png"  # PNG, square, 1024 to 16384 px, transparent background
background = "#0E9F8E"                    # #RRGGBB
monochrome = "brand/icon-mono.png"        # optional; the themed icon on Android 13+
padding    = 18                           # optional; 0 to 40, default 18
```

The CLI generates every size: Android's adaptive icon and legacy images, iOS's 1024 px `AppIcon` and, if the
app has `web/index.html`, a favicon and an apple-touch-icon. Commit what it writes, and `.mobiler/icon.json`
with it. Keep the source PNGs outside your assets folder (for example in `brand/`).

An icon you made by hand is never overwritten: if a launcher file was edited, that platform is skipped with
a warning naming the file, and you delete or restore it to hand it back. Removing `[icon]` puts the
template's icon back. On Android 12+ the system splash shows the launcher icon when `[splash]` has no
`logo`. iOS 18's dark and tinted icon variants are not generated yet.

## Bundled files

```toml
[assets]
dir = "assets"   # optional; the default
```

Files in `dir` ship inside the app and load offline. Show one with `image("asset:logo.png", ...)`,
`avatar("asset:people/ann.jpg")` or a video poster; those are the widgets that accept `asset:`. Names are
case-sensitive, and a missing file shows as a failed load. Everything in the folder ends up in the app, so
keep only files shown at runtime: they add to the download and installed size.

`dir` can't start with a folder the CLI writes (`Android`, `iOS`, `web`, `shared`, `.mobiler`, `target`) or
name a hidden folder (`.git`, anything starting with `.`), and each segment may use only letters, digits,
space, `-`, `_`, `.` and `+`. The CLI warns about a font, splash
or icon source inside it and about symlinks in it. If the folder doesn't exist yet (git doesn't keep an empty
one), only Android is set up for it: the web and iOS builds would fail on a missing folder, so they include it
once it exists. Create it, then build or run `mobiler assets sync`. An app made before CLI 0.75 runs
`mobiler upgrade --apply` once to get the markers the sync fills.

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
