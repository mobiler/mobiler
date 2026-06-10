# Chapter 10 — Security, settings & polish

Saldo holds someone's whole financial life, so it should lock. And after nine chapters of *function*, it
deserves a *look*. This final chapter adds an optional **biometric app lock**, the Saldo **brand theme**
with a **light/dark** toggle, and a real **app icon** — and with that, the app is feature-complete.

## 1. A biometric app lock

We don't need to invent a passcode screen: the device already has one. The `biometric` plugin runs Face
ID / Touch ID and **falls back to the device passcode** automatically, so one call covers everything.

```sh
mobiler plugin add biometric
```

The lock is a persisted setting plus a runtime flag. On the **first** settings load after launch (a
`lock_checked` guard makes sure it's only the first), if the lock is on we set `locked` and prompt:

```rust
"lock" if v == "1" => {
    model.lock_enabled = true;
    if first_load {
        model.locked = true;
        cx.plugin("biometric", "authenticate", tr(model, "lock.prompt"), |r| Msg::Authed(r.ok));
    }
}

Msg::Authed(ok) => model.locked = !ok, // unlock on success; stay locked (and offer retry) otherwise
```

`view()` short-circuits to a lock screen whenever `locked`, so **nothing else ever renders** until you
authenticate:

```rust
fn view(&self, model: &Model) -> Widget {
    if model.locked {
        let lock = column(vec![ /* title, hint, */ button(tr(model, "lock.unlock"), Filled, Msg::Unlock) ]);
        return with_theme(scaffold("Saldo", model.dark, vec![], lock), saldo_theme());
    }
    // …the normal app…
}
```

A **Security** card in Settings toggles it (`Msg::SetLock`), persisting `"lock"` to the settings table.
For storing actual *secrets* (tokens, keys) you'd pair `biometric` with the `securestore` plugin
(Keychain / EncryptedSharedPreferences) — Saldo's lock only needs the yes/no, so the setting suffices.

> On Android the `biometric` prompt needs a `FragmentActivity` host — the Mobiler template's
> `MainActivity` already is one, so there's nothing to wire.

## 2. A brand, as data

Mobiler carries the theme **on the scaffold**, so branding is one wrapper — no new builders, no CSS. A
`Theme` is just colour + shape + type:

```rust
fn saldo_theme() -> Theme {
    Theme {
        seed: Rgb::new(0x0E, 0x9F, 0x8E), // teal — money / balance
        accent: None,                      // derived from the seed
        corner: Corner::Large,
        density: Density::Comfortable,
        font: FontFamily::System,
    }
}

// at the end of view():
with_theme(root, saldo_theme())
```

The shells map `seed` to the platform's accent — Material 3's colour scheme on Android, the tint on iOS,
the CSS custom properties on web — so the whole app re-skins from those three bytes.

## 3. Light & dark

`scaffold`'s second argument is a `dark_mode: bool` — theme-as-data again, owned by the app. So dark mode
is just another persisted setting and a toggle, threaded into the scaffold:

```rust
Msg::SetDark(on) => { model.dark = on; save_setting(cx, "dark", if on { "1" } else { "" }); }

// view():
let mut root = scaffold(heading, model.dark, tabs, body);
```

An **Appearance** card in Settings flips Light/Dark. (Mobiler leaves "follow the system" to the app —
there's no OS-appearance signal in the core — so Saldo makes it an explicit choice that sticks.)

## 4. A real icon

The scaffold gave us a placeholder diamond; Saldo ships a teal **"S" monogram**. iOS takes a single
1024² source (`Assets.xcassets/AppIcon.appiconset/icon-1024.png`) and `actool` derives the rest; Android
uses an **adaptive icon** — a teal background vector plus the white "S" as the foreground layer — with the
legacy `mipmap` rasters regenerated to match for pre-adaptive devices.

## What we built — and the whole app

A biometric lock that gates the app on launch, a teal brand theme with light/dark, and a real icon. The
security and theming are entirely **data the core declares** — `cx.plugin("biometric", …)`, a `Theme` on
the scaffold, a `dark_mode` bool — which is the through-line of this whole build: **one Rust core, typed
state in and a `Widget` tree out, rendered natively on iOS, Android and the web.**

Across ten chapters Saldo grew from an empty scaffold into a real money manager — a persisted
income/expense/transfer ledger, accounts and net worth, an editable category tree, Stats charts,
locale-aware formatting, five languages, recurring transactions, CSV/JSON export & backup, and now a lock
and a brand. Along the way it justified two framework primitives — `mobiler_core::format`'s Ukrainian
support and the new `mobiler_core::i18n` — exactly the point of building a real app on the framework.

> Full source: [`demos/saldo/shared/src/app.rs`](../../../demos/saldo/shared/src/app.rs). That's the
> whole app — every screen, in one file you can read top to bottom.
