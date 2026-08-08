import AppKit
import CoreGraphics
import Foundation
import ImageIO
import ScreenCaptureKit
import UniformTypeIdentifiers

func fail(_ message: String) -> Never {
    FileHandle.standardError.write(Data((message + "\n").utf8))
    exit(1)
}

func captureImage(filter: SCContentFilter, configuration: SCStreamConfiguration) async throws -> CGImage {
    try await withCheckedThrowingContinuation { continuation in
        SCScreenshotManager.captureImage(
            contentFilter: filter,
            configuration: configuration
        ) { image, error in
            if let image {
                continuation.resume(returning: image)
            } else if let error {
                continuation.resume(throwing: error)
            } else {
                continuation.resume(throwing: NSError(
                    domain: "capture-macos-window",
                    code: 1,
                    userInfo: [NSLocalizedDescriptionKey: "capture returned no image"]
                ))
            }
        }
    }
}

guard CommandLine.arguments.count == 8 else {
    fail("usage: capture-macos-window OWNER TITLE X Y WIDTH HEIGHT OUTPUT.png")
}

let owner = CommandLine.arguments[1]
let title = CommandLine.arguments[2]
guard
    let x = Double(CommandLine.arguments[3]),
    let y = Double(CommandLine.arguments[4]),
    let width = Double(CommandLine.arguments[5]),
    let height = Double(CommandLine.arguments[6]),
    width > 0,
    height > 0
else {
    fail("capture rectangle must contain finite positive numbers")
}
let output = URL(fileURLWithPath: CommandLine.arguments[7])
_ = NSApplication.shared

do {
    let content = try await SCShareableContent.excludingDesktopWindows(
        false,
        onScreenWindowsOnly: true
    )
    let matches = content.windows.filter { window in
        window.owningApplication?.applicationName == owner && window.title == title
    }
    guard matches.count == 1, let window = matches.first else {
        fail("expected one visible \(owner)/\(title) window, found \(matches.count)")
    }
    let relative = CGRect(
        x: x - window.frame.minX,
        y: y - window.frame.minY,
        width: width,
        height: height
    )
    guard
        relative.minX >= 0,
        relative.minY >= 0,
        relative.maxX <= window.frame.width,
        relative.maxY <= window.frame.height
    else {
        fail("capture rectangle is outside the matched window")
    }

    let scale = 2.0
    let configuration = SCStreamConfiguration()
    configuration.sourceRect = relative
    configuration.width = Int(relative.width * scale)
    configuration.height = Int(relative.height * scale)
    configuration.showsCursor = false
    let filter = SCContentFilter(desktopIndependentWindow: window)
    let image = try await captureImage(filter: filter, configuration: configuration)
    guard let destination = CGImageDestinationCreateWithURL(
        output as CFURL,
        UTType.png.identifier as CFString,
        1,
        nil
    ) else {
        fail("cannot create PNG destination")
    }
    CGImageDestinationAddImage(destination, image, nil)
    guard CGImageDestinationFinalize(destination) else {
        fail("cannot publish PNG capture")
    }
} catch {
    fail("window capture failed: \(error)")
}
