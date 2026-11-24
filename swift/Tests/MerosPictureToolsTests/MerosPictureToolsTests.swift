import Metal
import XCTest

@testable import MerosPictureTools

/// Runs in the simulator or on a Mac, with the Rust library (the crate's
/// `capi` feature) linked into the test bundle.
final class MerosPictureToolsTests: XCTestCase {
    func testUniformsAreTheCrates() {
        var p = PictureToolsParams()
        p.zebras = [.init(level: 0.95)]
        p.zebraStyle = .animated(stripePx: 8, speedPxPerS: 40)
        let u = PictureToolsRenderer.uniforms(p, width: 640, height: 360, time: 2)
        XCTAssertEqual(u.count, 64)
        XCTAssertEqual(Array(u[0..<3]), [640, 360, 2])
    }

    func testScopesThroughTheABI() {
        let px = [UInt8](repeating: 128, count: 64 * 36 * 4)
        let frame = px.withUnsafeBytes { ScopeFrame.compute(.waveform, format: .rgba, pixels: $0, width: 64, height: 36, stride: 256) }
        XCTAssertEqual(frame?.width, 256)
        XCTAssertEqual(frame?.height, 256)
    }

    /// The Metal shader over a solid picture with no tools gives the
    /// picture back, with no LUT texture of its own bound.
    func testTheMetalShaderDrawsThePicture() throws {
        guard let device = MTLCreateSystemDefaultDevice(), let queue = device.makeCommandQueue() else {
            throw XCTSkip("no Metal device")
        }
        let renderer = try PictureToolsRenderer(device: device, pixelFormat: .rgba8Unorm)
        let (w, h) = (32, 16)
        let d = MTLTextureDescriptor.texture2DDescriptor(pixelFormat: .rgba8Unorm, width: w, height: h, mipmapped: false)
        d.usage = [.shaderRead, .renderTarget]
        let input = device.makeTexture(descriptor: d)!
        let pixels: [UInt8] = (0..<(w * h * 4)).map { $0 % 4 == 3 ? 255 : 100 }
        input.replace(region: MTLRegionMake2D(0, 0, w, h), mipmapLevel: 0, withBytes: pixels, bytesPerRow: w * 4)
        let output = device.makeTexture(descriptor: d)!
        let pass = MTLRenderPassDescriptor()
        pass.colorAttachments[0].texture = output
        pass.colorAttachments[0].loadAction = .clear
        pass.colorAttachments[0].storeAction = .store
        let buffer = queue.makeCommandBuffer()!
        let encoder = buffer.makeRenderCommandEncoder(descriptor: pass)!
        renderer.encode(into: encoder, input: input, params: PictureToolsParams(), width: w, height: h)
        encoder.endEncoding()
        buffer.commit()
        buffer.waitUntilCompleted()
        var out = [UInt8](repeating: 0, count: w * h * 4)
        output.getBytes(&out, bytesPerRow: w * 4, from: MTLRegionMake2D(0, 0, w, h), mipmapLevel: 0)
        XCTAssertEqual(Array(out[0..<4]), [100, 100, 100, 255])
    }
}
