//! Verified execution, caching, and rollback-capable publication of prepared plans.
//!
//! Native rendering accepts only an invariant-protected prepared plan,
//! re-verifies source content reached by cache-aware execution, and reuses
//! compatible cached artifacts. It executes `FFmpeg` primitives and publishes
//! the MP4 and manifest as one in-process transaction. The `browser` adapter
//! serializes the same closed recipes for an isolated WebAssembly host.

#[cfg(feature = "native")]
mod artifact;
pub mod browser;
#[cfg(feature = "native")]
mod cache;
mod color;
mod execute;
#[cfg(feature = "native")]
mod execution_plan;
#[cfg(feature = "native")]
mod lock;
#[cfg(feature = "native")]
mod manifest;
#[cfg(feature = "native")]
mod native;
#[cfg(feature = "native")]
mod publication;
#[cfg(feature = "native")]
mod staging;

#[cfg(feature = "native")]
pub use native::{
    CacheMode, MaterializationMode, RenderOptions, RenderReport, render, render_with_cache_root,
    render_with_options, render_without_cache,
};
