//! Exact calculator core for diagrammatic algebra.
//!
//! Everything is a linear combination of layered words in a presentation of a strict monoidal
//! supercategory. Nothing here is specific to KLR algebras: a family of algebras enters through
//! a presentation file and, where needed, a representation or normal-form strategy behind a
//! general interface. See `docs/design.md`.

/// The crate version, reported by every surface.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
