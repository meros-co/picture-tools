//! The C ABI (feature `capi`), for Swift and for apps without Rust
//! bindings. `include/meros_picture_tools.h` is generated from it with
//! cbindgen.
//!
//! Strings in are UTF-8; strings out (errors) are freed with
//! [`mpt_string_free`]. Output buffers are the caller's; pass a null buffer
//! first to learn the size.

use std::ffi::{CStr, CString, c_char, c_int};

use crate::lut::{Lut3d, parse_by_name};
use crate::scopes::{Picture, ScopeKind, ScopeOptions, compute};
use crate::tools::{PictureToolsParams, uniforms_for};

/// A parsed LUT.
pub struct MptLut(Lut3d);

/// Pixel formats for [`mpt_scope`].
pub const MPT_PIXEL_RGBA: c_int = 0;
pub const MPT_PIXEL_BGRA: c_int = 1;
pub const MPT_PIXEL_UYVY: c_int = 2;
pub const MPT_PIXEL_NV12: c_int = 3;

/// Shader dialects for [`mpt_shader_source`].
pub const MPT_SHADER_FLUTTER: c_int = 0;
pub const MPT_SHADER_GLES3: c_int = 1;
pub const MPT_SHADER_METAL: c_int = 2;
pub const MPT_SHADER_WEBGL1: c_int = 3;

fn set_error(err: *mut *mut c_char, message: &str) {
    if !err.is_null() {
        let c = CString::new(message.replace('\0', " ")).unwrap_or_default();
        unsafe { *err = c.into_raw() };
    }
}

/// Parses a `.cube` or `.3dl` LUT (by `name`'s extension) from `len` bytes
/// of `text`. Null on failure, with the reason in `*err` when `err` isn't
/// null (free it with [`mpt_string_free`]).
///
/// # Safety
/// `name` is a NUL-terminated string; `text` points to `len` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mpt_lut_parse(name: *const c_char, text: *const c_char, len: usize, err: *mut *mut c_char) -> *mut MptLut {
    if name.is_null() || text.is_null() {
        set_error(err, "null argument");
        return std::ptr::null_mut();
    }
    let name = unsafe { CStr::from_ptr(name) }.to_string_lossy();
    let bytes = unsafe { std::slice::from_raw_parts(text as *const u8, len) };
    let Ok(text) = std::str::from_utf8(bytes) else {
        set_error(err, "the LUT isn't UTF-8 text");
        return std::ptr::null_mut();
    };
    match parse_by_name(&name, text) {
        Ok(lut) => Box::into_raw(Box::new(MptLut(lut))),
        Err(e) => {
            set_error(err, &e.0);
            std::ptr::null_mut()
        }
    }
}

/// The LUT's size N (for `u_lut_size`).
///
/// # Safety
/// `lut` is from [`mpt_lut_parse`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mpt_lut_size(lut: *const MptLut) -> u32 {
    if lut.is_null() { 0 } else { unsafe { &*lut }.0.size }
}

/// The LUT's N²×N RGBA8 atlas: writes its size to `w` and `h`, and its
/// pixels to `out` (`w × h × 4` bytes) unless `out` is null.
///
/// # Safety
/// `lut` is from [`mpt_lut_parse`]; `w` and `h` are valid; `out` is null
/// or holds `w × h × 4` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mpt_lut_atlas_rgba8(lut: *const MptLut, w: *mut u32, h: *mut u32, out: *mut u8) {
    if lut.is_null() || w.is_null() || h.is_null() {
        return;
    }
    let (aw, ah, px) = unsafe { &*lut }.0.atlas_rgba8();
    unsafe {
        *w = aw;
        *h = ah;
        if !out.is_null() {
            std::ptr::copy_nonoverlapping(px.as_ptr(), out, px.len());
        }
    }
}

/// Frees a LUT. Null is fine.
///
/// # Safety
/// `lut` is from [`mpt_lut_parse`] and not already freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mpt_lut_free(lut: *mut MptLut) {
    if !lut.is_null() {
        drop(unsafe { Box::from_raw(lut) });
    }
}

/// Frees a string this library returned.
///
/// # Safety
/// `s` is from this library and not already freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mpt_string_free(s: *mut c_char) {
    if !s.is_null() {
        drop(unsafe { CString::from_raw(s) });
    }
}

/// The shader uniforms' values, flattened in `shaders/uniforms.json` order,
/// for the tools in `params_json` (a `PictureToolsParams` as JSON, plus
/// optional `"size": [w, h]` and `"lut_size": N`) at `time_s`. Writes up to
/// `cap` floats to `out` and returns how many there are (64), or 0 when the
/// JSON is invalid.
///
/// # Safety
/// `params_json` is a NUL-terminated string; `out` holds `cap` floats or is
/// null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mpt_uniforms(params_json: *const c_char, time_s: f32, out: *mut f32, cap: usize) -> usize {
    let json = if params_json.is_null() { "{}".into() } else { unsafe { CStr::from_ptr(params_json) }.to_string_lossy() };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&json) else { return 0 };
    let size = value
        .get("size")
        .and_then(|s| s.as_array())
        .map(|a| [a.first().and_then(|v| v.as_f64()).unwrap_or(0.0) as f32, a.get(1).and_then(|v| v.as_f64()).unwrap_or(0.0) as f32])
        .unwrap_or([0.0, 0.0]);
    let lut_size = value.get("lut_size").and_then(|v| v.as_u64()).unwrap_or(33) as u32;
    let Ok(params) = serde_json::from_value::<PictureToolsParams>(value) else { return 0 };
    let floats: Vec<f32> = uniforms_for(&params, size, lut_size, time_s).into_iter().flat_map(|u| u.value).collect();
    if !out.is_null() {
        let n = floats.len().min(cap);
        unsafe { std::ptr::copy_nonoverlapping(floats.as_ptr(), out, n) };
    }
    floats.len()
}

/// Computes scope `kind` (0 waveform, 1 parade, 2 vectorscope, 3
/// histogram) of a `w`×`h` picture in `pixel_format` (`MPT_PIXEL_*`) with
/// rows `stride` bytes apart, using `options_json` (a `ScopeOptions` as
/// JSON; null for the defaults). Writes the scope's size to `out_w` and
/// `out_h` and its intensities to `out` unless `out` is null. Returns 0, or
/// −1 for a bad argument.
///
/// # Safety
/// `px` holds the picture; `out_w`, `out_h` are valid; `out` is null or
/// holds `out_w × out_h` bytes.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn mpt_scope(
    kind: c_int,
    pixel_format: c_int,
    px: *const u8,
    w: u32,
    h: u32,
    stride: u32,
    options_json: *const c_char,
    out_w: *mut u32,
    out_h: *mut u32,
    out: *mut u8,
) -> c_int {
    let Some(kind) = ScopeKind::from_index(kind) else { return -1 };
    if px.is_null() || out_w.is_null() || out_h.is_null() || w == 0 || h == 0 {
        return -1;
    }
    let options = if options_json.is_null() {
        ScopeOptions::CONTROLLER
    } else {
        let json = unsafe { CStr::from_ptr(options_json) }.to_string_lossy();
        match serde_json::from_str(&json) {
            Ok(o) => o,
            Err(_) => return -1,
        }
    };
    let len = match pixel_format {
        MPT_PIXEL_NV12 => stride as usize * h as usize * 3 / 2,
        _ => stride as usize * h as usize,
    };
    let bytes = unsafe { std::slice::from_raw_parts(px, len) };
    let pic = match pixel_format {
        MPT_PIXEL_RGBA => Picture::from_rgba(bytes, w, h, stride, &options),
        MPT_PIXEL_BGRA => Picture::from_bgra(bytes, w, h, stride, &options),
        MPT_PIXEL_UYVY => Picture::from_uyvy(bytes, w, h, stride, &options),
        MPT_PIXEL_NV12 => Picture::from_nv12(bytes, w, h, stride, &options),
        _ => return -1,
    };
    let frame = compute(kind, &pic, &options);
    unsafe {
        *out_w = frame.width;
        *out_h = frame.height;
        if !out.is_null() {
            std::ptr::copy_nonoverlapping(frame.data.as_ptr(), out, frame.data.len());
        }
    }
    0
}

/// A generated shader's source (`MPT_SHADER_*`), NUL-terminated and
/// static; null for an unknown dialect.
#[unsafe(no_mangle)]
pub extern "C" fn mpt_shader_source(dialect: c_int) -> *const c_char {
    use std::sync::OnceLock;
    static SOURCES: OnceLock<[CString; 4]> = OnceLock::new();
    let s = SOURCES.get_or_init(|| {
        use crate::shaders::*;
        [FLUTTER_FRAGMENT, GLES3_FRAGMENT, METAL_SOURCE, WEBGL1_FRAGMENT].map(|t| CString::new(t).unwrap())
    });
    usize::try_from(dialect).ok().and_then(|i| s.get(i)).map_or(std::ptr::null(), |c| c.as_ptr())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_lut_round_trips_through_the_abi() {
        let text = Lut3d::identity(4).to_cube();
        let mut err = std::ptr::null_mut();
        let lut = unsafe { mpt_lut_parse(c"look.cube".as_ptr(), text.as_ptr() as *const c_char, text.len(), &mut err) };
        assert!(!lut.is_null() && err.is_null());
        assert_eq!(unsafe { mpt_lut_size(lut) }, 4);
        let (mut w, mut h) = (0, 0);
        unsafe { mpt_lut_atlas_rgba8(lut, &mut w, &mut h, std::ptr::null_mut()) };
        assert_eq!((w, h), (16, 4));
        let mut px = vec![0u8; (w * h * 4) as usize];
        unsafe { mpt_lut_atlas_rgba8(lut, &mut w, &mut h, px.as_mut_ptr()) };
        assert_eq!(&px[4..8], &[85, 0, 0, 255]);
        unsafe { mpt_lut_free(lut) };
        let bad = unsafe { mpt_lut_parse(c"x.cube".as_ptr(), c"nope".as_ptr(), 4, &mut err) };
        assert!(bad.is_null() && !err.is_null());
        unsafe { mpt_string_free(err) };
    }

    #[test]
    fn uniforms_and_scopes_through_the_abi() {
        let json = c"{\"size\":[640,360],\"zebras\":[{\"level\":0.95}],\"zebra_style\":{\"style\":\"animated\",\"stripe_px\":8,\"speed_px_per_s\":40}}";
        let n = unsafe { mpt_uniforms(json.as_ptr(), 2.0, std::ptr::null_mut(), 0) };
        assert_eq!(n, 64);
        let mut f = vec![0f32; n];
        unsafe { mpt_uniforms(json.as_ptr(), 2.0, f.as_mut_ptr(), n) };
        assert_eq!(&f[..3], &[640.0, 360.0, 2.0]);
        assert_eq!(unsafe { mpt_uniforms(c"not json".as_ptr(), 0.0, std::ptr::null_mut(), 0) }, 0);

        let px = vec![128u8; 64 * 36 * 4];
        let (mut w, mut h) = (0, 0);
        assert_eq!(unsafe { mpt_scope(0, MPT_PIXEL_RGBA, px.as_ptr(), 64, 36, 256, std::ptr::null(), &mut w, &mut h, std::ptr::null_mut()) }, 0);
        assert_eq!((w, h), (256, 256));
        let mut out = vec![0u8; (w * h) as usize];
        unsafe { mpt_scope(0, MPT_PIXEL_RGBA, px.as_ptr(), 64, 36, 256, std::ptr::null(), &mut w, &mut h, out.as_mut_ptr()) };
        assert!(out[(255 - 128) * 256 + 100] > 0);
        assert_eq!(unsafe { mpt_scope(9, 0, px.as_ptr(), 64, 36, 256, std::ptr::null(), &mut w, &mut h, std::ptr::null_mut()) }, -1);
        assert!(!mpt_shader_source(MPT_SHADER_METAL).is_null());
        assert!(mpt_shader_source(7).is_null());
    }
}
