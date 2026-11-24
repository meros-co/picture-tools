import CMerosPictureTools
import CoreGraphics
import Foundation
import SwiftUI

/// The scope kinds, as the Rust crate's `ScopeKind`.
public enum ScopeKind: Int32 {
    case waveform = 0, parade, vectorscope, histogram
}

/// Pixel layouts `ScopeFrame.compute` reads.
public enum ScopePixelFormat: Int32 {
    case rgba = 0, bgra, uyvy, nv12
}

/// One scope: an intensity (0–255) per cell, rows from the top.
public struct ScopeFrame: Equatable {
    public let kind: ScopeKind
    public let width: Int
    public let height: Int
    public let data: [UInt8]

    public init(kind: ScopeKind, width: Int, height: Int, data: [UInt8]) {
        self.kind = kind
        self.width = width
        self.height = height
        self.data = data
    }

    /// Computes a scope of a picture through the C ABI. `optionsJSON` is a
    /// `ScopeOptions` as JSON; nil for the defaults.
    public static func compute(
        _ kind: ScopeKind,
        format: ScopePixelFormat,
        pixels: UnsafeRawBufferPointer,
        width: Int,
        height: Int,
        stride: Int,
        optionsJSON: String? = nil
    ) -> ScopeFrame? {
        guard let base = pixels.baseAddress?.assumingMemoryBound(to: UInt8.self) else { return nil }
        func run(_ options: UnsafePointer<CChar>?) -> ScopeFrame? {
            var w: UInt32 = 0, h: UInt32 = 0
            guard mpt_scope(kind.rawValue, format.rawValue, base, UInt32(width), UInt32(height), UInt32(stride), options, &w, &h, nil) == 0 else {
                return nil
            }
            var out = [UInt8](repeating: 0, count: Int(w * h))
            _ = mpt_scope(kind.rawValue, format.rawValue, base, UInt32(width), UInt32(height), UInt32(stride), options, &w, &h, &out)
            return ScopeFrame(kind: kind, width: Int(w), height: Int(h), data: out)
        }
        if let optionsJSON {
            return optionsJSON.withCString { run($0) }
        }
        return run(nil)
    }

    /// The trace as an image, tinted per parade panel; nil for the
    /// histogram, which is drawn as curves.
    func traceImage(trace: (CGFloat, CGFloat, CGFloat), panels: [(CGFloat, CGFloat, CGFloat)]) -> CGImage? {
        guard kind != .histogram, width > 0, height > 0 else { return nil }
        var px = [UInt8](repeating: 0, count: width * height * 4)
        let count = kind == .parade ? 3 : 1
        let panelWidth = max(width / count, 1)
        for y in 0..<height {
            for x in 0..<width {
                let i = y * width + x
                let v = i < data.count ? data[i] : 0
                if v == 0 { continue }
                let c = kind == .parade ? panels[min(x / panelWidth, 2)] : trace
                let o = i * 4
                px[o] = UInt8(c.0 * CGFloat(v))
                px[o + 1] = UInt8(c.1 * CGFloat(v))
                px[o + 2] = UInt8(c.2 * CGFloat(v))
                px[o + 3] = v
            }
        }
        guard let provider = CGDataProvider(data: Data(px) as CFData) else { return nil }
        return CGImage(
            width: width, height: height, bitsPerComponent: 8, bitsPerPixel: 32, bytesPerRow: width * 4,
            space: CGColorSpaceCreateDeviceRGB(),
            bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.premultipliedLast.rawValue),
            provider: provider, decode: nil, shouldInterpolate: true, intent: .defaultIntent)
    }
}

/// Draws a scope with its graticule: IRE lines on the waveform and parade
/// (panels marked), the edge, axes, skin-tone line and 75% bar targets on
/// the vectorscope, and the histogram's four channels as curves.
@available(iOS 15.0, tvOS 15.0, macOS 12.0, *)
public struct ScopeView: View {
    public let frame: ScopeFrame
    /// The gain the vectorscope was computed with.
    public var vectorscopeGain: CGFloat = 1

    public init(_ frame: ScopeFrame, vectorscopeGain: CGFloat = 1) {
        self.frame = frame
        self.vectorscopeGain = vectorscopeGain
    }

    static let bars: [(CGFloat, CGFloat, CGFloat)] = [(191, 191, 0), (0, 191, 191), (0, 191, 0), (191, 0, 191), (191, 0, 0), (0, 0, 191)]

    public var body: some View {
        let image = frame.traceImage(trace: (0.91, 0.91, 0.91), panels: [(1, 0.32, 0.32), (0.41, 0.94, 0.68), (0.27, 0.54, 1)])
        Canvas { context, size in
            let grid = GraphicsContext.Shading.color(.white.opacity(0.2))
            switch frame.kind {
            case .waveform, .parade:
                for f in [0.0, 0.25, 0.5, 0.75, 1.0] {
                    let y = (1 - f) * (size.height - 1) + 0.5
                    context.stroke(Path { $0.move(to: CGPoint(x: 0, y: y)); $0.addLine(to: CGPoint(x: size.width, y: y)) }, with: grid)
                }
                if frame.kind == .parade {
                    for f in [1.0 / 3, 2.0 / 3] {
                        let x = size.width * f
                        context.stroke(Path { $0.move(to: CGPoint(x: x, y: 0)); $0.addLine(to: CGPoint(x: x, y: size.height)) }, with: grid)
                    }
                }
                if let image { context.draw(Image(decorative: image, scale: 1), in: CGRect(origin: .zero, size: size)) }
            case .vectorscope:
                let side = min(size.width, size.height)
                let origin = CGPoint(x: (size.width - side) / 2, y: (size.height - side) / 2)
                let center = CGPoint(x: origin.x + side / 2, y: origin.y + side / 2)
                context.stroke(Path(ellipseIn: CGRect(origin: origin, size: CGSize(width: side, height: side))), with: grid)
                context.stroke(Path { $0.move(to: CGPoint(x: center.x, y: origin.y)); $0.addLine(to: CGPoint(x: center.x, y: origin.y + side)) }, with: grid)
                context.stroke(Path { $0.move(to: CGPoint(x: origin.x, y: center.y)); $0.addLine(to: CGPoint(x: origin.x + side, y: center.y)) }, with: grid)
                let skin = 123 * CGFloat.pi / 180
                context.stroke(
                    Path { $0.move(to: center); $0.addLine(to: CGPoint(x: center.x + cos(skin) * side / 2, y: center.y - sin(skin) * side / 2)) },
                    with: .color(Color(red: 1, green: 0.67, blue: 0.57).opacity(0.7)))
                let box = side / 40
                for b in Self.bars {
                    let y = 0.2126 * b.0 + 0.7152 * b.1 + 0.0722 * b.2
                    let cb = (b.2 - y) / 1.8556 * vectorscopeGain / 256
                    let cr = (b.0 - y) / 1.5748 * vectorscopeGain / 256
                    let at = CGPoint(x: center.x + cb * side, y: center.y - cr * side)
                    context.stroke(Path(CGRect(x: at.x - box / 2, y: at.y - box / 2, width: box, height: box)), with: grid)
                }
                if let image { context.draw(Image(decorative: image, scale: 1), in: CGRect(origin: origin, size: CGSize(width: side, height: side))) }
            case .histogram:
                let bins = frame.width
                guard bins > 1 else { return }
                let rows: [Color] = [.white.opacity(0.6), .red.opacity(0.35), .green.opacity(0.35), .blue.opacity(0.35)]
                for r in stride(from: 3, through: 0, by: -1) {
                    var p = Path()
                    p.move(to: CGPoint(x: 0, y: size.height))
                    for i in 0..<bins {
                        let k = r * bins + i
                        let v = k < frame.data.count ? CGFloat(frame.data[k]) : 0
                        p.addLine(to: CGPoint(x: CGFloat(i) * size.width / CGFloat(bins - 1), y: size.height - v / 255 * size.height))
                    }
                    p.addLine(to: CGPoint(x: size.width, y: size.height))
                    context.fill(p, with: .color(rows[r]))
                }
            }
        }
    }
}
