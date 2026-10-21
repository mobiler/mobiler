# mobiler.rs — the documentation site

**Asked by:** the maintainer, 2026-10-20: "mobiler.rs should be useful information for developers who wish to build
web & mobile apps from shared code (same as we do in appointments now)… explain what is shared and reused… point to
our demos as example of code and Saldo as step-by-step tutorial and available in store to try and play with it."
"You can continue alone." "Just use latest versions of astro and other libs."
**Constrained by:** ADR-0001 (generic shells: the app writes no native code), ADR-0015 (barbershop is the reference
shell), ADR-0023 (the web shell is `mobiler-web`), ADR-0063 (demos but barbershop are CLI-managed). No new ADR: the
site is documentation, not a contract.

## Who it is for

A developer who wants one codebase for a mobile app (Android and iOS) and a web app, possibly with a Rust server,
and is deciding whether Mobiler fits. They leave knowing:
1. what they write once and what Mobiler provides per platform;
2. how a real app is laid out (core, shells, web client, server sharing crates);
3. where to see real code (the demos) and how to build one step by step (the Saldo tutorial);
4. how to start (`cargo install mobiler`, `mobiler new`) and stay current (`mobiler upgrade`).

## What it says

- **Home:** "One Rust core. Native Android, native iOS, and the web." The coffee trio (Android, iOS, web
  screenshots of one core), three cards (shared core, native widgets, capabilities), and buttons to Get started,
  the tutorial and the demos. The experimental status is stated, as in the root README.
- **What's shared:** the heart of the site. A table of what lives once in Rust (state, business logic, validation,
  navigation, the UI as a `Widget` tree, theme and palette, i18n and formatting, HTTP and storage calls, device
  capabilities through plugins) against what is per platform and maintained by the CLI (the generic Compose /
  SwiftUI / web shells, plugin native halves, icons, display name, store config). The full-stack shape: a `shared`
  crate used by the mobile core, the web client and an Axum server (fullstack-todo is the public example; the
  appointments app is not named). Honest limits: native look follows each platform; platform-only options
  (iOS glass tabs); experimental status; what the web shell does differently (it rebuilds the DOM).
- **How it works:** core → `Widget` tree → generic shell → events back; capabilities as plugins; the wire ABI is
  fixed so the shells never change per app.
- **Get started:** install, `mobiler new`, `dev`/`build` for Android, iOS and web, the project layout.
- **Guides:** upgrading, theming and palette, capabilities and plugins, `mobiler.toml` options (deep links,
  splash, `[ios] local_network`, display name), the web client, full-stack with a server.
- **Reference (generated):** built-in capabilities from `capabilities.json`, plugins from each
  `mobiler/plugins/*/mobiler-plugin.toml`, widgets and CLI commands.
- **Examples:** each demo with its screenshots, what it shows, and a link to its code on GitHub.
- **Tutorial:** the ten Saldo chapters, plus "Try Saldo": store badges when the listings are live; until then a
  "coming to the App Store and Google Play" note (Saldo is on TestFlight only, Play lane not built).
- **Decisions:** a page pointing to the ADR index on GitHub.

## How it is made (my calls)

- `site/` in this repo: Astro 7.4 + Starlight 0.42 (the latest at build time), npm with a committed lockfile,
  `starlight-links-validator` so a broken link fails the build.
- **One source of truth.** Pages that exist elsewhere in the repo are generated at build time by
  `site/scripts/sync.mjs`, never copied by hand: the tutorial chapters, the capability and plugin reference and
  the demo screenshots. Generated files are gitignored. Relative links in imported Markdown are rewritten to site
  routes (chapters) or GitHub URLs (code).
- **Accuracy.** The tutorial was written for CLI 0.47 (June). Its chapters are audited against the current
  API and Saldo's current code. Drift is fixed in `docs/tutorial/saldo/` (living docs); a chapter that tells
  history (e.g. "we add i18n to the framework") stays, with a note on the version it landed in.
- **Living docs rule.** CLAUDE.md's "Every README that describes a behaviour moves with it" extends to
  `site/src/content/docs/`.
- **CI:** a `site` job builds the site (so links and generation are checked on every PR).
- **Deploy:** a GitHub Pages workflow, manual (`workflow_dispatch`) until the maintainer has set the
  `mobiler.rs` DNS and says to publish. Publishing is outward-facing, so it waits for them.
- **Look:** Starlight's default theme, a teal accent, a text wordmark. No custom design system.

## Non-goals

- A blog, search beyond Starlight's built-in, translations of the site, versioned docs.
- Naming or showing the appointments app.
- Building the Play lane or store listings (separate work).

## Anti-goals

- A page that contradicts the code (a stale API in a sample): worse than no page.
- Hand-copied content that drifts from the README or the tutorial.
- Overselling: the site keeps the README's "experimental" honesty.
