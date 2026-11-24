import CMerosPictureTools
import Foundation
import Metal

/// Why the renderer couldn't do something.
public struct PictureToolsError: Error, CustomStringConvertible {
    public let description: String
}

/// A LUT as the shader samples it: its N²×N RGBA atlas as a texture.
public final class PictureToolsLUT {
    public let texture: MTLTexture
    /// N, for an N³ LUT.
    public let size: UInt32

    /// Parses a `.cube` or `.3dl` file (by `name`'s extension).
    public convenience init(device: MTLDevice, name: String, text: String) throws {
        var err: UnsafeMutablePointer<CChar>?
        let bytes = Array(text.utf8)
        let lut: OpaquePointer? = bytes.withUnsafeBufferPointer { buf in
            buf.baseAddress!.withMemoryRebound(to: CChar.self, capacity: buf.count) { text in
                name.withCString { mpt_lut_parse($0, text, buf.count, &err) }
            }
        }
        guard let lut else {
            let message = err.map { String(cString: $0) } ?? "the LUT couldn't be read"
            if let err { mpt_string_free(err) }
            throw PictureToolsError(description: message)
        }
        defer { mpt_lut_free(lut) }
        var w: UInt32 = 0, h: UInt32 = 0
        mpt_lut_atlas_rgba8(lut, &w, &h, nil)
        var px = [UInt8](repeating: 0, count: Int(w * h * 4))
        mpt_lut_atlas_rgba8(lut, &w, &h, &px)
        try self.init(device: device, atlas: px, width: Int(w), height: Int(h))
    }

    /// From an atlas made elsewhere (`Lut3d::atlas_rgba8`).
    public init(device: MTLDevice, atlas: [UInt8], width: Int, height: Int) throws {
        let d = MTLTextureDescriptor.texture2DDescriptor(pixelFormat: .rgba8Unorm, width: width, height: height, mipmapped: false)
        d.usage = .shaderRead
        guard let t = device.makeTexture(descriptor: d) else { throw PictureToolsError(description: "no LUT texture") }
        atlas.withUnsafeBytes { raw in
            t.replace(region: MTLRegionMake2D(0, 0, width, height), mipmapLevel: 0, withBytes: raw.baseAddress!, bytesPerRow: width * 4)
        }
        texture = t
        size = UInt32(height)
    }
}

/// Runs the picture tools over a texture with the generated Metal shader
/// (`picture_tools_vertex` / `picture_tools_fragment`): one full-screen
/// triangle into whatever render pass the caller has begun.
public final class PictureToolsRenderer {
    public let device: MTLDevice
    private let pipeline: MTLRenderPipelineState
    /// Bound for the LUT when there's none: the shader doesn't sample it
    /// then, but Metal wants every texture argument bound.
    private let blank: MTLTexture

    /// `pixelFormat` is the render target's.
    public init(device: MTLDevice, pixelFormat: MTLPixelFormat) throws {
        self.device = device
        let library: MTLLibrary
        do {
            library = try device.makeDefaultLibrary(bundle: .module)
        } catch {
            // Built without Metal compilation (a plain `swift build`):
            // compile the source the crate carries.
            guard let source = mpt_shader_source(MPT_SHADER_METAL) else {
                throw PictureToolsError(description: "no Metal shader source")
            }
            library = try device.makeLibrary(source: String(cString: source), options: nil)
        }
        guard let vertex = library.makeFunction(name: "picture_tools_vertex"),
              let fragment = library.makeFunction(name: "picture_tools_fragment")
        else { throw PictureToolsError(description: "the picture-tools functions are missing from the library") }
        let d = MTLRenderPipelineDescriptor()
        d.vertexFunction = vertex
        d.fragmentFunction = fragment
        d.colorAttachments[0].pixelFormat = pixelFormat
        pipeline = try device.makeRenderPipelineState(descriptor: d)
        let bd = MTLTextureDescriptor.texture2DDescriptor(pixelFormat: .rgba8Unorm, width: 1, height: 1, mipmapped: false)
        guard let b = device.makeTexture(descriptor: bd) else { throw PictureToolsError(description: "no texture") }
        b.replace(region: MTLRegionMake2D(0, 0, 1, 1), mipmapLevel: 0, withBytes: [UInt8](arrayLiteral: 0, 0, 0, 255), bytesPerRow: 4)
        blank = b
    }

    /// The shader's uniforms for `params` on an output of `width`×`height`
    /// at `time` (seconds), as the packed floats its buffer takes.
    public static func uniforms(_ params: PictureToolsParams, width: Float, height: Float, lutSize: UInt32 = 33, time: Float = 0) -> [Float] {
        let json = params.json(width: width, height: height, lutSize: lutSize)
        let count = json.withCString { mpt_uniforms($0, time, nil, 0) }
        var out = [Float](repeating: 0, count: count)
        _ = json.withCString { mpt_uniforms($0, time, &out, count) }
        return out
    }

    /// Draws `input` with the tools into the encoder's render target, which
    /// is `width`×`height` pixels.
    public func encode(
        into encoder: MTLRenderCommandEncoder,
        input: MTLTexture,
        params: PictureToolsParams,
        lut: PictureToolsLUT? = nil,
        width: Int,
        height: Int,
        time: Float = 0
    ) {
        let floats = Self.uniforms(params, width: Float(width), height: Float(height), lutSize: lut?.size ?? 33, time: time)
        encoder.setRenderPipelineState(pipeline)
        floats.withUnsafeBytes { raw in
            encoder.setFragmentBytes(raw.baseAddress!, length: raw.count, index: 0)
        }
        encoder.setFragmentTexture(input, index: 0)
        encoder.setFragmentTexture(params.lut ? (lut?.texture ?? blank) : blank, index: 1)
        encoder.drawPrimitives(type: .triangle, vertexStart: 0, vertexCount: 3)
    }
}
