//! Media-aware preparation of compiled programs for rendering.
//!
//! Browser preparation accepts host-supplied virtual asset facts and performs no
//! filesystem or process I/O. With the `native` feature, preflight becomes the
//! first pipeline phase allowed to inspect result-reachable assets, `FFmpeg`, or
//! `FFprobe`; it verifies those contracts and produces an invariant-protected
//! native prepared plan.
//!
//! ```no_run
//! # #[cfg(feature = "native")]
//! # fn run() -> Result<(), clipasm::diagnostic::Diagnostic> {
//! use std::path::Path;
//!
//! let source = clipasm::language::parse_file(Path::new("program.clipasm"))?;
//! let compiled = clipasm::compiler::compile(&source)?;
//! let plan = clipasm::preflight::preflight(&compiled)?;
//! let result = &plan.nodes()[plan.result().get() as usize];
//! println!("prepared {} frames", result.video_domain().expect("Video result").frames().0);
//! # Ok(())
//! # }
//! ```

#[cfg(feature = "native")]
mod assets;
pub mod browser;
#[cfg(feature = "native")]
mod capabilities;
mod color;
mod identity;
mod lower;
#[cfg(feature = "native")]
mod native;
mod plan;
mod policy;
pub(crate) mod tools;

pub(crate) const MAX_COMPOSED_ZOOM_FILTER_BYTES: usize = 24 * 1024;

#[cfg(feature = "native")]
pub(crate) use assets::verify_prepared_asset;
pub use color::{ChromaLocation, PreparedSourceColor, SourceColorConvention};
#[cfg(feature = "native")]
pub use native::preflight;
#[cfg(feature = "native")]
pub use plan::PreparedPlan;
#[cfg(feature = "native")]
pub(crate) use plan::PreparedResource;
pub(crate) use plan::WorkingArtifactContract;
pub use plan::{
    PreparedAsset, PreparedAudioKind, PreparedExternalArgument, PreparedExternalParameterValue,
    PreparedNode, PreparedNodeMedia, PreparedVideoKind, PreparedZoomCurve,
};
pub(crate) use policy::{AudioEncoding, RenderPolicy, VideoEncoding};
pub use tools::ExternalToolIdentity;
