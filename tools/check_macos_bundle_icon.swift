// Verify the icon macOS resolves for a built app, not just the .icns on disk.
import AppKit
import Foundation
import UniformTypeIdentifiers

func fail(_ message: String) -> Never {
    fputs("\(message)\n", stderr)
    exit(1)
}

guard CommandLine.arguments.count == 2 else {
    fail("Usage: swift tools/check_macos_bundle_icon.swift PATH.app")
}

let bundle = CommandLine.arguments[1]
guard FileManager.default.fileExists(atPath: bundle + "/Contents/Info.plist") else {
    fail("Not an app bundle: \(bundle)")
}

func thumbnail(_ icon: NSImage) -> Data {
    let size = 64
    guard let bitmap = NSBitmapImageRep(
        bitmapDataPlanes: nil,
        pixelsWide: size,
        pixelsHigh: size,
        bitsPerSample: 8,
        samplesPerPixel: 4,
        hasAlpha: true,
        isPlanar: false,
        colorSpaceName: .deviceRGB,
        bytesPerRow: 0,
        bitsPerPixel: 0
    ), let context = NSGraphicsContext(bitmapImageRep: bitmap) else {
        fail("Cannot render app icon")
    }
    NSGraphicsContext.saveGraphicsState()
    NSGraphicsContext.current = context
    NSColor.clear.setFill()
    NSRect(x: 0, y: 0, width: size, height: size).fill()
    icon.draw(in: NSRect(x: 0, y: 0, width: size, height: size))
    context.flushGraphics()
    NSGraphicsContext.restoreGraphicsState()
    guard let png = bitmap.representation(using: .png, properties: [:]) else {
        fail("Cannot encode app icon thumbnail")
    }
    return png
}

let resolved = thumbnail(NSWorkspace.shared.icon(forFile: bundle))
let generic = thumbnail(NSWorkspace.shared.icon(for: UTType.applicationBundle))
guard resolved != generic else {
    fail("macOS resolved the generic application icon for \(bundle)")
}
print("macOS resolves a custom icon for \(bundle)")
