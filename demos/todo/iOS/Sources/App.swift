import SwiftUI
import SharedTypes

/// App entry — the generic Mobiler shell. `Core` drives the Rust core; `render`
/// turns its `Widget` tree into SwiftUI. The whole UI is decided in Rust.
@main
struct TodoApp: App {
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
        content
            // Full-bleed system background so the status-bar and home-indicator
            // areas blend with the content instead of showing black bands — the
            // edge-to-edge look the Android shell has. Interactive content (bars,
            // scroll body) still stays within the safe area.
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
            .background(Color(.systemBackground).ignoresSafeArea())
    }

    // Re-renders whenever the core publishes a new view model. A Scaffold renders
    // its own bars + scrollable body; any other root we wrap in a scroll view so
    // tall content scrolls (mirrors the Android shell).
    @ViewBuilder private var content: some View {
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
