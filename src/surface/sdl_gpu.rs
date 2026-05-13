//! SDL GPU surface (platform bridge).
//!
//! The `SdlGpuSurface` type and the renderer that drives it live together
//! in [`crate::draw::sdl_gpu`] because they share an `sdl2::Canvas`
//! handle. This file re-exports the platform side so user code can still
//! write `use mirui::surface::sdl_gpu::SdlGpuSurface`.
pub use crate::draw::sdl_gpu::{SdlGpuFactory, SdlGpuSurface};
