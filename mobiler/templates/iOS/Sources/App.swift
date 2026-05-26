import SwiftUI

/// App entry — the generic Mobiler shell. `Core` drives the Rust core; `render`
/// turns its `Widget` tree into SwiftUI. The whole UI is decided in Rust.
@main
struct {{NAME}}App: App {
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
        // Re-renders whenever the core publishes a new view model.
        render(core.view) { core.update($0) }
    }
}
