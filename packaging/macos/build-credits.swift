// Generate the native macOS About panel's rich-text credits at packaging time.
// Only the resulting assets are shipped; no Swift component is linked to the app.
import AppKit

let arguments = CommandLine.arguments
guard arguments.count == 3 else { fatalError("Expected logo PNG and output Credits.rtfd path") }
let input = URL(fileURLWithPath: arguments[1])
let output = URL(fileURLWithPath: arguments[2], isDirectory: true)
guard let logo = NSImage(contentsOf: input), logo.size.width > 0 else {
    fatalError("Could not load LetzKwad logo")
}
// Keep logo and role credits visible together in the standard compact About panel.
let displayedSize = NSSize(width: 48, height: 48 * logo.size.height / logo.size.width)
let pixels = NSSize(width: displayedSize.width * 2, height: displayedSize.height * 2)
guard let bitmap = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: Int(pixels.width),
    pixelsHigh: Int(pixels.height), bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true,
    isPlanar: false, colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0),
    let graphics = NSGraphicsContext(bitmapImageRep: bitmap) else {
    fatalError("Could not render native About logo")
}
NSGraphicsContext.saveGraphicsState()
NSGraphicsContext.current = graphics
let canvas = NSRect(origin: .zero, size: pixels)
NSColor.white.setFill()
canvas.fill()
logo.draw(in: canvas, from: .zero, operation: .sourceOver, fraction: 1)
NSGraphicsContext.restoreGraphicsState()
bitmap.size = displayedSize
guard let png = bitmap.representation(using: .png, properties: [:]) else {
    fatalError("Could not encode native About logo")
}
let imageFile = FileWrapper(regularFileWithContents: png)
imageFile.preferredFilename = "letzkwad.png"
let attachment = NSTextAttachment(fileWrapper: imageFile)
attachment.bounds = NSRect(origin: .zero, size: displayedSize)
let credits = NSMutableAttributedString(attributedString: NSAttributedString(attachment: attachment))
let requested = "Coded by Claude, Codex, Driven by Defane, Thanat0s"
credits.append(NSAttributedString(string: "\n" + requested + "\n"))
for (label, target) in [("LetzKwad", "https://letzkwad.lu/"),
                         ("FPV Rush project", "https://github.com/defane/FPV-rush-extractor")] {
    credits.append(NSAttributedString(string: label, attributes: [.link: URL(string: target)!]))
    credits.append(NSAttributedString(string: "\n"))
}
let paragraph = NSMutableParagraphStyle()
paragraph.alignment = .center
let range = NSRange(location: 0, length: credits.length)
credits.addAttributes([.paragraphStyle: paragraph, .font: NSFont.systemFont(ofSize: 12)], range: range)
guard let wrapper = credits.rtfdFileWrapper(from: range, documentAttributes: [:]) else {
    fatalError("Could not generate native About credits")
}
try wrapper.write(to: output, options: .atomic, originalContentsURL: nil)
// Check the same file/format AppKit loads for orderFrontStandardAboutPanel:.
let restored = try NSAttributedString(url: output, options: [:], documentAttributes: nil)
guard restored.string.contains(requested) else { fatalError("Native About credits are missing") }
var attachments = 0
var targets = Set<String>()
restored.enumerateAttributes(in: NSRange(location: 0, length: restored.length)) { attrs, _, _ in
    if let attachment = attrs[.attachment] as? NSTextAttachment {
        guard attachment.fileWrapper?.regularFileContents?.isEmpty == false else {
            fatalError("Native About logo attachment is empty")
        }
        attachments += 1
    }
    if let url = attrs[.link] as? URL { targets.insert(url.absoluteString) }
    else if let text = attrs[.link] as? String { targets.insert(text) }
}
guard attachments == 1 && targets.contains("https://letzkwad.lu/") &&
    targets.contains("https://github.com/defane/FPV-rush-extractor") else {
    fatalError("Native About logo or links are missing")
}
print("Native About credits verified: logo, exact credit text and both links.")
