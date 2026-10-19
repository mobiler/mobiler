# iOS glass tab bar, new icons, and Saldo finished as a CLI-managed app

**Asked by:** the maintainer, 2026-10-19. Saldo's refresh (branch `saldo-cli-managed`, ADR-0059) would drop Saldo's
look, which lived as hand edits in its old shells: the iOS floating Liquid Glass tab bar and glass add button
(#171, #173, #175), the finance tab icons (#180) and the Aqua Mint background and tint (#187). They chose **framework
first**: add the missing pieces to the framework, then finish Saldo on top. Their words on the glass bar: "it is iOS
option, and if Android has something similar we should show them that, or even maybe hide option for them in case
that it doesn't do anything". On icons: "Moj Termin has its own icons... you do not need to touch it".
**Release:** libraries first (`mobiler-ui` 0.33, `mobiler-core` 0.48, `mobiler-web` 0.48), then **CLI 0.72.0**, which
carries the template shells and Saldo's refresh.
**Constrained by:**
- ADR-0001: shells stay generic renderers of the `Widget` tree.
- ADR-0009 / the template-lands-after-publish rule: template code using a new ABI item lands in the CLI PR after the
  libraries are live. Barbershop and Saldo build against the workspace, so their shells change in the libraries PR.
- ADR-0015: barbershop's shells change first; the template is ported from its diff.
- ADR-0019: theme tokens are optional; an unset one renders exactly as before.
- ADR-0021: iOS 17 deployment target, so iOS 26 APIs need an availability check.
- ADR-0050: the tab bar and rail draw badges; the glass bar must too.
- ADR-0057 / ADR-0058: kept tab screens; the glass bar sits over them.
- ADR-0059 (on the held branch, not yet merged, so it is tightened in place): Saldo equals the current template.

**New record:** ADR-0060 — a platform-only option in the shared core. A widget option that only one platform can
render is named for that platform (`ios_…`), documented as a no-op elsewhere, and every other shell ignores it, so an
app sets it once and each platform shows its own native style. The glass tab bar is the first.

## Why

Saldo is the showcase app people will try. Its look was built by editing its shells, which ADR-0059 now forbids. The
pieces must exist in the framework before Saldo can use them through ordinary APIs, and once there, every app gets
them.

## Goals

1. **iOS glass tabs, an iOS-only theme option.** An app opts in on its `Theme`. On iOS:
   - the bottom tab bar becomes a floating capsule over the content (content scrolls behind it), with Liquid Glass on
     iOS 26+ and a frosted material capsule below 26 (Saldo's old `glassPill` / `frostedPill`);
   - the FAB becomes a matching glass circle with a brand-tinted glyph, raised above the floating bar.
   On Android and the web nothing changes: the standard bottom bar and FAB. Material 3 has no navigation-bar
   equivalent (its floating toolbar is for actions), so there is nothing to map it to.
   The option's name says it is iOS-only, and its doc says "iOS only; Android and the web keep the standard bar".
   **My call: it lives on the `Scaffold`, not on `Theme`** — a builder `with_ios_glass_tabs()` and a field
   `ios_glass_tabs: bool` (default false), like `with_bottom_bar`. `Theme` has public fields and apps (barbershop, and
   very likely Moj Termin) build it as a full struct literal, so a new `Theme` field would break their code; a
   `Scaffold` builder doesn't. The wire ABI changes either way (see Anti-goals). An app sets it on its root scaffold.
   **The FAB under glass (my call):** a plain FAB becomes a glass circle; an extended FAB (icon + label) becomes a glass
   capsule that keeps its label. A set palette `fab` role tints the glass (`glassEffect(.regular.tint(…))` on 26+, the
   frosted capsule's tint below); a set `on_fab` colours the glyph and label; unset, the glyph uses the brand colour
   (ADR-0019: a set role still renders).
2. **More built-in icons.** Add icons drawn natively on all three shells (Material on Android, SF Symbols on iOS, a
   glyph on the web). My call on the set: Saldo's three plus a few common ones the current 33 lack:
   `Ledger` (ReceiptLong / list.bullet.rectangle.fill), `Chart` (ShowChart / chart.line.uptrend.xyaxis), `Wallet`
   (AccountBalanceWallet / wallet.bifold.fill), `CreditCard`, `Lock`, `Download`, `Upload`, `Globe`. The builder may
   adjust the platform glyph names to what exists on iOS 17 and in `material-icons-extended`.
3. **Saldo's Aqua Mint look through config.** Saldo's Rust core gets a `Theme.palette` (ADR-0019 roles) that restores
   #187: the light Aqua Mint background (`#E9F6F1`), brand teal for the selected tab, toggles, star and info tone (no
   system blue), and dark mode as before. Saldo's tabs use the new `Ledger` / `Chart` / `Wallet` icons and turn on
   `ios_glass_tabs`.
4. **Saldo finished as a CLI-managed app** (the held PR 2, rebased onto the CLI 0.72 template), with the review's
   fixes:
   - The ADR-0059 test is tightened. An inserted block must be a bundled plugin's `register` / dependency / permission
     line from its `mobiler-plugin.toml`. Each of Saldo's plugin source files must equal the bundled source rendered.
     Every template file must be in the base snapshot or on an explicit allowlist. `Info.plist` and
     `settings.gradle.kts` are checked. The `CFBundleDisplayName` exception matches exactly
     `CFBundleDisplayName: "Saldo"` once.
   - `demos/saldo/Android/build-android.sh` is committed executable. `new_app_scripts_are_executable` checks the
     template's real script paths exist.
   - The picker plugin can't hang:
     - Android resolves the request on any dismiss (`setOnDismissListener`), not only cancel or a pick.
     - iOS presents only from a view controller that isn't being dismissed, and answers `ok:false` if presenting
       fails.
     - Both default `selected` to none (-1) the same way.
   - The APK workflow's header comment says the key is new per run, so an update means uninstalling, which wipes
     data. It picks a stable build-tools version, never an `-rc`.
   - The spec `2026-10-19-ios-kept-tabs-and-saldo-refresh-design.md` gets a dated amendment recording the hold and
     this follow-up.
5. **Lost-behaviour audit, done right — first, before the libraries PR (my call).** Every Saldo-touching commit (`git log -- demos/saldo`, the
   design pass #169–#188 and later) is checked against the new shells by content (`git log -S` per symbol), and
   listed in the PR body with how it is now covered. Nothing Saldo did may be silently dropped. Running it first means
   a gap that needs an ABI item joins this libraries release instead of forcing a second publish. A gap found that
   isn't worth a framework feature is brought to the maintainer before the libraries publish, never dropped
   silently.

## Non-goals

- Moj Termin (appointments): untouched.
- A glass or floating style on Android or the web.
- App-supplied custom icons (an SVG or asset icon type). The built-in set grows instead.
- Store publishing (Play, App Store). TestFlight and the maintainer's A32 only.
- The other demos (coffee, todo, fullstack-todo) stay as they are.

## Anti-goals

- An option that silently does nothing on a platform without saying so in its name and doc.
- Any change to how an app looks when it doesn't set the new option or use the new icons (ADR-0019).
- App code that stops compiling: hence the `Scaffold` builder, not a `Theme` field.
- An old shell crashing on a new icon. The new `Icon` variants and the `Scaffold` field are an ABI change: the libraries and the CLI ship
  together as one release step, as for every enum addition before. The release notes and README name it, as for
  `Tab.badge`.
- Saldo regressing anything it does today on either platform.

## The how, as decided

- **iOS rendering** (barbershop first, then the template):
  - The floating capsule holds the existing tab buttons, including badges (ADR-0050) and `tabSpoken`.
  - The body gets bottom padding so the last row can scroll clear of the bar.
  - On a regular width (iPad), the rail is used as today, and the option affects only the phone bottom bar.
  - The `#if compiler(>=6.2)` + `#available(iOS 26.0, *)` gating follows Saldo's old `glassPill`.
  - With a `bottom_bar` (with_bottom_bar), the pinned bar stays above the floating tab bar (the builder decides the
    exact stacking and reports it).
- **Kept tabs (ADR-0058)** are untouched: the glass bar is chrome around `TabSlots`.
- **Web:** the new icons get glyphs. `ios_glass_tabs` is read and ignored.
- **Order of work:**
  0. The Saldo audit (Goal 5), against the CLI 0.71 template; its findings may add to step 1.
  1. Libraries PR: `mobiler-ui` adds the `Icon` variants and the `Scaffold` field with its builder; `mobiler-core`
     re-exports them; `mobiler-web` gets the glyphs. Every demo's shells get the new icon arms (coffee, todo,
     fullstack-todo and main's old Saldo shells have exhaustive icon switches and build against the workspace core;
     only the minimal arms, as in #235). The barbershop shells render the glass option, and a Profile toggle (like
     "Large controls") turns it on, so TestFlight shows both bars and the stock look and screenshots stay. Then ADR-0060 and the
     conformance test. Then publish the libraries (an irreversible step that needs the maintainer's OK).
  2. CLI PR: template shells ported from barbershop; template pins move to the new libraries; Saldo's held branch
     rebased and finished as Goals 3–5; `mobiler upgrade --apply` on Saldo; CLI 0.72.0.
  3. Device checks: Saldo on the A32 (I drive it), and Saldo and barbershop on iPhone through TestFlight (the
     maintainer). Then tag CLI 0.72.0 with the maintainer's OK.

## Left to the builder

- The exact SF Symbol / Material / web glyph per new icon, and whether the set needs trimming for availability.
- The capsule's exact padding, shadow and the FAB's offset above it (start from Saldo's old values: capsule padding
  18/10, outer 16/6, FAB 56 pt raised 92 pt).
- How the ADR-0060 conformance test proves every non-iOS shell ignores the option and the iOS shell reads it (and its
  mutation proof).
- The palette's exact role values beyond the background, taken from #187's diff.

## Risks

- **Liquid Glass compile gating:** CI's Xcode must be 26+ for `glassEffect` (TestFlight already requires it), while
  the `#if compiler(>=6.2)` fallback keeps older toolchains building.
- **New icons are an ABI change:** an app whose shells are older than its `mobiler-core` fails to decode a new icon.
  Covered by the usual upgrade note and `mobiler upgrade`.
- **Saldo audit misses again:** mitigated by Goal 5's commit-by-commit check and the reviewers' explicit brief.

## Amendment 2026-10-19 (the Saldo audit, before any code)

The commit-by-commit audit (65 commits touching Saldo's shells) found two more iOS gaps beyond the known ones: #188's
outlined buttons (a 10% teal fill and a 45% teal border) and #187's two selected styles (a solid teal segment vs a pale
teal chip, which the template draws from one `secondary_container` pair). The maintainer chose **exact parity through
new optional palette roles** in this release:

- `outlined_fill: Option<Rgba>` — an outlined button's background. Unset: transparent, as before.
- `outlined_border: Option<Rgba>` — an outlined button's border. Unset: `outline`, as before.
- `segment_selected: Option<Rgb>` / `on_segment_selected: Option<Rgb>` — the selected segment's fill and text. Unset:
  the `secondary_container` pair, as before.

All three shells (Android, iOS, web) draw them; ADR-0019 already covers adding roles. Saldo's palette sets them. Two
smaller differences stay, as rulings: the template's filled-button padding (14×7 vs Saldo's 18×11) is not changed,
because it would change every app's buttons; and Saldo's palette must set `info` (solid teal, white text) so the
swipe "Edit" isn't system blue, and leave `danger` and a dark `background` unset.
