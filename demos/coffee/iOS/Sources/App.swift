import SwiftUI
import SharedTypes

/// App entry — the generic Mobiler shell. `Core` drives the Rust core; `render`
/// turns its `Widget` tree into SwiftUI. The whole UI is decided in Rust.
@main
struct CoffeeApp: App {
    @StateObject private var core = Core()

    var body: some Scene {
        WindowGroup {
            RootView(core: core)
        }
    }
}

private struct RootView: View {
    @ObservedObject var core: Core
    var body: some View {
        // Re-renders whenever the core publishes a new view model. A Scaffold
        // renders its own bars + scrollable body; any other root we wrap in a
        // scroll view so tall content scrolls (mirrors the Android shell).
        if case .scaffold = core.view {
            render(core.view) { core.update($0) }
        } else {
            ScrollView {
                render(core.view) { core.update($0) }
                    .padding(16)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
    }
}
