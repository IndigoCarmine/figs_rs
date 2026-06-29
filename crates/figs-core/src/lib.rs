//! figs-core — headless layout engine for poster/figure composition.
//!
//! Pipeline: TOML source → [`schema::Document`] (parse + resolve) →
//! [`layout::ComputedLayout`] (the backend-agnostic IR) → renderers (PNG/PDF,
//! added in later milestones).
//!
//! The crate performs no filesystem or windowing I/O of its own beyond reading
//! fonts and images during measurement/rendering, so it can be reused verbatim
//! by the Phase 2 Tauri backend.

pub mod geom;
pub mod layout;
pub mod render;
pub mod schema;
pub mod text;
pub mod units;

pub use layout::{layout, ComputedLayout, LeafMeasure, NullMeasurer};
pub use render::{render_png, PngError};
pub use schema::Document;
pub use text::{FontStore, ShapedText};

/// Top-level error for the parse → resolve pipeline.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Parse(#[from] schema::parse::ParseError),
    #[error(transparent)]
    Resolve(#[from] schema::ResolveError),
}
