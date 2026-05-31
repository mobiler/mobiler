import SwiftUI
import SharedTypes
import UIKit
import ImageIO

// The ENTIRE iOS shell renderer. Knows only the fixed Mobiler ABI — `Widget`
// (what to draw) + `Action` (what to send back). No app-specific types; this exact
// code renders any Mobiler app. Style *intent* (TextStyle, Tone, …) is decided in
// Rust; the concrete look (fonts, colors, dp) is decided here — the iOS twin of the
// Compose `Render`. Recursion is type-erased through `AnyView`.

func render(_ widget: SharedTypes.Widget, _ send: @escaping (Action) -> Void) -> AnyView {
    switch widget {

    // MARK: content
    case .text(let content, let style):
        return AnyView(Text(content).modifier(TextStyleMod(style)))

    case .image(let source, let shape, let ratio):
        // Size the box from a ratio'd `Color.clear` (an intrinsic-less sizing view)
        // and let the image fill it as an overlay. Applying `.aspectRatio` to
        // `AsyncImage` directly is unreliable. Local file URLs (a picked photo) load
        // via UIImage — AsyncImage doesn't reliably fetch `file://`; remote URLs use
        // AsyncImage.
        let fill: AnyView
        if source.hasPrefix("file:"), let url = URL(string: source) {
            // Decode local images (a full-res camera/picked photo) OFF the main thread —
            // decoding a many-megapixel file inline in `render` (which runs on every update)
            // can stall the UI. FileImageView loads + downsamples asynchronously and caches.
            fill = AnyView(FileImageView(url: url))
        } else {
            fill = AnyView(AsyncImage(url: URL(string: source)) { image in
                image.resizable().aspectRatio(contentMode: .fill)
            } placeholder: { Color.gray.opacity(0.15) })
        }
        return AnyView(
            Color.clear
                .aspectRatio(aspect(ratio), contentMode: .fit)
                .overlay(fill)
                .clipShape(imageShape(shape))
                // An image is decoration, never a touch target. The sizing `Color.clear`
                // is hit-testable and has no accessibility element, so without this it
                // silently swallows taps meant for an adjacent control — e.g. an in-body
                // "← Back" button rendered just above an image, whose taps never reach
                // their action (verified on-sim). Let touches pass through.
                .allowsHitTesting(false)
        )

    case .badge(let label, let tone):
        let (bg, fg) = toneColors(tone)
        return AnyView(
            Text(label).font(.footnote.weight(.semibold))
                .padding(.horizontal, 12).padding(.vertical, 5)
                .background(bg).foregroundColor(fg).clipShape(Capsule())
        )

    case .colorDot(let color):
        return AnyView(Circle().fill(projectColor(color)).frame(width: 12, height: 12))

    case .divider:
        return AnyView(Divider())

    case .spacer(let size):
        return AnyView(Color.clear.frame(height: spacing(size)))

    // MARK: layout
    case .row(let children):
        return AnyView(HStack(spacing: 8) { childViews(children, send) })

    case .column(let children):
        return AnyView(VStack(alignment: .leading, spacing: 6) { childViews(children, send) })

    case .card(let child, let style, let onPress):
        let body = AnyView(render(child, send).padding(14).frame(maxWidth: .infinity, alignment: .leading).modifier(CardMod(style)))
        if let token = onPress {
            return AnyView(Button(action: { send(.fired(token: token)) }) { body }.buttonStyle(.plain))
        }
        return body

    case .box(let children, let align, let scrim):
        // A scrim box (hero banner) must be sized by its background — the first
        // child, an image — not by the dimming layer. A bare `Color` is an
        // infinitely greedy view, so leaving it as a ZStack sibling inflates the
        // box to fill the scroll view. Instead the scrim + foreground ride along
        // as an `.overlay` on the image (the SwiftUI twin of Compose's
        // `matchParentSize()`): sized to the image, never driving layout. The
        // clipShape matches the rounded hero image so the dim doesn't bleed corners.
        if scrim, children.count > 1, let first = children.first {
            return AnyView(
                render(first, send).overlay(
                    ZStack(alignment: boxAlign(align)) {
                        Color.black.opacity(0.4)
                        VStack(alignment: .leading) { childViews(Array(children.dropFirst()), send) }
                            .padding().foregroundColor(.white)
                    }
                    .clipShape(RoundedRectangle(cornerRadius: 16))
                )
            )
        }
        return AnyView(
            ZStack(alignment: boxAlign(align)) { childViews(children, send) }
        )

    case .grid(let children):
        // Column count adapts to width: 2 on a phone (compact), more on iPad.
        return AnyView(GridView(children: children, send: send))

    // MARK: input / actions
    case .button(let label, let style, let onPress):
        return AnyView(Button(label) { send(.fired(token: onPress)) }.modifier(ButtonStyleMod(style)))

    case .iconButton(let icon, let onPress):
        return AnyView(
            Button(action: { send(.fired(token: onPress)) }) {
                Image(systemName: sfSymbol(icon)).foregroundColor(iconTint(icon))
            }.buttonStyle(.plain)
        )

    case .chip(let label, let selected, let onPress):
        return AnyView(
            Button(action: { send(.fired(token: onPress)) }) {
                Text(label).font(.subheadline)
                    .padding(.horizontal, 12).padding(.vertical, 6)
                    .background(selected ? Color.accentColor.opacity(0.18) : Color.gray.opacity(0.12))
                    .foregroundColor(selected ? Color.accentColor : .primary)
                    .overlay(Capsule().stroke(selected ? Color.accentColor : .clear))
                    .clipShape(Capsule())
            }.buttonStyle(.plain)
        )

    case .textField(let id, let placeholder, let value):
        return AnyView(
            TextField(placeholder, text: Binding(
                get: { value },
                set: { send(.input(id: id, value: .text($0))) }
            ))
            .textFieldStyle(.roundedBorder)
        )

    case .toggle(let id, let label, let value):
        return AnyView(
            Toggle(label, isOn: Binding(
                get: { value },
                set: { send(.input(id: id, value: .bool($0))) }
            ))
        )

    case .checkbox(let id, let label, let value):
        // iOS has no native checkbox; a leading toggle-style control + label.
        return AnyView(
            Button(action: { send(.input(id: id, value: .bool(!value))) }) {
                HStack(spacing: 10) {
                    Image(systemName: value ? "checkmark.square.fill" : "square")
                        .foregroundColor(value ? .accentColor : .secondary)
                    Text(label).foregroundColor(.primary)
                    Spacer()
                }
            }.buttonStyle(.plain)
        )

    case .slider(let id, let value, let max):
        return AnyView(
            Slider(
                value: Binding(
                    get: { Double(value) },
                    set: { send(.input(id: id, value: .int(Int64($0.rounded())))) }
                ),
                in: 0...Double(max)
            )
        )

    case .stepper(let value, let onDecrement, let onIncrement):
        return AnyView(
            HStack(spacing: 12) {
                Button("−") { send(.fired(token: onDecrement)) }.buttonStyle(.bordered)
                Text("\(value)").font(.title3)
                Button("+") { send(.fired(token: onIncrement)) }.buttonStyle(.bordered)
            }
        )

    case .scaffold(let title, let body, let tabs, let back, let darkMode, let theme, let route, let depth):
        // Theme-as-data: stash the active theme so the (non-View) mapper helpers — spacing(),
        // imageShape(), CardMod, TextStyleMod — pick up corner/density/font. The brand color
        // is applied as a SwiftUI `.tint` on the ScaffoldView (it cascades to controls).
        ActiveTheme.current = theme
        return AnyView(ScaffoldView(
            title: title, content: body, tabs: tabs, back: back,
            darkMode: darkMode, theme: theme, route: route, depth: depth, send: send
        ))
    }
}

/// The active app [`Theme`] (set when a Scaffold renders), read by the non-View mapper helpers
/// for corner/density/font. `nil` ⇒ framework defaults (no visual change). Single-threaded,
/// main-actor render, so a static is safe — and the theme is app-global, like dark mode.
@MainActor
enum ActiveTheme {
    static var current: Theme?
}

/// Concrete look derived from the active theme (with framework defaults when un-themed).
extension Theme {
    var brandColor: Color { Color(red: Double(seed.r) / 255, green: Double(seed.g) / 255, blue: Double(seed.b) / 255) }
    var cardRadius: CGFloat { switch corner { case .none: 0; case .small: 8; case .medium: 14; case .large: 22 } }
    var imageRadius: CGFloat { switch corner { case .none: 0; case .small: 10; case .medium: 16; case .large: 24 } }
    var densityScale: CGFloat { switch density { case .compact: 0.75; case .comfortable: 1.0 } }
    var fontDesign: Font.Design { switch font { case .system: .default; case .rounded: .rounded; case .serif: .serif; case .monospace: .monospaced } }
}

/// Renders a `[Widget]` as sibling views (children of a stack/grid).
@ViewBuilder
private func childViews(_ children: [SharedTypes.Widget], _ send: @escaping (Action) -> Void) -> some View {
    ForEach(Array(children.enumerated()), id: \.offset) { _, child in
        render(child, send)
    }
}

/// A responsive grid: 2 columns on a phone (compact width), 4 on an iPad (regular) —
/// the iOS twin of the web shell's `auto-fill` grid and Android's width-derived count.
private struct GridView: View {
    let children: [SharedTypes.Widget]
    let send: (Action) -> Void
    @Environment(\.horizontalSizeClass) private var hSize
    var body: some View {
        let cols = hSize == .regular ? 4 : 2
        LazyVGrid(columns: Array(repeating: GridItem(.flexible(), spacing: 12), count: cols), spacing: 12) {
            childViews(children, send)
        }
    }
}

// MARK: - Scaffold (top bar + scrollable body + bottom tabs + theme-as-data)

private struct ScaffoldView: View {
    let title: String
    let content: SharedTypes.Widget
    let tabs: [SharedTypes.Tab]
    let back: String?
    let darkMode: Bool
    let theme: Theme?
    let route: String
    let depth: UInt32
    let send: (Action) -> Void

    // Remember the depth of the previous route so a route change knows its
    // direction (push vs pop). Updated after each route settles.
    @State private var prevDepth: UInt32 = 0
    // Regular width (iPad / large landscape) swaps the bottom tab-bar for a side rail.
    @Environment(\.horizontalSizeClass) private var hSize

    var body: some View {
        let useRail = hSize == .regular && !tabs.isEmpty
        Group {
            if useRail {
                HStack(spacing: 0) {
                    navRail
                    Divider()
                    mainColumn(showBottomTabs: false)
                }
            } else {
                mainColumn(showBottomTabs: true)
            }
        }
        .preferredColorScheme(darkMode ? .dark : .light)
        // Brand color cascades to buttons (.borderedProminent), chips, the .info tone, star,
        // toggles, sliders, text fields — one modifier themes most controls.
        .tint(theme?.brandColor)
        .animation(.easeInOut(duration: 0.28), value: route)
        // Edge-swipe to go back — the iOS idiom for Android's system BackHandler. The
        // gesture only engages past a 90pt drag from the leading edge, so it doesn't
        // interfere with taps (a tap has ~0 translation < the 24pt minimum). It's a
        // `.simultaneousGesture`, so it never blocks the buttons beneath it.
        .simultaneousGesture(
            DragGesture(minimumDistance: 24, coordinateSpace: .local).onEnded { value in
                guard let back = back else { return }
                let horizontal = abs(value.translation.width) > abs(value.translation.height)
                if value.startLocation.x < 32, value.translation.width > 90, horizontal {
                    send(.fired(token: back))
                }
            }
        )
        // After each route settles, record its depth for the next transition.
        .task(id: route) { prevDepth = depth }
    }

    // Top bar + scrollable body, optionally with the phone's bottom tab-bar.
    private func mainColumn(showBottomTabs: Bool) -> some View {
        VStack(spacing: 0) {
            HStack {
                if let back = back {
                    Button(action: { send(.fired(token: back)) }) {
                        Image(systemName: "chevron.left")
                    }
                }
                Spacer()
                Text(title).font(.headline)
                Spacer()
                // keep the title centered when a back button is present
                if back != nil { Image(systemName: "chevron.left").hidden() }
            }
            .padding()

            Divider()

            // The body is keyed by `route`, so a push/pop swaps the whole screen
            // (with a slide+fade; lateral move crossfades); a same-route update
            // just re-renders in place. The iOS twin of Android's AnimatedContent.
            // On a regular width the column is capped + centered so it doesn't stretch.
            ScrollView {
                VStack(alignment: .leading, spacing: 6) { render(self.content, send) }
                    .padding(16)
                    .frame(maxWidth: hSize == .regular ? 760 : .infinity, alignment: .leading)
                    .frame(maxWidth: .infinity)
            }
            .id(route)
            .transition(navTransition)
            .frame(maxWidth: .infinity, maxHeight: .infinity)

            if showBottomTabs && !tabs.isEmpty {
                Divider()
                HStack {
                    ForEach(Array(tabs.enumerated()), id: \.offset) { _, tab in
                        Button(action: { send(.fired(token: tab.onSelect)) }) {
                            Text(tab.label)
                                .fontWeight(tab.selected ? .semibold : .regular)
                                .foregroundColor(tab.selected ? .accentColor : .secondary)
                                .frame(maxWidth: .infinity)
                        }
                    }
                }
                .padding(.vertical, 10)
            }
        }
    }

    // Vertical navigation rail (regular width) — the iPad twin of the bottom tabs.
    private var navRail: some View {
        VStack(spacing: 4) {
            ForEach(Array(tabs.enumerated()), id: \.offset) { _, tab in
                Button(action: { send(.fired(token: tab.onSelect)) }) {
                    Text(tab.label)
                        .fontWeight(tab.selected ? .semibold : .regular)
                        .foregroundColor(tab.selected ? .accentColor : .secondary)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .padding(.vertical, 12)
                        .padding(.horizontal, 14)
                        .background(tab.selected ? Color.accentColor.opacity(0.12) : Color.clear)
                        .clipShape(RoundedRectangle(cornerRadius: 10))
                }
                .buttonStyle(.plain)
            }
            Spacer()
        }
        .padding(.vertical, 12)
        .padding(.horizontal, 8)
        .frame(width: 220)
    }

    private var navTransition: AnyTransition {
        if depth == prevDepth { return .opacity }          // lateral move → crossfade
        let forward = depth > prevDepth                     // push vs pop
        return .asymmetric(
            insertion: .move(edge: forward ? .trailing : .leading).combined(with: .opacity),
            removal: .move(edge: forward ? .leading : .trailing).combined(with: .opacity)
        )
    }
}

// MARK: - Style-token → concrete look (the only place that decides looks)

private struct TextStyleMod: ViewModifier {
    let style: TextStyle
    init(_ s: TextStyle) { style = s }
    // The theme's font design (rounded/serif/mono); `.default` when un-themed.
    private var design: Font.Design { ActiveTheme.current?.fontDesign ?? .default }
    func body(content: Content) -> some View {
        switch style {
        case .title: return AnyView(content.font(.system(.largeTitle, design: design).bold()))
        case .subtitle: return AnyView(content.font(.system(.title3, design: design).weight(.semibold)))
        case .caption: return AnyView(content.font(.system(.footnote, design: design)).foregroundColor(.secondary))
        case .emphasis: return AnyView(content.font(.system(.body, design: design).weight(.semibold)))
        case .body: return AnyView(content.font(.system(.body, design: design)))
        }
    }
}

private struct ButtonStyleMod: ViewModifier {
    let style: SharedTypes.ButtonStyle
    init(_ s: SharedTypes.ButtonStyle) { style = s }
    func body(content: Content) -> some View {
        switch style {
        case .filled: return AnyView(content.buttonStyle(.borderedProminent))
        case .outlined: return AnyView(content.buttonStyle(.bordered))
        case .text: return AnyView(content.buttonStyle(.borderless))
        }
    }
}

private struct CardMod: ViewModifier {
    let style: CardStyle
    init(_ s: CardStyle) { style = s }
    func body(content: Content) -> some View {
        let shape = RoundedRectangle(cornerRadius: ActiveTheme.current?.cardRadius ?? 14)
        switch style {
        case .elevated:
            return AnyView(content.background(shape.fill(Color(.secondarySystemBackground)))
                .shadow(color: .black.opacity(0.08), radius: 4, y: 2))
        case .filled:
            return AnyView(content.background(shape.fill(Color(.tertiarySystemBackground))))
        case .outlined:
            return AnyView(content.overlay(shape.stroke(Color.gray.opacity(0.3))))
        }
    }
}

private func spacing(_ s: Spacing) -> CGFloat {
    let base: CGFloat = { switch s { case .xs: return 4; case .sm: return 8; case .md: return 12; case .lg: return 16; case .xl: return 24 } }()
    return base * (ActiveTheme.current?.densityScale ?? 1.0)
}

private func toneColors(_ tone: Tone) -> (Color, Color) {
    switch tone {
    case .neutral: return (Color.gray.opacity(0.15), .secondary)
    case .success: return (Color.green.opacity(0.15), .green)
    case .warning: return (Color.orange.opacity(0.15), .orange)
    case .danger: return (Color.red.opacity(0.15), .red)
    case .info: return (Color.accentColor.opacity(0.15), .accentColor)
    }
}

private func projectColor(_ c: ProjectColor) -> Color {
    switch c {
    case .indigo: return Color(red: 0.36, green: 0.42, blue: 0.75)
    case .teal: return Color(red: 0.15, green: 0.65, blue: 0.60)
    case .coral: return Color(red: 1.0, green: 0.44, blue: 0.26)
    case .amber: return Color(red: 1.0, green: 0.70, blue: 0.0)
    case .lime: return Color(red: 0.61, green: 0.80, blue: 0.40)
    case .pink: return Color(red: 0.93, green: 0.25, blue: 0.48)
    }
}

private func sfSymbol(_ icon: Icon) -> String {
    switch icon {
    case .delete: return "trash"
    case .add: return "plus"
    case .edit: return "pencil"
    case .close: return "xmark"
    case .settings: return "gearshape"
    case .check: return "checkmark"
    case .star: return "star.fill"
    }
}

private func iconTint(_ icon: Icon) -> Color {
    switch icon { case .star: return .accentColor; default: return .primary }
}

private func imageShape(_ s: ImageShape) -> AnyShape {
    switch s {
    case .square: return AnyShape(Rectangle())
    case .rounded: return AnyShape(RoundedRectangle(cornerRadius: ActiveTheme.current?.imageRadius ?? 16))
    case .circle: return AnyShape(Circle())
    }
}

private func aspect(_ r: ImageRatio) -> CGFloat {
    switch r { case .wide: return 16.0 / 10.0; case .square: return 1.0; case .tall: return 3.0 / 4.0 }
}

private func boxAlign(_ a: BoxAlign) -> Alignment {
    switch a {
    case .topStart: return .topLeading
    case .topEnd: return .topTrailing
    case .center: return .center
    case .bottomStart: return .bottomLeading
    case .bottomCenter: return .bottom
    case .bottomEnd: return .bottomTrailing
    }
}

/// Renders a local file image, decoding it OFF the main thread (downsample + cache) with a
/// neutral placeholder until ready — so a large photo never blocks `render` (which runs on
/// every state update). A perf safeguard for many-megapixel camera/picked photos.
private struct FileImageView: View {
    let url: URL
    @State private var image: UIImage?
    var body: some View {
        ZStack {
            if let image {
                Image(uiImage: image).resizable().aspectRatio(contentMode: .fill)
            } else {
                Color.gray.opacity(0.15)
            }
        }
        .task(id: url.path) {
            if let cached = fileImageCache.object(forKey: url.path as NSString) { image = cached; return }
            image = await Task.detached(priority: .userInitiated) { downsampledFileImage(at: url) }.value
        }
    }
}

// A picked/captured photo can be many megapixels; decoding it full-size stalls the UI.
// Downsample with ImageIO to a display size and cache by path, so each file image is
// decoded once and kept small. Called from FileImageView's background task (off-main).
private let fileImageCache = NSCache<NSString, UIImage>()
private func downsampledFileImage(at url: URL, maxPixel: CGFloat = 1400) -> UIImage? {
    let key = url.path as NSString
    if let cached = fileImageCache.object(forKey: key) { return cached }
    guard let src = CGImageSourceCreateWithURL(url as CFURL, nil) else { return nil }
    let opts: [CFString: Any] = [
        kCGImageSourceCreateThumbnailFromImageAlways: true,
        kCGImageSourceCreateThumbnailWithTransform: true,
        kCGImageSourceShouldCacheImmediately: true,
        kCGImageSourceThumbnailMaxPixelSize: maxPixel,
    ]
    guard let cg = CGImageSourceCreateThumbnailAtIndex(src, 0, opts as CFDictionary) else { return nil }
    let img = UIImage(cgImage: cg)
    fileImageCache.setObject(img, forKey: key)
    return img
}
