# LazyList fill, remaining shell strings, iOS polish — design (Release 4)

**Date:** 2026-09-22.
**Base:** `main` @ `b6fc464`, with ui 0.26.0 / core 0.37.0 / web 0.37.0 and CLI 0.54.0 published.
**Origin:** the "Still open" list after Release 3, agreed with the user on 2026-09-22.

**Target:** ui 0.27.0 / core 0.38.0 / web 0.38.0 + CLI 0.55.0. Same two-PR release shape as before.

---

## 1. `LazyList` that fills the screen

### Problem

A `LazyList` is capped in height: 480dp on Android, 420pt on iOS, 60vh on web. It sits inside a scaffold body that scrolls as a page. When the list is the screen's main content, the list stops halfway down a tall phone and scrolls inside the page.

### Design

- **ABI:** `Widget::LazyList` gains `fill: bool` as its **last** field, after `end_label`.
  - Core: `with_fill(widget) -> Widget` sets it. It is a no-op on other widgets, and it combines with `with_refresh` and `with_end_label` in any order.
  - `lazy_list` and `lazy_list_static` default to `fill: false`.
- **The fill rule** is the same in every shell. A scaffold is in fill mode when its `body` is either:
  - a `LazyList` with `fill == true`, or
  - a `Column` whose **direct** children include a `LazyList` with `fill == true`. Only the first such child counts.

  Any other fill list (nested deeper, or a second one) keeps today's capped height, so no layout breaks.
- **In fill mode:**
  - The body does **not** scroll as a page. It is a full-height column.
  - Children before and after the fill list keep their natural height.
  - The fill list takes all remaining height and scrolls itself, with load-more and pull-to-refresh working as before.
  - A scaffold-level `with_refresh` still shows its progress bar. The pull gesture belongs to the list, if the list has `on_refresh`.
- **Per shell:**
  - **Android:**
    - The body `Column` is `fillMaxSize()` with no `verticalScroll`.
    - The scaffold renders the body `Column`'s children directly. It wraps the fill child in `Box(Modifier.weight(1f).fillMaxWidth())` and provides a `CompositionLocal` that tells that LazyList to use `fillMaxSize()` in place of `heightIn(max = 480.dp)`, both for the list and for its `PullToRefreshBox`.
    - If the body itself is the fill list, the whole body is that box.
  - **iOS:**
    - In place of the `ScrollView`, the body is a `VStack` that renders the Column's children directly.
    - The fill child gets `.frame(maxHeight: .infinity)`, and through an environment value `LazyListView` drops its fixed `.frame(height: 420)`.
  - **Web:**
    - The scaffold gets the class `scaffold-fill`: a `height: 100dvh` flex column (grid rows on the tablet rail layout) with `min-height: 0` down the chain.
    - The fill list gets `lazylist-fill` (`flex: 1; min-height: 0; max-height: none`).
    - The rules are scoped so they apply only to `.scaffold-fill .scaffold-body > .lazylist-fill` and `.scaffold-fill .scaffold-body > .col > .lazylist-fill`. A nested fill list keeps the 60vh cap.
- **Unchanged:** apps that don't call `with_fill` keep today's layout.

## 2. The remaining English shell strings

`ShellLabels` gains three optional fields **after** `done`:
- **`pdf_error`:** Android `PdfView` error text. With a label, the label alone is shown. With no label, today's `"PDF: {platform error}"` stays.
- **`pdf_title`:** the web PDF `<iframe title>`, default `"PDF"`.
- **`web_title`:** the web WebView `<iframe title>`, default `"Web"`.

The same precedence applies as in Release 3: the scaffold label, then the default, with an empty label treated as absent. The builder gains `.pdf_error(..)`, `.pdf_title(..)` and `.web_title(..)`.

## 3. iOS polish

- **Theme from the root only.**
  - `ActiveTheme.current` is set in `Core.swift` from the root view (init and `.render`), next to `ActiveLabels`.
  - The per-scaffold write in `Render.swift` is removed.
  - `ScaffoldView` keeps using its own `theme` parameter for the tint.
- **A replacement confirm always appears.** Today, `present` can be dropped while the previous alert's dismissal transition is running. The fix:
  - **We dismiss the old alert:** present the new alert in the `dismiss(animated:false, completion:)` completion.
  - **The old alert is already dismissing:** present after `presenter.transitionCoordinator` completes, or immediately if there is no coordinator.
  - The new alert is stored in `openConfirm` before presenting, so a third confirm arriving in between still supersedes it correctly.

## Showcase and acceptance

- **Barbershop:**
  - A new **Feed** tab, with an existing `Icon`. Its body is a `column` of a short caption followed by `with_fill(with_end_label(with_refresh(lazy_list(…), …), "You're all caught up"))`.
  - The feed card moves off Profile.
  - Barbershop also sets `.pdf_error(…)`, `.pdf_title(…)` and `.web_title(…)` in its `ShellLabels`.
- **Web (CDP):**
  - On the Feed tab, the list's bottom is within 2px of the top of the tab bar or the viewport bottom.
  - The page itself doesn't scroll (`scrollHeight == clientHeight` on the document).
  - The list scrolls, and "Show more" loads more rows.
  - Profile's iframes have the app's `title`s.
  - Coffee is unchanged.
- **Android:**
  - On the Feed tab, the list fills to just above the tab bar (bounds from a uiautomator dump), scrolls, and loads more.
  - Never leave the Profile tab: the pre-existing swiftshader crash. The Feed tab is reached from Home.
- **iOS:** compiled by CI; the logic is reviewed.
- **Core tests:**
  - `LazyList` round-trips with `fill`.
  - `with_fill` sets it, is a no-op elsewhere, and combines in any order.
  - `ShellLabels` round-trips with the new fields.

## Out of scope

- A `fill` for widgets other than `LazyList`.
- Multiple fill regions.
- Fill inside `Split` panes or sheets. All of these fall back to the cap.
