import XCTest

// Behavioral regression test for the generic Mobiler iOS shell, driven through the
// coffee demo. Guards the bug fixed in build 4: the in-body "← Back" button sits
// directly above the hero image, and the image's hit-testable `Color.clear` used to
// swallow the button's taps (`.allowsHitTesting(false)` on images is the fix). A unit
// test can't catch this — it only shows up when a real tap is routed through the view
// hierarchy — so this runs on the simulator via `xcodebuild test`.
final class CoffeeUITests: XCTestCase {

    /// Open a product, tap "← Back", and confirm we return to the storefront. If an
    /// image (or anything else) ever eats the back button's tap again, this fails.
    func testInBodyBackButtonNavigatesBack() throws {
        let app = XCUIApplication(bundleIdentifier: "dev.mobiler.coffee")
        app.launch()

        // Open a product (any card — labelled with the coffee name).
        let card = app.buttons.matching(NSPredicate(format: "label CONTAINS[c] 'Mocha'")).firstMatch
        XCTAssertTrue(card.waitForExistence(timeout: 10), "storefront product card should appear")
        card.tap()

        // On the detail screen the in-body back button exists.
        let back = app.buttons["← Back"]
        XCTAssertTrue(back.waitForExistence(timeout: 5), "detail screen should show the ← Back button")

        // Tapping it must return to the storefront — the regression under test.
        back.tap()

        let getStarted = app.buttons["Get Started"]
        XCTAssertTrue(getStarted.waitForExistence(timeout: 5),
                      "tapping ← Back should return to the storefront (regression: image swallowed the tap)")
        XCTAssertFalse(app.buttons["← Back"].exists, "← Back should be gone once back on the storefront")
    }
}
