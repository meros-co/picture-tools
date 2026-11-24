import Foundation

/// Everything the tools do to a picture: the Rust crate's
/// `PictureToolsParams`, in its JSON form.
public struct PictureToolsParams: Codable, Equatable {
    public enum Range: String, Codable { case full, limited }
    public enum LumaWeights: String, Codable { case bt709, bt601, bt2020 }
    public enum FalseColorOutside: String, Codable { case picture, grayscale }

    public struct FalseColorBand: Codable, Equatable {
        public var lo: Float
        public var hi: Float
        public var rgb: [Float]
        public init(_ lo: Float, _ hi: Float, _ rgb: [Float]) {
            self.lo = lo
            self.hi = hi
            self.rgb = rgb
        }
    }

    public struct FalseColor: Codable, Equatable {
        public var bands: [FalseColorBand]
        public var outside: FalseColorOutside
        public init(bands: [FalseColorBand], outside: FalseColorOutside = .picture) {
            self.bands = bands
            self.outside = outside
        }
    }

    public struct Zebra: Codable, Equatable {
        public var level: Float
        public var band: Float?
        public init(level: Float, band: Float? = nil) {
            self.level = level
            self.band = band
        }
    }

    /// Still black-and-white stripes, or dark stripes moving along the
    /// diagonal.
    public enum ZebraStyle: Codable, Equatable {
        case still(stripePx: Float)
        case animated(stripePx: Float, speedPxPerS: Float)

        enum CodingKeys: String, CodingKey { case style, stripe_px, speed_px_per_s }

        public init(from decoder: Decoder) throws {
            let c = try decoder.container(keyedBy: CodingKeys.self)
            let stripe = try c.decodeIfPresent(Float.self, forKey: .stripe_px) ?? 8
            if try c.decode(String.self, forKey: .style) == "animated" {
                self = .animated(stripePx: stripe, speedPxPerS: try c.decodeIfPresent(Float.self, forKey: .speed_px_per_s) ?? 40)
            } else {
                self = .still(stripePx: stripe)
            }
        }

        public func encode(to encoder: Encoder) throws {
            var c = encoder.container(keyedBy: CodingKeys.self)
            switch self {
            case .still(let stripe):
                try c.encode("static", forKey: .style)
                try c.encode(stripe, forKey: .stripe_px)
            case .animated(let stripe, let speed):
                try c.encode("animated", forKey: .style)
                try c.encode(stripe, forKey: .stripe_px)
                try c.encode(speed, forKey: .speed_px_per_s)
            }
        }
    }

    public struct Peaking: Codable, Equatable {
        public var threshold: Float
        public var color: [Float]
        public init(threshold: Float, color: [Float] = [1, 0, 0]) {
            self.threshold = threshold
            self.color = color
        }
    }

    public var range: Range = .full
    public var luma: LumaWeights = .bt709
    public var lut: Bool = false
    public var falseColor: FalseColor?
    public var zebras: [Zebra] = []
    public var zebraStyle: ZebraStyle = .still(stripePx: 8)
    public var peaking: Peaking?

    enum CodingKeys: String, CodingKey {
        case range, luma, lut, zebras, peaking
        case falseColor = "false_color"
        case zebraStyle = "zebra_style"
    }

    public init() {}

    /// A camera app's false color.
    public static let falseColorCamera: [FalseColorBand] = [
        .init(-1, 0.025, [0.5, 0.0, 0.6]),
        .init(0.025, 0.10, [0.0, 0.2, 1.0]),
        .init(0.38, 0.42, [0.0, 0.8, 0.2]),
        .init(0.52, 0.56, [1.0, 0.45, 0.7]),
        .init(0.99, 2, [1.0, 0.0, 0.0]),
        .init(0.97, 0.99, [1.0, 0.9, 0.0]),
    ]

    /// A production controller's false color.
    public static let falseColorController: [FalseColorBand] = [
        .init(-1, 0.025, [0.55, 0.0, 0.7]),
        .init(0.025, 0.10, [0.1, 0.25, 1.0]),
        .init(0.38, 0.42, [0.1, 0.8, 0.2]),
        .init(0.52, 0.56, [1.0, 0.55, 0.75]),
        .init(0.97, 0.99, [1.0, 0.95, 0.1]),
        .init(0.99, 2, [1.0, 0.1, 0.1]),
    ]

    /// True when nothing is changed.
    public var isIdentity: Bool { !lut && falseColor == nil && zebras.isEmpty && peaking == nil }

    /// The JSON the C ABI takes, with the output size and LUT size added.
    func json(width: Float, height: Float, lutSize: UInt32) -> String {
        guard let data = try? JSONEncoder().encode(self),
              var object = try? JSONSerialization.jsonObject(with: data) as? [String: Any]
        else { return "{}" }
        object["size"] = [width, height]
        object["lut_size"] = lutSize
        guard let out = try? JSONSerialization.data(withJSONObject: object) else { return "{}" }
        return String(decoding: out, as: UTF8.self)
    }
}
