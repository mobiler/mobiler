# Large Touch Targets — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship the appointments app's "busy hands" request: a theme-level `Density::Large` that enlarges controls, button tone/icon/full-width + a Tonal style, a localized calendar with 0–3 busy-dots per day, and an opt-in scroller edge fade, as one libs release (ui 0.24 / core 0.35 / web 0.35) and one CLI release (0.52).

**Architecture:** Every piece is a `mobiler-ui` wire-ABI change (new enum variants / widget fields), so the shells (web `mobiler-web`, Android `MainActivity.kt`, iOS `Render.swift`) change with it. Locale-dependent calendar layout is computed in `mobiler-core` so shells only draw. New behaviour is gated so that existing apps (`Compact`/`Comfortable`, neutral tone, no icon, not wide, no markers, no fade) render **pixel-identical** to 0.51.0.

**Tech Stack:** Rust (facet typegen ABI), Leptos/WASM + CSS (web), Jetpack Compose Material 3 (Android), SwiftUI iOS 16+ (iOS).

**Spec:** `docs/large-touch-targets.md` (the appointments team's request, untracked) + the decisions below, agreed with the user 2026-09-17.

## Agreed decisions (do not relitigate)

- `Density::Large` changes **control sizes, control spacing, and control label sizes only**. Body text is left to the system font scale (Android `fontScale`, iOS Dynamic Type, browser font size). No per-widget sizes in the app API.
- Danger is a **tone**, not a style. `ButtonStyle` gains `Tonal`. `Widget::Button` gains `tone: Tone` (reusing the existing `Tone`, where `Neutral` = today's primary look), `icon: Option<Icon>`, and `wide: bool`. `button()` keeps its signature. The new builder is `button_with(label, style, on_press, ButtonOpts)`.
- The calendar: core computes `title`, the rotated `weekday_labels`, `leading_blanks`, and clamped `markers`. The week start comes from the `Locale` (`EnUs` = Sunday, all others = Monday). `calendar()` = `calendar_in(Locale::EnUs, …, &[] …)`, which renders identically to today.
- The scroller hint is an **opt-in trailing-edge fade** plus a 32-unit trailing spacer, so at scroll-end the fade covers blank space and no scroll-state detection is needed. Builder: `scroller_hinted(children)`.
- Week-strip dots stay app-side text (`•`) for now. No new dots widget.

## Global Constraints

- **Pixel identity:** with `Compact`/`Comfortable`, neutral tone, no icon, `wide: false`, empty `markers`, and `edge_fade: false`, every shell must produce the same modifiers/params/DOM classes as 0.51.0 (enforced by code review, not an emulator diff — the only framework users are our demos and the appointments app). Pattern: `if (large) X else <the exact original value>`. Never "equivalent" values: `TextButton`'s default padding is `ButtonDefaults.TextButtonContentPadding`, not `ContentPadding`.
- **Release shape = two PRs, one lib publish, one CLI publish.** The CI lane `scaffold + build (template, Android)` scaffolds from the template against **crates.io** `mobiler-core`. So template shell changes cannot land before core 0.35 is published.
  - **PR-A** (`feat/large-touch-targets`) = libs + all 5 demos' shells + barbershop usage + version bumps. **Template untouched.** It has one commit per feature and is merged with **rebase-merge** (not squash).
  - Then `release-libs` publishes ui 0.24.0 → core 0.35.0 → web 0.35.0.
  - **PR-C** (`feat/large-touch-targets-cli`) = template `Render.swift` + `MainActivity.kt` port, the `Cargo.toml.tmpl` pin `"0.35"`, CLI 0.51.0 → 0.52.0, and READMEs. Then `release-cli` with tag `v0.52.0`.
- The 5 demo shells are `demos/{coffee,todo,barbershop,saldo}` and `demos/fullstack-todo/mobile`. The iOS `Render.swift` is **byte-identical** in coffee/todo/barbershop/fullstack-todo and the template, but **saldo diverges**. Android `MainActivity.kt` drifts in all of them. Edit barbershop first, `cp` the iOS file to the 3 identical ones, and port by anchor to saldo iOS + the other 4 Android files.
- Generated `SharedTypes` / `*/generated/` are gitignored. **Commit source only.**
- iOS Swift compiles **only on macOS CI** (`iOS build (…)` lanes). Android and web compile locally.
- Local build gotchas:
  - `~/.cargo/config.toml` sets a global target-dir, so Android builds need `CARGO_TARGET_DIR=$PWD/target`.
  - The CLI binary is `/media/zmilan/data2/cargo-target/debug/mobiler`.
  - Use JDK `~/jdk21`.
  - The AVD `mobiler_pixel7` has a full /data partition (do not uninstall apps from it), so use `Pixel_Fold_API_36`.
- `NOTES.md` is gitignored. Update it, never `git add` it. Do not commit `docs/large-touch-targets.md`.
- `main` is protected. Land PRs with the `ship-pr` skill, but merge PR-A with `gh pr merge --rebase`.
- `SCRATCH` below = `/tmp/claude-1000/-home-zmilan-working-docker-rust-mobiler/3e954f6a-5e08-4305-bca1-3726d22b06c6/scratchpad` (or any session scratch dir).

### Generated-type names shells will see

| Rust | Kotlin | Swift |
|---|---|---|
| `Density::Large` | `Density.LARGE` | `.large` |
| `ButtonStyle::Tonal` | `ButtonStyle.TONAL` | `.tonal` |
| `Widget::Button { label, style, on_press, tone, icon, wide }` | `widget.tone`, `widget.icon: WidgetIcon?`, `widget.wide: Boolean` | `.button(label, style, onPress, tone, icon, wide)` |
| `Widget::Calendar { year, month, title, weekday_labels, leading_blanks, selected, on_day, markers }` | `widget.title`, `widget.weekdayLabels: List<String>`, `widget.leadingBlanks: UByte`, `widget.markers: List<UByte>` | `.calendar(year, month, title, weekdayLabels, leadingBlanks, selected, onDay, markers)` |
| `Widget::Scroller { children, edge_fade }` | `widget.edgeFade: Boolean` | `.scroller(children, edgeFade)` |

## File Structure

| File | Responsibility |
|---|---|
| `mobiler-core/src/format.rs` | `Weekday`, `Locale::week_start`, `weekday_short`, `month_year` |
| `mobiler-ui/src/lib.rs` | ABI: `Density::Large`, `ButtonStyle::Tonal`, `Button`/`Calendar`/`Scroller` fields + round-trip tests |
| `mobiler-core/src/lib.rs` | builders `calendar_in`, `button_with` + `ButtonOpts`, `scroller_hinted`; re-export `Weekday` |
| `mobiler-web/src/lib.rs`, `mobiler-web/src/mobiler.css` | web render arms + CSS |
| `demos/*/Android/app/src/main/java/**/MainActivity.kt` (5) | Compose render arms |
| `demos/*/iOS/Sources/Render.swift` (5) | SwiftUI render arms |
| `demos/barbershop/app-core/src/lib.rs` | showcase: Large toggle, Serbian calendar toggle, toned/wide/icon buttons, hinted scroller |
| `mobiler/templates/{Android,iOS}/…`, `mobiler/templates/shared/Cargo.toml.tmpl`, `mobiler/Cargo.toml` | PR-C only |

---

# PR-A — libs + demos

### Task 1: Branch + drift baselines (no commit)

**Files:** none modified. This task writes to `$SCRATCH/baseline/`.

- [ ] **Step 1: Branch from fresh main**

```bash
cd /home/zmilan/working_docker/rust/mobiler
git switch main && git pull --ff-only
git switch -c feat/large-touch-targets
git tag ltt-base
```

- [ ] **Step 2: Record shell drift baselines** (used by Task 16 to prove ports didn't diverge)

```bash
mkdir -p $SCRATCH/baseline
B=demos/barbershop
for d in coffee todo saldo fullstack-todo/mobile; do
  n=$(echo $d | tr / _)
  diff $B/Android/app/src/main/java/dev/mobiler/barbershop/MainActivity.kt $(find demos/$d/Android -name MainActivity.kt -not -path '*/build/*') | grep '^[<>]' > $SCRATCH/baseline/android-$n.diff
done
diff $B/Android/app/src/main/java/dev/mobiler/barbershop/MainActivity.kt mobiler/templates/Android/app/src/main/java/__PACKAGE_PATH__/MainActivity.kt | grep '^[<>]' > $SCRATCH/baseline/android-template.diff
diff $B/iOS/Sources/Render.swift demos/saldo/iOS/Sources/Render.swift | grep '^[<>]' > $SCRATCH/baseline/ios-saldo.diff
wc -l $SCRATCH/baseline/*.diff
```

Expected: non-empty android diffs, ios-saldo ≈ 250 lines.

---

### Task 2: Locale weekdays, week start, month title (mobiler-core `format`)

**Files:**
- Modify: `mobiler-core/src/format.rs` (types after `enum Currency`; functions after `month_name`; tests in `mod tests`)
- Modify: `mobiler-core/src/lib.rs:15` (re-export)

**Interfaces:**
- Produces:
  - `pub enum Weekday { Sunday, Monday, Tuesday, Wednesday, Thursday, Friday, Saturday }` with `const fn sun0(self) -> u8`
  - `Locale::week_start(self) -> Weekday`
  - `pub fn weekday_short(sun0: u8, locale: Locale) -> &'static str` (wraps mod 7)
  - `pub fn month_year(year: u32, month: u32, locale: Locale) -> String` (capitalized month + space + year)
  - `mobiler_core::Weekday` re-export

- [ ] **Step 1: Write the failing tests** (append inside `mod tests` in `format.rs`)

```rust
    #[test]
    fn week_start_and_weekday_labels() {
        assert_eq!(Locale::EnUs.week_start(), Weekday::Sunday);
        assert_eq!(Locale::EnGb.week_start(), Weekday::Monday);
        assert_eq!(Locale::SrLatn.week_start(), Weekday::Monday);
        assert_eq!(Weekday::Tuesday.sun0(), 2);
        let sr: Vec<&str> = (1..8).map(|i| weekday_short(i, Locale::SrLatn)).collect();
        assert_eq!(sr, ["P", "U", "S", "Č", "P", "S", "N"], "Monday-first Serbian, wraps past Saturday");
        let en: Vec<&str> = (0..7).map(|i| weekday_short(i, Locale::EnUs)).collect();
        assert_eq!(en, ["S", "M", "T", "W", "T", "F", "S"]);
        assert_eq!(weekday_short(1, Locale::SrCyrl), "П");
        assert_eq!(weekday_short(1, Locale::UkUa), "Пн");
        assert_eq!(weekday_short(4, Locale::ItIt), "G");
    }

    #[test]
    fn month_year_capitalizes() {
        assert_eq!(month_year(2026, 6, Locale::EnUs), "June 2026");
        assert_eq!(month_year(2026, 9, Locale::SrLatn), "Septembar 2026");
        assert_eq!(month_year(2026, 9, Locale::SrCyrl), "Септембар 2026");
        assert_eq!(month_year(2026, 6, Locale::UkUa), "Червень 2026");
    }
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p mobiler-core format::tests`
Expected: FAIL to compile (`Weekday`, `weekday_short`, `month_year` not found).

- [ ] **Step 3: Implement** (in `format.rs`: the enum after `enum Currency`, the `impl` + constants + functions directly after `fn month_name`)

```rust
/// A day of the week — the first column of a localized calendar ([`Locale::week_start`]).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Weekday {
    Sunday,
    Monday,
    Tuesday,
    Wednesday,
    Thursday,
    Friday,
    Saturday,
}

impl Weekday {
    /// 0 = Sunday … 6 = Saturday (the index `weekday_short` and the calendar layout use).
    #[must_use]
    pub const fn sun0(self) -> u8 {
        self as u8
    }
}
```

```rust
impl Locale {
    /// The first day of the week: Sunday for US English, Monday for every other supported locale.
    #[must_use]
    pub const fn week_start(self) -> Weekday {
        match self {
            Locale::EnUs => Weekday::Sunday,
            _ => Weekday::Monday,
        }
    }
}

// Narrow weekday labels for a calendar header, Sunday-first (index = `Weekday::sun0`).
const WEEKDAYS_EN: [&str; 7] = ["S", "M", "T", "W", "T", "F", "S"];
const WEEKDAYS_DE: [&str; 7] = ["S", "M", "D", "M", "D", "F", "S"];
const WEEKDAYS_FR: [&str; 7] = ["D", "L", "M", "M", "J", "V", "S"];
const WEEKDAYS_IT: [&str; 7] = ["D", "L", "M", "M", "G", "V", "S"];
// Ukrainian calendars use the two-letter forms (single letters are ambiguous).
const WEEKDAYS_UK: [&str; 7] = ["Нд", "Пн", "Вт", "Ср", "Чт", "Пт", "Сб"];
const WEEKDAYS_SR_LATN: [&str; 7] = ["N", "P", "U", "S", "Č", "P", "S"];
const WEEKDAYS_SR_CYRL: [&str; 7] = ["Н", "П", "У", "С", "Ч", "П", "С"];

/// The localized narrow weekday label for a calendar header. `sun0` is 0 = Sunday … 6 = Saturday
/// and wraps, so `weekday_short(start + i, …)` walks a week from any start day.
#[must_use]
pub fn weekday_short(sun0: u8, locale: Locale) -> &'static str {
    let idx = usize::from(sun0 % 7);
    match locale.lang() {
        Lang::En => WEEKDAYS_EN[idx],
        Lang::De => WEEKDAYS_DE[idx],
        Lang::Fr => WEEKDAYS_FR[idx],
        Lang::It => WEEKDAYS_IT[idx],
        Lang::Uk => WEEKDAYS_UK[idx],
        Lang::SrLatn => WEEKDAYS_SR_LATN[idx],
        Lang::SrCyrl => WEEKDAYS_SR_CYRL[idx],
    }
}

/// A calendar title: the localized month name, capitalized, then the year (`"Septembar 2026"`).
#[must_use]
pub fn month_year(year: u32, month: u32, locale: Locale) -> String {
    let name = month_name(month, locale);
    let mut chars = name.chars();
    let capitalized: String = chars.next().map(|c| c.to_uppercase().chain(chars).collect()).unwrap_or_default();
    format!("{capitalized} {year}")
}
```

In `mobiler-core/src/lib.rs` change line 15 to:

```rust
pub use format::{Currency, Locale, Weekday};
```

- [ ] **Step 4: Run tests + clippy**

Run: `cargo test -p mobiler-core format:: && cargo clippy -p mobiler-core --all-targets`
Expected: PASS, no new warnings.

- [ ] **Step 5: Commit**

```bash
git add mobiler-core/src/format.rs mobiler-core/src/lib.rs
git commit -m "feat(format): weekday labels, locale week start, month_year title"
```

---

### Task 3: Calendar ABI + `calendar_in` (ui + core)

**Files:**
- Modify: `mobiler-ui/src/lib.rs:535-538` (variant), `:690` (round-trip)
- Modify: `mobiler-core/src/lib.rs:815-823` (builders), `:1643-1647` (existing test), `mod tests` (new test)

**Interfaces:**
- Consumes: `format::{weekday_short, month_year}`, `Locale::week_start`, `Weekday::sun0` (Task 2); existing private `days_in_month`, `weekday`, `tok`.
- Produces:
  - `Widget::Calendar { year: u32, month: u8, title: String, weekday_labels: Vec<String>, leading_blanks: u8, selected: Option<u8>, on_day: Vec<ActionToken>, markers: Vec<u8> }`
  - `pub fn calendar_in<E: Serialize>(locale: Locale, year: u32, month: u8, selected: Option<u8>, markers: &[u8], on_day: impl Fn(u8) -> E) -> Widget`

- [ ] **Step 1: Update the ABI variant** (`mobiler-ui/src/lib.rs`, replace the Calendar doc + variant)

```rust
    /// An inline month calendar. The core pre-computes everything locale-dependent so shells only
    /// draw: `title` (e.g. "Septembar 2026"), the 7 `weekday_labels` in column order (week start
    /// first), and `leading_blanks` (empty cells before day 1). `on_day[d-1]` fires when day `d` is
    /// tapped (length = days in the month); `selected` highlights a day. `markers` is empty (no
    /// markers) or one level per day, `0..=3`, drawn as that many small dots under the day number.
    Calendar {
        year: u32,
        month: u8,
        title: String,
        weekday_labels: Vec<String>,
        leading_blanks: u8,
        selected: Option<u8>,
        on_day: Vec<ActionToken>,
        markers: Vec<u8>,
    },
```

Replace the round-trip at line 690:

```rust
        round_trips(&Widget::Calendar { year: 2026, month: 9, title: "Septembar 2026".to_string(), weekday_labels: ["P", "U", "S", "Č", "P", "S", "N"].map(String::from).to_vec(), leading_blanks: 1, selected: Some(15), on_day: vec!["d1".to_string(), "d2".to_string()], markers: vec![0, 3] });
```

- [ ] **Step 2: Write the failing core tests**

In `mobiler-core/src/lib.rs` replace the existing assertion (the `// June 2026 has 30 days…` block at ~1643) with:

```rust
        // June 2026 has 30 days and starts on a Monday; US English is Sunday-first → 1 blank.
        assert!(matches!(
            calendar(2026, 6, Some(3), |d| Ev::Open(u32::from(d))),
            Widget::Calendar { leading_blanks: 1, selected: Some(3), ref on_day, ref title, ref markers, .. }
                if on_day.len() == 30 && title == "June 2026" && markers.is_empty()
        ));
```

Add a new test in `mod tests`:

```rust
    #[test]
    fn calendar_in_localizes_layout_and_clamps_markers() {
        // 1 September 2026 is a Tuesday; Serbian weeks start Monday → 1 leading blank, "U" 2nd column.
        let w = calendar_in(Locale::SrLatn, 2026, 9, None, &[1, 2, 3, 9], |d| Ev::Open(u32::from(d)));
        let Widget::Calendar { title, weekday_labels, leading_blanks, on_day, markers, .. } = w else { panic!("not a calendar") };
        assert_eq!(title, "Septembar 2026");
        assert_eq!(weekday_labels, ["P", "U", "S", "Č", "P", "S", "N"]);
        assert_eq!(leading_blanks, 1);
        assert_eq!(on_day.len(), 30);
        assert_eq!(markers.len(), 30, "padded to one level per day");
        assert_eq!(&markers[..5], &[1, 2, 3, 3, 0], "clamped to 3, missing days = 0");
        // Same month, US English (Sunday-first) → 2 leading blanks.
        assert!(matches!(
            calendar_in(Locale::EnUs, 2026, 9, None, &[], |_| Ev::Tap),
            Widget::Calendar { leading_blanks: 2, ref markers, .. } if markers.is_empty()
        ));
    }
```

- [ ] **Step 3: Run to verify failure**

Run: `cargo test -p mobiler-core calendar`
Expected: FAIL to compile (`calendar_in` missing; `Widget::Calendar` has no `leading_blanks` in the builder).

- [ ] **Step 4: Implement the builders** (replace `calendar` in `mobiler-core/src/lib.rs`)

```rust
/// An inline month calendar for `year`/`month` (1–12), US English (Sunday-first). `on_day(d)`
/// builds the tap event for each day `d`; `selected` highlights a day. See [`calendar_in`] for a
/// localized calendar with per-day markers.
#[must_use]
pub fn calendar<E: Serialize>(year: u32, month: u8, selected: Option<u8>, on_day: impl Fn(u8) -> E) -> Widget {
    calendar_in(Locale::EnUs, year, month, selected, &[], on_day)
}

/// A localized inline month calendar: the title, weekday header and week start follow `locale`
/// (e.g. [`Locale::SrLatn`] → "Septembar 2026", Monday-first `P U S Č P S N`). `markers` is one
/// busy-level per day (`markers[d-1]`, `0..=3`, drawn as that many dots; `0` = none) — pass `&[]`
/// for no markers. Shorter slices pad with `0`; levels above 3 clamp to 3.
#[must_use]
pub fn calendar_in<E: Serialize>(
    locale: Locale,
    year: u32,
    month: u8,
    selected: Option<u8>,
    markers: &[u8],
    on_day: impl Fn(u8) -> E,
) -> Widget {
    let n = days_in_month(year, month);
    let start = locale.week_start().sun0();
    let weekday_labels = (0..7).map(|i| format::weekday_short(start + i, locale).to_string()).collect();
    let leading_blanks = (weekday(year, month, 1) + 7 - start) % 7;
    let markers = if markers.is_empty() {
        Vec::new()
    } else {
        (0..usize::from(n)).map(|i| markers.get(i).copied().unwrap_or(0).min(3)).collect()
    };
    Widget::Calendar {
        year,
        month,
        title: format::month_year(year, u32::from(month), locale),
        weekday_labels,
        leading_blanks,
        selected,
        on_day: (1..=n).map(|d| tok(on_day(d))).collect(),
        markers,
    }
}
```

- [ ] **Step 5: Run tests + clippy (root workspace)**

Run: `cargo test -p mobiler-ui -p mobiler-core && cargo clippy -p mobiler-ui -p mobiler-core --all-targets`
Expected: PASS. (The web crate and demo shells don't compile yet. Tasks 4–6 fix them.)

- [ ] **Step 6: Commit**

```bash
git add mobiler-ui/src/lib.rs mobiler-core/src/lib.rs
git commit -m "feat(calendar): localized title/weekdays/week start + per-day markers (ABI)"
```

---

### Task 4: Calendar — web shell

**Files:**
- Modify: `mobiler-web/src/lib.rs:1317-1339` (Calendar arm)
- Modify: `mobiler-web/src/mobiler.css:256-265` (calendar block)

**Interfaces:** Consumes the Task 3 `Widget::Calendar` fields.

- [ ] **Step 1: Replace the Calendar arm**

```rust
        Widget::Calendar { title, weekday_labels, leading_blanks, selected, on_day, markers, .. } => {
            let heads: Vec<_> = weekday_labels.iter().map(|w| view! { <div class="cal-head">{w.clone()}</div> }).collect();
            let blanks: Vec<_> = (0..*leading_blanks).map(|_| view! { <div class="cal-blank"></div> }).collect();
            let selected = *selected;
            let days: Vec<_> = on_day.iter().enumerate().map(|(i, token)| {
                let day = (i + 1) as u8;
                let token = token.clone();
                let send = send.clone();
                let cls = if selected == Some(day) { "cal-day cal-sel" } else { "cal-day" };
                // 0–3 busy-dots under the number; nothing at all for level 0 / no markers.
                let level = markers.get(i).copied().unwrap_or(0).min(3);
                let dots = (level > 0).then(|| {
                    let d: Vec<_> = (0..level).map(|_| view! { <span class="cal-dot"></span> }).collect();
                    view! { <span class="cal-dots">{d}</span> }
                });
                view! { <button class=cls on:click=move |_| send(Action::Fired { token: token.clone() })>{day.to_string()}{dots}</button> }
            }).collect();
            view! {
                <div class="calendar">
                    <div class="cal-title">{title.clone()}</div>
                    <div class="cal-grid">{heads}{blanks}{days}</div>
                </div>
            }.into_any()
        }
```

- [ ] **Step 2: Add the dot CSS** (after the `.cal-sel:hover` rule)

```css
.cal-day { position: relative; }
.cal-dots { position: absolute; left: 0; right: 0; bottom: 12%; display: flex; justify-content: center; gap: 2px; pointer-events: none; }
.cal-dot { width: 4px; height: 4px; border-radius: 50%; background: var(--primary); }
.cal-sel .cal-dot { background: var(--primary-ink); }
```

- [ ] **Step 3: Build for wasm**

Run: `cd /home/zmilan/working_docker/rust/mobiler/mobiler-web && cargo build --target wasm32-unknown-unknown; cd /home/zmilan/working_docker/rust/mobiler`
Expected: builds. (The Button/Scroller arms still match the old fields, which is fine until Tasks 8 and 11.)

- [ ] **Step 4: Commit**

```bash
git add mobiler-web/src/lib.rs mobiler-web/src/mobiler.css
git commit -m "feat(web): localized calendar header + busy-dot markers"
```

---

### Task 5: Calendar — Android shells (5 demos)

**Files:** Modify the `is Widget.Calendar -> { … }` arm (from that line up to the line before `is Widget.SwipeAction ->`) in:
- `demos/barbershop/Android/app/src/main/java/dev/mobiler/barbershop/MainActivity.kt`
- `demos/coffee/Android/app/src/main/java/dev/mobiler/coffee/MainActivity.kt`
- `demos/todo/Android/app/src/main/java/dev/mobiler/todo/MainActivity.kt`
- `demos/fullstack-todo/mobile/Android/app/src/main/java/dev/mobiler/mobile/MainActivity.kt`
- `demos/saldo/Android/app/src/main/java/rs/mobiler/saldo/MainActivity.kt`

**Interfaces:** Consumes the Task 3 fields (`title`, `weekdayLabels`, `leadingBlanks`, `markers`).

- [ ] **Step 1: Replace the arm in all 5 files with**

```kotlin
        is Widget.Calendar -> {
            // Title, weekday header and leading blanks are pre-localized by the core — just draw.
            val cells = ArrayList<Int?>()
            repeat(widget.leadingBlanks.toInt()) { cells.add(null) }
            for (d in 1..widget.onDay.size) cells.add(d)
            Column(modifier = Modifier.fillMaxWidth().padding(vertical = 4.dp)) {
                Text(widget.title, style = MaterialTheme.typography.titleMedium)
                Row(modifier = Modifier.fillMaxWidth()) {
                    widget.weekdayLabels.forEach { Text(it, style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant, modifier = Modifier.weight(1f), textAlign = TextAlign.Center) }
                }
                cells.chunked(7).forEach { week ->
                    Row(modifier = Modifier.fillMaxWidth()) {
                        week.forEach { day ->
                            if (day == null) {
                                Box(modifier = Modifier.weight(1f).height(40.dp))
                            } else {
                                val isSel = widget.selected?.toInt() == day
                                val token = widget.onDay[day - 1]
                                val level = (widget.markers.getOrNull(day - 1)?.toInt() ?: 0).coerceIn(0, 3)
                                Box(
                                    modifier = Modifier.weight(1f).height(40.dp).padding(2.dp)
                                        .clip(CircleShape)
                                        .background(if (isSel) MaterialTheme.colorScheme.primary else Color.Transparent)
                                        .clickable { send(Action.Fired(token)) },
                                    contentAlignment = Alignment.Center,
                                ) {
                                    Text("$day", color = if (isSel) MaterialTheme.colorScheme.onPrimary else MaterialTheme.colorScheme.onSurface)
                                    // 0–3 busy-dots under the number, in the theme primary (onPrimary when selected).
                                    if (level > 0) {
                                        Row(
                                            modifier = Modifier.align(Alignment.BottomCenter).padding(bottom = 3.dp),
                                            horizontalArrangement = Arrangement.spacedBy(2.dp),
                                        ) {
                                            repeat(level) {
                                                Box(Modifier.size(4.dp).clip(CircleShape).background(if (isSel) MaterialTheme.colorScheme.onPrimary else MaterialTheme.colorScheme.primary))
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        repeat(7 - week.size) { Box(modifier = Modifier.weight(1f)) }
                    }
                }
            }
        }
```

If a demo's pre-existing arm differs from barbershop's (compare before editing), keep that file's own differences and change only the title/weekday/blank/marker parts. Task 16's drift check catches mistakes.

- [ ] **Step 2: Compile barbershop Android** (it exercises the Kotlin against freshly generated types)

```bash
cd demos/barbershop && CARGO_TARGET_DIR=$PWD/target JAVA_HOME=$HOME/jdk21 ANDROID_HOME=$HOME/Android/Sdk /media/zmilan/data2/cargo-target/debug/mobiler build android; cd ../..
```

Expected: **succeeds** (no other Kotlin arm references a removed field). Fix any unresolved import (`Arrangement`, `size`, `Alignment` are already imported in all shells; add `import androidx.compose.foundation.layout.size` if a demo lacks it).

- [ ] **Step 3: Compile saldo Android** (the most divergent shell)

```bash
cd demos/saldo && CARGO_TARGET_DIR=$PWD/target JAVA_HOME=$HOME/jdk21 ANDROID_HOME=$HOME/Android/Sdk /media/zmilan/data2/cargo-target/debug/mobiler build android; cd ../..
```

Expected: succeeds. (coffee/todo/fullstack are compiled by CI and diff-checked in Task 16.)

- [ ] **Step 4: Commit**

```bash
git add demos/*/Android/app/src/main/java demos/fullstack-todo/mobile/Android/app/src/main/java
git commit -m "feat(android): localized calendar header + busy-dot markers"
```

---

### Task 6: Calendar — iOS shells (5 demos)

**Files:**
- Modify: `demos/barbershop/iOS/Sources/Render.swift` (the `.calendar` case ~line 110, `struct CalendarView` ~line 895), then `cp` to coffee/todo/fullstack-todo/mobile
- Modify: `demos/saldo/iOS/Sources/Render.swift` (same two anchors, by hand)

- [ ] **Step 1: Replace the `.calendar` case in barbershop**

```swift
    case .calendar(_, _, let title, let weekdayLabels, let leadingBlanks, let selected, let onDay, let markers):
        return AnyView(CalendarView(title: title, weekdayLabels: weekdayLabels, leadingBlanks: leadingBlanks, selected: selected, onDay: onDay, markers: markers, send: send))
```

- [ ] **Step 2: Replace `struct CalendarView` (and its leading comment) in barbershop**

```swift
// Inline month calendar — title, weekday header and leading blanks come pre-localized from the
// core; tappable days with 0–3 busy-dots under the number.
private struct CalendarView: View {
    let title: String
    let weekdayLabels: [String]
    let leadingBlanks: UInt8
    let selected: UInt8?
    let onDay: [String]
    let markers: [UInt8]
    let send: (Action) -> Void
    private let cols = Array(repeating: GridItem(.flexible(), spacing: 4), count: 7)
    var body: some View {
        VStack(spacing: 6) {
            Text(title).font(.headline)
            LazyVGrid(columns: cols, spacing: 4) {
                ForEach(Array(weekdayLabels.enumerated()), id: \.offset) { _, w in
                    Text(w).font(.caption2).foregroundColor(.secondary)
                }
                ForEach(0..<Int(leadingBlanks), id: \.self) { _ in Color.clear.frame(height: 32) }
                ForEach(Array(onDay.enumerated()), id: \.offset) { idx, token in
                    let day = idx + 1
                    let isSel = selected.map { Int($0) == day } ?? false
                    let level = idx < markers.count ? min(Int(markers[idx]), 3) : 0
                    Button(action: { send(.fired(token: token)) }) {
                        Text("\(day)").frame(maxWidth: .infinity, minHeight: 32)
                            .background(isSel ? Color.accentColor : Color.clear)
                            .foregroundColor(isSel ? .white : .primary)
                            .clipShape(Circle())
                            .overlay(alignment: .bottom) {
                                if level > 0 {
                                    HStack(spacing: 2) {
                                        ForEach(0..<level, id: \.self) { _ in
                                            Circle().fill(isSel ? Color.white : Color.accentColor).frame(width: 4, height: 4)
                                        }
                                    }
                                    .padding(.bottom, 3)
                                }
                            }
                    }.buttonStyle(.plain)
                }
            }
        }.padding(.vertical, 4)
    }
}
```

- [ ] **Step 3: Propagate**

```bash
for d in coffee todo fullstack-todo/mobile; do cp demos/barbershop/iOS/Sources/Render.swift demos/$d/iOS/Sources/Render.swift; done
```

Apply Steps 1–2 by hand to `demos/saldo/iOS/Sources/Render.swift`.

- [ ] **Step 4: Check saldo drift**

```bash
diff demos/barbershop/iOS/Sources/Render.swift demos/saldo/iOS/Sources/Render.swift | grep '^[<>]' | diff - $SCRATCH/baseline/ios-saldo.diff && echo SALDO-DRIFT-UNCHANGED
```

Expected: `SALDO-DRIFT-UNCHANGED`.

- [ ] **Step 5: Commit**

```bash
git add demos/*/iOS/Sources/Render.swift demos/fullstack-todo/mobile/iOS/Sources/Render.swift
git commit -m "feat(ios): localized calendar header + busy-dot markers"
git tag ltt-cal
```

---

### Task 7: Button ABI + `button_with` (ui + core)

**Files:**
- Modify: `mobiler-ui/src/lib.rs:55` (`ButtonStyle`), `:590` (`Button` variant), `widget_round_trips` test
- Modify: `mobiler-core/src/lib.rs:903-906` (`button`), `mod tests`

**Interfaces:**
- Produces:
  - `ButtonStyle::Tonal`
  - `Widget::Button { label: String, style: ButtonStyle, on_press: ActionToken, tone: Tone, icon: Option<Icon>, wide: bool }`
  - `pub struct ButtonOpts { pub tone: Tone, pub icon: Option<Icon>, pub wide: bool }` with `Default` (Neutral, None, false) and chainable `const fn tone(self, Tone) -> Self`, `icon(self, Icon) -> Self`, `wide(self) -> Self`
  - `pub fn button_with<E: Serialize>(label: impl Into<String>, style: ButtonStyle, on_press: E, opts: ButtonOpts) -> Widget`

- [ ] **Step 1: ABI**

```rust
/// Button emphasis. `Tonal` is the quieter filled secondary (M3 filled-tonal).
#[derive(Facet, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
pub enum ButtonStyle { Filled, Outlined, Text, Tonal }
```

```rust
    /// A tappable button. `tone` recolors it (`Neutral` = the brand/primary look; `Danger` = the
    /// error color pair for destructive actions). `icon` draws a leading glyph; `wide` stretches it
    /// to the available width.
    Button { label: String, style: ButtonStyle, on_press: ActionToken, tone: Tone, icon: Option<Icon>, wide: bool },
```

Add to `widget_round_trips`:

```rust
        round_trips(&Widget::Button { label: "Otkaži".to_string(), style: ButtonStyle::Tonal, on_press: "x".to_string(), tone: Tone::Danger, icon: Some(Icon::Close), wide: true });
```

- [ ] **Step 2: Write the failing core test** (in `mod tests`)

```rust
    #[test]
    fn button_with_carries_tone_icon_and_width() {
        assert!(matches!(
            button("Go", ButtonStyle::Filled, Ev::Tap),
            Widget::Button { style: ButtonStyle::Filled, tone: Tone::Neutral, icon: None, wide: false, .. }
        ));
        assert!(matches!(
            button_with("Cancel", ButtonStyle::Tonal, Ev::Tap, ButtonOpts::default().tone(Tone::Danger).icon(Icon::Close).wide()),
            Widget::Button { style: ButtonStyle::Tonal, tone: Tone::Danger, icon: Some(Icon::Close), wide: true, .. }
        ));
    }
```

- [ ] **Step 3: Run to verify failure**

Run: `cargo test -p mobiler-core button_with`
Expected: FAIL to compile.

- [ ] **Step 4: Implement** (replace `button` in `mobiler-core/src/lib.rs`)

```rust
/// Extra options for [`button_with`]; `ButtonOpts::default()` is a plain [`button`].
///
/// ```
/// use mobiler_core::{ButtonOpts, Icon, Tone};
/// let danger_wide = ButtonOpts::default().tone(Tone::Danger).icon(Icon::Close).wide();
/// assert!(danger_wide.wide);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ButtonOpts {
    pub tone: Tone,
    pub icon: Option<Icon>,
    pub wide: bool,
}

impl Default for ButtonOpts {
    fn default() -> Self {
        Self { tone: Tone::Neutral, icon: None, wide: false }
    }
}

impl ButtonOpts {
    /// Recolor the button (`Tone::Danger` for destructive actions).
    #[must_use]
    pub const fn tone(mut self, tone: Tone) -> Self {
        self.tone = tone;
        self
    }
    /// A leading icon.
    #[must_use]
    pub const fn icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }
    /// Stretch to the available width (a screen's main action).
    #[must_use]
    pub const fn wide(mut self) -> Self {
        self.wide = true;
        self
    }
}

#[must_use]
pub fn button<E: Serialize>(label: impl Into<String>, style: ButtonStyle, on_press: E) -> Widget {
    button_with(label, style, on_press, ButtonOpts::default())
}

/// A button with a [`ButtonOpts`] tone, leading icon, and/or full width.
#[must_use]
pub fn button_with<E: Serialize>(label: impl Into<String>, style: ButtonStyle, on_press: E, opts: ButtonOpts) -> Widget {
    Widget::Button { label: label.into(), style, on_press: tok(on_press), tone: opts.tone, icon: opts.icon, wide: opts.wide }
}
```

- [ ] **Step 5: Run tests + clippy + doctest**

Run: `cargo test -p mobiler-ui -p mobiler-core && cargo clippy -p mobiler-ui -p mobiler-core --all-targets`
Expected: PASS (including the `ButtonOpts` doctest).

- [ ] **Step 6: Commit**

```bash
git add mobiler-ui/src/lib.rs mobiler-core/src/lib.rs
git commit -m "feat(button): Tonal style + tone, leading icon, wide (ABI) + button_with"
```

---

### Task 8: Button — web shell

**Files:**
- Modify: `mobiler-web/src/lib.rs:1523-1532` (Button arm), `:1879-1885` (`button_class`), add `button_tone_class` after it
- Modify: `mobiler-web/src/mobiler.css` (after `.btn-text`)

- [ ] **Step 1: Replace the Button arm**

```rust
        Widget::Button { label, style, on_press, tone, icon, wide } => {
            let (send, token, label) = (send.clone(), on_press.clone(), label.clone());
            // Neutral + not wide keeps the exact original class string.
            let mut class = format!("btn {}", button_class(*style));
            if *tone != Tone::Neutral {
                class.push(' ');
                class.push_str(button_tone_class(*tone));
            }
            if *wide {
                class.push_str(" btn-wide");
            }
            let glyph = icon.map(|i| view! { <span class="btn-icon">{icon_glyph(i)}</span> });
            view! {
                <button class=class on:click=move |_| send(Action::Fired { token: token.clone() })>
                    {glyph}
                    {label}
                </button>
            }
            .into_any()
        }
```

- [ ] **Step 2: Class mappers**

```rust
fn button_class(s: ButtonStyle) -> &'static str {
    match s {
        ButtonStyle::Filled => "btn-filled",
        ButtonStyle::Outlined => "btn-outlined",
        ButtonStyle::Text => "btn-text",
        ButtonStyle::Tonal => "btn-tonal",
    }
}

fn button_tone_class(t: Tone) -> &'static str {
    match t {
        Tone::Neutral => "",
        Tone::Success => "btn-success",
        Tone::Warning => "btn-warning",
        Tone::Danger => "btn-danger",
        Tone::Info => "btn-info",
    }
}
```

- [ ] **Step 3: CSS** (after `.btn-text`)

```css
.btn-tonal    { background: var(--accent-soft); color: var(--primary); }
.btn-wide     { width: 100%; }
.btn-icon     { margin-right: 8px; }
/* Toned buttons: --tone is the strong color, --tone-soft the container. */
.btn-danger  { --tone: #c0392b; --tone-soft: #fde8e8; }
.btn-success { --tone: #2e7d32; --tone-soft: #e3f3e4; }
.btn-warning { --tone: #b35c00; --tone-soft: #fdeede; }
.btn-info    { --tone: #1f6fb2; --tone-soft: #e3effa; }
.theme-dark .btn-danger  { --tone: #ff8a80; --tone-soft: rgba(255, 138, 128, 0.18); }
.theme-dark .btn-success { --tone: #81c784; --tone-soft: rgba(129, 199, 132, 0.18); }
.theme-dark .btn-warning { --tone: #ffb74d; --tone-soft: rgba(255, 183, 77, 0.18); }
.theme-dark .btn-info    { --tone: #64b5f6; --tone-soft: rgba(100, 181, 246, 0.18); }
.btn-filled:is(.btn-danger, .btn-success, .btn-warning, .btn-info)   { background: var(--tone); color: #fff; }
.theme-dark .btn-filled:is(.btn-danger, .btn-success, .btn-warning, .btn-info) { color: #15171f; }
.btn-outlined:is(.btn-danger, .btn-success, .btn-warning, .btn-info) { color: var(--tone); box-shadow: inset 0 0 0 1px var(--tone); }
.btn-text:is(.btn-danger, .btn-success, .btn-warning, .btn-info)     { color: var(--tone); }
.btn-tonal:is(.btn-danger, .btn-success, .btn-warning, .btn-info)    { background: var(--tone-soft); color: var(--tone); }
```

- [ ] **Step 4: Build for wasm**

Run: `cd mobiler-web && cargo build --target wasm32-unknown-unknown; cd ..`
Expected: builds.

- [ ] **Step 5: Commit**

```bash
git add mobiler-web/src/lib.rs mobiler-web/src/mobiler.css
git commit -m "feat(web): button tone, Tonal style, leading icon, wide"
```

---

### Task 9: Button — Android shells (5 demos)

**Files:** in each of the 5 `MainActivity.kt`: replace the `is Widget.Button -> when (widget.style) { … }` arm, add a `MobilerButton` composable + `toneStrong` next to `toneColors`, and add imports.

- [ ] **Step 1: Replace the arm**

```kotlin
        is Widget.Button -> MobilerButton(widget, send)
```

- [ ] **Step 2: Add after the `toneColors` function**

```kotlin
// Strong (filled) color pair for a toned button.
@Composable
private fun toneStrong(tone: Tone): Pair<Color, Color> {
    val cs = MaterialTheme.colorScheme
    return when (tone) {
        Tone.NEUTRAL -> cs.primary to cs.onPrimary
        Tone.SUCCESS -> Color(0xFF2E7D32) to Color.White
        Tone.WARNING -> Color(0xFFE65100) to Color.White
        Tone.DANGER -> cs.error to cs.onError
        Tone.INFO -> cs.tertiary to cs.onTertiary
    }
}

// Widget.Button. A NEUTRAL tone keeps each M3 button's default colors (an un-toned button renders
// exactly as before); other tones recolor FILLED/OUTLINED/TEXT from the strong tone color and TONAL
// from the tone's container pair. `icon` is a leading glyph; `wide` fills the width.
@Composable
private fun MobilerButton(widget: Widget.Button, send: (Action) -> Unit) {
    val onClick = { send(Action.Fired(widget.onPress)) }
    val modifier = if (widget.wide) Modifier.fillMaxWidth() else Modifier
    val neutral = widget.tone == Tone.NEUTRAL
    val (strong, onStrong) = toneStrong(widget.tone)
    val (soft, onSoft) = toneColors(widget.tone)
    val content: @Composable RowScope.() -> Unit = {
        widget.icon?.let {
            Icon(iconFor(it), contentDescription = null, modifier = Modifier.size(ButtonDefaults.IconSize))
            Spacer(Modifier.width(ButtonDefaults.IconSpacing))
        }
        Text(widget.label)
    }
    when (widget.style) {
        ButtonStyle.FILLED -> Button(
            onClick = onClick,
            modifier = modifier,
            colors = if (neutral) ButtonDefaults.buttonColors() else ButtonDefaults.buttonColors(containerColor = strong, contentColor = onStrong),
            content = content,
        )
        ButtonStyle.TONAL -> FilledTonalButton(
            onClick = onClick,
            modifier = modifier,
            colors = if (neutral) ButtonDefaults.filledTonalButtonColors() else ButtonDefaults.filledTonalButtonColors(containerColor = soft, contentColor = onSoft),
            content = content,
        )
        ButtonStyle.OUTLINED -> if (neutral) {
            OutlinedButton(onClick = onClick, modifier = modifier, content = content)
        } else {
            OutlinedButton(
                onClick = onClick,
                modifier = modifier,
                colors = ButtonDefaults.outlinedButtonColors(contentColor = strong),
                border = BorderStroke(1.dp, strong),
                content = content,
            )
        }
        ButtonStyle.TEXT -> TextButton(
            onClick = onClick,
            modifier = modifier,
            colors = if (neutral) ButtonDefaults.textButtonColors() else ButtonDefaults.textButtonColors(contentColor = strong),
            content = content,
        )
    }
}
```

- [ ] **Step 3: Add missing imports** (only those a file lacks. Check with `grep -n "^import androidx.compose.material3.FilledTonalButton" <file>` etc.)

```kotlin
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.layout.RowScope
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.FilledTonalButton
```

- [ ] **Step 4: Compile barbershop + saldo Android** (commands as in Task 5, Steps 2–3)

Expected: both succeed.

- [ ] **Step 5: Commit**

```bash
git add demos/*/Android/app/src/main/java demos/fullstack-todo/mobile/Android/app/src/main/java
git commit -m "feat(android): button tone, Tonal style, leading icon, wide"
```

---

### Task 10: Button — iOS shells (5 demos)

**Files:** barbershop `Render.swift` (`.button` case ~line 190; `ButtonStyleMod` ~line 1194), then `cp` to 3 identical files; saldo by hand.

- [ ] **Step 1: Replace the `.button` case**

```swift
    case .button(let label, let style, let onPress, let tone, let icon, let wide):
        return AnyView(MobilerButton(label: label, style: style, tone: tone, icon: icon, wide: wide) { send(.fired(token: onPress)) })
```

- [ ] **Step 2: Replace `ButtonStyleMod` and add `MobilerButton` + `TonalButtonStyle`**

```swift
// Widget::Button. A neutral tone with no icon and not wide renders exactly the original
// `Button(label).modifier(ButtonStyleMod(style))`; a tone tints it, `icon` adds a leading SF Symbol,
// `wide` stretches the label to the available width.
private struct MobilerButton: View {
    let label: String
    let style: SharedTypes.ButtonStyle
    let tone: Tone
    let icon: Icon?
    let wide: Bool
    let action: () -> Void

    private var neutral: Bool { if case .neutral = tone { return true }; return false }
    private var tint: Color { neutral ? .accentColor : toneColors(tone).1 }

    var body: some View {
        let button = Button(action: action) { labelView }
        if neutral {
            button.modifier(ButtonStyleMod(style))
        } else {
            button.modifier(ButtonStyleMod(style, tint: tint)).tint(tint)
        }
    }

    @ViewBuilder private var labelView: some View {
        if let icon {
            if wide { Label(label, systemImage: sfSymbol(icon)).frame(maxWidth: .infinity) } else { Label(label, systemImage: sfSymbol(icon)) }
        } else if wide {
            Text(label).frame(maxWidth: .infinity)
        } else {
            Text(label)
        }
    }
}

private struct ButtonStyleMod: ViewModifier {
    let style: SharedTypes.ButtonStyle
    let tint: Color
    init(_ s: SharedTypes.ButtonStyle, tint: Color = .accentColor) { style = s; self.tint = tint }
    func body(content: Content) -> some View {
        switch style {
        case .filled: return AnyView(content.buttonStyle(.borderedProminent))
        case .outlined: return AnyView(content.buttonStyle(.bordered))
        case .text: return AnyView(content.buttonStyle(.borderless))
        case .tonal: return AnyView(content.buttonStyle(TonalButtonStyle(color: tint)))
        }
    }
}

// M3-style filled-tonal: tinted label on a soft tint capsule (the quieter secondary action).
private struct TonalButtonStyle: SwiftUI.ButtonStyle {
    let color: Color
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.body.weight(.semibold))
            .padding(.horizontal, 14).padding(.vertical, 7)
            .foregroundColor(color)
            .background(color.opacity(0.18))
            .clipShape(Capsule())
            .opacity(configuration.isPressed ? 0.7 : 1)
    }
}
```

(`SwiftUI.ButtonStyle` is qualified because the shared `ButtonStyle` enum shadows the protocol name.)

- [ ] **Step 3: Propagate + saldo + drift check**

```bash
for d in coffee todo fullstack-todo/mobile; do cp demos/barbershop/iOS/Sources/Render.swift demos/$d/iOS/Sources/Render.swift; done
```

Apply Steps 1–2 to saldo by hand, then run the Task 6 Step 4 drift check. Expected: `SALDO-DRIFT-UNCHANGED`.

- [ ] **Step 4: Commit**

```bash
git add demos/*/iOS/Sources/Render.swift demos/fullstack-todo/mobile/iOS/Sources/Render.swift
git commit -m "feat(ios): button tone, Tonal style, leading icon, wide"
git tag ltt-btn
```

---

### Task 11: Scroller edge fade — ABI + web

**Files:**
- Modify: `mobiler-ui/src/lib.rs:576` (+ round-trip), `mobiler-core/src/lib.rs:883-885` (+ test)
- Modify: `mobiler-web/src/lib.rs:1458-1461`, `mobiler-web/src/mobiler.css:80-82`

**Interfaces:**
- Produces: `Widget::Scroller { children: Vec<Widget>, edge_fade: bool }`; `pub fn scroller_hinted(children: Vec<Widget>) -> Widget`.

- [ ] **Step 1: ABI**

```rust
    /// Horizontally scrolling row. `edge_fade` fades the trailing edge (plus trailing room so the
    /// last item clears the fade at scroll-end) to hint there is more to scroll.
    Scroller { children: Vec<Widget>, edge_fade: bool },
```

Round-trip: `round_trips(&Widget::Scroller { children: vec![Widget::Divider], edge_fade: true });`

- [ ] **Step 2: Failing core test** (in `mod tests`)

```rust
    #[test]
    fn scroller_hint_is_opt_in() {
        assert!(matches!(scroller(vec![text("a")]), Widget::Scroller { edge_fade: false, .. }));
        assert!(matches!(scroller_hinted(vec![text("a")]), Widget::Scroller { edge_fade: true, ref children } if children.len() == 1));
    }
```

Run: `cargo test -p mobiler-core scroller_hint`. Expected: FAIL to compile.

- [ ] **Step 3: Builders**

```rust
/// Horizontally scrolling row of children (a carousel / chip rail).
#[must_use]
pub fn scroller(children: Vec<Widget>) -> Widget { Widget::Scroller { children, edge_fade: false } }
/// A [`scroller`] whose trailing edge fades out — a hint that it scrolls.
#[must_use]
pub fn scroller_hinted(children: Vec<Widget>) -> Widget { Widget::Scroller { children, edge_fade: true } }
```

Run: `cargo test -p mobiler-ui -p mobiler-core && cargo clippy -p mobiler-ui -p mobiler-core --all-targets`. Expected: PASS.

- [ ] **Step 4: Web arm**

```rust
        Widget::Scroller { children, edge_fade } => {
            let kids = render_all(children, send);
            if *edge_fade {
                view! { <div class="scroller scroller-fade">{kids}<div class="scroller-end"></div></div> }.into_any()
            } else {
                view! { <div class="scroller">{kids}</div> }.into_any()
            }
        }
```

CSS, after `.scroller > * { … }`:

```css
.scroller-fade { -webkit-mask-image: linear-gradient(to right, #000 calc(100% - 32px), transparent); mask-image: linear-gradient(to right, #000 calc(100% - 32px), transparent); }
.scroller > .scroller-end { flex: 0 0 32px; }
```

Run: `cd mobiler-web && cargo build --target wasm32-unknown-unknown; cd ..`. Expected: builds.

- [ ] **Step 5: Commit**

```bash
git add mobiler-ui/src/lib.rs mobiler-core/src/lib.rs mobiler-web/src/lib.rs mobiler-web/src/mobiler.css
git commit -m "feat(scroller): opt-in trailing edge fade (ABI) + web"
```

---

### Task 12: Scroller edge fade — Android + iOS

**Files:** the 5 `MainActivity.kt` (`is Widget.Scroller ->` arm) and the 5 `Render.swift` (`.scroller` case).

- [ ] **Step 1: Android arm** (all 5)

```kotlin
        is Widget.Scroller -> if (!widget.edgeFade) {
            Row(
                modifier = Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()),
                horizontalArrangement = Arrangement.spacedBy(12.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) { widget.children.forEach { Render(it, send) } }
        } else {
            // Fade the viewport's trailing 32.dp (DstIn over an offscreen layer, so it works on any
            // background) + 32.dp of trailing room so the last item clears the fade at scroll-end.
            Row(
                modifier = Modifier.fillMaxWidth()
                    .graphicsLayer { compositingStrategy = CompositingStrategy.Offscreen }
                    .drawWithContent {
                        drawContent()
                        val fade = 32.dp.toPx()
                        drawRect(
                            brush = Brush.horizontalGradient(listOf(Color.Black, Color.Transparent), startX = size.width - fade, endX = size.width),
                            topLeft = Offset(size.width - fade, 0f),
                            size = Size(fade, size.height),
                            blendMode = BlendMode.DstIn,
                        )
                    }
                    .horizontalScroll(rememberScrollState()),
                horizontalArrangement = Arrangement.spacedBy(12.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                widget.children.forEach { Render(it, send) }
                Spacer(Modifier.width(32.dp))
            }
        }
```

Imports to add where missing:

```kotlin
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.BlendMode
import androidx.compose.ui.graphics.CompositingStrategy
import androidx.compose.ui.graphics.graphicsLayer
```

- [ ] **Step 2: iOS case** (barbershop, then cp ×3, saldo by hand)

```swift
    case .scroller(let children, let edgeFade):
        if !edgeFade {
            return AnyView(
                ScrollView(.horizontal, showsIndicators: false) {
                    HStack(spacing: 12) { childViews(children, send) }
                }
            )
        }
        // Trailing 32pt fade + 32pt of trailing room so the last item clears it at scroll-end.
        return AnyView(
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: 12) {
                    childViews(children, send)
                    Color.clear.frame(width: 32)
                }
            }
            .mask(
                HStack(spacing: 0) {
                    Rectangle()
                    LinearGradient(colors: [.black, .clear], startPoint: .leading, endPoint: .trailing).frame(width: 32)
                }
            )
        )
```

- [ ] **Step 3: Verify**

```bash
for d in coffee todo fullstack-todo/mobile; do cp demos/barbershop/iOS/Sources/Render.swift demos/$d/iOS/Sources/Render.swift; done
diff demos/barbershop/iOS/Sources/Render.swift demos/saldo/iOS/Sources/Render.swift | grep '^[<>]' | diff - $SCRATCH/baseline/ios-saldo.diff && echo SALDO-DRIFT-UNCHANGED
```

Compile barbershop + saldo Android (Task 5 commands). Expected: drift unchanged, both builds succeed.

- [ ] **Step 4: Commit**

```bash
git add demos/*/Android/app/src/main/java demos/fullstack-todo/mobile/Android/app/src/main/java demos/*/iOS/Sources/Render.swift demos/fullstack-todo/mobile/iOS/Sources/Render.swift
git commit -m "feat(native): scroller trailing edge fade on Android + iOS"
git tag ltt-scr
```

---

### Task 13: `Density::Large` — ABI + web

**Files:**
- Modify: `mobiler-ui/src/lib.rs:315-317` (+ round-trip)
- Modify: `mobiler-web/src/lib.rs:1757` (scaffold class), `:1822-1825` (`theme_css` density), `mobiler-web/src/mobiler.css` (end of file)

**Interfaces:** Produces `Density::Large`.

- [ ] **Step 1: ABI**

```rust
/// Global density. `Comfortable` ≈ the current (un-themed) look; `Compact` tightens spacing.
/// `Large` is for hurried / wet / gloved hands: bigger controls (56 buttons & segmented, 48 chips
/// & calendar days, 56 icon-button targets), 16 control labels, ≥ 12 between adjacent tappables.
/// Body text stays on the platform's font-scale setting.
#[derive(Facet, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
pub enum Density { Compact, Comfortable, Large }
```

Add a themed-scaffold round-trip copy with `density: Density::Large` (duplicate the existing themed `round_trips(&Widget::Scaffold { … })` block, change only `density`).

Run: `cargo test -p mobiler-ui`. Expected: PASS.

- [ ] **Step 2: Web scaffold class + gaps**

Replace `let class = if *dark_mode { "scaffold theme-dark" } else { "scaffold" };` with:

```rust
            // `density-large` scopes the Density::Large control sizes in mobiler.css.
            let large = theme.as_ref().is_some_and(|t| t.density == Density::Large);
            let class = format!("scaffold{}{}", if *dark_mode { " theme-dark" } else { "" }, if large { " density-large" } else { "" });
```

In `theme_css`:

```rust
    let (gap, pad) = match t.density {
        Density::Compact => ("8px", "10px"),
        Density::Comfortable => ("12px", "14px"),
        Density::Large => ("16px", "18px"),
    };
```

- [ ] **Step 3: Large CSS** (append to `mobiler.css`)

```css
/* ---- Density::Large — bigger controls for busy hands (control sizes only; text follows the browser). ---- */
.density-large .row, .density-large .col { gap: 12px; }
.density-large .btn { min-height: 56px; padding: 0 24px; font-size: 16px; }
.density-large .chip { min-height: 48px; padding: 0 18px; font-size: 16px; }
.density-large .segment { min-height: 48px; font-size: 16px; }
.density-large .iconbtn { min-width: 56px; min-height: 56px; font-size: 24px; }
.density-large .cal-day { min-height: 48px; font-size: 16px; }
.density-large .tab { font-size: 14px; }
```

(The segmented track has 4px padding, so a 48px segment = a 56px control.)

Run: `cd mobiler-web && cargo build --target wasm32-unknown-unknown; cd ..`. Expected: builds.

- [ ] **Step 4: Commit**

```bash
git add mobiler-ui/src/lib.rs mobiler-web/src/lib.rs mobiler-web/src/mobiler.css
git commit -m "feat(density): Density::Large (ABI) + web control sizing"
```

---

### Task 14: `Density::Large` — Android shells (5 demos)

**Files:** each `MainActivity.kt`: `densityScale`, new `isLarge`, the Row/Column/Calendar/Button/IconButton/Chip/Segmented arms, NavigationBarItem + NavigationRailItem labels.

- [ ] **Step 1: Density helpers** (replace `densityScale`)

```kotlin
// Spacing multiplier from the theme's density. Comfortable (or un-themed) = 1.0; Compact tightens;
// Large loosens.
private val densityScale: Float
    get() = when (activeTheme?.density) {
        Density.COMPACT -> 0.75f
        Density.COMFORTABLE, null -> 1.0f
        Density.LARGE -> 1.25f
    }

// Density.LARGE also enlarges controls (56.dp buttons, 48.dp chips/day cells, 16.sp labels). Every
// use is `if (isLarge) <large> else <the original value>` so other densities stay pixel-identical.
private val isLarge: Boolean
    get() = activeTheme?.density == Density.LARGE

private val LargeButtonPadding = PaddingValues(horizontal = 24.dp, vertical = 8.dp)
```

- [ ] **Step 2: Layout spacing**
  - `is Widget.Row` → `horizontalArrangement = Arrangement.spacedBy(if (isLarge) 12.dp else 8.dp),`
  - `is Widget.Column` → `verticalArrangement = Arrangement.spacedBy(if (isLarge) 12.dp else 6.dp),`

- [ ] **Step 3: Calendar cells.** In the Calendar arm, add `val cellH = if (isLarge) 48.dp else 40.dp` right after `val cells = ArrayList<Int?>()`, and replace both `.height(40.dp)` with `.height(cellH)`.

- [ ] **Step 4: Button** (edit `MobilerButton`)

Replace `val modifier = …` with:

```kotlin
    val large = isLarge
    val modifier = Modifier
        .then(if (widget.wide) Modifier.fillMaxWidth() else Modifier)
        .then(if (large) Modifier.heightIn(min = 56.dp) else Modifier)
```

Replace `Text(widget.label)` in `content` with `Text(widget.label, fontSize = if (large) 16.sp else TextUnit.Unspecified)`.

Add a `contentPadding` argument to each call:
- `Button`, `FilledTonalButton`, both `OutlinedButton` calls: `contentPadding = if (large) LargeButtonPadding else ButtonDefaults.ContentPadding,`
- `TextButton`: `contentPadding = if (large) LargeButtonPadding else ButtonDefaults.TextButtonContentPadding,`

- [ ] **Step 5: IconButton, Chip, Segmented, nav labels**

```kotlin
        is Widget.IconButton -> IconButton(onClick = { send(Action.Fired(widget.onPress)) }, modifier = if (isLarge) Modifier.size(56.dp) else Modifier) {
            Icon(imageVector = iconFor(widget.icon), contentDescription = widget.icon.name.lowercase(), tint = iconTintFor(widget.icon))
        }

        is Widget.Chip -> FilterChip(
            selected = widget.selected,
            onClick = { send(Action.Fired(widget.onPress)) },
            label = { Text(widget.label, fontSize = if (isLarge) 16.sp else TextUnit.Unspecified) },
            modifier = if (isLarge) Modifier.height(48.dp) else Modifier,
        )
```

In `is Widget.Segmented`, the `SegmentedButton(...)` call gains `modifier = if (isLarge) Modifier.height(56.dp) else Modifier,` and its content becomes `{ Text(seg.label, fontSize = if (isLarge) 16.sp else TextUnit.Unspecified) }`.

In both `NavigationRailItem` and `NavigationBarItem`: `label = { Text(t.label, fontSize = if (isLarge) 14.sp else TextUnit.Unspecified) },`

Imports where missing:

```kotlin
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.ui.unit.TextUnit
```

- [ ] **Step 6: Compile barbershop + saldo Android** (Task 5 commands). Expected: both succeed. A non-exhaustive `when` on `Density` elsewhere would fail here, so fix it with the same `LARGE` branch.

- [ ] **Step 7: Commit**

```bash
git add demos/*/Android/app/src/main/java demos/fullstack-todo/mobile/Android/app/src/main/java
git commit -m "feat(android): Density.LARGE control sizing"
```

---

### Task 15: `Density::Large` — iOS shells (5 demos)

**Files:** barbershop `Render.swift` (then cp ×3; saldo by hand): `Theme.densityScale`, a new `isLargeDensity()`, `.row`/`.column`/`.iconButton`/`.chip`/`.segmented` cases, `CalendarView`, `MobilerButton` + a new `LargeButtonStyle`, bottom-tab label.

- [ ] **Step 1: Helpers**

In `extension Theme`: `var densityScale: CGFloat { switch density { case .compact: 0.75; case .comfortable: 1.0; case .large: 1.25 } }`

After the `extension Theme { … }` block:

```swift
/// `Density.large` enlarges controls (56pt buttons, 48pt chips/day cells, larger labels). Every use is
/// `large ? <large> : <the original value>` so the other densities stay pixel-identical.
private func isLargeDensity() -> Bool {
    if case .large? = ActiveTheme.current?.density { return true }
    return false
}
```

- [ ] **Step 2: Layout spacing**

```swift
    case .row(let children):
        return AnyView(HStack(spacing: isLargeDensity() ? 12 : 8) { childViews(children, send) })

    case .column(let children):
        return AnyView(VStack(alignment: .leading, spacing: isLargeDensity() ? 12 : 6) { childViews(children, send) })
```

- [ ] **Step 3: Icon button, chip, segmented**

```swift
    case .iconButton(let icon, let onPress):
        let large = isLargeDensity()
        return AnyView(
            Button(action: { send(.fired(token: onPress)) }) {
                if large {
                    Image(systemName: sfSymbol(icon)).foregroundColor(iconTint(icon))
                        .font(.system(size: 24)).frame(minWidth: 56, minHeight: 56).contentShape(Rectangle())
                } else {
                    Image(systemName: sfSymbol(icon)).foregroundColor(iconTint(icon))
                }
            }.buttonStyle(.plain)
        )

    case .chip(let label, let selected, let onPress):
        let large = isLargeDensity()
        return AnyView(
            Button(action: { send(.fired(token: onPress)) }) {
                Group {
                    if large {
                        Text(label).font(.body).padding(.horizontal, 16).frame(minHeight: 48)
                    } else {
                        Text(label).font(.subheadline).padding(.horizontal, 12).padding(.vertical, 6)
                    }
                }
                .background(selected ? Color.accentColor.opacity(0.18) : Color.gray.opacity(0.12))
                .foregroundColor(selected ? Color.accentColor : .primary)
                .overlay(Capsule().stroke(selected ? Color.accentColor : .clear))
                .clipShape(Capsule())
            }.buttonStyle(.plain)
        )
```

In `.segmented`, add `let large = isLargeDensity()` before `return`, and change the segment label chain to:

```swift
                        Text(seg.label).font(large ? .body.weight(.semibold) : .subheadline.weight(.semibold))
                            .frame(maxWidth: .infinity, minHeight: large ? 48 : nil)
                            .padding(.vertical, large ? 0 : 8)
```

- [ ] **Step 4: Calendar cells.** In `CalendarView.body`, add `let cell: CGFloat = isLargeDensity() ? 48 : 32` as the first line (before `VStack`, with `return VStack…` if the compiler requires an explicit return), and replace both `32`s (`frame(height: 32)`, `minHeight: 32`) with `cell`.

- [ ] **Step 5: Button.** In `MobilerButton.body` put the Large branch first:

```swift
    var body: some View {
        let button = Button(action: action) { labelView }
        if isLargeDensity() {
            button.buttonStyle(LargeButtonStyle(style: style, color: tint))
        } else if neutral {
            button.modifier(ButtonStyleMod(style))
        } else {
            button.modifier(ButtonStyleMod(style, tint: tint)).tint(tint)
        }
    }
```

Add after `TonalButtonStyle`:

```swift
// Density.large button: 56pt min height, 24pt side padding, body-semibold label (scales with Dynamic
// Type), capsule — one style for all four ButtonStyles so every large button is the same height.
private struct LargeButtonStyle: SwiftUI.ButtonStyle {
    let style: SharedTypes.ButtonStyle
    let color: Color
    func makeBody(configuration: Configuration) -> some View {
        let (fill, fg, stroke): (Color, Color, Color) = {
            switch style {
            case .filled: return (color, .white, .clear)
            case .tonal: return (color.opacity(0.18), color, .clear)
            case .outlined: return (.clear, color, color)
            case .text: return (.clear, color, .clear)
            }
        }()
        return configuration.label
            .font(.body.weight(.semibold))
            .padding(.horizontal, 24)
            .frame(minHeight: 56)
            .foregroundColor(fg)
            .background(fill)
            .overlay(Capsule().stroke(stroke, lineWidth: 1))
            .clipShape(Capsule())
            .contentShape(Capsule())
            .opacity(configuration.isPressed ? 0.7 : 1)
    }
}
```

- [ ] **Step 6: Bottom tab label.** In `ScaffoldView`'s bottom tab bar: `Text(tab.label).font(isLargeDensity() ? .subheadline : .caption)`.

- [ ] **Step 7: Propagate + drift check** (Task 12 Step 3 commands). Expected: `SALDO-DRIFT-UNCHANGED`.

- [ ] **Step 8: Commit**

```bash
git add demos/*/iOS/Sources/Render.swift demos/fullstack-todo/mobile/iOS/Sources/Render.swift
git commit -m "feat(ios): Density.large control sizing"
git tag ltt-den
```

---

### Task 16: Drift + test sweep (no commit unless fixes)

- [ ] **Step 1: Android drift check** (each demo still differs from barbershop exactly as before)

```bash
B=demos/barbershop/Android/app/src/main/java/dev/mobiler/barbershop/MainActivity.kt
for d in coffee todo saldo fullstack-todo/mobile; do
  n=$(echo $d | tr / _)
  diff $B $(find demos/$d/Android -name MainActivity.kt -not -path '*/build/*') | grep '^[<>]' | diff -q - $SCRATCH/baseline/android-$n.diff >/dev/null && echo "$d OK" || echo "$d DRIFTED"
done
```

Expected: 4× `OK`. On `DRIFTED`, inspect with `diff $B <file>` and port the missing hunk.

- [ ] **Step 2: Full local test sweep**

```bash
cargo clippy --workspace --all-targets && cargo test --workspace
(cd mobiler-web && cargo build --target wasm32-unknown-unknown)
(cd demos/barbershop/web && trunk build)
```

Expected: all pass.

---

### Task 17: Barbershop showcase

**Files:** Modify `demos/barbershop/app-core/src/lib.rs`: imports (line 6+), `Model` (~line 294) + `Default` (~line 409), `fn input` (~line 1080), theme in `fn view` (~line 1120), `category_carousel` (~line 1197), `bookings_screen` (~line 1417), `profile_screen` (~line 1891), `mod tests`.

**Interfaces:** Consumes `calendar_in`, `button_with`, `ButtonOpts`, `scroller_hinted`, `Density::Large`, `ButtonStyle::Tonal`, `toggle`.

- [ ] **Step 1: Failing test** (in `mod tests`)

```rust
    #[test]
    fn large_controls_and_serbian_calendar_toggles() {
        let (app, mut model) = app();
        let mut cx = Cx::<Msg>::default();
        app.input("large_controls", InputValue::Bool(true), &mut model, &mut cx);
        app.input("serbian_calendar", InputValue::Bool(true), &mut model, &mut cx);
        assert!(model.large_controls && model.serbian_calendar);
        assert!(matches!(
            app.view(&model),
            Widget::Scaffold { theme: Some(Theme { density: Density::Large, .. }), .. }
        ));
    }
```

Run: `cd demos/barbershop && cargo test -p app-core large_controls; cd ../..` (use the package name from `demos/barbershop/app-core/Cargo.toml` if it differs). Expected: FAIL to compile.

- [ ] **Step 2: Model + Default**

In `pub struct Model` after `picked_day`:

```rust
    /// Profile "Large controls" toggle → `Theme.density = Density::Large`.
    large_controls: bool,
    /// Bookings "Serbian calendar" toggle → `calendar_in(Locale::SrLatn, …)`, Monday-first September 2026.
    serbian_calendar: bool,
```

In `impl Default for Model` after `picked_day: None,`:

```rust
            large_controls: false,
            serbian_calendar: false,
```

- [ ] **Step 3: Input.** In `fn input`'s `match value`, add before its final catch-all arm (or as a new arm if none):

```rust
            InputValue::Bool(on) => match id {
                "large_controls" => model.large_controls = on,
                "serbian_calendar" => model.serbian_calendar = on,
                _ => {}
            },
```

- [ ] **Step 4: Theme.** `density: if model.large_controls { Density::Large } else { Density::Comfortable },`

- [ ] **Step 5: Hinted scroller.** In `category_carousel`, `scroller(` → `scroller_hinted(`.

- [ ] **Step 6: Bookings.** Replace `let month = calendar(2026, 6, model.picked_day, Msg::PickDay);` (and its comment) with:

```rust
    // Localized month Calendar with 0–3 busy-dots per day. The toggle flips to Serbian
    // (Monday-first) September 2026 — 1 Sept is a Tuesday, so it sits under "U".
    let busy: Vec<u8> = (1..=31u8).map(|d| d % 4).collect();
    let month = if model.serbian_calendar {
        calendar_in(Locale::SrLatn, 2026, 9, model.picked_day, &busy, Msg::PickDay)
    } else {
        calendar_in(Locale::EnUs, 2026, 6, model.picked_day, &busy, Msg::PickDay)
    };
```

In the returned `column(vec![…])`, replace `subtitle("Pick a date"), card(month, CardStyle::Outlined),` with:

```rust
        subtitle("Pick a date"),
        toggle("serbian_calendar", "Serbian calendar (Monday first)", model.serbian_calendar),
        card(month, CardStyle::Outlined),
```

Replace the final `button("Book now", ButtonStyle::Filled, Msg::Book),` with:

```rust
        // Main action: wide + icon. Secondary: tonal. Destructive: danger tone (outlined + filled).
        button_with("Book now", ButtonStyle::Filled, Msg::Book, ButtonOpts::default().icon(Icon::Calendar).wide()),
        row(vec![
            button("Reschedule", ButtonStyle::Tonal, Msg::Book),
            button_with("No-show", ButtonStyle::Outlined, Msg::CancelBooking(0), ButtonOpts::default().tone(Tone::Danger)),
        ]),
        button_with("Cancel next booking", ButtonStyle::Filled, Msg::CancelBooking(0), ButtonOpts::default().tone(Tone::Danger).icon(Icon::Close).wide()),
```

(`Msg::CancelBooking` already guards an out-of-range index.)

- [ ] **Step 7: Profile.** After `progress(Some(completeness(model))),`:

```rust
        // Theme-level switch: Density::Large enlarges every control on every screen.
        toggle("large_controls", "Large controls", model.large_controls),
```

- [ ] **Step 8: Imports.** Add `button_with, calendar_in, scroller_hinted, toggle, ButtonOpts` to the `use mobiler_core::{…}` list (skip any already there). Remove `calendar` and `scroller` if clippy reports them unused.

- [ ] **Step 9: Test + clippy + web build**

```bash
cd demos/barbershop && cargo test && cargo clippy --all-targets && (cd web && trunk build); cd ../..
```

Expected: all tests pass (including the new one), no warnings, web builds.

- [ ] **Step 10: Commit**

```bash
git add demos/barbershop/app-core/src/lib.rs
git commit -m "feat(barbershop): showcase Large controls, localized calendar dots, toned buttons, hinted scroller"
git tag ltt-demo
```

---

### Task 18: Runtime acceptance (Android emulator + web), no commit

Density for px math: `D=$(adb shell wm density | awk '{print $NF}' | tr -d '\r')`, px = dp × D / 160.

- [ ] **Step 1:** Rebuild + install barbershop Android (Task 1 Step 4 commands).
- [ ] **Step 2: Large sizes.**
  1. On Profile, tap "Large controls" (dump → tap centre).
  2. Services tab: dump and check the chip label rows. Compose reports text bounds, not the chip container, so screenshot too (`adb exec-out screencap -p > $SCRATCH/large-services.png`) and measure the chip height in the PNG (`python3 -c "from PIL import Image; …"` or view the image).
  3. Bookings tab: measure "Book now" and a calendar day cell.

  Expected: button ≈ 56 dp, chip ≈ 48 dp, day cell ≥ 48 dp, segmented ≈ 56 dp. Save screenshots to `$SCRATCH/acceptance/`.
- [ ] **Step 3: Wide.** "Book now" spans the content width (screen width minus the 16 dp body padding each side).
- [ ] **Step 4: Tones.** "Cancel next booking" uses the error container colour (red fill); "Reschedule" uses the secondary-container fill. Check visually in the screenshot.
- [ ] **Step 5: Calendar.** Toggle "Serbian calendar". The header reads `P U S Č P S N` and the title "Septembar 2026". Day 1 sits in the 2nd column; days 1/2/3/4 show 1/2/3/0 dots.
- [ ] **Step 6: Scroller.** On Home, the category chip rail's trailing edge fades; after dragging to the end, the last chip is fully visible.
- [ ] **Step 7: Toggle Large off.** Controls return to the normal look.
- [ ] **Step 8: Web.**
  1. `cd demos/barbershop/web && trunk serve --port 8123` (background).
  2. Headless-Chrome screenshots at 412×915 of the default view: `google-chrome --headless=new --disable-gpu --no-sandbox --hide-scrollbars --window-size=412,915 --virtual-time-budget=9000 --screenshot=$SCRATCH/acceptance/web-home.png http://127.0.0.1:8123/`.
  3. For the Bookings/Profile toggles, use the CDP click recipe from memory `mobiler-web-shell`.

  Check the same six points. Stop trunk afterwards.
- [ ] **Step 9:** Shut the emulator down: `adb emu kill`. Record the results (pass/fail + measured values) for the PR body.

---

### Task 19: Versions, docs, clean history, ship PR-A

**Files:** `mobiler-ui/Cargo.toml`, `mobiler-core/Cargo.toml`, `mobiler-web/Cargo.toml`, lockfiles, `mobiler-ui/README.md` / `mobiler-core/README.md` / `mobiler-web/README.md` (only where they list widgets/density/builders), root `README.md` (widget list line ~110), `NOTES.md` (untracked).

- [ ] **Step 1: Bump versions**
  - `mobiler-ui/Cargo.toml` → `version = "0.24.0"`
  - `mobiler-core/Cargo.toml` → `version = "0.35.0"`, `mobiler-ui = { path = "../mobiler-ui", version = "0.24.0" }`
  - `mobiler-web/Cargo.toml` → `version = "0.35.0"`, `mobiler-core = { path = "../mobiler-core", version = "0.35" }`

```bash
cargo update -p mobiler-ui -p mobiler-core
(cd mobiler-web && cargo update -p mobiler-core -p mobiler-ui)
for w in demos/coffee demos/todo demos/barbershop demos/saldo demos/fullstack-todo demos/fullstack-todo/mobile demos/fullstack-sqlx demos/coffee/web demos/todo/web demos/barbershop/web demos/fullstack-todo/web-widgets demos/fullstack-sqlx/web; do
  [ -f $w/Cargo.lock ] && (cd $w && cargo update -p mobiler-core -p mobiler-ui 2>/dev/null; true)
done
```

- [ ] **Step 2: README audit.** Run `grep -n "Density\|Comfortable\|calendar\|scroller\|ButtonStyle" mobiler-*/README.md README.md`. Document `Density::Large`, `button_with`/`ButtonOpts` + `ButtonStyle::Tonal`, `calendar_in` + markers, and `scroller_hinted` wherever the neighbouring API is documented. Then run `cargo run -p xtask -- gen-readme --check`. Expected: in sync.

- [ ] **Step 3: NOTES.md.** Add a subsection "Large touch targets (ui 0.24 / core 0.35 / web 0.35)" with the *why* of each agreed decision (theme-level density, tone ≠ style, core-computed calendar layout, fade + trailing spacer, two-PR release), and bump the footer `*Last updated:*`. Do not `git add`.

- [ ] **Step 4: Commit the bumps + plan**

```bash
git add mobiler-ui/Cargo.toml mobiler-core/Cargo.toml mobiler-web/Cargo.toml Cargo.lock mobiler-web/Cargo.lock demos README.md mobiler-*/README.md docs/superpowers/plans/2026-09-17-large-touch-targets.md
git status --short   # confirm: no NOTES.md, no docs/large-touch-targets.md, no generated/ dirs
git commit -m "chore(release): mobiler-ui 0.24.0, mobiler-core 0.35.0, mobiler-web 0.35.0 + docs"
git tag ltt-rel
```

- [ ] **Step 5: Rebuild the history as one commit per feature** (each commit = the exact tree at that feature's end, so every commit on `main` builds)

```bash
BR=feat/large-touch-targets
git switch -c $BR-clean ltt-base
git restore --source=ltt-cal --staged --worktree :/ && git commit -m "feat(calendar): localized title, weekday header and week start + busy-dot markers"
git restore --source=ltt-btn --staged --worktree :/ && git commit -m "feat(button): Tonal style, tone (danger), leading icon, full width"
git restore --source=ltt-scr --staged --worktree :/ && git commit -m "feat(scroller): opt-in trailing edge fade"
git restore --source=ltt-den --staged --worktree :/ && git commit -m "feat(density): Density::Large — bigger controls for busy hands"
git restore --source=ltt-demo --staged --worktree :/ && git commit -m "feat(barbershop): showcase Large controls, localized calendar, toned buttons, hinted scroller"
git restore --source=ltt-rel --staged --worktree :/ && git commit -m "chore(release): mobiler-ui 0.24.0, mobiler-core 0.35.0, mobiler-web 0.35.0 + docs"
git diff $BR $BR-clean --stat   # MUST be empty
git branch -M $BR-clean $BR
```

Each commit message ends with the `Co-Authored-By` trailer from the session's attribution rules. (If any fix commits landed after a tag in Task 16, the tags were moved there, so the trees include the fixes.)

- [ ] **Step 6: Ship.** Use the `ship-pr` skill for push, PR, and all CI checks. PR body:
  - summary per feature
  - Task 18 acceptance table
  - "Template intentionally untouched — PR-C after the lib publish"

  **Merge with `gh pr merge <n> --rebase --delete-branch`** (not squash), then sync `main`. Delete the local `ltt-*` tags: `git tag -d ltt-base ltt-cal ltt-btn ltt-scr ltt-den ltt-demo ltt-rel`.

- [ ] **Step 7:** Clean regenerable caches per memory `cleanup-after-phase` (`demos/*/target`, `demos/*/Android/app/build`, `*/web/dist`). Keep the root `target/`.

---

### Task 20: Publish libs

- [ ] **Step 1:** Invoke the `release-libs` skill for ui 0.24.0 → core 0.35.0 → web 0.35.0 (dep order, each confirmed indexed, `cd` back to the repo root before root-workspace publishes). It has its own irreversible-publish gate, so confirm with the user there.

---

# PR-C — CLI template + release

### Task 21: Port the template shells, pin core, bump the CLI

**Files:**
- Modify: `mobiler/templates/iOS/Sources/Render.swift`
- Modify: `mobiler/templates/Android/app/src/main/java/__PACKAGE_PATH__/MainActivity.kt`
- Modify: `mobiler/templates/shared/Cargo.toml.tmpl` (`mobiler-core = "0.35"`)
- Modify: `mobiler/Cargo.toml` (`version = "0.52.0"`), `Cargo.lock`, `mobiler/README.md`

- [ ] **Step 1: Branch + iOS template**

```bash
git switch main && git pull --ff-only && git switch -c feat/large-touch-targets-cli
cp demos/barbershop/iOS/Sources/Render.swift mobiler/templates/iOS/Sources/Render.swift
```

- [ ] **Step 2: Android template.** Apply the Task 5, 9, 12, and 14 edits to the template `MainActivity.kt`. Imports use the same `androidx…` lines; shared types are already imported via `{{PACKAGE_SHARED_TYPES}}`. Drift check:

```bash
diff demos/barbershop/Android/app/src/main/java/dev/mobiler/barbershop/MainActivity.kt mobiler/templates/Android/app/src/main/java/__PACKAGE_PATH__/MainActivity.kt | grep '^[<>]' > $SCRATCH/after-template.diff
diff $SCRATCH/after-template.diff $SCRATCH/baseline/android-template.diff && echo TEMPLATE-DRIFT-UNCHANGED
```

Expected: `TEMPLATE-DRIFT-UNCHANGED` (the baseline file is from Task 1).

- [ ] **Step 3: Pin + bump**
  - `Cargo.toml.tmpl`: `mobiler-core = "0.35"`
  - `mobiler/Cargo.toml`: `version = "0.52.0"`
  - Run `cargo update -p mobiler`

- [ ] **Step 4: Scaffold smoke against published core 0.35**

```bash
cargo build -p mobiler
rm -rf $SCRATCH/ltt_scaffold && cd $SCRATCH && /media/zmilan/data2/cargo-target/debug/mobiler new ltt_scaffold --package dev.mobiler.lttscaffold
cd ltt_scaffold && CARGO_TARGET_DIR=$PWD/target JAVA_HOME=$HOME/jdk21 ANDROID_HOME=$HOME/Android/Sdk /media/zmilan/data2/cargo-target/debug/mobiler build android
cd /home/zmilan/working_docker/rust/mobiler
```

Expected: the APK builds.

- [ ] **Step 5: README.** In `mobiler/README.md`'s widget/theming section, mention `Density::Large`, toned/wide/icon buttons, localized calendar markers, and the hinted scroller. Include a "shells updated — run `mobiler upgrade`" note. `mobiler upgrade` 3-way-merges shells, so no `upgrade.rs` change is needed; confirm with `grep -n "Calendar\|Scroller\|Button" mobiler/src/upgrade.rs`, which should return no widget-specific anchors.

- [ ] **Step 6: Commit + ship**

```bash
git add mobiler/templates mobiler/Cargo.toml Cargo.lock mobiler/README.md
git commit -m "feat(cli): template shells for Density::Large, toned buttons, localized calendar, scroller fade + mobiler 0.52.0"
```

Invoke `ship-pr` (squash is fine; it is one commit). Wait for all CI, including `scaffold + build (template, Android)`.

---

### Task 22: Release CLI + post-release

- [ ] **Step 1:** Invoke `release-cli` for tag `v0.52.0`.
- [ ] **Step 2:** Invoke `post-release`. Beyond its checklist, smoke an **upgrade from 0.51**:
  1. Scaffold with `cargo install mobiler --version 0.51.0 --root $SCRATCH/m51`.
  2. Plant a comment sentinel in its `MainActivity.kt`.
  3. Run `mobiler upgrade --apply` with 0.52.0.
  4. Expect: the core pin is 0.35, the new arms are merged, the sentinel is preserved, there are no `.mobiler-new` conflicts, and the APK builds.
- [ ] **Step 3:** Update auto-memory `large-touch-targets.md` to SHIPPED (versions, PR numbers, lessons) and its `MEMORY.md` line. Ask the user whether to delete the untracked `docs/large-touch-targets.md` now that it is resolved.

---

## Self-Review Notes

- **Spec coverage:**
  - §2.1 Large → Tasks 13–15. Body-text row intentionally not implemented (agreed).
  - §2.2 wide/tonal/danger/icon → Tasks 7–10.
  - §2.3 locale + markers → Tasks 2–6.
  - §2.4 scroller hint → Tasks 11–12. Week-strip dots app-side (agreed).
  - §3 acceptance 1–6 → Task 18; acceptance 2 (pixel-identical) → the coding rule in Global Constraints, checked in review.
- **Pixel-identity traps checked:**
  - `TextButton` default padding (`TextButtonContentPadding`)
  - `.font(nil)` avoided on iOS (if/else instead)
  - `Modifier.then(Modifier)` is identity
  - `.frame(maxWidth: .infinity, minHeight: nil)` equals the original `.frame(maxWidth: .infinity)`
  - web class strings unchanged for neutral/not-wide/non-large
  - empty `markers` → no dot nodes
- **Type consistency:**
  - `leading_blanks`/`leadingBlanks`, `weekday_labels`/`weekdayLabels`, and `edge_fade`/`edgeFade` are used identically across Tasks 3–6, 11–12, and 21.
  - `ButtonOpts` methods `tone`/`icon`/`wide` match between Tasks 7 and 17.
  - `isLarge` (Kotlin) and `isLargeDensity()` (Swift) are introduced in Tasks 14/15 before use.
- **Known unknowns to resolve while implementing, not guesses baked in:**
  - iOS large-control heights are only compile-checked in CI. Visual confirmation is on-device (user) or MacinCloud.
  - Compose chip container height is measured from screenshots, because uiautomator reports only text bounds.
