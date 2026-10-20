import XCTest

// Behavioral regression test for the generic Mobiler iOS shell, driven through barbershop (moved here from
// the coffee demo when coffee became CLI-managed, ADR-0063: the template can't carry a second Xcode target).
// Guards the bug fixed in coffee's build 4: an in-body "Back" button sitting directly above an image had its
// taps swallowed by the image's hit-testable `Color.clear` (`.allowsHitTesting(false)` on images is the fix).
// A unit test can't catch it — only a real tap routed through the view hierarchy does — so this runs on the
// simulator via `xcodebuild test`. Barbershop's phone Services screen is the same shape: the selected
// service's detail shows an in-body Back right above the service's image.
final class FadehouseUITests: XCTestCase {

    func testInBodyBackButtonNavigatesBack() throws {
        let app = XCUIApplication(bundleIdentifier: "dev.mobiler.barbershop")
        app.launch()

        let services = app.buttons["Services"]
        XCTAssertTrue(services.waitForExistence(timeout: 20), "the Services tab should appear")
        services.tap()

        // Open a service (its card is labelled with the service's name).
        let card = app.buttons.matching(NSPredicate(format: "label CONTAINS[c] 'Classic Cut'")).firstMatch
        XCTAssertTrue(card.waitForExistence(timeout: 20), "the Classic Cut card should appear")
        card.tap()

        // The detail shows the in-body Back above the service's image.
        let back = app.buttons["Back"]
        XCTAssertTrue(back.waitForExistence(timeout: 20), "the detail should show the in-body Back button")
        XCTAssertTrue(app.buttons["Book this"].exists, "the detail's Book this button should show")

        // Tapping it must return to the grid — the regression under test.
        back.tap()
        XCTAssertTrue(card.waitForExistence(timeout: 20), "tapping Back should return to the services grid")
        XCTAssertFalse(app.buttons["Book this"].exists, "the detail should be gone once back on the grid")
    }
}
