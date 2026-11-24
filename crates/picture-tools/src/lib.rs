//! Picture tools for video apps: 3D LUTs ([`lut`]), false color, zebras
//! and focus peaking ([`tools`], run on the GPU by the generated
//! [`shaders`] and on the CPU by [`tools::apply_cpu`]), and video scopes
//! ([`scopes`]). Pure Rust (`std` and `serde`); the `capi` feature adds a C
//! ABI for apps without Rust bindings.

pub mod lut;
pub mod scopes;
pub mod shaders;
pub mod tools;

#[cfg(any(test, feature = "codegen"))]
#[doc(hidden)]
pub mod codegen;

#[cfg(feature = "capi")]
pub mod capi;
