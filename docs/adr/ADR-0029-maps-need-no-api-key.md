# ADR-0029: `Widget::Map` renders with engines that need no API key or per-app account (MapKit on iOS, MapLibre with a keyless default style on Android and web), so a map works in a fresh scaffold with no configuration

Status:        Accepted
Date decided:  2026-06-09
Deciding PRs:  #144, #146
Supersedes:    none
Code anchor:   mobiler-ui/src/lib.rs (Widget::Map, MapMarker), mobiler-core/src/lib.rs (map, with_markers, with_map_style, marker, marker_titled), template shells (iOS Render.swift MapWidgetView; Android MainActivity.kt MapWidget + `org.maplibre.gl:android-sdk` in app/build.gradle.kts), mobiler-web/src/lib.rs (inject_maplibre_support, the Map arm)
Conformance:   none — the engine and its keyless default live in Kotlin, Swift, gradle and injected JS with no unit-test harness; the builds would still pass with a keyed SDK (a missing key fails at runtime), so review enforces it

## 1. Context (The Problem)

Batch 7 of the roadmap added a map widget. The common mobile map SDKs need an API key tied to an
account: Google Maps on Android needs one in the app's manifest. A framework widget is rendered by
the generic shell that every app gets (ADR-0001), so a keyed engine would mean either every app
configures a key before any map shows, or the CLI grows per-app key plumbing. A key is also a
billing relationship with a third party.

## 2. Hypothesis

If each shell renders `Widget::Map` with an engine that needs no key:

- a `map(id, lat, lng, zoom)` call shows a working map in a freshly scaffolded app on every shell,
  with no manifest, plist or CLI step;
- no app is signed up for a paid map account by using the framework;
- an app that wants a different look on Android and web passes its own `style_url`.

### 2.1. Refutation Conditions

- **Condition 1 — no key anywhere.** No template or shell file carries a map API key, key
  placeholder or keyed map SDK dependency.
  - **Validation Metric:** review. (`grep -rn 'com.google.android.geo\|play-services-maps'` over
    `mobiler/templates` is empty today.)
- **Condition 2 — the default style is keyless.** With `style_url: None`, Android and web load
  `https://tiles.openfreemap.org/styles/liberty`.
  - **Validation Metric:** review. `widget_round_trips` covers only that `style_url` crosses the
    wire, not which style loads.

## 3. Considered Options & Rationale for Refutation

- **Option A — Google Maps SDK on Android (and Google or Apple on iOS)** `[reconstructed]`
  Not chosen. Needs a per-app API key and account before a map renders, and would need CLI support
  to write the key into the Android manifest.
- **Option B — MapKit on iOS, MapLibre on Android and web, keyless default style** `[recorded: commit 7fe9dde ("Per-platform engine (no key):"; "Default style = OpenFreeMap liberty (free, no key)."); mobiler-ui/src/lib.rs, the `Widget::Map` doc comment: "web MapLibre-GL — no API key"]`
  Chosen. MapKit ships with iOS and needs no key or package. MapLibre is open source and loads any
  MapLibre style URL.
- **Option C — a map as an opt-in plugin, so only apps that want it pay the setup** `[reconstructed]`
  Not chosen. A map is a view in the widget tree, not an effect, and plugins can't add widget
  variants (ADR-0002, ADR-0011).

## 4. Decision & Rationale for Corroboration

Option B. PR #144 added `Widget::Map` in mobiler-ui 0.22 and the arms in the demos: iOS `MKMapView`
through `UIViewRepresentable`; Android MapLibre Native (`org.maplibre.gl:android-sdk:11.5.2`, an
always-on dependency) with markers as a GeoJSON `CircleLayer`; the web lazily loads `maplibre-gl@4`
from jsDelivr on the first `.mobiler-map`. PR #146 put the arms and the MapLibre dependency into the
CLI templates after the libraries published (ADR-0009). Taps come back as `Action::Input`
(ADR-0026).

## 5. Consequences (Positive and Negative Predictions)

- **Positive:** maps work in every new app, on all three shells, with nothing to configure and no
  account.
- **Negative:** the platforms don't look the same. iOS shows Apple Maps and ignores `style_url`;
  Android and the web show the MapLibre style.
- **Negative:** the default tiles come from a free public service (OpenFreeMap) with no service
  agreement. An app with real traffic should pass its own `style_url`, and nothing tells it to.
- **Negative:** every Android app ships the MapLibre SDK whether it shows a map or not, and the web
  shell depends on jsDelivr at runtime for the first map.
- **Negative:** Google-only features (Street View, Google Places) aren't reachable through
  `Widget::Map`. An app that needs them has no route inside the framework today.
