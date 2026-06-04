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

    case .avatar(let source, let status):
        return AnyView(AvatarView(source: source, status: status))

    case .rating(let value, let max, let onRate):
        return AnyView(RatingView(value: value, max: max, onRate: onRate, send: send))

    case .divider:
        return AnyView(Divider())

    case .progress(let value):
        if let v = value {
            return AnyView(ProgressView(value: Double(v)).progressViewStyle(.linear).padding(.vertical, 4))
        }
        return AnyView(ProgressView().padding(.vertical, 4))

    case .skeleton:
        return AnyView(RoundedRectangle(cornerRadius: 8).fill(Color.gray.opacity(0.2)).frame(height: 48).padding(.vertical, 4))

    case .chart(let series, let labels, let style, let axis, let legend):
        return AnyView(ChartView(series: series, labels: labels, style: style, axis: axis, legend: legend))

    case .regionChart(let regions, let ticks, let xMax, let yMax, let refLines, let bracket, let legend):
        return AnyView(RegionChartView(regions: regions, ticks: ticks, xMax: xMax, yMax: yMax, refLines: refLines, bracket: bracket, legend: legend))

    case .calendar(let year, let month, let firstWeekday, let selected, let onDay):
        return AnyView(CalendarView(year: year, month: month, firstWeekday: firstWeekday, selected: selected, onDay: onDay, send: send))

    case .swipeAction(let child, let actions):
        return AnyView(SwipeActionView(content: child, actions: actions, send: send))

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

    case .scroller(let children):
        return AnyView(
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: 12) { childViews(children, send) }
            }
        )

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

    case .textField(let id, let placeholder, let value, let kind, let error):
        let binding = Binding(
            get: { value },
            set: { send(.input(id: id, value: .text($0))) }
        )
        let lowercase = (kind == .email || kind == .url)
        let kb: UIKeyboardType
        switch kind {
        case .email: kb = .emailAddress
        case .number: kb = .numberPad
        case .decimal: kb = .decimalPad
        case .phone: kb = .phonePad
        case .url: kb = .URL
        default: kb = .default
        }
        let control: AnyView
        switch kind {
        case .secure:
            control = AnyView(SecureField(placeholder, text: binding).textFieldStyle(.roundedBorder))
        case .multiline:
            control = AnyView(TextField(placeholder, text: binding, axis: .vertical)
                .lineLimit(3...6).textFieldStyle(.roundedBorder))
        default:
            control = AnyView(TextField(placeholder, text: binding)
                .textFieldStyle(.roundedBorder)
                .keyboardType(kb)
                .textInputAutocapitalization(lowercase ? .never : .sentences)
                .autocorrectionDisabled(lowercase))
        }
        return AnyView(
            VStack(alignment: .leading, spacing: 4) {
                control
                if let error = error {
                    Text(error).font(.caption).foregroundColor(.red)
                }
            }
        )

    case .searchField(let id, let placeholder, let value):
        return AnyView(
            HStack(spacing: 8) {
                Image(systemName: "magnifyingglass").foregroundColor(.secondary)
                TextField(placeholder, text: Binding(
                    get: { value },
                    set: { send(.input(id: id, value: .text($0))) }
                ))
            }
            .padding(.horizontal, 14).padding(.vertical, 10)
            .background(Color.gray.opacity(0.12))
            .clipShape(Capsule())
        )

    case .segmented(let segments):
        return AnyView(
            HStack(spacing: 4) {
                ForEach(Array(segments.enumerated()), id: \.offset) { _, seg in
                    Button(action: { send(.fired(token: seg.onSelect)) }) {
                        Text(seg.label).font(.subheadline.weight(.semibold))
                            .frame(maxWidth: .infinity)
                            .padding(.vertical, 8)
                            .background(seg.selected ? Color.accentColor : Color.clear)
                            .foregroundColor(seg.selected ? .white : .secondary)
                            .clipShape(Capsule())
                    }.buttonStyle(.plain)
                }
            }
            .padding(4)
            .background(Color.gray.opacity(0.12))
            .clipShape(Capsule())
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

    case .scaffold(let title, let body, let tabs, let back, let darkMode, let theme, let fab, let sheet, let onRefresh, let refreshing, let route, let depth):
        // Theme-as-data: stash the active theme so the (non-View) mapper helpers — spacing(),
        // imageShape(), CardMod, TextStyleMod — pick up corner/density/font. The brand color
        // is applied as a SwiftUI `.tint` on the ScaffoldView (it cascades to controls).
        ActiveTheme.current = theme
        return AnyView(ScaffoldView(
            title: title, content: body, tabs: tabs, back: back,
            darkMode: darkMode, theme: theme, fab: fab, sheet: sheet,
            onRefresh: onRefresh, refreshing: refreshing, route: route, depth: depth, send: send
        ))
    }
}

/// The active app [`Theme`] (set when a Scaffold renders), read by the non-View mapper helpers
/// for corner/density/font. `nil` ⇒ framework defaults (no visual change). Render is
/// single-threaded on the main actor, so `nonisolated(unsafe)` is sound here — it lets the
/// nonisolated mapper helpers (spacing/imageShape/TextStyleMod) read it; the theme is
/// app-global, like dark mode.
enum ActiveTheme {
    nonisolated(unsafe) static var current: Theme?
}

/// Concrete look derived from the active theme (with framework defaults when un-themed).
extension Theme {
    var brandColor: Color { Color(red: Double(seed.r) / 255, green: Double(seed.g) / 255, blue: Double(seed.b) / 255) }
    var accentColor: Color { accent.map { Color(red: Double($0.r) / 255, green: Double($0.g) / 255, blue: Double($0.b) / 255) } ?? brandColor }
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

// Circular avatar image with an optional colored status dot.
private struct AvatarView: View {
    let source: String
    let status: Tone?
    var body: some View {
        let img: AnyView
        if source.hasPrefix("file:"), let url = URL(string: source) {
            img = AnyView(FileImageView(url: url))
        } else {
            img = AnyView(AsyncImage(url: URL(string: source)) { image in
                image.resizable().aspectRatio(contentMode: .fill)
            } placeholder: { Color.gray.opacity(0.15) })
        }
        return img
            .frame(width: 48, height: 48)
            .clipShape(Circle())
            .overlay(alignment: .bottomTrailing) {
                if let status = status {
                    Circle().fill(toneColors(status).0)
                        .frame(width: 12, height: 12)
                        .overlay(Circle().stroke(Color(.systemBackground), lineWidth: 2))
                }
            }
    }
}

// Distinct fallback colors for series 1.. (series 0 with no override rides the accent color).
private let chartPalette: [Color]

// Palette as RGB (parallel to chartPalette) so RegionChart can compute per-band label contrast.
private let regionPaletteRGB: [(Double, Double, Double)] = [
    Color(red: 224.0 / 255, green: 119.0 / 255, blue: 44.0 / 255),
    Color(red: 46.0 / 255, green: 160.0 / 255, blue: 106.0 / 255),
    Color(red: 192.0 / 255, green: 70.0 / 255, blue: 107.0 / 255),
    Color(red: 138.0 / 255, green: 92.0 / 255, blue: 192.0 / 255),
    Color(red: 201.0 / 255, green: 162.0 / 255, blue: 39.0 / 255),
    Color(red: 63.0 / 255, green: 167.0 / 255, blue: 214.0 / 255),
]

// Multi-series chart: cartesian (bar/line/stacked) with optional y-axis + legend, or circular
// (pie/donut/rings/gauge). Drawn with SwiftUI Canvas; the iOS twin of the Compose Chart arm and
// mobiler-web's chart_view. Non-interactive.
private struct ChartView: View {
    let series: [ChartSeries]
    let labels: [String]
    let style: SharedTypes.ChartStyle
    let axis: Bool
    let legend: Bool

    private func color(_ i: Int) -> Color {
        if i < series.count, let c = series[i].color {
            return Color(red: Double(c.r) / 255, green: Double(c.g) / 255, blue: Double(c.b) / 255)
        }
        return i == 0 ? Color.accentColor : chartPalette[(i - 1) % chartPalette.count]
    }
    private func mag(_ i: Int) -> Float { i < series.count ? series[i].values.reduce(0, +) : 0 }
    private func fmtTick(_ v: Float) -> String {
        abs(v - v.rounded()) < 0.05 ? "\(Int(v))" : String(format: "%.1f", v)
    }
    private var isCartesian: Bool {
        switch style { case .bar, .line, .stackedBar, .stackedBar100: return true; default: return false }
    }
    private var nslots: Int { max(series.map { $0.values.count }.max() ?? 0, 1) }
    private func slotVal(_ s: ChartSeries, _ j: Int) -> Float { s.values.indices.contains(j) ? s.values[j] : 0 }
    private var maxV: Float {
        switch style {
        case .stackedBar:
            let totals = (0 ..< nslots).map { j in series.reduce(Float(0)) { $0 + slotVal($1, j) } }
            return max(totals.max() ?? 1, 1e-6)
        case .stackedBar100: return 1
        default: return max(series.flatMap { $0.values }.max() ?? 1, 1e-6)
        }
    }
    private var gaugePct: Int {
        guard !series.isEmpty else { return 0 }
        let g = series[0].goal ?? mag(0)
        return Int((min(mag(0) / (g <= 0 ? 1e-6 : g), 1) * 100).rounded())
    }

    var body: some View {
        VStack(spacing: 4) {
            if isCartesian {
                HStack(spacing: 4) {
                    if axis {
                        VStack(alignment: .trailing) {
                            Text(fmtTick(maxV)).font(.caption2).foregroundColor(.secondary)
                            Spacer()
                            Text(fmtTick(maxV / 2)).font(.caption2).foregroundColor(.secondary)
                            Spacer()
                            Text(fmtTick(0)).font(.caption2).foregroundColor(.secondary)
                        }.frame(height: 120)
                    }
                    cartesianCanvas.frame(height: 120)
                }
                if !labels.isEmpty {
                    HStack(spacing: 0) {
                        ForEach(Array(labels.enumerated()), id: \.offset) { _, l in
                            Text(l).font(.caption2).foregroundColor(.secondary).frame(maxWidth: .infinity)
                        }
                    }
                }
            } else {
                ZStack {
                    circularCanvas
                    if case .gauge = style { Text("\(gaugePct)%").font(.title2).bold() }
                }.frame(height: 140)
            }
            if legend, !series.isEmpty {
                HStack(spacing: 12) {
                    ForEach(Array(series.enumerated()), id: \.offset) { i, s in
                        HStack(spacing: 4) {
                            RoundedRectangle(cornerRadius: 2).fill(color(i)).frame(width: 10, height: 10)
                            Text(s.name).font(.caption2).foregroundColor(.secondary)
                        }
                    }
                }
            }
        }.padding(.vertical, 4)
    }

    private var cartesianCanvas: some View {
        Canvas { ctx, size in
            let w = size.width
            let top: CGFloat = 2
            let plot = max(size.height - 4, 1)
            let bottom = top + plot
            if axis {
                for k in 0 ... 4 {
                    let y = top + CGFloat(k) * plot / 4
                    var p = Path(); p.move(to: CGPoint(x: 0, y: y)); p.addLine(to: CGPoint(x: w, y: y))
                    ctx.stroke(p, with: .color(.gray.opacity(0.25)), lineWidth: 0.5)
                }
            }
            switch style {
            case .line:
                for (i, s) in series.enumerated() {
                    let n = max(s.values.count, 1)
                    var p = Path()
                    for (j, v) in s.values.enumerated() {
                        let x = n == 1 ? w / 2 : w * CGFloat(j) / CGFloat(n - 1)
                        let y = top + (1 - CGFloat(min(max(v / maxV, 0), 1))) * plot
                        if j == 0 { p.move(to: CGPoint(x: x, y: y)) } else { p.addLine(to: CGPoint(x: x, y: y)) }
                    }
                    ctx.stroke(p, with: .color(color(i)), style: StrokeStyle(lineWidth: 2, lineCap: .round, lineJoin: .round))
                }
            case .bar:
                let sw = w / CGFloat(nslots)
                let ns = CGFloat(max(series.count, 1))
                for (i, s) in series.enumerated() {
                    for (j, v) in s.values.enumerated() {
                        let bh = CGFloat(min(max(v / maxV, 0), 1)) * plot
                        let bw = sw * 0.8 / ns
                        let x = CGFloat(j) * sw + sw * 0.1 + CGFloat(i) * bw
                        ctx.fill(Path(CGRect(x: x, y: bottom - bh, width: bw, height: bh)), with: .color(color(i)))
                    }
                }
            default: // stackedBar / stackedBar100
                let sw = w / CGFloat(nslots)
                for j in 0 ..< nslots {
                    let slotTotal = max(series.reduce(Float(0)) { $0 + slotVal($1, j) }, 1e-6)
                    let denom: Float = { if case .stackedBar100 = style { return slotTotal } else { return maxV } }()
                    var acc: CGFloat = 0
                    for (i, s) in series.enumerated() {
                        let bh = CGFloat(min(max(slotVal(s, j) / denom, 0), 1)) * plot
                        let x = CGFloat(j) * sw + sw * 0.15
                        ctx.fill(Path(CGRect(x: x, y: bottom - acc - bh, width: sw * 0.7, height: bh)), with: .color(color(i)))
                        acc += bh
                    }
                }
            }
        }
    }

    private var circularCanvas: some View {
        Canvas { ctx, size in
            let cx = size.width / 2, cy = size.height / 2
            let center = CGPoint(x: cx, y: cy)
            let rad = min(size.width, size.height) / 2 - 6
            func ring(_ r: CGFloat, _ from: Double, _ to: Double) -> Path {
                var p = Path()
                p.addArc(center: center, radius: r, startAngle: .degrees(from), endAngle: .degrees(to), clockwise: false)
                return p
            }
            switch style {
            case .pie, .donut:
                let total = max((0 ..< series.count).reduce(Float(0)) { $0 + mag($1) }, 1e-6)
                var start = -90.0
                for i in 0 ..< series.count {
                    let sweep = Double(mag(i) / total) * 360
                    if case .donut = style {
                        ctx.stroke(ring(rad - 10, start, start + sweep), with: .color(color(i)), lineWidth: 20)
                    } else {
                        var p = Path()
                        p.move(to: center)
                        p.addArc(center: center, radius: rad, startAngle: .degrees(start), endAngle: .degrees(start + sweep), clockwise: false)
                        p.closeSubpath()
                        ctx.fill(p, with: .color(color(i)))
                    }
                    start += sweep
                }
            case .rings:
                let n = max(series.count, 1)
                for i in 0 ..< series.count {
                    let r = rad - CGFloat(i) * (rad * 0.62 / CGFloat(n)) - 2
                    ctx.stroke(Path(ellipseIn: CGRect(x: cx - r, y: cy - r, width: r * 2, height: r * 2)), with: .color(.gray.opacity(0.2)), lineWidth: 10)
                    let g = series[i].goal ?? mag(i)
                    let prog = Double(min(max(mag(i) / (g <= 0 ? 1e-6 : g), 0), 1))
                    if prog > 0 {
                        ctx.stroke(ring(r, -90, -90 + 360 * prog), with: .color(color(i)), style: StrokeStyle(lineWidth: 10, lineCap: .round))
                    }
                }
            default: // gauge
                let r = rad - 4
                ctx.stroke(ring(r, 135, 135 + 270), with: .color(.gray.opacity(0.2)), style: StrokeStyle(lineWidth: 12, lineCap: .round))
                if !series.isEmpty {
                    let g = series[0].goal ?? mag(0)
                    let prog = Double(min(max(mag(0) / (g <= 0 ? 1e-6 : g), 0), 1))
                    ctx.stroke(ring(r, 135, 135 + 270 * prog), with: .color(color(0)), style: StrokeStyle(lineWidth: 12, lineCap: .round))
                }
            }
        }
    }
}

// Variable-width stacked-region / coverage-gap chart: absolute-positioned region rectangles in the
// [0,xMax]×[0,yMax] plane + ref lines/chips + irregular x-ticks + optional right bracket + legend.
// The iOS twin of mobiler-web's region_chart_view. Non-interactive.
private struct RegionChartView: View {
    let regions: [ChartRegion]
    let ticks: [ChartTick]
    let xMax: Float
    let yMax: Float
    let refLines: [ChartRefLine]
    let bracket: ChartBracket?
    let legend: [ChartLegendItem]

    private func rgbOf(_ i: Int) -> (Double, Double, Double) {
        if i < regions.count, let c = regions[i].color {
            return (Double(c.r) / 255, Double(c.g) / 255, Double(c.b) / 255)
        }
        return regionPaletteRGB[i % regionPaletteRGB.count]
    }
    private func color(_ i: Int) -> Color {
        let (r, g, b) = rgbOf(i)
        return Color(red: r, green: g, blue: b)
    }
    private func textOn(_ i: Int) -> Color {
        let (r, g, b) = rgbOf(i)
        return (0.299 * r + 0.587 * g + 0.114 * b) > 0.55 ? Color(white: 0.1) : Color(white: 0.96)
    }
    private func fmtTick(_ v: Float) -> String {
        abs(v - v.rounded()) < 0.05 ? "\(Int(v))" : String(format: "%.1f", v)
    }

    // Everything is drawn in ONE Canvas coordinate space: a reserved left gutter (y labels) +
    // plot + right margin (chips/bracket). Regions, axes, ticks, and x-labels all map through the
    // same px()/py(), so they line up by construction. The iOS twin of mobiler-web's region chart.
    var body: some View {
        let xm = max(xMax, 1e-6)
        let ym = max(yMax, 1e-6)
        VStack(spacing: 6) {
            Canvas { ctx, size in
                let gutter: CGFloat = 42
                let chipW: CGFloat = 58
                let plotX = gutter
                let plotTop: CGFloat = 6
                let plotBottom = size.height - 22
                let plotH = max(plotBottom - plotTop, 1)
                let plotW = max(size.width - gutter - chipW, 1)
                let px: (Float) -> CGFloat = { plotX + CGFloat(min(max($0 / xm, 0), 1)) * plotW }
                let py: (Float) -> CGFloat = { plotTop + CGFloat(1 - min(max($0 / ym, 0), 1)) * plotH }
                let axisColor = Color.primary.opacity(0.8)
                let red = Color(red: 0.75, green: 0.22, blue: 0.17)

                for (i, r) in regions.enumerated() {
                    let rect = CGRect(x: px(r.x0), y: py(r.y1), width: px(r.x1) - px(r.x0), height: py(r.y0) - py(r.y1))
                    ctx.fill(Path(rect), with: .color(color(i)))
                    ctx.stroke(Path(rect), with: .color(.white.opacity(0.4)), lineWidth: 0.5)
                    if !r.label.isEmpty {
                        let avail = r.vertical ? (rect.height - 6) : (rect.width - 6)
                        var fs: CGFloat = 11
                        var t = ctx.resolve(Text(r.label).font(.system(size: fs)).foregroundColor(textOn(i)))
                        let mw = t.measure(in: CGSize(width: 4000, height: 4000)).width
                        if avail > 1, mw > avail {
                            fs = max(7, fs * avail / mw)
                            t = ctx.resolve(Text(r.label).font(.system(size: fs)).foregroundColor(textOn(i)))
                        }
                        if r.vertical {
                            var c = ctx
                            c.translateBy(x: rect.midX, y: rect.midY)
                            c.rotate(by: .degrees(-90))
                            c.draw(t, at: .zero, anchor: .center)
                        } else {
                            ctx.draw(t, at: CGPoint(x: rect.midX, y: rect.midY), anchor: .center)
                        }
                    }
                }
                for rl in refLines {
                    let y = py(rl.value)
                    var p = Path(); p.move(to: CGPoint(x: plotX, y: y)); p.addLine(to: CGPoint(x: plotX + plotW, y: y))
                    ctx.stroke(p, with: .color(red), style: rl.dashed ? StrokeStyle(lineWidth: 2, dash: [6, 5]) : StrokeStyle(lineWidth: 2))
                }
                if let b = bracket {
                    let bx = plotX + plotW + 4
                    let yt = py(b.y1); let yb = py(b.y0)
                    var p = Path()
                    p.move(to: CGPoint(x: bx, y: yt)); p.addLine(to: CGPoint(x: bx, y: yb))
                    p.move(to: CGPoint(x: bx, y: yt)); p.addLine(to: CGPoint(x: bx - 5, y: yt))
                    p.move(to: CGPoint(x: bx, y: yb)); p.addLine(to: CGPoint(x: bx - 5, y: yb))
                    ctx.stroke(p, with: .color(axisColor), lineWidth: 1.5)
                    ctx.draw(ctx.resolve(Text(b.info ? "ⓘ\n" + b.label : b.label).font(.system(size: 8)).foregroundColor(.secondary)), at: CGPoint(x: bx + 4, y: (yt + yb) / 2), anchor: .leading)
                }
                var ax = Path()
                ax.move(to: CGPoint(x: plotX, y: plotTop)); ax.addLine(to: CGPoint(x: plotX, y: plotBottom))
                ax.move(to: CGPoint(x: plotX, y: plotBottom)); ax.addLine(to: CGPoint(x: plotX + plotW, y: plotBottom))
                ctx.stroke(ax, with: .color(axisColor), lineWidth: 2)
                for k in 0 ... 4 {
                    let v = ym * Float(k) / 4
                    let y = py(v)
                    var p = Path(); p.move(to: CGPoint(x: plotX - 6, y: y)); p.addLine(to: CGPoint(x: plotX, y: y))
                    ctx.stroke(p, with: .color(axisColor), lineWidth: 1.5)
                    ctx.draw(ctx.resolve(Text(fmtTick(v)).font(.system(size: 9)).foregroundColor(.secondary)), at: CGPoint(x: plotX - 6, y: y), anchor: .trailing)
                }
                for t in ticks {
                    let x = px(t.at)
                    var p = Path(); p.move(to: CGPoint(x: x, y: plotBottom)); p.addLine(to: CGPoint(x: x, y: plotBottom + 5))
                    ctx.stroke(p, with: .color(axisColor), lineWidth: 1.5)
                    ctx.draw(ctx.resolve(Text(t.label).font(.caption2).foregroundColor(.secondary)), at: CGPoint(x: x, y: plotBottom + 13), anchor: .center)
                }
                for rl in refLines {
                    let y = py(rl.value)
                    let ct = ctx.resolve(Text(rl.label).font(.system(size: 9).weight(.semibold)).foregroundColor(Color(white: 0.1)))
                    let sz = ct.measure(in: CGSize(width: chipW, height: 40))
                    let cx = plotX + plotW + chipW / 2
                    let box = CGRect(x: cx - sz.width / 2 - 4, y: y - sz.height / 2 - 2, width: sz.width + 8, height: sz.height + 4)
                    ctx.fill(Path(roundedRect: box, cornerRadius: 4), with: .color(.white))
                    ctx.stroke(Path(roundedRect: box, cornerRadius: 4), with: .color(.black.opacity(0.2)), lineWidth: 0.5)
                    ctx.draw(ct, at: CGPoint(x: cx, y: y), anchor: .center)
                }
            }
            .frame(height: 312)
            if !legend.isEmpty {
                let rows = (legend.count + 2) / 3
                VStack(spacing: 2) {
                    ForEach(0 ..< rows, id: \.self) { row in
                        HStack(spacing: 12) {
                            ForEach(Array((row * 3) ..< min(row * 3 + 3, legend.count)), id: \.self) { idx in
                                let li = legend[idx]
                                HStack(spacing: 4) {
                                    RoundedRectangle(cornerRadius: 2)
                                        .fill(Color(red: Double(li.color.r) / 255, green: Double(li.color.g) / 255, blue: Double(li.color.b) / 255))
                                        .frame(width: 10, height: 10)
                                    Text(li.label).font(.caption2).foregroundColor(.secondary)
                                }
                            }
                        }
                    }
                }
            }
        }.padding(.vertical, 4)
    }
}

// A list row that reveals trailing action buttons on horizontal swipe; tap an action to fire it.
private struct SwipeActionView: View {
    let content: SharedTypes.Widget
    let actions: [SwipeButton]
    let send: (Action) -> Void
    @State private var offset: CGFloat = 0
    private var revealWidth: CGFloat { CGFloat(actions.count) * 84 }
    var body: some View {
        ZStack(alignment: .trailing) {
            HStack(spacing: 8) {
                ForEach(Array(actions.enumerated()), id: \.offset) { _, a in
                    Button(action: { send(.fired(token: a.onTap)); withAnimation { offset = 0 } }) {
                        Text(a.label)
                            .font(.subheadline.weight(.semibold))
                            .foregroundColor(.white)
                            .frame(width: 76)
                            .frame(maxHeight: .infinity)
                            .background(toneColors(a.tone).1)
                            .clipShape(RoundedRectangle(cornerRadius: 12))
                    }.buttonStyle(.plain)
                }
            }
            .padding(.vertical, 4)
            render(content, send)
                .background(Color(.systemBackground))
                .offset(x: offset)
                .gesture(
                    DragGesture()
                        .onChanged { v in offset = min(0, max(-revealWidth, v.translation.width)) }
                        .onEnded { _ in withAnimation { offset = offset < -revealWidth / 2 ? -revealWidth : 0 } }
                )
        }
        .clipped()
    }
}

// Inline month calendar — weekday header, leading blanks from `firstWeekday`, tappable days.
private struct CalendarView: View {
    let year: UInt32
    let month: UInt8
    let firstWeekday: UInt8
    let selected: UInt8?
    let onDay: [String]
    let send: (Action) -> Void
    private let cols = Array(repeating: GridItem(.flexible(), spacing: 4), count: 7)
    private let weekdays = ["S", "M", "T", "W", "T", "F", "S"]
    private let months = ["January", "February", "March", "April", "May", "June",
                          "July", "August", "September", "October", "November", "December"]
    var body: some View {
        VStack(spacing: 6) {
            Text("\(months[Int(month) - 1]) \(String(year))").font(.headline)
            LazyVGrid(columns: cols, spacing: 4) {
                ForEach(Array(weekdays.enumerated()), id: \.offset) { _, w in
                    Text(w).font(.caption2).foregroundColor(.secondary)
                }
                ForEach(0..<Int(firstWeekday), id: \.self) { _ in Color.clear.frame(height: 32) }
                ForEach(Array(onDay.enumerated()), id: \.offset) { idx, token in
                    let day = idx + 1
                    let isSel = selected.map { Int($0) == day } ?? false
                    Button(action: { send(.fired(token: token)) }) {
                        Text("\(day)").frame(maxWidth: .infinity, minHeight: 32)
                            .background(isSel ? Color.accentColor : Color.clear)
                            .foregroundColor(isSel ? .white : .primary)
                            .clipShape(Circle())
                    }.buttonStyle(.plain)
                }
            }
        }.padding(.vertical, 4)
    }
}

// Star rating; tappable when `onRate` carries one token per star.
private struct RatingView: View {
    let value: UInt32
    let max: UInt8
    let onRate: [String]?
    let send: (Action) -> Void
    var body: some View {
        HStack(spacing: 2) {
            ForEach(1...Int(max), id: \.self) { i in
                let threshold = UInt32(i) * 10
                let name = value >= threshold ? "star.fill" : (value + 5 >= threshold ? "star.leadinghalf.filled" : "star")
                if let tokens = onRate, i - 1 < tokens.count {
                    let token = tokens[i - 1]
                    Button(action: { send(.fired(token: token)) }) {
                        Image(systemName: name).foregroundColor(.accentColor)
                    }.buttonStyle(.plain)
                } else {
                    Image(systemName: name).foregroundColor(.accentColor)
                }
            }
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
    let fab: SharedTypes.Fab?
    let sheet: SharedTypes.Sheet?
    let onRefresh: String?
    let refreshing: Bool
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
        // Modal bottom sheet — a scrim (tap to dismiss) + a panel from the bottom.
        .overlay {
            if let sheet = sheet {
                ZStack(alignment: .bottom) {
                    Color.black.opacity(0.45).ignoresSafeArea()
                        .onTapGesture { send(.fired(token: sheet.onDismiss)) }
                    VStack(alignment: .leading, spacing: 8) {
                        Capsule().fill(Color.secondary.opacity(0.4))
                            .frame(width: 40, height: 4).frame(maxWidth: .infinity)
                        Text(sheet.title).font(.title3.bold())
                        render(sheet.child, send)
                    }
                    .padding(20)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .background(Color(.systemBackground))
                    .clipShape(RoundedRectangle(cornerRadius: 20))
                }
                .transition(.opacity)
            }
        }
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
                VStack(alignment: .leading, spacing: 6) {
                    if refreshing { ProgressView().frame(maxWidth: .infinity).padding(.vertical, 4) }
                    render(self.content, send)
                }
                    .padding(16)
                    .frame(maxWidth: hSize == .regular ? 760 : .infinity, alignment: .leading)
                    .frame(maxWidth: .infinity)
            }
            .refreshableIf(onRefresh, send)
            .id(route)
            .transition(navTransition)
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            // Floating action button — the raised primary action, over the body bottom-trailing.
            .overlay(alignment: .bottomTrailing) {
                if let fab = fab {
                    Button(action: { send(.fired(token: fab.onPress)) }) {
                        Image(systemName: sfSymbol(fab.icon))
                            .font(.title2)
                            .frame(width: 56, height: 56)
                            .background(theme?.brandColor ?? .accentColor)
                            .foregroundColor(.white)
                            .clipShape(RoundedRectangle(cornerRadius: 18))
                            .shadow(radius: 6, y: 3)
                    }
                    .padding(18)
                }
            }

            if showBottomTabs && !tabs.isEmpty {
                Divider()
                HStack {
                    ForEach(Array(tabs.enumerated()), id: \.offset) { _, tab in
                        Button(action: { send(.fired(token: tab.onSelect)) }) {
                            VStack(spacing: 2) {
                                if let icon = tab.icon {
                                    Image(systemName: sfSymbol(icon)).font(.system(size: 20))
                                }
                                Text(tab.label).font(.caption)
                            }
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
                    HStack(spacing: 10) {
                        if let icon = tab.icon {
                            Image(systemName: sfSymbol(icon)).font(.system(size: 18))
                        }
                        Text(tab.label)
                    }
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
        case .brand:
            let t = ActiveTheme.current
            let grad = LinearGradient(
                colors: [t?.brandColor ?? .accentColor, t?.accentColor ?? .accentColor],
                startPoint: .topLeading, endPoint: .bottomTrailing,
            )
            return AnyView(content.background(shape.fill(grad)).foregroundColor(.white))
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
    case .info: return "info.circle"
    case .home: return "house.fill"
    case .search: return "magnifyingglass"
    case .menu: return "line.3.horizontal"
    case .filter: return "line.3.horizontal.decrease.circle"
    case .back: return "chevron.left"
    case .forward: return "chevron.right"
    case .down: return "chevron.down"
    case .bell: return "bell.fill"
    case .cart: return "cart.fill"
    case .share: return "square.and.arrow.up"
    case .heart: return "heart"
    case .heartFilled: return "heart.fill"
    case .person: return "person.fill"
    case .people: return "person.2.fill"
    case .phone: return "phone.fill"
    case .mail: return "envelope.fill"
    case .calendar: return "calendar"
    case .clock: return "clock.fill"
    case .mapPin: return "mappin.and.ellipse"
    case .camera: return "camera.fill"
    case .photo: return "photo"
    case .play: return "play.fill"
    case .scissors: return "scissors"
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

// Pull-to-refresh, gated: attaches `.refreshable` only when the scaffold carries an on_refresh
// token (so screens without it aren't bouncy). The event is fire-and-forget; the brief sleep keeps
// the native pull spinner visible while the app's async reload kicks off (the model's `refreshing`
// flag drives the in-body indicator that reflects the real reload duration).
extension View {
    @ViewBuilder
    func refreshableIf(_ token: String?, _ send: @escaping (Action) -> Void) -> some View {
        if let token = token {
            self.refreshable {
                send(.fired(token: token))
                try? await Task.sleep(nanoseconds: 600_000_000)
            }
        } else {
            self
        }
    }
}
