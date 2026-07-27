//! # contreforts-declaration
//!
//! The declaration vocabulary, its meta-shapes, and the D14 `sh:xone`
//! cross-exclusion lint -- contreforts/contreforts-core#14, phase C item
//! C1 of the bootstrap epic (contreforts/contreforts-workspace#20).
//!
//! A connector describes itself in Turtle
//! (contreforts-connector-forgejo#9 is the pilot). This crate defines what
//! a valid self-description *is*, and checks it at build time:
//! [`validate`] parses a declaration, runs it against `meta-shapes.ttl`
//! (SHACL), and runs the D14/D2/structural lints that SHACL cannot express
//! as constraints on itself.
//!
//! ## This nesting is provisional
//!
//! `contreforts-declaration` is a separate crate, but it currently lives
//! *nested inside* `contreforts-core`, at `declaration/` -- so the
//! workspace member path is `crates/contreforts-core/declaration`. Every
//! other crate in this org is one-crate-per-repo; this one breaks that
//! deliberately, to avoid standing up a new repository before the
//! mechanism has proven itself. It is expected to be replaced by, or
//! extended into, its own repository once phase C is complete and the
//! shape of this crate has stopped moving.
//!
//! Two things are true so that promotion is a `git mv` plus one
//! `Cargo.toml` line, not a rewrite:
//!   - This crate does not depend on `contreforts-core` -- no `path =
//!     "../"`, no use of anything core exports. `contreforts-core`'s own
//!     manifest only gains `exclude = ["declaration"]`.
//!   - Nothing about this crate's own manifest assumes it is a member of
//!     the outer `contreforts-workspace` superproject it happens to sit
//!     inside today (see `Cargo.toml`'s dependency comments).
//!
//! See contreforts/contreforts-core#14 for the full design discussion; do
//! not mistake this nesting for a considered end state.
//!
//! ## Why not inside `contreforts-core` itself
//!
//! A `build.rs` cannot use its own crate's normal dependencies -- only
//! `[build-dependencies]`. Every connector's build script needs this code,
//! so it has to be a separate crate they can build-depend on. Folding it
//! into `contreforts-core` would instead force `oxigraph` and a SHACL
//! validator into the one crate every other crate in the workspace
//! depends on.
//!
//! ## What is explicitly out of scope here
//!
//! No Turtle-to-Rust emitter (phase D, once `contreforts-kg` is
//! generalised), no aggregation/product-graph/collision-detection (C3),
//! and no connector wiring (C7) -- this crate is usable by a connector's
//! `build.rs`, but nothing here calls it from one yet.

mod error;
mod lint;
mod model;
mod validate;

pub use error::{Rule, Violation, Violations};
pub use model::{Declaration, PropertyShape};
pub use validate::validate;

/// The declaration vocabulary (Part 1): three terms in
/// `https://contreforts.ds-labs.org/ontologies/declaration#` --
/// `contreforts:secret`, `contreforts:category`, `contreforts:uiShape`.
/// Embedded so a caller can inspect or re-serve it without a filesystem
/// dependency on this crate's source layout.
pub const VOCABULARY_TTL: &str = include_str!("vocabulary.ttl");

/// The meta-shapes (Part 2): the SHACL shapes a connector declaration's
/// own graph must satisfy. [`validate`] runs these internally; exposed
/// here mainly so a caller (or this crate's own tests) can inspect exactly
/// what ran.
pub const META_SHAPES_TTL: &str = include_str!("meta_shapes.ttl");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vocabulary_defines_exactly_the_three_terms() {
        for term in ["secret", "category", "uiShape"] {
            assert!(
                VOCABULARY_TTL.contains(&format!("contreforts:{term} a rdf:Property")),
                "vocabulary.ttl should define contreforts:{term}"
            );
        }
    }
}
